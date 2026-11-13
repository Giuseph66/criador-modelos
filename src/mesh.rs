//! Exportação 3D: transforma as camadas vetorizadas em sólidos (extrusão)
//! e grava em STL binário.
//!
//! Cada forma vira um "biscoito": tampa em cima, fundo embaixo e paredes
//! laterais ligando os dois. A tampa e o fundo são triangulados com
//! `earcut` (que entende furos).

use std::io::{self, Write};

use earcut::Earcut;

use crate::colors::Rgb8;
use crate::vectorize::{Point, Shape, VectorResult};

/// Triângulo 3D (três vértices x, y, z em mm).
pub type Triangle = [[f32; 3]; 3];

/// Alturas da peça.
#[derive(Clone, PartialEq, Debug)]
pub struct MeshParams {
    /// Espessura da camada de baixo (base), em mm.
    pub base_mm: f64,
    /// Quanto cada camada de cima sobe em relação à anterior, em mm.
    pub relief_mm: f64,
    /// Mesmo significado do "empilhar camadas" do SVG.
    pub stacked: bool,
}

impl Default for MeshParams {
    fn default() -> Self {
        Self { base_mm: 3.0, relief_mm: 1.0, stacked: true }
    }
}

impl MeshParams {
    /// Faixa de altura (z inicial, z final) da camada `index`.
    ///
    /// Empilhado: base de 0 a `base`; cada camada seguinte fica em cima da
    /// anterior. Não empilhado: todas começam no zero, cada uma mais alta.
    pub fn z_range(&self, index: usize) -> (f64, f64) {
        let base = self.base_mm.max(0.1);
        let relief = self.relief_mm.max(0.1);
        let top = base + relief * index as f64;
        if index == 0 {
            (0.0, base)
        } else if self.stacked {
            (top - relief, top)
        } else {
            (0.0, top)
        }
    }

    /// Altura total da peça com `layers` camadas.
    pub fn total_height(&self, layers: usize) -> f64 {
        if layers == 0 { 0.0 } else { self.z_range(layers - 1).1 }
    }
}

/// Malha 3D de uma camada.
pub struct LayerMesh {
    pub color: Rgb8,
    pub triangles: Vec<Triangle>,
}

/// Gera a malha de todas as camadas. Coordenadas em mm, com a imagem vista
/// de cima (o eixo Y é invertido para o desenho não sair espelhado).
pub fn build_meshes(v: &VectorResult, p: &MeshParams) -> Vec<LayerMesh> {
    let mm_per_px = v.width_mm / v.px_width.max(1) as f64;
    let to_mm = |pt: &Point| [pt[0] * mm_per_px, (v.px_height as f64 - pt[1]) * mm_per_px];

    v.geometry
        .iter()
        .enumerate()
        .map(|(i, layer)| {
            let (z0, z1) = p.z_range(i);
            let mut triangles = Vec::new();
            for shape in &layer.shapes {
                let mm = Shape {
                    outer: shape.outer.iter().map(to_mm).collect(),
                    holes: shape.holes.iter().map(|h| h.iter().map(to_mm).collect()).collect(),
                };
                extrude(&mm, z0, z1, &mut triangles);
            }
            LayerMesh { color: layer.color, triangles }
        })
        .collect()
}

/// Área com sinal (fórmula do laço): positiva = sentido anti-horário.
fn signed_area(ring: &[Point]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

/// Garante o sentido do anel: contorno anti-horário, furo horário.
/// Isso faz as paredes laterais apontarem sempre para fora do sólido.
fn oriented(ring: &[Point], counter_clockwise: bool) -> Vec<Point> {
    let mut r = ring.to_vec();
    if (signed_area(&r) > 0.0) != counter_clockwise {
        r.reverse();
    }
    r
}

/// Remove pontos repetidos e pontos no meio de uma reta (alinhados com os
/// vizinhos). O earcut se confunde com eles e pode deixar falhas na tampa.
fn clean_ring(ring: &[Point]) -> Vec<Point> {
    const MIN_DIST2: f64 = 1e-12;
    const MIN_CROSS: f64 = 1e-10;
    let close = |a: &Point, b: &Point| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) < MIN_DIST2;

    let mut pts: Vec<Point> = Vec::with_capacity(ring.len());
    for p in ring {
        if pts.last().is_none_or(|q| !close(q, p)) {
            pts.push(*p);
        }
    }
    while pts.len() > 1 && close(&pts[0], &pts[pts.len() - 1]) {
        pts.pop();
    }

    // Repete até não sobrar ponto alinhado.
    loop {
        let n = pts.len();
        if n < 3 {
            return pts;
        }
        let keep: Vec<Point> = (0..n)
            .filter(|&i| {
                let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
                let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
                cross.abs() > MIN_CROSS
            })
            .map(|i| pts[i])
            .collect();
        if keep.len() == n {
            return pts;
        }
        pts = keep;
    }
}

/// Extruda uma forma entre as alturas `z0` e `z1`.
fn extrude(shape: &Shape, z0: f64, z1: f64, out: &mut Vec<Triangle>) {
    let outer = clean_ring(&shape.outer);
    if outer.len() < 3 || signed_area(&outer).abs() < 1e-9 {
        return;
    }
    let outer = oriented(&outer, true);
    let holes: Vec<Vec<Point>> = shape
        .holes
        .iter()
        .map(|h| clean_ring(h))
        .filter(|h| h.len() >= 3)
        .map(|h| oriented(&h, false))
        .collect();

    // Todos os vértices numa lista só; `hole_starts` marca onde cada furo começa.
    let mut vertices: Vec<Point> = outer.clone();
    let mut hole_starts = Vec::new();
    for h in &holes {
        hole_starts.push(vertices.len());
        vertices.extend_from_slice(h);
    }

    let mut indices: Vec<usize> = Vec::new();
    Earcut::new().earcut(vertices.iter().copied(), hole_starts.as_slice(), &mut indices);

    let v3 = |p: Point, z: f64| [p[0] as f32, p[1] as f32, z as f32];
    for t in indices.chunks_exact(3) {
        let (mut a, b, mut c) = (vertices[t[0]], vertices[t[1]], vertices[t[2]]);
        // Tampa precisa estar no sentido anti-horário (normal para cima).
        if signed_area(&[a, b, c]) < 0.0 {
            std::mem::swap(&mut a, &mut c);
        }
        out.push([v3(a, z1), v3(b, z1), v3(c, z1)]); // tampa
        out.push([v3(a, z0), v3(c, z0), v3(b, z0)]); // fundo (sentido inverso)
    }

    // Paredes: um retângulo (dois triângulos) por aresta de cada anel.
    for ring in std::iter::once(&outer).chain(&holes) {
        let n = ring.len();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            out.push([v3(a, z0), v3(b, z0), v3(b, z1)]);
            out.push([v3(a, z0), v3(b, z1), v3(a, z1)]);
        }
    }
}

/// Normal (direção para fora) de um triângulo.
fn normal(t: &Triangle) -> [f32; 3] {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0 { [n[0] / len, n[1] / len, n[2] / len] } else { [0.0, 0.0, 0.0] }
}

/// Grava triângulos em STL binário.
///
/// Formato: 80 bytes de cabeçalho, nº de triângulos (u32) e, para cada
/// triângulo, normal + 3 vértices (12 floats) + 2 bytes zerados.
pub fn write_stl(triangles: &[Triangle], mut w: impl Write) -> io::Result<()> {
    let mut header = [0u8; 80];
    let title = b"Criador de Modelos";
    header[..title.len()].copy_from_slice(title);
    w.write_all(&header)?;
    w.write_all(&(triangles.len() as u32).to_le_bytes())?;
    for t in triangles {
        for value in normal(t).iter().chain(t.iter().flatten()) {
            w.write_all(&value.to_le_bytes())?;
        }
        w.write_all(&[0, 0])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(size: f64) -> Shape {
        Shape { outer: vec![[0.0, 0.0], [size, 0.0], [size, size], [0.0, size]], holes: vec![] }
    }

    /// Volume pelo teorema da divergência: só bate se a malha for fechada
    /// e com as normais para fora.
    fn volume(tris: &[Triangle]) -> f64 {
        tris.iter()
            .map(|t| {
                let [a, b, c] = t.map(|v| v.map(f64::from));
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.0
            })
            .sum()
    }

    #[test]
    fn cubo_tem_volume_certo() {
        let mut tris = Vec::new();
        extrude(&square(10.0), 0.0, 10.0, &mut tris);
        assert_eq!(tris.len(), 2 * 2 + 4 * 2); // tampa + fundo + 4 paredes
        assert!((volume(&tris) - 1000.0).abs() < 1e-3);
    }

    #[test]
    fn sentido_do_contorno_nao_importa() {
        let mut shape = square(10.0);
        shape.outer.reverse();
        let mut tris = Vec::new();
        extrude(&shape, 0.0, 2.0, &mut tris);
        assert!((volume(&tris) - 200.0).abs() < 1e-3);
    }

    #[test]
    fn furo_desconta_volume() {
        let mut shape = square(10.0);
        shape.holes.push(vec![[4.0, 4.0], [6.0, 4.0], [6.0, 6.0], [4.0, 6.0]]);
        let mut tris = Vec::new();
        extrude(&shape, 0.0, 1.0, &mut tris);
        assert!((volume(&tris) - (100.0 - 4.0)).abs() < 1e-3);
    }

    #[test]
    fn pontos_alinhados_e_repetidos_sao_removidos() {
        let ring = vec![[0.0, 0.0], [5.0, 0.0], [5.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]];
        assert_eq!(clean_ring(&ring), vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
    }

    #[test]
    fn alturas_das_camadas() {
        let p = MeshParams { base_mm: 3.0, relief_mm: 1.0, stacked: true };
        assert_eq!(p.z_range(0), (0.0, 3.0));
        assert_eq!(p.z_range(1), (3.0, 4.0));
        assert_eq!(p.z_range(2), (4.0, 5.0));
        let flat = MeshParams { stacked: false, ..p };
        assert_eq!(flat.z_range(2), (0.0, 5.0));
        assert_eq!(flat.total_height(3), 5.0);
    }

    #[test]
    fn stl_tem_tamanho_certo() {
        let mut tris = Vec::new();
        extrude(&square(1.0), 0.0, 1.0, &mut tris);
        let mut bytes = Vec::new();
        write_stl(&tris, &mut bytes).unwrap();
        assert_eq!(bytes.len(), 84 + 50 * tris.len());
    }
}
