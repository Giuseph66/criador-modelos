//! Componentes visuais reaproveitáveis: botões com ícone, controle
//! segmentado, cartão de seção, amostra de cor e quadriculado.
//!
//! Os botões são desenhados à mão (em vez de usar `egui::Button`) para
//! controlar exatamente cores, cantos e o alinhamento ícone + texto.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Margin, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, vec2};

use criador_modelos::colors::Rgb8;

use super::theme::{
    ACCENT, ACCENT_HOVER, ACCENT_PRESSED, BORDER, BORDER_STRONG, DANGER, INSET, SURFACE_2, SURFACE_3, TEXT, TEXT_DISABLED,
    TEXT_WEAK, accent_soft, bold, icons,
};

pub fn to_color32(c: Rgb8) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

// ---------------------------------------------------------------------
// Botões
// ---------------------------------------------------------------------

/// Aparência de um botão.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Ação principal: preenchido com a cor da marca.
    Primary,
    /// Ação comum: fundo suave com borda.
    Secondary,
    /// Sem fundo até passar o mouse.
    Ghost,
    /// Item de um controle segmentado.
    Segment { selected: bool },
    /// Botão de ferramenta (só ícone).
    Tool { selected: bool },
}

const BUTTON_HEIGHT: f32 = 32.0;
const BUTTON_RADIUS: u8 = 8;

/// Desenha e controla um botão com ícone e/ou texto.
///
/// `width`: largura fixa (senão se ajusta ao conteúdo).
fn draw_button(ui: &mut egui::Ui, icon: &str, label: &str, kind: Kind, width: Option<f32>) -> egui::Response {
    let enabled = ui.is_enabled();
    let icon_size = if matches!(kind, Kind::Tool { .. }) { 19.0 } else { 16.0 };

    // Com `Color32::PLACEHOLDER` a cor é escolhida só na hora de pintar
    // (depois de saber se o mouse está em cima).
    let painter = ui.painter().clone();
    let icon_galley = (!icon.is_empty())
        .then(|| painter.layout_no_wrap(icon.to_owned(), FontId::proportional(icon_size), Color32::PLACEHOLDER));
    let label_galley = (!label.is_empty())
        .then(|| painter.layout_no_wrap(label.to_owned(), FontId::proportional(13.5), Color32::PLACEHOLDER));

    let icon_w = icon_galley.as_ref().map_or(0.0, |g| g.size().x);
    let label_w = label_galley.as_ref().map_or(0.0, |g| g.size().x);
    let gap = if icon_galley.is_some() && label_galley.is_some() { 7.0 } else { 0.0 };
    let content_w = icon_w + gap + label_w;

    let natural = if label.is_empty() { BUTTON_HEIGHT } else { content_w + 24.0 };
    let size = vec2(width.unwrap_or(natural), BUTTON_HEIGHT);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp };

    let hovered = enabled && resp.hovered();
    let pressed = enabled && resp.is_pointer_button_down_on();
    let (fill, stroke, fg) = colors(kind, hovered, pressed, enabled);

    let radius = CornerRadius::same(BUTTON_RADIUS);
    ui.painter().rect_filled(rect, radius, fill);
    if stroke != Stroke::NONE {
        ui.painter().rect_stroke(rect, radius, stroke, StrokeKind::Inside);
    }

    // Conteúdo centralizado.
    let mut x = rect.center().x - content_w / 2.0;
    if let Some(g) = icon_galley {
        let pos = Pos2::new(x, rect.center().y - g.size().y / 2.0);
        ui.painter().galley(pos, g, fg);
        x += icon_w + gap;
    }
    if let Some(g) = label_galley {
        let pos = Pos2::new(x, rect.center().y - g.size().y / 2.0);
        ui.painter().galley(pos, g, fg);
    }
    resp
}

/// (fundo, borda, cor do conteúdo) de cada tipo de botão em cada estado.
fn colors(kind: Kind, hovered: bool, pressed: bool, enabled: bool) -> (Color32, Stroke, Color32) {
    let none = Color32::TRANSPARENT;
    if !enabled {
        return match kind {
            Kind::Primary => (SURFACE_3, Stroke::NONE, TEXT_DISABLED),
            Kind::Secondary => (SURFACE_2, Stroke::new(1.0, BORDER), TEXT_DISABLED),
            _ => (none, Stroke::NONE, TEXT_DISABLED),
        };
    }
    match kind {
        Kind::Primary => {
            let fill = if pressed {
                ACCENT_PRESSED
            } else if hovered {
                ACCENT_HOVER
            } else {
                ACCENT
            };
            (fill, Stroke::NONE, Color32::WHITE)
        }
        Kind::Secondary => {
            let fill = if hovered { Color32::from_rgb(46, 52, 65) } else { SURFACE_3 };
            let border = if hovered { BORDER_STRONG } else { BORDER };
            (fill, Stroke::new(1.0, border), if hovered { Color32::WHITE } else { TEXT })
        }
        Kind::Ghost => {
            let fill = if pressed {
                Color32::from_rgb(58, 65, 82)
            } else if hovered {
                SURFACE_3
            } else {
                none
            };
            (fill, Stroke::NONE, if hovered { Color32::WHITE } else { TEXT })
        }
        Kind::Segment { selected } => {
            if selected {
                (SURFACE_3, Stroke::new(1.0, BORDER_STRONG), Color32::WHITE)
            } else {
                (if hovered { SURFACE_2 } else { none }, Stroke::NONE, if hovered { TEXT } else { TEXT_WEAK })
            }
        }
        Kind::Tool { selected } => {
            if selected {
                (ACCENT, Stroke::NONE, Color32::WHITE)
            } else {
                (if hovered { SURFACE_3 } else { none }, Stroke::NONE, if hovered { Color32::WHITE } else { TEXT })
            }
        }
    }
}

/// Botão principal (cor da marca).
pub fn primary(ui: &mut egui::Ui, icon: &str, label: &str) -> egui::Response {
    draw_button(ui, icon, label, Kind::Primary, None)
}

/// Botão comum.
pub fn secondary(ui: &mut egui::Ui, icon: &str, label: &str) -> egui::Response {
    draw_button(ui, icon, label, Kind::Secondary, None)
}

/// Botão sem fundo (só aparece ao passar o mouse).
pub fn ghost(ui: &mut egui::Ui, icon: &str, label: &str) -> egui::Response {
    draw_button(ui, icon, label, Kind::Ghost, None)
}

/// Botão quadrado só com ícone (barras de ferramentas).
pub fn tool(ui: &mut egui::Ui, icon: &str, selected: bool) -> egui::Response {
    draw_button(ui, icon, "", Kind::Tool { selected }, None)
}

/// Controle segmentado: uma fileira de opções, uma selecionada.
/// `items`: (valor, ícone, texto). Com `equal_width`, ocupa toda a largura.
pub fn segmented<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    items: &[(T, &str, &str)],
    equal_width: bool,
) -> bool {
    let mut changed = false;
    egui::Frame::new()
        .fill(INSET)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let spacing = ui.spacing().item_spacing.x;
            let item_w = equal_width.then(|| ((ui.available_width() - spacing * (items.len() as f32 - 1.0)) / items.len() as f32).floor());
            ui.horizontal(|ui| {
                for &(v, icon, label) in items {
                    let selected = *value == v;
                    if draw_button(ui, icon, label, Kind::Segment { selected }, item_w).clicked() && !selected {
                        *value = v;
                        changed = true;
                    }
                }
            });
        });
    changed
}

// ---------------------------------------------------------------------
// Cartões e textos
// ---------------------------------------------------------------------

/// Grupo de botões numa cápsula (barras de ferramentas).
pub fn group<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(INSET)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(add_contents).inner
        })
        .inner
}

/// Cartão com título e ícone, usado para agrupar opções no painel lateral.
pub fn section<R>(ui: &mut egui::Ui, icon: &str, title: &str, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let out = egui::Frame::new()
        .fill(SURFACE_2)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(icon).size(17.0).color(ACCENT_HOVER));
                ui.label(bold(title).size(14.5).color(TEXT));
            });
            ui.add_space(2.0);
            add_contents(ui)
        })
        .inner;
    ui.add_space(10.0);
    out
}

/// Cartão "afundado" para agrupar controles dentro de um cartão.
pub fn inset<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(INSET)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

/// Logotipo do app: quadrado com a cor da marca, ícone de cubo e o nome.
pub fn brand(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(9), ACCENT);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icons::CUBE, FontId::proportional(19.0), Color32::WHITE);
    ui.label(bold("Criador de Modelos").size(16.0));
}

/// Ícone grande dentro de um círculo suave (tela inicial).
pub fn hero_icon(ui: &mut egui::Ui, icon: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(84.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 42.0, accent_soft(36));
    ui.painter().circle_stroke(rect.center(), 42.0, Stroke::new(1.0, accent_soft(90)));
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, FontId::proportional(40.0), ACCENT_HOVER);
}

// ---------------------------------------------------------------------
// Cores
// ---------------------------------------------------------------------

/// Folga ao redor da amostra para caber o anel de seleção.
const SWATCH_PAD: f32 = 3.0;

/// Amostra de cor clicável. `crossed` = riscada (cor de fundo).
pub fn swatch(ui: &mut egui::Ui, color: Rgb8, size: f32, selected: bool, crossed: bool) -> egui::Response {
    let outer = Vec2::splat(size + SWATCH_PAD * 2.0);
    let (rect, resp) = ui.allocate_exact_size(outer, Sense::click());
    let inner = rect.shrink(SWATCH_PAD);
    let radius = CornerRadius::same((size * 0.28).clamp(3.0, 9.0) as u8);
    let painter = ui.painter();

    if crossed {
        // Fundo quadriculado + risco: mostra que a cor é "transparente".
        painter.rect_filled(inner, radius, Color32::from_gray(200));
        painter.rect_filled(inner, radius, to_color32(color).gamma_multiply(0.55));
        painter.line_segment(
            [inner.left_bottom() + vec2(3.0, -3.0), inner.right_top() + vec2(-3.0, 3.0)],
            Stroke::new(2.0, DANGER),
        );
    } else {
        painter.rect_filled(inner, radius, to_color32(color));
    }
    painter.rect_stroke(inner, radius, Stroke::new(1.0, Color32::from_black_alpha(90)), StrokeKind::Inside);

    if selected {
        painter.rect_stroke(rect.shrink(0.5), CornerRadius::same(radius.nw + 3), Stroke::new(2.0, ACCENT_HOVER), StrokeKind::Inside);
    } else if resp.hovered() {
        painter.rect_stroke(rect.shrink(1.0), CornerRadius::same(radius.nw + 3), Stroke::new(1.5, BORDER_STRONG), StrokeKind::Inside);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Quadrado "+" para adicionar uma cor.
pub fn add_tile(ui: &mut egui::Ui, size: f32) -> egui::Response {
    let outer = Vec2::splat(size + SWATCH_PAD * 2.0);
    let (rect, resp) = ui.allocate_exact_size(outer, Sense::click());
    let inner = rect.shrink(SWATCH_PAD);
    let radius = CornerRadius::same((size * 0.28).clamp(3.0, 9.0) as u8);
    let hovered = resp.hovered();
    let painter = ui.painter();
    painter.rect_filled(inner, radius, if hovered { SURFACE_3 } else { INSET });
    painter.rect_stroke(inner, radius, Stroke::new(1.0, if hovered { ACCENT_HOVER } else { BORDER_STRONG }), StrokeKind::Inside);
    painter.text(
        inner.center(),
        Align2::CENTER_CENTER,
        icons::PLUS,
        FontId::proportional(size * 0.5),
        if hovered { Color32::WHITE } else { TEXT_WEAK },
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------
// Imagens
// ---------------------------------------------------------------------

/// Mostra a textura centralizada, ocupando o máximo de espaço sem distorcer.
pub fn show_fit(ui: &mut egui::Ui, texture: Option<&egui::TextureHandle>) {
    let Some(tex) = texture else {
        ui.centered_and_justified(|ui| ui.spinner());
        return;
    };
    let avail = ui.available_rect_before_wrap();
    let size = tex.size_vec2();
    let scale = (avail.width() / size.x).min(avail.height() / size.y).max(0.01);
    let rect = Rect::from_center_size(avail.center(), size * scale);
    ui.allocate_rect(rect, Sense::hover());
    egui::Image::new((tex.id(), size * scale)).corner_radius(CornerRadius::same(6)).paint_at(ui, rect);
}

/// Fundo quadriculado (padrão de "transparente"), desenhado em `area`.
pub fn checkerboard(painter: &egui::Painter, area: Rect) {
    if !area.is_positive() {
        return;
    }
    const TILE: f32 = 12.0;
    painter.rect_filled(area, 0.0, Color32::from_gray(208));
    let dark = Color32::from_gray(176);
    let mut y = area.min.y;
    let mut row = 0;
    while y < area.max.y {
        let mut x = area.min.x + if row % 2 == 1 { TILE } else { 0.0 };
        while x < area.max.x {
            let tile = Rect::from_min_max(
                egui::pos2(x, y),
                egui::pos2((x + TILE).min(area.max.x), (y + TILE).min(area.max.y)),
            );
            painter.rect_filled(tile, 0.0, dark);
            x += 2.0 * TILE;
        }
        y += TILE;
        row += 1;
    }
}

/// Etiqueta arredondada flutuante (dicas sobre a tela de edição).
pub fn pill(painter: &egui::Painter, center_bottom: Pos2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(12.5), Color32::PLACEHOLDER);
    let size = galley.size() + vec2(24.0, 12.0);
    let rect = Rect::from_min_size(Pos2::new(center_bottom.x - size.x / 2.0, center_bottom.y - size.y), size);
    painter.rect_filled(rect, CornerRadius::same((size.y / 2.0) as u8), Color32::from_rgba_unmultiplied(12, 14, 18, 215));
    painter.rect_stroke(rect, CornerRadius::same((size.y / 2.0) as u8), Stroke::new(1.0, BORDER), StrokeKind::Inside);
    painter.galley(rect.min + vec2(12.0, 6.0), galley, Color32::from_rgb(210, 215, 225));
}
