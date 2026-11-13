//! Etapa 2 do pipeline: máscara preto e branco -> SVG.
//!
//! Substitui o "site que converte imagem em vetor". Usa a crate
//! `visioncortex` (o motor do VTracer): agrupa pixels pretos vizinhos em
//! "clusters", traça o contorno de cada um e simplifica em curvas.
//!
//! O SVG sai com tamanho físico em milímetros, então o slicer já importa
//! na escala certa.

use std::fmt::Write as _;

use image::{GrayImage, imageops};
use visioncortex::{BinaryImage, CompoundPath, CompoundPathElement, PathSimplifyMode, PointF64};

use crate::colors::{Rgb8, hex};

/// Tipo de contorno gerado.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CurveMode {
    /// Curvas suaves (Bézier). Melhor para logos e desenhos.
    Spline,
    /// Só retas. Bom para formas geométricas.
    Polygon,
    /// Segue os pixels exatamente (pixel art).
    Pixel,
}

impl CurveMode {
    fn to_visioncortex(self) -> PathSimplifyMode {
        match self {
            CurveMode::Spline => PathSimplifyMode::Spline,
            CurveMode::Polygon => PathSimplifyMode::Polygon,
            CurveMode::Pixel => PathSimplifyMode::None,
        }
    }
}

/// Parâmetros da vetorização. Valores padrão iguais ao preset "bw" do VTracer.
#[derive(Clone, PartialEq, Debug)]
pub struct VectorParams {
    pub mode: CurveMode,
    /// Descarta manchas menores que `filter_speckle x filter_speckle` pixels.
    pub filter_speckle: usize,
    /// Ângulo (graus) acima do qual um ponto vira "quina" em vez de curva.
    pub corner_threshold_deg: f64,
    /// Comprimento mínimo de segmento (px). Maior = menos nós.
    pub length_threshold: f64,
    /// Ângulo (graus) para emendar segmentos em uma curva só.
    pub splice_threshold_deg: f64,
    pub max_iterations: usize,
    /// Largura final do desenho em milímetros (altura segue a proporção).
    pub width_mm: f64,
}

impl Default for VectorParams {
    fn default() -> Self {
        Self {
            mode: CurveMode::Spline,
            filter_speckle: 4,
            corner_threshold_deg: 60.0,
            length_threshold: 4.0,
            splice_threshold_deg: 45.0,
            max_iterations: 10,
            width_mm: 50.0,
        }
    }
}

/// SVG de uma camada só (para exportar cada cor separada).
pub struct LayerSvg {
    pub color: Rgb8,
    pub svg: String,
    pub path_count: usize,
}

/// Ponto 2D em pixels (já recortado).
pub type Point = [f64; 2];

/// Uma forma fechada: contorno externo + furos. Curvas já convertidas em
/// segmentos de reta (usado para gerar o STL).
#[derive(Clone, Debug)]
pub struct Shape {
    pub outer: Vec<Point>,
    pub holes: Vec<Vec<Point>>,
}

/// Formas de uma camada.
pub struct LayerGeometry {
    pub color: Rgb8,
    pub shapes: Vec<Shape>,
}

/// SVG gerado, geometria e algumas estatísticas.
pub struct VectorResult {
    /// Todas as camadas juntas, de baixo para cima.
    pub svg: String,
    pub path_count: usize,
    pub width_mm: f64,
    pub height_mm: f64,
    pub layers: Vec<LayerSvg>,
    /// Mesmas camadas como polígonos (para o STL).
    pub geometry: Vec<LayerGeometry>,
    /// Tamanho em pixels da área recortada (coordenadas da geometria).
    pub px_width: u32,
    pub px_height: u32,
}

/// Converte uma máscara (0 = preto/forma) em SVG preto.
pub fn vectorize(mask: &GrayImage, p: &VectorParams) -> VectorResult {
    vectorize_layers(mask.width(), mask.height(), &[([0, 0, 0], mask)], p)
}

/// Converte várias máscaras (uma por cor) em um SVG com uma camada `<g>`
/// por cor. Todas as máscaras devem ter `width` x `height` pixels.
pub fn vectorize_layers(
    width: u32,
    height: u32,
    layers: &[(Rgb8, &GrayImage)],
    p: &VectorParams,
) -> VectorResult {
    // Recorta tudo na área do desenho (sem as margens vazias): assim a
    // largura em mm é a largura real da peça.
    let (x0, y0, width, height) = content_bounds(width, height, layers).unwrap_or((0, 0, width, height));
    let cropped: Vec<(Rgb8, GrayImage)> = layers
        .iter()
        .map(|&(color, mask)| (color, imageops::crop_imm(mask, x0, y0, width, height).to_image()))
        .collect();

    // Vetoriza cada camada numa thread própria. `thread::scope` garante que
    // todas terminam antes de sair do bloco, então podem emprestar `layers`.
    let traced: Vec<(Rgb8, Traced)> = std::thread::scope(|scope| {
        let handles: Vec<_> = cropped
            .iter()
            .map(|(color, mask)| {
                let color = *color;
                scope.spawn(move || (color, trace(mask, p)))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("thread de vetorização falhou"))
            .collect()
    });

    let width_mm = p.width_mm.max(1.0);
    let height_mm = width_mm * height as f64 / width.max(1) as f64;
    let doc = |groups: &str| document(width, height, width_mm, height_mm, groups);

    let mut all_groups = String::new();
    let mut layer_svgs = Vec::new();
    let mut geometry = Vec::new();
    let mut path_count = 0;
    for (i, (color, t)) in traced.into_iter().enumerate() {
        let g = group(i + 1, color, &t.paths);
        all_groups.push_str(&g);
        path_count += t.count;
        layer_svgs.push(LayerSvg { color, svg: doc(&g), path_count: t.count });
        geometry.push(LayerGeometry { color, shapes: t.shapes });
    }

    VectorResult {
        svg: doc(&all_groups),
        path_count,
        width_mm,
        height_mm,
        layers: layer_svgs,
        geometry,
        px_width: width,
        px_height: height,
    }
}

/// Margem (px) deixada em volta do desenho ao recortar.
const CROP_PADDING: u32 = 2;

/// Retângulo (x, y, largura, altura) que contém todos os pixels pretos de
/// todas as camadas, com uma pequena margem. `None` se não houver nada.
fn content_bounds(width: u32, height: u32, layers: &[(Rgb8, &GrayImage)]) -> Option<(u32, u32, u32, u32)> {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (u32::MAX, u32::MAX, 0, 0);
    for (_, mask) in layers {
        for (x, y, px) in mask.enumerate_pixels() {
            if px.0[0] < 128 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x > max_x {
        return None;
    }
    let x0 = min_x.saturating_sub(CROP_PADDING);
    let y0 = min_y.saturating_sub(CROP_PADDING);
    let x1 = (max_x + CROP_PADDING).min(width - 1);
    let y1 = (max_y + CROP_PADDING).min(height - 1);
    Some((x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

/// Resultado do traçado de uma camada.
struct Traced {
    /// Elementos `<path>` do SVG.
    paths: String,
    count: usize,
    shapes: Vec<Shape>,
}

/// Traça os contornos das formas pretas da máscara.
fn trace(mask: &GrayImage, p: &VectorParams) -> Traced {
    let (w, h) = (mask.width() as usize, mask.height() as usize);

    let mut binary = BinaryImage::new_w_h(w, h);
    for (x, y, px) in mask.enumerate_pixels() {
        if px.0[0] < 128 {
            binary.set_pixel(x as usize, y as usize, true);
        }
    }

    let min_area = p.filter_speckle * p.filter_speckle;
    fill_small_holes(&mut binary, min_area);

    let clusters = binary.to_clusters(false);

    let mut paths = String::new();
    let mut count = 0;
    let mut shapes = Vec::new();
    for i in 0..clusters.len() {
        let cluster = clusters.get_cluster(i);
        if cluster.size() < min_area {
            continue;
        }
        let compound = cluster.to_compound_path(
            p.mode.to_visioncortex(),
            p.corner_threshold_deg.to_radians(),
            p.length_threshold,
            p.max_iterations,
            p.splice_threshold_deg.to_radians(),
        );
        let (d, offset) = compound.to_svg_string(true, PointF64::default(), Some(2));
        // `write!` em String nunca falha, por isso o `.ok()`.
        writeln!(paths, r#"    <path d="{d}" transform="translate({},{})"/>"#, offset.x, offset.y).ok();
        count += 1;
        if let Some(shape) = compound_to_shape(&compound) {
            shapes.push(shape);
        }
    }
    Traced { paths, count, shapes }
}

/// Converte o contorno traçado em polígonos. No visioncortex, o primeiro
/// caminho de cada forma é o contorno externo e os demais são os furos.
fn compound_to_shape(compound: &CompoundPath) -> Option<Shape> {
    let mut rings = compound.paths.iter().map(flatten).filter(|r| r.len() >= 3);
    let outer = rings.next()?;
    Some(Shape { outer, holes: rings.collect() })
}

/// Transforma um caminho (retas ou curvas Bézier) em lista de pontos.
fn flatten(element: &CompoundPathElement) -> Vec<Point> {
    let mut pts: Vec<Point> = match element {
        CompoundPathElement::PathI32(path) => path.path.iter().map(|p| [p.x as f64, p.y as f64]).collect(),
        CompoundPathElement::PathF64(path) => path.path.iter().map(|p| [p.x, p.y]).collect(),
        CompoundPathElement::Spline(spline) => flatten_spline(&spline.points),
    };
    // Remove pontos repetidos seguidos (inclusive o fechamento no fim).
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    if pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    pts
}

/// Curvas Bézier cúbicas encadeadas: `[p0, c1, c2, p1, c1, c2, p2, ...]`.
/// Cada curva vira alguns segmentos, mais segmentos quanto maior ela for.
fn flatten_spline(points: &[PointF64]) -> Vec<Point> {
    let mut out = Vec::new();
    let Some(first) = points.first() else { return out };
    out.push([first.x, first.y]);
    for c in points.windows(4).step_by(3) {
        let (p0, p1, p2, p3) = (c[0], c[1], c[2], c[3]);
        let length = p0.distance_to(p1) + p1.distance_to(p2) + p2.distance_to(p3);
        let steps = ((length / 2.0).ceil() as usize).clamp(2, 24);
        for s in 1..=steps {
            let t = s as f64 / steps as f64;
            let u = 1.0 - t;
            let (a, b, cc, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            out.push([
                a * p0.x + b * p1.x + cc * p2.x + d * p3.x,
                a * p0.y + b * p1.y + cc * p2.y + d * p3.y,
            ]);
        }
    }
    out
}

/// Preenche furos (áreas brancas cercadas de preto) menores que `min_area`
/// pixels. Complementa o "remover manchas": aquele tira pontinhos pretos
/// soltos, este fecha pontinhos brancos dentro das formas.
fn fill_small_holes(binary: &mut BinaryImage, min_area: usize) {
    if min_area == 0 {
        return;
    }
    let (w, h) = (binary.width, binary.height);
    let mut empty = BinaryImage::new_w_h(w, h);
    for y in 0..h {
        for x in 0..w {
            if !binary.get_pixel(x, y) {
                empty.set_pixel(x, y, true);
            }
        }
    }
    let regions = empty.to_clusters(false);
    for i in 0..regions.len() {
        let region = regions.get_cluster(i);
        // Região que encosta na borda é o lado de fora, não um furo.
        let r = &region.rect;
        let touches_border = r.left <= 0 || r.top <= 0 || r.right >= w as i32 || r.bottom >= h as i32;
        if region.size() < min_area && !touches_border {
            for p in &region.points {
                binary.set_pixel(p.x as usize, p.y as usize, true);
            }
        }
    }
}

/// Grupo `<g>` de uma camada, com a cor de preenchimento.
fn group(index: usize, color: Rgb8, paths: &str) -> String {
    format!(
        "  <g id=\"camada-{index}\" fill=\"{}\" fill-rule=\"evenodd\">\n{paths}  </g>\n",
        hex(color)
    )
}

/// Documento SVG com tamanho físico em mm.
fn document(w: u32, h: u32, width_mm: f64, height_mm: f64, groups: &str) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#, "\n",
            r#"<!-- Gerado por Criador de Modelos -->"#, "\n",
            r#"<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="{wmm:.2}mm" height="{hmm:.2}mm" viewBox="0 0 {w} {h}">"#, "\n",
            "{groups}",
            "</svg>\n",
        ),
        wmm = width_mm,
        hmm = height_mm,
        w = w,
        h = h,
        groups = groups,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    #[test]
    fn quadrado_vira_um_caminho() {
        // Quadrado preto 20x20 no meio de uma imagem branca 40x40.
        let mask = GrayImage::from_fn(40, 40, |x, y| {
            let inside = (10..30).contains(&x) && (10..30).contains(&y);
            Luma([if inside { 0 } else { 255 }])
        });
        let r = vectorize(&mask, &VectorParams::default());
        assert_eq!(r.path_count, 1);
        assert!(r.svg.contains(r#"width="50.00mm""#));
        // Recortado no quadrado (20 px) + 2 px de margem de cada lado.
        assert!(r.svg.contains(r#"viewBox="0 0 24 24""#));
    }

    #[test]
    fn manchas_pequenas_sao_removidas() {
        let mask = GrayImage::from_fn(40, 40, |x, y| Luma([if x == 5 && y == 5 { 0 } else { 255 }]));
        let r = vectorize(&mask, &VectorParams::default());
        assert_eq!(r.path_count, 0);
    }

    #[test]
    fn furos_pequenos_sao_fechados() {
        // Quadrado preto com um furo branco de 1 px no meio.
        let mask = GrayImage::from_fn(40, 40, |x, y| {
            let inside = (10..30).contains(&x) && (10..30).contains(&y);
            Luma([if inside && !(x == 20 && y == 20) { 0 } else { 255 }])
        });
        let r = vectorize(&mask, &VectorParams::default());
        // Sem furo, o contorno é um único subcaminho (um só "M" nos <path>).
        let subpaths: usize = r.svg.lines().filter(|l| l.contains("<path")).map(|l| l.matches('M').count()).sum();
        assert_eq!(subpaths, 1);
    }

    #[test]
    fn uma_camada_por_cor() {
        let a = GrayImage::from_fn(40, 40, |x, _| Luma([if x < 20 { 0 } else { 255 }]));
        let b = GrayImage::from_fn(40, 40, |x, _| Luma([if x >= 20 { 0 } else { 255 }]));
        let r = vectorize_layers(40, 40, &[([255, 0, 0], &a), ([0, 0, 255], &b)], &VectorParams::default());
        assert_eq!(r.layers.len(), 2);
        assert!(r.svg.contains(r##"id="camada-1" fill="#ff0000""##));
        assert!(r.svg.contains(r##"id="camada-2" fill="#0000ff""##));
        assert!(!r.layers[0].svg.contains("camada-2"));
        assert_eq!(r.geometry.len(), 2);
        assert_eq!(r.geometry[0].shapes.len(), 1);
    }

    #[test]
    fn geometria_tem_furos() {
        // Anel: quadrado 30x30 com furo 10x10 no meio.
        let mask = GrayImage::from_fn(50, 50, |x, y| {
            let outer = (10..40).contains(&x) && (10..40).contains(&y);
            let hole = (20..30).contains(&x) && (20..30).contains(&y);
            Luma([if outer && !hole { 0 } else { 255 }])
        });
        let r = vectorize(&mask, &VectorParams::default());
        let shape = &r.geometry[0].shapes[0];
        assert!(shape.outer.len() >= 4);
        assert_eq!(shape.holes.len(), 1);
    }
}
