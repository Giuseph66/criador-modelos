//! Componentes visuais reaproveitáveis: botões, abas, controle segmentado,
//! cabeçalhos de seção, carretéis de cor.
//!
//! Os botões são desenhados à mão (em vez de usar `egui::Button`) para
//! controlar exatamente cores, cantos e o alinhamento ícone + texto.

use std::f32::consts::TAU;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Margin, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2, vec2,
};

use criador_modelos::colors::Rgb8;

use super::theme::{
    CUTTER, CUTTER_HOVER, CUTTER_PRESSED, DISABLED, GRAPHITE, INK, MAT_DEEP, MAT_INK, PAPER, PLATE, RULE,
    RULE_STRONG, SIGNAL, TRACK, bold, display, icons,
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
    /// Ação principal: amarelo de estilete.
    Primary,
    /// Ação comum: papel com borda fina.
    Secondary,
    /// Sem fundo até passar o mouse.
    Ghost,
    /// Item de um controle segmentado.
    Segment { selected: bool },
    /// Ferramenta do trilho (só ícone). Ativa = grafite com ícone amarelo.
    Tool { selected: bool },
}

const BUTTON_HEIGHT: f32 = 30.0;

/// Desenha e controla um botão com ícone e/ou texto.
/// `width`: largura fixa (senão se ajusta ao conteúdo).
fn draw_button(ui: &mut egui::Ui, icon: &str, label: &str, kind: Kind, width: Option<f32>) -> egui::Response {
    let enabled = ui.is_enabled();
    let is_tool = matches!(kind, Kind::Tool { .. });
    let height = if is_tool { 38.0 } else { BUTTON_HEIGHT };
    let icon_size = if is_tool { 20.0 } else { 16.0 };

    // Com `Color32::PLACEHOLDER` a cor é escolhida só na hora de pintar
    // (depois de saber se o mouse está em cima).
    let painter = ui.painter().clone();
    let icon_galley = (!icon.is_empty())
        .then(|| painter.layout_no_wrap(icon.to_owned(), FontId::proportional(icon_size), Color32::PLACEHOLDER));
    let label_font = if kind == Kind::Primary {
        FontId::new(13.0, super::theme::bold_family())
    } else {
        FontId::proportional(13.0)
    };
    let label_galley = (!label.is_empty()).then(|| painter.layout_no_wrap(label.to_owned(), label_font, Color32::PLACEHOLDER));

    let icon_w = icon_galley.as_ref().map_or(0.0, |g| g.size().x);
    let label_w = label_galley.as_ref().map_or(0.0, |g| g.size().x);
    let gap = if icon_galley.is_some() && label_galley.is_some() { 6.0 } else { 0.0 };
    let content_w = icon_w + gap + label_w;

    let natural = if label.is_empty() { height } else { content_w + 22.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(width.unwrap_or(natural), height), Sense::click());
    let resp = if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp };

    let hovered = enabled && resp.hovered();
    let pressed = enabled && resp.is_pointer_button_down_on();
    let (fill, stroke, fg) = colors(kind, hovered, pressed, enabled);

    let radius = CornerRadius::same(if is_tool { 8 } else { 6 });
    ui.painter().rect_filled(rect, radius, fill);
    if stroke != Stroke::NONE {
        ui.painter().rect_stroke(rect, radius, stroke, StrokeKind::Inside);
    }

    let mut x = rect.center().x - content_w / 2.0;
    if let Some(g) = icon_galley {
        ui.painter().galley(Pos2::new(x, rect.center().y - g.size().y / 2.0), g, fg);
        x += icon_w + gap;
    }
    if let Some(g) = label_galley {
        ui.painter().galley(Pos2::new(x, rect.center().y - g.size().y / 2.0), g, fg);
    }
    resp
}

/// (fundo, borda, cor do conteúdo) de cada tipo de botão em cada estado.
fn colors(kind: Kind, hovered: bool, pressed: bool, enabled: bool) -> (Color32, Stroke, Color32) {
    let none = Color32::TRANSPARENT;
    if !enabled {
        return match kind {
            Kind::Primary => (TRACK, Stroke::NONE, DISABLED),
            Kind::Secondary => (PLATE, Stroke::new(1.0, RULE), DISABLED),
            _ => (none, Stroke::NONE, DISABLED),
        };
    }
    match kind {
        Kind::Primary => {
            let fill = if pressed {
                CUTTER_PRESSED
            } else if hovered {
                CUTTER_HOVER
            } else {
                CUTTER
            };
            (fill, Stroke::new(1.0, Color32::from_rgb(196, 156, 0)), INK)
        }
        Kind::Secondary => {
            let fill = if pressed { TRACK } else if hovered { Color32::WHITE } else { PAPER };
            (fill, Stroke::new(1.0, if hovered { RULE_STRONG } else { RULE }), INK)
        }
        Kind::Ghost => {
            let fill = if pressed { TRACK } else if hovered { Color32::from_black_alpha(14) } else { none };
            (fill, Stroke::NONE, if hovered { INK } else { GRAPHITE })
        }
        Kind::Segment { selected } => {
            if selected {
                (PAPER, Stroke::new(1.0, RULE_STRONG), INK)
            } else {
                (if hovered { Color32::from_white_alpha(90) } else { none }, Stroke::NONE, if hovered { INK } else { GRAPHITE })
            }
        }
        Kind::Tool { selected } => {
            if selected {
                (INK, Stroke::NONE, CUTTER)
            } else {
                (if hovered { Color32::from_black_alpha(18) } else { none }, Stroke::NONE, if hovered { INK } else { GRAPHITE })
            }
        }
    }
}

/// Botão da ação principal (amarelo).
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

/// Ferramenta do trilho lateral.
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
        .fill(TRACK)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let n = items.len() as f32;
            let item_w = equal_width.then(|| ((ui.available_width() - 2.0 * (n - 1.0)) / n).floor());
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

/// Aba de texto com sublinhado amarelo quando ativa (barra superior).
pub fn tab(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool) -> egui::Response {
    let font = FontId::new(13.5, if selected { super::theme::bold_family() } else { egui::FontFamily::Proportional });
    let text = format!("{icon}  {label}");
    let galley = ui.painter().layout_no_wrap(text, font, Color32::PLACEHOLDER);
    let size = vec2(galley.size().x + 20.0, 40.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    let color = if selected || hovered { INK } else { GRAPHITE };
    let pos = Pos2::new(rect.center().x - galley.size().x / 2.0, rect.center().y - galley.size().y / 2.0 - 1.0);
    ui.painter().galley(pos, galley, color);
    if selected {
        let bar = Rect::from_min_max(Pos2::new(rect.min.x + 6.0, rect.max.y - 3.0), Pos2::new(rect.max.x - 6.0, rect.max.y));
        ui.painter().rect_filled(bar, CornerRadius::same(1), CUTTER);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------
// Estrutura do inspetor
// ---------------------------------------------------------------------

/// Seção do inspetor: título (Quicksand) com ícone, conteúdo e uma linha
/// fina embaixo. Sem cartão: a linha é a única divisão.
pub fn section<R>(ui: &mut egui::Ui, icon: &str, title: &str, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon).size(16.0).color(GRAPHITE));
        ui.label(display(title).size(15.5).color(INK));
    });
    ui.add_space(2.0);
    let out = add_contents(ui);
    ui.add_space(12.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, RULE);
    ui.add_space(4.0);
    out
}

/// Quadro em papel para destacar um grupo de controles dentro de uma seção.
pub fn inset<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(PAPER)
        .stroke(Stroke::new(1.0, RULE))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

/// Rótulo + valor (barra de status e resumo de medidas).
pub fn field(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(egui::RichText::new(label).size(12.0).color(GRAPHITE));
    ui.label(bold(value).size(12.0).color(INK));
}

/// Marca do app: lâmina de estilete amarela + nome em Quicksand.
pub fn wordmark(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
    // Lâmina: um paralelogramo inclinado, como a ponta de um estilete.
    let (l, r, t, b) = (rect.left(), rect.right(), rect.top() + 2.0, rect.bottom() - 2.0);
    let blade = vec![
        Pos2::new(l + 2.0, b),
        Pos2::new(l + 9.0, t),
        Pos2::new(r - 1.0, t),
        Pos2::new(r - 8.0, b),
    ];
    ui.painter().add(Shape::convex_polygon(blade, CUTTER, Stroke::new(1.0, INK)));
    ui.label(display("criador de modelos").size(17.0).color(INK));
}

// ---------------------------------------------------------------------
// Carretéis (amostras de cor)
// ---------------------------------------------------------------------

/// Folga ao redor do carretel para caber o anel de seleção.
const SPOOL_PAD: f32 = 4.0;

/// Cor como um carretel de filamento: disco colorido com o furo central.
/// `background` = cor de fundo: aparece apagada e riscada.
pub fn spool(ui: &mut egui::Ui, color: Rgb8, diameter: f32, selected: bool, background: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(diameter + SPOOL_PAD * 2.0), Sense::click());
    let c = rect.center();
    let r = diameter / 2.0;
    let painter = ui.painter();

    if selected {
        painter.circle_stroke(c, r + 3.0, Stroke::new(2.0, INK));
    } else if resp.hovered() {
        painter.circle_stroke(c, r + 2.5, Stroke::new(1.5, RULE_STRONG));
    }

    let fill = to_color32(color);
    if background {
        painter.circle_filled(c, r, PAPER);
        painter.circle_filled(c, r, fill.gamma_multiply(0.35));
    } else {
        painter.circle_filled(c, r, fill);
    }
    // Aro e furo central do carretel.
    painter.circle_stroke(c, r - 0.5, Stroke::new(1.0, Color32::from_black_alpha(55)));
    painter.circle_filled(c, r * 0.3, PLATE);
    painter.circle_stroke(c, r * 0.3, Stroke::new(1.0, Color32::from_black_alpha(45)));
    if background {
        let d = r * 0.72;
        painter.line_segment([c + vec2(-d, d), c + vec2(d, -d)], Stroke::new(1.8, SIGNAL));
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Carretel vazio, tracejado, com "+": adicionar uma cor.
pub fn add_spool(ui: &mut egui::Ui, diameter: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(diameter + SPOOL_PAD * 2.0), Sense::click());
    let c = rect.center();
    let r = diameter / 2.0 - 0.5;
    let hovered = resp.hovered();
    let color = if hovered { INK } else { RULE_STRONG };
    let circle: Vec<Pos2> = (0..=48).map(|i| c + Vec2::angled(i as f32 / 48.0 * TAU) * r).collect();
    ui.painter().extend(Shape::dashed_line(&circle, Stroke::new(1.2, color), 3.0, 2.5));
    ui.painter().text(c, Align2::CENTER_CENTER, icons::PLUS, FontId::proportional(diameter * 0.45), color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------
// Imagens
// ---------------------------------------------------------------------

/// Etiqueta flutuante sobre a base de corte (dica da ferramenta).
pub fn pill(painter: &egui::Painter, center_bottom: Pos2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(12.5), Color32::PLACEHOLDER);
    let size = galley.size() + vec2(22.0, 10.0);
    let rect = Rect::from_min_size(Pos2::new(center_bottom.x - size.x / 2.0, center_bottom.y - size.y), size);
    let radius = CornerRadius::same((size.y / 2.0) as u8);
    painter.rect_filled(rect, radius, MAT_DEEP.gamma_multiply(0.92));
    painter.galley(rect.min + vec2(11.0, 5.0), galley, MAT_INK);
}
