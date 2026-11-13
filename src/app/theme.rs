//! Identidade visual "Bancada": o app como uma bancada de oficina.
//!
//! - A área de edição é uma base de corte verde, com grade em milímetros.
//! - Barras e painéis em alumínio claro.
//! - Amarelo de estilete marca ações principais, ferramenta ativa e cotas.
//! - Quicksand (pontas arredondadas, como letra extrudada) nos títulos e
//!   medidas; Lato no texto da interface.
//!
//! Tudo que for cor ou tamanho "de marca" fica aqui.

use std::collections::BTreeMap;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText, Shadow, Stroke,
    TextStyle, ThemePreference, vec2,
};

/// Ícones Phosphor (fonte de ícones). Uso: `icons::PAINT_BRUSH`.
pub use egui_phosphor::regular as icons;

// ---------------------------------------------------------------------
// Paleta
// ---------------------------------------------------------------------

/// Base de corte (área de edição).
pub const MAT: Color32 = Color32::from_rgb(31, 74, 63);
/// Faixa das réguas, um tom abaixo da base.
pub const MAT_DEEP: Color32 = Color32::from_rgb(23, 57, 48);
/// Linhas finas da grade (milímetros).
pub const MAT_LINE: Color32 = Color32::from_rgb(42, 92, 79);
/// Linhas fortes da grade (centímetros).
pub const MAT_LINE_MAJOR: Color32 = Color32::from_rgb(66, 124, 107);
/// Números e marcas impressos na base.
pub const MAT_INK: Color32 = Color32::from_rgb(169, 211, 194);

/// Barras e trilho de ferramentas.
pub const ALUMINIUM: Color32 = Color32::from_rgb(220, 224, 218);
/// Inspetor (painel de propriedades).
pub const PLATE: Color32 = Color32::from_rgb(238, 240, 236);
/// Superfície mais clara (campos, quadros internos).
pub const PAPER: Color32 = Color32::from_rgb(249, 250, 247);
/// Trilho de sliders, fundo de controles segmentados.
pub const TRACK: Color32 = Color32::from_rgb(205, 211, 204);
/// Linhas finas de divisão.
pub const RULE: Color32 = Color32::from_rgb(190, 197, 189);
pub const RULE_STRONG: Color32 = Color32::from_rgb(150, 160, 152);

/// Texto principal.
pub const INK: Color32 = Color32::from_rgb(23, 32, 28);
/// Texto secundário.
pub const GRAPHITE: Color32 = Color32::from_rgb(86, 98, 93);
pub const DISABLED: Color32 = Color32::from_rgb(150, 158, 153);

/// Amarelo de estilete: ação principal, seleção, cotas.
pub const CUTTER: Color32 = Color32::from_rgb(245, 196, 0);
pub const CUTTER_HOVER: Color32 = Color32::from_rgb(255, 210, 38);
pub const CUTTER_PRESSED: Color32 = Color32::from_rgb(214, 170, 0);

/// Erros e cores marcadas como fundo.
pub const SIGNAL: Color32 = Color32::from_rgb(196, 65, 47);

// ---------------------------------------------------------------------
// Fontes e textos
// ---------------------------------------------------------------------

/// Lato Bold.
pub fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// Quicksand Bold: títulos, marca e medidas.
pub fn display_family() -> FontFamily {
    FontFamily::Name("display".into())
}

/// Texto em negrito.
pub fn bold(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).family(bold_family())
}

/// Texto de título (Quicksand).
pub fn display(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).family(display_family())
}

/// Texto secundário.
pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(GRAPHITE)
}

/// Milímetros no formato brasileiro: `50,0 mm`.
pub fn mm(value: f64) -> String {
    format!("{value:.1} mm").replace('.', ",")
}

/// Só o número com vírgula: `50,0`.
pub fn num(value: f64) -> String {
    format!("{value:.1}").replace('.', ",")
}

// ---------------------------------------------------------------------
// Instalação
// ---------------------------------------------------------------------

/// Aplica fontes, ícones e estilo. Chamar uma vez na criação do app.
pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    // Tema único (claro), independente do sistema.
    ctx.set_theme(ThemePreference::Light);
    ctx.all_styles_mut(|style| {
        style.visuals = visuals();

        let s = &mut style.spacing;
        s.item_spacing = vec2(8.0, 7.0);
        s.button_padding = vec2(10.0, 5.0);
        s.interact_size = vec2(30.0, 28.0);
        s.slider_width = 132.0;
        s.slider_rail_height = 3.0;
        s.menu_margin = Margin::same(6);
        s.combo_width = 120.0;

        style.interaction.tooltip_delay = 0.35;

        style.text_styles = BTreeMap::from([
            (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(15.0, display_family())),
            (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
        ]);
    });
}

/// Fontes: Lato (texto), Quicksand (títulos) e Phosphor (ícones). Os ícones
/// entram como reserva das outras fontes, então podem ser misturados no texto.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.into(), FontData::from_static(bytes).into());
    };
    add("lato", include_bytes!("../../assets/fonts/Lato-Regular.ttf"));
    add("lato-bold", include_bytes!("../../assets/fonts/Lato-Bold.ttf"));
    add("quicksand-bold", include_bytes!("../../assets/fonts/Quicksand-Bold.ttf"));

    // Lato na frente; as fontes padrão do egui ficam de reserva.
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "lato".into());
    // Phosphor entra logo depois da Lato.
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    // Demais famílias: mesma cadeia de reserva, trocando a primeira fonte.
    let fallbacks: Vec<String> = fonts
        .families
        .get(&FontFamily::Proportional)
        .map(|v| v.iter().filter(|n| n.as_str() != "lato").cloned().collect())
        .unwrap_or_default();
    for (family, first) in [(bold_family(), "lato-bold"), (display_family(), "quicksand-bold")] {
        let mut chain = vec![first.to_owned()];
        chain.extend(fallbacks.iter().cloned());
        fonts.families.insert(family, chain);
    }

    ctx.set_fonts(fonts);
}

/// Cores e cantos dos controles padrão do egui.
fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    let radius = CornerRadius::same(6);

    v.override_text_color = None;
    v.panel_fill = ALUMINIUM;
    v.window_fill = PAPER;
    v.extreme_bg_color = PAPER;
    v.faint_bg_color = PLATE;
    v.code_bg_color = TRACK;
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_stroke = Stroke::new(1.0, RULE_STRONG);
    let shadow = Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(45) };
    v.window_shadow = shadow;
    v.popup_shadow = shadow;
    v.hyperlink_color = MAT;
    v.warn_fg_color = Color32::from_rgb(166, 110, 0);
    v.error_fg_color = SIGNAL;
    v.weak_text_color = Some(GRAPHITE);

    // Seleção (e parte preenchida dos sliders): amarelo de estilete.
    v.selection.bg_fill = CUTTER;
    v.selection.stroke = Stroke::new(1.0, INK);

    let w = &mut v.widgets;

    // Elementos sem interação: rótulos, separadores.
    w.noninteractive.bg_fill = ALUMINIUM;
    w.noninteractive.weak_bg_fill = ALUMINIUM;
    w.noninteractive.bg_stroke = Stroke::new(1.0, RULE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, INK);
    w.noninteractive.corner_radius = radius;

    // Em repouso.
    w.inactive.bg_fill = TRACK;
    w.inactive.weak_bg_fill = PAPER;
    w.inactive.bg_stroke = Stroke::new(1.0, RULE);
    w.inactive.fg_stroke = Stroke::new(1.0, INK);
    w.inactive.corner_radius = radius;

    // Mouse em cima.
    w.hovered.bg_fill = Color32::from_rgb(190, 198, 189);
    w.hovered.weak_bg_fill = Color32::WHITE;
    w.hovered.bg_stroke = Stroke::new(1.0, RULE_STRONG);
    w.hovered.fg_stroke = Stroke::new(1.5, INK);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    // Pressionado.
    w.active.bg_fill = CUTTER_PRESSED;
    w.active.weak_bg_fill = CUTTER;
    w.active.bg_stroke = Stroke::new(1.0, INK);
    w.active.fg_stroke = Stroke::new(1.5, INK);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    // Menu aberto.
    w.open = w.hovered;

    v
}
