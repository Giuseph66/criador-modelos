//! Etapa 1 do pipeline: imagem colorida -> máscara preto e branco.
//!
//! Substitui o passo "pedir pro ChatGPT deixar preto e branco".
//! Convenção da máscara: pixel 0 (preto) = "tinta" (vira forma no SVG),
//! pixel 255 (branco) = fundo.

use image::{GrayImage, Luma, Rgb, RgbImage, RgbaImage, imageops};

/// Ampliação máxima aplicada a imagens pequenas.
const MAX_UPSCALE: f32 = 4.0;

/// Como decidir se um pixel vira preto ou branco.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThresholdMode {
    /// Corte fixo escolhido no slider.
    Fixed,
    /// Otsu: calcula automaticamente o melhor corte pelo histograma.
    Otsu,
    /// Bradley-Roth: corte local, bom para fotos/scans com luz irregular.
    Adaptive,
}

/// Parâmetros da conversão para preto e branco.
#[derive(Clone, PartialEq, Debug)]
pub struct BwParams {
    /// Desfoque gaussiano antes do corte (suaviza bordas e ruído).
    pub blur_sigma: f32,
    pub mode: ThresholdMode,
    /// Corte usado no modo `Fixed` (0..=255).
    pub threshold: u8,
    /// Sensibilidade (%) do modo adaptativo.
    pub adaptive_sensitivity: f32,
    /// Troca preto por branco.
    pub invert: bool,
}

impl Default for BwParams {
    fn default() -> Self {
        Self {
            blur_sigma: 0.8,
            mode: ThresholdMode::Otsu,
            threshold: 128,
            adaptive_sensitivity: 15.0,
            invert: false,
        }
    }
}

/// Resultado da etapa P&B.
pub struct BwResult {
    pub mask: GrayImage,
    /// Corte efetivamente usado (None no modo adaptativo, que usa um corte por pixel).
    pub threshold_used: Option<u8>,
}

/// Maior lado (px) padrão usado no processamento.
pub const DEFAULT_MAX_SIDE: u32 = 1200;

/// Prepara a imagem para processar: transparência vira branco e o tamanho
/// é ajustado para `max_side`. Reduz imagens grandes (velocidade) e amplia
/// pequenas até 4x — ampliar com interpolação deixa as bordas do vetor mais lisas.
pub fn prepare_rgb(original: &RgbaImage, max_side: u32) -> RgbImage {
    // Primeiro mistura com fundo branco, depois redimensiona: assim os
    // pixels transparentes não "sujam" as bordas de preto.
    let flat = RgbImage::from_fn(original.width(), original.height(), |x, y| {
        let [r, g, b, a] = original.get_pixel(x, y).0;
        let alpha = a as f32 / 255.0;
        let mix = |c: u8| (c as f32 * alpha + 255.0 * (1.0 - alpha)).round() as u8;
        Rgb([mix(r), mix(g), mix(b)])
    });

    let (w, h) = flat.dimensions();
    let scale = (max_side as f32 / w.max(h) as f32).min(MAX_UPSCALE);
    if (scale - 1.0).abs() <= 0.01 {
        return flat;
    }
    let nw = ((w as f32 * scale).round() as u32).max(1);
    let nh = ((h as f32 * scale).round() as u32).max(1);
    imageops::resize(&flat, nw, nh, imageops::FilterType::Triangle)
}

/// Converte para tons de cinza (luminância perceptual, Rec. 601).
pub fn to_gray(rgb: &RgbImage) -> GrayImage {
    GrayImage::from_fn(rgb.width(), rgb.height(), |x, y| {
        let [r, g, b] = rgb.get_pixel(x, y).0;
        let luma = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
        Luma([luma.round() as u8])
    })
}

/// Suaviza as bordas de uma máscara P&B: desfoca e corta de novo no meio.
/// Arredonda serrilhados e some com pontinhos isolados.
pub fn smooth_mask(mask: &GrayImage, sigma: f32) -> GrayImage {
    if sigma <= 0.05 {
        return mask.clone();
    }
    apply_threshold(&imageops::blur(mask, sigma), 128)
}

/// Converte a imagem cinza em máscara preto e branco.
pub fn to_black_and_white(gray: &GrayImage, p: &BwParams) -> BwResult {
    let blurred = if p.blur_sigma > 0.05 {
        imageops::blur(gray, p.blur_sigma)
    } else {
        gray.clone()
    };

    let (mut mask, threshold_used) = match p.mode {
        ThresholdMode::Fixed => (apply_threshold(&blurred, p.threshold), Some(p.threshold)),
        ThresholdMode::Otsu => {
            let t = otsu_threshold(&blurred).saturating_add(1);
            (apply_threshold(&blurred, t), Some(t))
        }
        ThresholdMode::Adaptive => (adaptive_threshold(&blurred, p.adaptive_sensitivity), None),
    };

    if p.invert {
        imageops::invert(&mut mask);
    }

    BwResult { mask, threshold_used }
}

/// Pixel mais escuro que `t` vira preto (0), o resto branco (255).
fn apply_threshold(img: &GrayImage, t: u8) -> GrayImage {
    let mut out = img.clone();
    for p in out.pixels_mut() {
        p.0[0] = if p.0[0] < t { 0 } else { 255 };
    }
    out
}

/// Método de Otsu: escolhe o corte que maximiza a variância entre as
/// duas classes (escuro x claro) do histograma.
fn otsu_threshold(img: &GrayImage) -> u8 {
    let mut hist = [0u64; 256];
    for p in img.pixels() {
        hist[p.0[0] as usize] += 1;
    }

    let total = (img.width() as u64 * img.height() as u64) as f64;
    let sum_all: f64 = hist.iter().enumerate().map(|(i, &n)| i as f64 * n as f64).sum();

    let mut sum_bg = 0.0;
    let mut weight_bg = 0.0;
    let mut best_var = -1.0;
    let mut best_t = 128u8;

    for (t, &n) in hist.iter().enumerate() {
        weight_bg += n as f64;
        if weight_bg == 0.0 {
            continue;
        }
        let weight_fg = total - weight_bg;
        if weight_fg == 0.0 {
            break;
        }
        sum_bg += t as f64 * n as f64;
        let mean_bg = sum_bg / weight_bg;
        let mean_fg = (sum_all - sum_bg) / weight_fg;
        let between_var = weight_bg * weight_fg * (mean_bg - mean_fg).powi(2);
        if between_var > best_var {
            best_var = between_var;
            best_t = t as u8;
        }
    }
    best_t
}

/// Limiar adaptativo de Bradley-Roth usando imagem integral.
/// Cada pixel é comparado com a média de uma janela ao redor dele.
fn adaptive_threshold(img: &GrayImage, sensitivity_pct: f32) -> GrayImage {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let window = (w.max(h) / 8).max(3);
    let half = window / 2;
    let stride = w + 1;

    // integral[(y+1)*stride + (x+1)] = soma de todos os pixels em [0..=x, 0..=y]
    let mut integral = vec![0u64; stride * (h + 1)];
    let raw = img.as_raw();
    for y in 0..h {
        let mut row_sum = 0u64;
        for x in 0..w {
            row_sum += raw[y * w + x] as u64;
            integral[(y + 1) * stride + x + 1] = integral[y * stride + x + 1] + row_sum;
        }
    }

    let factor = 1.0 - (sensitivity_pct as f64 / 100.0);
    let mut out = GrayImage::new(w as u32, h as u32);
    for y in 0..h {
        let y1 = y.saturating_sub(half);
        let y2 = (y + half).min(h - 1);
        for x in 0..w {
            let x1 = x.saturating_sub(half);
            let x2 = (x + half).min(w - 1);
            let count = ((x2 - x1 + 1) * (y2 - y1 + 1)) as f64;
            let sum = integral[(y2 + 1) * stride + x2 + 1] + integral[y1 * stride + x1]
                - integral[y1 * stride + x2 + 1]
                - integral[(y2 + 1) * stride + x1];
            let v = raw[y * w + x] as f64;
            let is_ink = v * count < sum as f64 * factor;
            out.put_pixel(x as u32, y as u32, Luma([if is_ink { 0 } else { 255 }]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otsu_separa_duas_cores() {
        // Metade preta (20), metade clara (230): corte deve ficar entre elas.
        let img = GrayImage::from_fn(10, 10, |x, _| Luma([if x < 5 { 20 } else { 230 }]));
        let t = otsu_threshold(&img);
        assert!((20..230).contains(&t), "corte {t} fora do intervalo");
    }

    #[test]
    fn transparente_vira_branco() {
        let img = RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 0]));
        let rgb = prepare_rgb(&img, 100);
        assert!(rgb.pixels().all(|p| p.0 == [255, 255, 255]));
    }

    #[test]
    fn redimensiona_com_limite() {
        let img = RgbaImage::new(100, 50);
        assert_eq!(prepare_rgb(&img, 50).dimensions(), (50, 25)); // reduz
        assert_eq!(prepare_rgb(&img, 200).dimensions(), (200, 100)); // amplia
        assert_eq!(prepare_rgb(&img, 3000).dimensions(), (400, 200)); // máx 4x
    }

    #[test]
    fn inverter_troca_cores() {
        let gray = GrayImage::from_fn(4, 1, |x, _| Luma([if x < 2 { 0 } else { 255 }]));
        let params = BwParams { blur_sigma: 0.0, invert: true, ..Default::default() };
        let r = to_black_and_white(&gray, &params);
        assert_eq!(r.mask.get_pixel(0, 0).0[0], 255);
        assert_eq!(r.mask.get_pixel(3, 0).0[0], 0);
    }
}
