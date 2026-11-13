//! Cores e documento editável.
//!
//! Fluxo:
//! 1. `quantize`     — k-means agrupa os pixels da imagem em N cores.
//! 2. `Document`     — a imagem editável: cada pixel aponta para uma cor da
//!    paleta. Pincel, balde, juntar e adicionar cores mexem aqui.
//! 3. `build_layers` — cores iguais viram uma camada; cada camada vira uma
//!    máscara preto e branco pronta para vetorizar.

use image::{GrayImage, RgbImage};

use crate::processing;

/// Cor RGB de 8 bits por canal.
pub type Rgb8 = [u8; 3];

/// Máximo de cores na paleta (cada pixel guarda o índice num `u8`).
pub const MAX_COLORS: usize = 255;

/// Formata a cor como `#rrggbb`.
pub fn hex(c: Rgb8) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

// =====================================================================
// Detecção de cores (k-means)
// =====================================================================

/// Resultado do k-means: cada pixel aponta para uma cor da paleta.
pub struct Quantized {
    pub width: u32,
    pub height: u32,
    /// Índice da cor (em `palette`) de cada pixel, linha por linha.
    pub labels: Vec<u8>,
    /// Cores detectadas, da mais comum para a menos comum.
    pub palette: Vec<Rgb8>,
    /// Cor que mais aparece na borda — provavelmente o fundo.
    pub background: usize,
}

const MAX_SAMPLES: usize = 60_000;
const MAX_ITERATIONS: usize = 20;
/// Cores que ocupam menos que isso da imagem são absorvidas pela mais próxima.
const MIN_COVERAGE: f32 = 0.002;

type Px = [f32; 3];

/// Distância entre cores. Os pesos aproximam a percepção humana
/// (o olho é mais sensível ao verde).
fn dist2(a: &Px, b: &Px) -> f32 {
    let dr = a[0] - b[0];
    let dg = a[1] - b[1];
    let db = a[2] - b[2];
    2.0 * dr * dr + 4.0 * dg * dg + 3.0 * db * db
}

fn nearest(centers: &[Px], p: &Px) -> usize {
    let mut best = 0;
    let mut best_d = f32::MAX;
    for (i, c) in centers.iter().enumerate() {
        let d = dist2(c, p);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

fn to_px(p: &image::Rgb<u8>) -> Px {
    [p.0[0] as f32, p.0[1] as f32, p.0[2] as f32]
}

fn argmax(values: &[usize]) -> usize {
    values.iter().enumerate().max_by_key(|&(_, v)| *v).map_or(0, |(i, _)| i)
}

fn count_labels(labels: &[u8], n: usize) -> Vec<usize> {
    let mut counts = vec![0usize; n];
    for &l in labels {
        if let Some(c) = counts.get_mut(l as usize) {
            *c += 1;
        }
    }
    counts
}

/// Agrupa os pixels da imagem em até `count` cores (k-means).
pub fn quantize(img: &RgbImage, count: usize) -> Quantized {
    let count = count.clamp(1, MAX_COLORS);
    let (w, h) = img.dimensions();
    let total = (w as usize * h as usize).max(1);

    // Para achar os centros basta uma amostra; classificar usa todos os pixels.
    let step = (total / MAX_SAMPLES).max(1);
    let samples: Vec<Px> = img.pixels().step_by(step).map(to_px).collect();

    let mut centers = init_centers(&samples, count);
    for _ in 0..MAX_ITERATIONS {
        let mut sums = vec![[0f64; 3]; centers.len()];
        let mut counts = vec![0usize; centers.len()];
        for s in &samples {
            let i = nearest(&centers, s);
            for (sum, &v) in sums[i].iter_mut().zip(s) {
                *sum += v as f64;
            }
            counts[i] += 1;
        }
        let mut moved = 0.0f32;
        for (i, center) in centers.iter_mut().enumerate() {
            if counts[i] == 0 {
                continue;
            }
            let n = counts[i] as f64;
            let new = [(sums[i][0] / n) as f32, (sums[i][1] / n) as f32, (sums[i][2] / n) as f32];
            moved = moved.max(dist2(center, &new));
            *center = new;
        }
        if moved < 1.0 {
            break;
        }
    }

    // Classifica todos os pixels; descarta cores desprezíveis e reclassifica.
    let classify = |centers: &[Px]| -> Vec<u8> {
        img.pixels().map(|p| nearest(centers, &to_px(p)) as u8).collect()
    };
    let mut labels = classify(&centers);
    let counts = count_labels(&labels, centers.len());
    let min = (total as f32 * MIN_COVERAGE) as usize;
    let mut keep: Vec<usize> = (0..centers.len()).filter(|&i| counts[i] > min).collect();
    if keep.is_empty() {
        keep.push(argmax(&counts));
    }
    if keep.len() < centers.len() {
        centers = keep.iter().map(|&i| centers[i]).collect();
        labels = classify(&centers);
    }

    // Cor final de cada grupo = média real dos seus pixels.
    let n = centers.len();
    let mut sums = vec![[0u64; 3]; n];
    let mut counts = vec![0usize; n];
    for (p, &l) in img.pixels().zip(&labels) {
        for (sum, &v) in sums[l as usize].iter_mut().zip(&p.0) {
            *sum += v as u64;
        }
        counts[l as usize] += 1;
    }

    // Ordena da cor mais comum para a menos comum.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(counts[i]));
    let mut remap = vec![0u8; n];
    for (new, &old) in order.iter().enumerate() {
        remap[old] = new as u8;
    }
    for l in labels.iter_mut() {
        *l = remap[*l as usize];
    }

    let palette: Vec<Rgb8> = order
        .iter()
        .map(|&i| {
            let c = counts[i].max(1) as u64;
            [(sums[i][0] / c) as u8, (sums[i][1] / c) as u8, (sums[i][2] / c) as u8]
        })
        .collect();
    let background = border_majority(&labels, w as usize, h as usize, palette.len());

    Quantized { width: w, height: h, labels, palette, background }
}

/// k-means++: escolhe centros iniciais espalhados (cada novo centro tende a
/// ser uma cor distante das já escolhidas). Gerador fixo = resultado repetível.
fn init_centers(samples: &[Px], k: usize) -> Vec<Px> {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut random = move || {
        // xorshift64: gerador pseudoaleatório simples.
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    let mut centers = vec![samples[(random() % samples.len() as u64) as usize]];
    let mut dist: Vec<f32> = samples.iter().map(|s| dist2(s, &centers[0])).collect();

    while centers.len() < k {
        let sum: f64 = dist.iter().map(|&d| d as f64).sum();
        if sum <= 0.0 {
            break; // a imagem tem menos cores distintas que `k`.
        }
        let mut target = (random() as f64 / u64::MAX as f64) * sum;
        let mut chosen = samples.len() - 1;
        for (i, &d) in dist.iter().enumerate() {
            target -= d as f64;
            if target <= 0.0 {
                chosen = i;
                break;
            }
        }
        let c = samples[chosen];
        centers.push(c);
        for (d, s) in dist.iter_mut().zip(samples) {
            *d = d.min(dist2(s, &c));
        }
    }
    centers
}

/// Cor mais frequente nas bordas da imagem.
fn border_majority(labels: &[u8], w: usize, h: usize, n: usize) -> usize {
    let mut counts = vec![0usize; n];
    for x in 0..w {
        counts[labels[x] as usize] += 1;
        counts[labels[(h - 1) * w + x] as usize] += 1;
    }
    for y in 0..h {
        counts[labels[y * w] as usize] += 1;
        counts[labels[y * w + w - 1] as usize] += 1;
    }
    argmax(&counts)
}

// =====================================================================
// Documento editável
// =====================================================================

/// Uma cor da paleta do documento.
#[derive(Clone, PartialEq, Debug)]
pub struct ColorEntry {
    /// Cor como foi detectada (ou criada). Usada pelo botão "original".
    pub original: Rgb8,
    /// Cor atual.
    pub color: Rgb8,
    /// Fração da imagem (0..1) pintada com esta cor.
    pub coverage: f32,
    /// `true` = fundo: não entra no SVG/STL.
    pub background: bool,
}

impl ColorEntry {
    pub fn new(color: Rgb8, background: bool) -> Self {
        Self { original: color, color, coverage: 0.0, background }
    }
}

/// Retângulo de pixels alterados: `x0..x1`, `y0..y1` (fim exclusivo).
/// Serve para atualizar só o pedaço da tela que mudou.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dirty {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl Dirty {
    pub fn union(self, other: Dirty) -> Dirty {
        Dirty {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }

    pub fn width(&self) -> u32 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> u32 {
        self.y1 - self.y0
    }
}

/// Junta dois `Option<Dirty>`.
fn merge_dirty(a: Option<Dirty>, b: Option<Dirty>) -> Option<Dirty> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.union(b)),
        (a, None) => a,
        (None, b) => b,
    }
}

/// A imagem editável: cada pixel é um índice na paleta (`entries`).
///
/// `Clone` é usado pelo histórico (Ctrl+Z): cada versão é uma cópia.
#[derive(Clone, Debug)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    /// Índice da cor de cada pixel, linha por linha.
    pub labels: Vec<u8>,
    pub entries: Vec<ColorEntry>,
}

impl Document {
    /// Documento a partir das cores detectadas pelo k-means.
    pub fn from_quantized(q: Quantized) -> Self {
        let entries = q
            .palette
            .iter()
            .enumerate()
            .map(|(i, &c)| ColorEntry::new(c, i == q.background))
            .collect();
        let mut doc = Self { width: q.width, height: q.height, labels: q.labels, entries };
        doc.update_coverage();
        doc
    }

    /// Documento de duas cores a partir de uma máscara P&B (0 = preto).
    pub fn from_mask(mask: &GrayImage) -> Self {
        let labels = mask.pixels().map(|p| u8::from(p.0[0] < 128)).collect();
        let entries = vec![ColorEntry::new([255, 255, 255], true), ColorEntry::new([0, 0, 0], false)];
        let mut doc = Self { width: mask.width(), height: mask.height(), labels, entries };
        doc.update_coverage();
        doc
    }

    fn index(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    /// Índice da cor no pixel (x, y), se estiver dentro da imagem.
    pub fn label_at(&self, x: i64, y: i64) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return None;
        }
        Some(self.labels[self.index(x as u32, y as u32)] as usize)
    }

    /// Primeira cor marcada como fundo.
    pub fn background_index(&self) -> Option<usize> {
        self.entries.iter().position(|e| e.background)
    }

    /// Recalcula quanto da imagem cada cor ocupa.
    pub fn update_coverage(&mut self) {
        let counts = count_labels(&self.labels, self.entries.len());
        let total = self.labels.len().max(1) as f32;
        for (e, c) in self.entries.iter_mut().zip(counts) {
            e.coverage = c as f32 / total;
        }
    }

    /// Adiciona uma cor à paleta. Devolve o índice, ou `None` se a paleta
    /// já está cheia.
    pub fn add_color(&mut self, color: Rgb8, background: bool) -> Option<usize> {
        if self.entries.len() >= MAX_COLORS {
            return None;
        }
        self.entries.push(ColorEntry::new(color, background));
        Some(self.entries.len() - 1)
    }

    /// Índice da cor de fundo; cria um fundo branco se não existir.
    pub fn ensure_background(&mut self) -> Option<usize> {
        self.background_index().or_else(|| self.add_color([255, 255, 255], true))
    }

    /// Pinta um círculo (uma "pincelada" pontual).
    pub fn paint_dab(&mut self, cx: f32, cy: f32, radius: f32, label: u8) -> Option<Dirty> {
        let r = radius.max(0.5);
        let x0 = (cx - r).floor().max(0.0) as u32;
        let y0 = (cy - r).floor().max(0.0) as u32;
        let x1 = ((cx + r).ceil() + 1.0).clamp(0.0, self.width as f32) as u32;
        let y1 = ((cy + r).ceil() + 1.0).clamp(0.0, self.height as f32) as u32;
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let r2 = r * r;
        for y in y0..y1 {
            let dy = y as f32 + 0.5 - cy;
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - cx;
                if dx * dx + dy * dy <= r2 {
                    let i = self.index(x, y);
                    self.labels[i] = label;
                }
            }
        }
        Some(Dirty { x0, y0, x1, y1 })
    }

    /// Pinta uma linha de círculos de `from` até `to` (traço do pincel).
    pub fn paint_line(&mut self, from: [f32; 2], to: [f32; 2], radius: f32, label: u8) -> Option<Dirty> {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let dist = (dx * dx + dy * dy).sqrt();
        // Círculos próximos o bastante para o traço sair contínuo.
        let spacing = (radius * 0.35).max(0.5);
        let steps = (dist / spacing).ceil() as usize;
        let mut dirty = None;
        for i in 0..=steps {
            let t = if steps == 0 { 1.0 } else { i as f32 / steps as f32 };
            let d = self.paint_dab(from[0] + dx * t, from[1] + dy * t, radius, label);
            dirty = merge_dirty(dirty, d);
        }
        dirty
    }

    /// Balde de tinta: troca a cor da região contínua (mesma cor, vizinhos
    /// em cima/baixo/esquerda/direita) que contém (x, y).
    pub fn flood_fill(&mut self, x: u32, y: u32, label: u8) -> Option<Dirty> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let start = self.index(x, y);
        let target = self.labels[start];
        if target == label {
            return None;
        }
        let (w, h) = (self.width as usize, self.height as usize);
        let mut dirty = Dirty { x0: x, y0: y, x1: x + 1, y1: y + 1 };
        let mut stack = vec![start];
        self.labels[start] = label;
        while let Some(i) = stack.pop() {
            let (px, py) = (i % w, i / w);
            dirty = dirty.union(Dirty {
                x0: px as u32,
                y0: py as u32,
                x1: px as u32 + 1,
                y1: py as u32 + 1,
            });
            let neighbours = [
                (px > 0).then(|| i - 1),
                (px + 1 < w).then(|| i + 1),
                (py > 0).then(|| i - w),
                (py + 1 < h).then(|| i + w),
            ];
            for n in neighbours.into_iter().flatten() {
                if self.labels[n] == target {
                    self.labels[n] = label;
                    stack.push(n);
                }
            }
        }
        Some(dirty)
    }

    /// Junta a cor `from` dentro da cor `into`: os pixels passam para `into`
    /// e `from` sai da paleta.
    pub fn merge(&mut self, from: usize, into: usize) {
        let n = self.entries.len();
        if from == into || from >= n || into >= n {
            return;
        }
        let (from8, into8) = (from as u8, into as u8);
        for l in self.labels.iter_mut() {
            let mut v = if *l == from8 { into8 } else { *l };
            // Índices depois de `from` andam uma posição para trás.
            if v > from8 {
                v -= 1;
            }
            *l = v;
        }
        self.entries.remove(from);
        self.update_coverage();
    }

    /// Apaga uma cor: os pixels dela viram fundo.
    pub fn remove_color(&mut self, index: usize) {
        if index >= self.entries.len() || self.entries[index].background {
            return;
        }
        if let Some(bg) = self.ensure_background() {
            self.merge(index, bg);
        }
    }

    /// Pixels RGBA de um retângulo, para desenhar na tela. Cores de fundo
    /// ficam transparentes se `transparent_background`.
    pub fn region_rgba(&self, d: Dirty, transparent_background: bool) -> Vec<u8> {
        let colors: Vec<[u8; 4]> = self
            .entries
            .iter()
            .map(|e| {
                let a = if transparent_background && e.background { 0 } else { 255 };
                [e.color[0], e.color[1], e.color[2], a]
            })
            .collect();
        let mut out = Vec::with_capacity((d.width() * d.height() * 4) as usize);
        for y in d.y0..d.y1 {
            let row = self.index(d.x0, y);
            for &l in &self.labels[row..row + d.width() as usize] {
                out.extend_from_slice(colors.get(l as usize).unwrap_or(&[255, 0, 255, 255]));
            }
        }
        out
    }

    /// Retângulo do documento inteiro.
    pub fn full(&self) -> Dirty {
        Dirty { x0: 0, y0: 0, x1: self.width, y1: self.height }
    }

    /// Imagem RGB com as cores atuais (para exportar PNG).
    pub fn to_rgb(&self) -> RgbImage {
        let mut img = RgbImage::new(self.width, self.height);
        for (p, &l) in img.pixels_mut().zip(&self.labels) {
            p.0 = self.entries.get(l as usize).map_or([255, 0, 255], |e| e.color);
        }
        img
    }
}

// =====================================================================
// Camadas
// =====================================================================

/// Camada: um grupo de cores iguais.
pub struct LayerGroup {
    pub color: Rgb8,
    pub coverage: f32,
    /// Índices das cores da paleta que formam esta camada.
    pub members: Vec<usize>,
}

/// Camada pronta para vetorizar.
pub struct Layer {
    pub color: Rgb8,
    /// 0 (preto) = forma, 255 = vazio.
    pub mask: GrayImage,
}

/// Agrupa cores iguais (ignorando fundo e cores sem pixels) em camadas,
/// da maior (base, embaixo) para a menor (topo).
pub fn layer_groups(entries: &[ColorEntry]) -> Vec<LayerGroup> {
    let mut groups: Vec<LayerGroup> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if e.background || e.coverage <= 0.0 {
            continue;
        }
        match groups.iter_mut().find(|g| g.color == e.color) {
            Some(g) => {
                g.coverage += e.coverage;
                g.members.push(i);
            }
            None => groups.push(LayerGroup { color: e.color, coverage: e.coverage, members: vec![i] }),
        }
    }
    groups.sort_by(|a, b| b.coverage.total_cmp(&a.coverage));
    groups
}

/// Gera a máscara de cada camada.
///
/// `stacked` (empilhado): cada camada também cobre a área das camadas acima
/// dela. Assim a base do chaveiro sai inteira, sem buracos onde fica o texto.
pub fn build_layers(doc: &Document, stacked: bool, smooth_sigma: f32) -> Vec<Layer> {
    let groups = layer_groups(&doc.entries);

    // Para cada cor da paleta: em qual camada ela está (None = fundo).
    let mut layer_of: Vec<Option<usize>> = vec![None; doc.entries.len()];
    for (gi, g) in groups.iter().enumerate() {
        for &m in &g.members {
            layer_of[m] = Some(gi);
        }
    }

    groups
        .iter()
        .enumerate()
        .map(|(gi, g)| {
            let mut mask = GrayImage::new(doc.width, doc.height);
            for (p, &l) in mask.pixels_mut().zip(&doc.labels) {
                let ink = match layer_of.get(l as usize).copied().flatten() {
                    Some(j) => j == gi || (stacked && j > gi),
                    None => false,
                };
                p.0[0] = if ink { 0 } else { 255 };
            }
            Layer { color: g.color, mask: processing::smooth_mask(&mask, smooth_sigma) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// Fundo branco, quadrado rosa no meio, faixa preta dentro do rosa.
    fn sample() -> RgbImage {
        RgbImage::from_fn(60, 60, |x, y| {
            let in_pink = (10..50).contains(&x) && (10..50).contains(&y);
            let in_black = in_pink && (25..35).contains(&y);
            if in_black {
                Rgb([0, 0, 0])
            } else if in_pink {
                Rgb([240, 130, 210])
            } else {
                Rgb([255, 255, 255])
            }
        })
    }

    fn sample_doc() -> Document {
        Document::from_quantized(quantize(&sample(), 3))
    }

    fn index_of(doc: &Document, color: Rgb8) -> usize {
        doc.entries.iter().position(|e| e.original == color).unwrap()
    }

    #[test]
    fn detecta_tres_cores_e_fundo() {
        let doc = sample_doc();
        assert_eq!(doc.entries.len(), 3);
        let bg = doc.background_index().unwrap();
        assert_eq!(doc.entries[bg].color, [255, 255, 255]);
        assert!(doc.entries.iter().any(|e| e.color == [0, 0, 0]));
    }

    #[test]
    fn camadas_empilhadas_tem_base_solida() {
        let doc = sample_doc();
        let layers = build_layers(&doc, true, 0.0);
        assert_eq!(layers.len(), 2);
        // Base (rosa) cobre também a faixa preta.
        assert_eq!(layers[0].mask.get_pixel(30, 30).0[0], 0);
        // Sem empilhar, a faixa preta é buraco na base.
        let flat = build_layers(&doc, false, 0.0);
        assert_eq!(flat[0].mask.get_pixel(30, 30).0[0], 255);
    }

    #[test]
    fn juntar_cores_vira_uma_camada() {
        let mut doc = sample_doc();
        let black = index_of(&doc, [0, 0, 0]);
        let bg = doc.background_index().unwrap();
        let pink = (0..3).find(|&i| i != black && i != bg).unwrap();
        doc.merge(black, pink);
        assert_eq!(doc.entries.len(), 2);
        assert_eq!(layer_groups(&doc.entries).len(), 1);
        // Pixel que era preto agora é rosa.
        let l = doc.label_at(30, 30).unwrap();
        assert_eq!(doc.entries[l].original, [240, 130, 210]);
    }

    #[test]
    fn pincel_pinta_e_marca_area_alterada() {
        let mut doc = sample_doc();
        let red = doc.add_color([255, 0, 0], false).unwrap() as u8;
        let dirty = doc.paint_line([2.0, 2.0], [8.0, 2.0], 1.5, red).unwrap();
        assert_eq!(doc.label_at(5, 2), Some(red as usize));
        assert!(dirty.x0 <= 2 && dirty.x1 >= 8);
        doc.update_coverage();
        assert!(doc.entries[red as usize].coverage > 0.0);
        // Nova cor com pixels vira camada.
        assert_eq!(layer_groups(&doc.entries).len(), 3);
    }

    #[test]
    fn balde_pinta_so_a_regiao_continua() {
        let mut doc = sample_doc();
        let red = doc.add_color([255, 0, 0], false).unwrap() as u8;
        let black = index_of(&doc, [0, 0, 0]);
        doc.flood_fill(30, 30, red);
        // Faixa preta virou vermelha; o rosa em volta não.
        assert_eq!(doc.label_at(30, 30), Some(red as usize));
        assert_ne!(doc.label_at(30, 15), Some(red as usize));
        assert!(!doc.labels.contains(&(black as u8)));
    }

    #[test]
    fn apagar_cor_vira_fundo() {
        let mut doc = sample_doc();
        let black = index_of(&doc, [0, 0, 0]);
        doc.remove_color(black);
        let l = doc.label_at(30, 30).unwrap();
        assert!(doc.entries[l].background);
    }

    #[test]
    fn mascara_pb_vira_documento() {
        let mask = GrayImage::from_fn(4, 1, |x, _| image::Luma([if x < 2 { 0 } else { 255 }]));
        let doc = Document::from_mask(&mask);
        assert_eq!(doc.label_at(0, 0), Some(1));
        assert!(doc.entries[0].background);
        assert!((doc.entries[1].coverage - 0.5).abs() < 1e-6);
    }
}
