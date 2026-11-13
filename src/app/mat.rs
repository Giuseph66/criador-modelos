//! Base de corte: o fundo verde da área de trabalho, com grade em
//! milímetros reais, réguas e cotas de desenho técnico.
//!
//! A grade não é decoração: quando o SVG já foi gerado, 1 quadradinho da
//! base vale 1 mm (ou 2, 5, 10... conforme o zoom) da peça exportada.

use eframe::egui::{self, Align2, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2, vec2};

use super::theme::{CUTTER, INK, MAT, MAT_DEEP, MAT_INK, MAT_LINE, MAT_LINE_MAJOR, display_family, mm};

/// Largura das réguas, em pixels de tela.
pub const RULER: f32 = 22.0;
const RADIUS: u8 = 6;

/// Onde fica o zero da grade e quantos pixels de tela valem 1 mm.
#[derive(Clone, Copy, Debug)]
pub struct Scale {
    pub origin: Pos2,
    pub px_per_mm: f32,
}

/// Espaçamento (linha fina, linha forte) em mm, escolhido para as linhas
/// finas ficarem a pelo menos 9 px uma da outra.
fn steps(px_per_mm: f32) -> (f32, f32) {
    const STEPS: [(f32, f32); 8] = [
        (1.0, 10.0),
        (2.0, 10.0),
        (5.0, 50.0),
        (10.0, 50.0),
        (20.0, 100.0),
        (50.0, 500.0),
        (100.0, 500.0),
        (500.0, 5000.0),
    ];
    STEPS.into_iter().find(|(minor, _)| minor * px_per_mm >= 9.0).unwrap_or(STEPS[STEPS.len() - 1])
}

/// Índices das linhas da grade (k * passo a partir da origem) dentro de [min, max].
fn line_range(origin: f32, step: f32, min: f32, max: f32) -> std::ops::RangeInclusive<i64> {
    let first = ((min - origin) / step).ceil() as i64;
    let last = ((max - origin) / step).floor() as i64;
    first..=last
}

/// Fundo verde com grade. Sem escala conhecida, usa uma grade fixa de tela.
pub fn paint_mat(painter: &egui::Painter, area: Rect, scale: Option<Scale>) {
    painter.rect_filled(area, CornerRadius::same(RADIUS), MAT);

    let (origin, minor_px, every) = match scale {
        Some(s) => {
            let (minor, major) = steps(s.px_per_mm);
            (s.origin, minor * s.px_per_mm, (major / minor).round() as i64)
        }
        None => (area.min, 18.0, 5),
    };
    if minor_px < 2.0 {
        return;
    }
    let line = |k: i64| Stroke::new(1.0, if k.rem_euclid(every) == 0 { MAT_LINE_MAJOR } else { MAT_LINE });

    let xs = line_range(origin.x, minor_px, area.min.x, area.max.x);
    let ys = line_range(origin.y, minor_px, area.min.y, area.max.y);
    if xs.clone().count() > 2000 || ys.clone().count() > 2000 {
        return;
    }
    for k in xs {
        let x = origin.x + k as f32 * minor_px;
        painter.line_segment([Pos2::new(x, area.min.y), Pos2::new(x, area.max.y)], line(k));
    }
    for k in ys {
        let y = origin.y + k as f32 * minor_px;
        painter.line_segment([Pos2::new(area.min.x, y), Pos2::new(area.max.x, y)], line(k));
    }
}

/// Réguas em cima e à esquerda, numeradas em mm a partir do canto da peça.
pub fn paint_rulers(painter: &egui::Painter, area: Rect, scale: Scale) {
    let top = Rect::from_min_max(area.min, Pos2::new(area.max.x, area.min.y + RULER));
    let left = Rect::from_min_max(area.min, Pos2::new(area.min.x + RULER, area.max.y));
    painter.rect_filled(top, CornerRadius { nw: RADIUS, ne: RADIUS, sw: 0, se: 0 }, MAT_DEEP);
    painter.rect_filled(left, CornerRadius { nw: RADIUS, ne: 0, sw: RADIUS, se: 0 }, MAT_DEEP);

    let (minor, major) = steps(scale.px_per_mm);
    let minor_px = minor * scale.px_per_mm;
    let every = (major / minor).round() as i64;
    let half = every / 2;
    let tick = Stroke::new(1.0, MAT_INK);
    let font = FontId::proportional(10.5);
    let tick_len = |k: i64| {
        if k.rem_euclid(every) == 0 {
            9.0
        } else if half > 0 && k.rem_euclid(half) == 0 {
            6.0
        } else {
            3.5
        }
    };
    let label = |k: i64| format!("{}", ((k as f32 * minor).abs()).round() as i64);

    for k in line_range(scale.origin.x, minor_px, top.min.x + RULER, top.max.x) {
        let x = scale.origin.x + k as f32 * minor_px;
        painter.line_segment([Pos2::new(x, top.max.y - tick_len(k)), Pos2::new(x, top.max.y)], tick);
        if k.rem_euclid(every) == 0 {
            painter.text(Pos2::new(x + 3.0, top.min.y + 3.0), Align2::LEFT_TOP, label(k), font.clone(), MAT_INK);
        }
    }
    for k in line_range(scale.origin.y, minor_px, left.min.y + RULER, left.max.y) {
        let y = scale.origin.y + k as f32 * minor_px;
        painter.line_segment([Pos2::new(left.max.x - tick_len(k), y), Pos2::new(left.max.x, y)], tick);
        if k.rem_euclid(every) == 0 {
            painter.text(Pos2::new(left.min.x + 3.0, y + 2.0), Align2::LEFT_TOP, label(k), font.clone(), MAT_INK);
        }
    }

    // Canto onde as réguas se encontram.
    let corner = Rect::from_min_size(area.min, Vec2::splat(RULER));
    painter.rect_filled(corner, CornerRadius { nw: RADIUS, ne: 0, sw: 0, se: 0 }, MAT_DEEP);
    painter.text(corner.center(), Align2::CENTER_CENTER, "mm", FontId::proportional(9.5), MAT_INK);
    let edge = Stroke::new(1.0, MAT_LINE_MAJOR);
    painter.line_segment([Pos2::new(left.max.x, top.max.y), Pos2::new(top.max.x, top.max.y)], edge);
    painter.line_segment([Pos2::new(left.max.x, top.max.y), Pos2::new(left.max.x, left.max.y)], edge);
}

/// Cotas de desenho técnico: largura embaixo da peça, altura à direita.
pub fn paint_dimensions(painter: &egui::Painter, piece: Rect, width_mm: f64, height_mm: f64) {
    if piece.width() < 30.0 || piece.height() < 30.0 {
        return;
    }
    let line = Stroke::new(1.2, CUTTER);
    let guide = Stroke::new(1.0, CUTTER.gamma_multiply(0.45));
    const GAP: f32 = 16.0;

    // Largura.
    let y = piece.max.y + GAP;
    painter.line_segment([Pos2::new(piece.min.x, y), Pos2::new(piece.max.x, y)], line);
    for x in [piece.min.x, piece.max.x] {
        painter.line_segment([Pos2::new(x, piece.max.y + 3.0), Pos2::new(x, y + 5.0)], guide);
        painter.line_segment([Pos2::new(x, y - 5.0), Pos2::new(x, y + 5.0)], line);
    }
    tag(painter, Pos2::new(piece.center().x, y), &mm(width_mm));

    // Altura.
    let x = piece.max.x + GAP;
    painter.line_segment([Pos2::new(x, piece.min.y), Pos2::new(x, piece.max.y)], line);
    for y in [piece.min.y, piece.max.y] {
        painter.line_segment([Pos2::new(piece.max.x + 3.0, y), Pos2::new(x + 5.0, y)], guide);
        painter.line_segment([Pos2::new(x - 5.0, y), Pos2::new(x + 5.0, y)], line);
    }
    tag(painter, Pos2::new(x, piece.center().y), &mm(height_mm));
}

/// Etiqueta amarela com a medida, centrada em `center`.
fn tag(painter: &egui::Painter, center: Pos2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::new(12.5, display_family()), INK);
    let rect = Rect::from_center_size(center, galley.size() + vec2(12.0, 4.0));
    painter.rect_filled(rect, CornerRadius::same(4), CUTTER);
    painter.galley(rect.min + vec2(6.0, 2.0), galley, INK);
}

/// Mostra uma imagem pronta (original ou vetor) sobre a base de corte.
pub fn board(ui: &mut egui::Ui, texture: Option<&egui::TextureHandle>) {
    let area = ui.available_rect_before_wrap();
    ui.allocate_rect(area, Sense::hover());
    paint_mat(ui.painter(), area, None);
    let Some(tex) = texture else {
        ui.put(Rect::from_center_size(area.center(), Vec2::splat(32.0)), egui::Spinner::new().size(28.0));
        return;
    };
    let inner = area.shrink(28.0);
    let size = tex.size_vec2();
    let scale = (inner.width() / size.x).min(inner.height() / size.y).max(0.01);
    let rect = Rect::from_center_size(inner.center(), size * scale);
    // Sombra curta, como uma folha apoiada na base.
    ui.painter().rect_filled(rect.translate(vec2(0.0, 3.0)), CornerRadius::same(3), MAT_DEEP);
    egui::Image::new((tex.id(), size * scale)).corner_radius(CornerRadius::same(3)).paint_at(ui, rect);
}

/// Comparação: as imagens lado a lado (ou empilhadas) numa base só,
/// no arranjo que deixa cada uma maior.
pub fn compare(ui: &mut egui::Ui, items: &[(&str, Option<&egui::TextureHandle>)]) {
    let area = ui.available_rect_before_wrap();
    ui.allocate_rect(area, Sense::hover());
    paint_mat(ui.painter(), area, None);

    let n = items.len().max(1) as f32;
    // Proporção de referência: a primeira imagem disponível.
    let aspect = items.iter().find_map(|(_, t)| t.map(|t| t.size_vec2())).unwrap_or(vec2(4.0, 3.0));
    const PAD: f32 = 24.0;
    const TITLE: f32 = 26.0;
    let inner = area.shrink(PAD);
    let row_cell = vec2((inner.width() - PAD * (n - 1.0)) / n, inner.height());
    let col_cell = vec2(inner.width(), (inner.height() - PAD * (n - 1.0)) / n);
    let fit = |cell: Vec2| ((cell.x / aspect.x).min((cell.y - TITLE) / aspect.y)).max(0.0);
    let in_row = fit(row_cell) >= fit(col_cell);
    let cell = if in_row { row_cell } else { col_cell };

    for (i, (title, tex)) in items.iter().enumerate() {
        let offset = if in_row { vec2(i as f32 * (cell.x + PAD), 0.0) } else { vec2(0.0, i as f32 * (cell.y + PAD)) };
        let rect = Rect::from_min_size(inner.min + offset, cell);
        let image_area = Rect::from_min_max(Pos2::new(rect.min.x, rect.min.y + TITLE), rect.max);
        let Some(tex) = tex else {
            ui.put(Rect::from_center_size(image_area.center(), Vec2::splat(28.0)), egui::Spinner::new().size(24.0));
            continue;
        };
        let size = tex.size_vec2();
        let scale = (image_area.width() / size.x).min(image_area.height() / size.y).max(0.01);
        let img = Rect::from_center_size(image_area.center(), size * scale);
        ui.painter().text(
            Pos2::new(img.min.x, img.min.y - 8.0),
            Align2::LEFT_BOTTOM,
            *title,
            FontId::new(14.0, display_family()),
            MAT_INK,
        );
        ui.painter().rect_filled(img.translate(vec2(0.0, 3.0)), CornerRadius::same(3), MAT_DEEP);
        egui::Image::new((tex.id(), size * scale)).corner_radius(CornerRadius::same(3)).paint_at(ui, img);
    }
}
