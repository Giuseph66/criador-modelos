//! Identidade visual do app: paleta de cores, fontes, ícones e estilo
//! padrão dos controles do egui.
//!
//! Tudo que for cor ou tamanho "de marca" fica aqui, para mudar o visual
//! do app inteiro mexendo num lugar só.

use std::collections::BTreeMap;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText, Shadow, Stroke,
    TextStyle, ThemePreference, vec2,
};

/// Ícones Phosphor (fonte de ícones). Uso: `icons::PAINT_BRUSH`.
pub use egui_pop_phosphor::regular as icons;

// ---------------------------------------------------------------------
// Paleta
// ---------------------------------------------------------------------

/// Fundo da área de edição (o mais escuro).
pub const BG: Color32 = Color32::from_rgb(11, 13, 17);
/// Painéis e barras.
pub const SURFACE: Color32 = Color32::from_rgb(19, 22, 28);
/// Cartões dentro dos painéis.
pub const SURFACE_2: Color32 = Color32::from_rgb(26, 30, 38);
/// Botões, trilhos de slider, itens em destaque.
pub const SURFACE_3: Color32 = Color32::from_rgb(38, 43, 54);
/// Cartão "afundado", dentro de outro cartão.
pub const INSET: Color32 = Color32::from_rgb(15, 17, 22);
pub const BORDER: Color32 = Color32::from_rgb(42, 47, 59);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(70, 78, 96);

pub const TEXT: Color32 = Color32::from_rgb(230, 233, 239);
pub const TEXT_WEAK: Color32 = Color32::from_rgb(136, 144, 160);
pub const TEXT_DISABLED: Color32 = Color32::from_rgb(84, 91, 105);

/// Cor de destaque da marca (violeta).
pub const ACCENT: Color32 = Color32::from_rgb(124, 92, 255);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(146, 120, 255);
pub const ACCENT_PRESSED: Color32 = Color32::from_rgb(100, 72, 224);

pub const DANGER: Color32 = Color32::from_rgb(239, 98, 98);
pub const WARNING: Color32 = Color32::from_rgb(245, 183, 49);

/// Cor de destaque com transparência (fundos suaves).
pub fn accent_soft(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(124, 92, 255, alpha)
}

/// Família de fonte em negrito.
pub fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// Texto em negrito.
pub fn bold(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).family(bold_family())
}

/// Texto secundário (cinza).
pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(TEXT_WEAK)
}

// ---------------------------------------------------------------------
// Instalação
// ---------------------------------------------------------------------

/// Aplica fontes, ícones e estilo. Chamar uma vez na criação do app.
pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    // O app tem um tema só (escuro), independente do sistema.
    ctx.set_theme(ThemePreference::Dark);
    ctx.all_styles_mut(|style| {
        style.visuals = visuals();

        let s = &mut style.spacing;
        s.item_spacing = vec2(8.0, 8.0);
        s.button_padding = vec2(10.0, 6.0);
        s.interact_size = vec2(32.0, 30.0);
        s.slider_width = 128.0;
        s.slider_rail_height = 4.0;
        s.menu_margin = Margin::same(6);
        s.combo_width = 120.0;

        style.interaction.tooltip_delay = 0.35;

        let text_styles: BTreeMap<TextStyle, FontId> = BTreeMap::from([
            (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.5, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(15.0, bold_family())),
            (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
        ]);
        style.text_styles = text_styles;
    });
}

/// Fontes: Lato (texto) + Phosphor (ícones). Os ícones entram como reserva
/// da fonte de texto, então podem ser misturados em qualquer texto.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts
        .font_data
        .insert("lato".into(), FontData::from_static(include_bytes!("../../assets/fonts/Lato-Regular.ttf")).into());
    fonts
        .font_data
        .insert("lato-bold".into(), FontData::from_static(include_bytes!("../../assets/fonts/Lato-Bold.ttf")).into());

    // Lato vai na frente; as fontes padrão do egui ficam de reserva
    // (cobrem símbolos que a Lato não tem).
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "lato".into());

    // Entra logo depois da Lato (posição 1) na família de texto.
    egui_pop_phosphor::add_to_fonts(&mut fonts, egui_pop_phosphor::Variant::Regular);

    // Família negrito: Lato Bold + as mesmas reservas do texto normal.
    let mut bold_chain = vec!["lato-bold".to_owned()];
    if let Some(regular) = fonts.families.get(&FontFamily::Proportional) {
        bold_chain.extend(regular.iter().filter(|name| name.as_str() != "lato").cloned());
    }
    fonts.families.insert(bold_family(), bold_chain);

    ctx.set_fonts(fonts);
}

/// Cores e cantos de todos os controles.
fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    let radius = CornerRadius::same(8);

    v.override_text_color = None;
    v.panel_fill = SURFACE;
    v.window_fill = SURFACE_2;
    v.extreme_bg_color = INSET;
    v.faint_bg_color = SURFACE_2;
    v.code_bg_color = SURFACE_3;
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_stroke = Stroke::new(1.0, BORDER);
    let shadow = Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(120) };
    v.window_shadow = shadow;
    v.popup_shadow = shadow;
    v.hyperlink_color = ACCENT_HOVER;
    v.warn_fg_color = WARNING;
    v.error_fg_color = DANGER;
    v.weak_text_color = Some(TEXT_WEAK);

    // Seleção e parte preenchida dos sliders.
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);

    let w = &mut v.widgets;

    // Elementos sem interação (rótulos, separadores).
    w.noninteractive.bg_fill = SURFACE;
    w.noninteractive.weak_bg_fill = SURFACE;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = radius;

    // Em repouso.
    w.inactive.bg_fill = SURFACE_3;
    w.inactive.weak_bg_fill = SURFACE_3;
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = radius;

    // Mouse em cima.
    w.hovered.bg_fill = Color32::from_rgb(50, 56, 70);
    w.hovered.weak_bg_fill = Color32::from_rgb(50, 56, 70);
    w.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    w.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    // Pressionado.
    w.active.bg_fill = ACCENT_PRESSED;
    w.active.weak_bg_fill = ACCENT_PRESSED;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    // Menu aberto.
    w.open = w.hovered;

    v
}
