//! Tela de edição: zoom, mover, e as ferramentas (selecionar, pincel,
//! borracha, balde). É um submódulo de `app`, por isso enxerga os campos
//! privados de `CriadorApp`.

use eframe::egui::{self, Color32, CursorIcon, PointerButton, Pos2, Rect, Vec2};

use criador_modelos::colors::{Dirty, Document, hex};

use super::mat;
use super::theme::{GRAPHITE, INK, RULE_STRONG, display, icons, muted};
use super::widgets::{ghost, pill, spool, tool};
use super::{CriadorApp, EditTag, View};

/// Ferramenta ativa.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Move,
    Select,
    Brush,
    Eraser,
    Fill,
}

impl Tool {
    pub const ALL: [Tool; 5] = [Tool::Move, Tool::Select, Tool::Brush, Tool::Eraser, Tool::Fill];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Move => "Mover",
            Tool::Select => "Conta-gotas",
            Tool::Brush => "Pincel",
            Tool::Eraser => "Borracha",
            Tool::Fill => "Balde de tinta",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tool::Move => icons::HAND,
            Tool::Select => icons::EYEDROPPER,
            Tool::Brush => icons::PAINT_BRUSH,
            Tool::Eraser => icons::ERASER,
            Tool::Fill => icons::PAINT_BUCKET,
        }
    }

    pub fn key(self) -> egui::Key {
        match self {
            Tool::Move => egui::Key::H,
            Tool::Select => egui::Key::V,
            Tool::Brush => egui::Key::B,
            Tool::Eraser => egui::Key::E,
            Tool::Fill => egui::Key::G,
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Move => "Arraste para mover a imagem. Roda do mouse = zoom.",
            Tool::Select => "Clique numa parte da imagem para pegar a cor dela.",
            Tool::Brush => "Clique e arraste para pintar com a cor ativa. [ e ] mudam o tamanho.",
            Tool::Eraser => "Clique e arraste para apagar (vira fundo, não entra no modelo).",
            Tool::Fill => "Clique numa área para pintar só aquela parte com a cor ativa.",
        }
    }
}

/// Posição e escala da imagem dentro da área de edição.
#[derive(Clone, Copy, Debug)]
pub struct ViewTransform {
    /// Pixels de tela por pixel da imagem.
    pub scale: f32,
    /// Canto superior esquerdo da imagem, relativo ao canto da área.
    pub offset: Vec2,
}

impl ViewTransform {
    /// Encaixa a imagem inteira na área, deixando espaço para as réguas
    /// (em cima e à esquerda) e para as cotas (embaixo e à direita).
    pub fn fit(area: Vec2, image: Vec2) -> Self {
        let lead = Vec2::new(mat::RULER + 24.0, mat::RULER + 20.0);
        let trail = Vec2::new(64.0, 44.0);
        let avail = (area - lead - trail).max(Vec2::splat(20.0));
        let scale = (avail.x / image.x).min(avail.y / image.y).max(0.01);
        Self { scale, offset: lead + (avail - image * scale) / 2.0 }
    }

    /// Zoom mantendo fixo o ponto `anchor` (relativo ao canto da área).
    pub fn zoom_at(&mut self, anchor: Vec2, factor: f32) {
        let new_scale = (self.scale * factor).clamp(0.02, 64.0);
        let f = new_scale / self.scale;
        self.offset = anchor - (anchor - self.offset) * f;
        self.scale = new_scale;
    }
}

/// Posição da peça na tela (para grade, réguas e cotas).
#[derive(Clone, Copy)]
struct PieceOnScreen {
    rect: Rect,
    scale: mat::Scale,
    width_mm: f64,
    height_mm: f64,
}

/// Traço do pincel em andamento.
struct Stroke {
    last: [f32; 2],
    label: u8,
}

/// Estado da tela de edição.
pub struct CanvasState {
    pub tool: Tool,
    /// Raio do pincel/borracha em pixels da imagem.
    pub brush_radius: f32,
    /// Opacidade das cores editadas (abaixo de 1 mostra a original por baixo).
    pub edit_opacity: f32,
    /// `None` = encaixar a imagem na tela automaticamente.
    pub view: Option<ViewTransform>,
    /// Tamanho da última área de edição (para os botões de zoom).
    area: Vec2,
    stroke: Option<Stroke>,
    pub texture: Option<egui::TextureHandle>,
    /// Pede para refazer a textura inteira (paleta mudou, desfazer...).
    pub texture_dirty: bool,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            tool: Tool::Brush,
            brush_radius: 12.0,
            edit_opacity: 1.0,
            view: None,
            area: Vec2::new(800.0, 600.0),
            stroke: None,
            texture: None,
            texture_dirty: true,
        }
    }
}

impl CanvasState {
    pub fn is_stroking(&self) -> bool {
        self.stroke.is_some()
    }

    pub fn cancel_stroke(&mut self) {
        self.stroke = None;
    }

    /// Zoom pelos botões/teclado, centrado na área.
    pub fn zoom_by(&mut self, factor: f32, image: Vec2) {
        let mut view = self.view.unwrap_or_else(|| ViewTransform::fit(self.area, image));
        view.zoom_at(self.area / 2.0, factor);
        self.view = Some(view);
    }

    pub fn grow_brush(&mut self, factor: f32) {
        self.brush_radius = (self.brush_radius * factor).clamp(1.0, 200.0);
    }
}

/// Opções de textura: pixels nítidos ao aproximar, suaves ao afastar.
fn texture_options() -> egui::TextureOptions {
    egui::TextureOptions {
        magnification: egui::TextureFilter::Nearest,
        minification: egui::TextureFilter::Linear,
        ..egui::TextureOptions::LINEAR
    }
}

fn region_image(doc: &Document, d: Dirty) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied([d.width() as usize, d.height() as usize], &doc.region_rgba(d, true))
}

impl CriadorApp {
    /// Garante que a textura da tela corresponde ao documento.
    fn ensure_texture(&mut self, ctx: &egui::Context) {
        let Some(doc) = &self.doc else { return };
        let size = [doc.width as usize, doc.height as usize];
        let needs_new = self.canvas.texture.as_ref().is_none_or(|t| t.size() != size);
        if !needs_new && !self.canvas.texture_dirty {
            return;
        }
        let image = region_image(doc, doc.full());
        if !needs_new && let Some(tex) = &mut self.canvas.texture {
            tex.set(image, texture_options());
        } else {
            self.canvas.texture = Some(ctx.load_texture("edicao", image, texture_options()));
        }
        self.canvas.texture_dirty = false;
    }

    /// Atualiza só o pedaço da textura que mudou (rápido durante o traço).
    fn upload_region(&mut self, dirty: Option<Dirty>) {
        let (Some(d), Some(doc), Some(tex)) = (dirty, &self.doc, &mut self.canvas.texture) else { return };
        if d.width() == 0 || d.height() == 0 {
            return;
        }
        tex.set_partial([d.x0 as usize, d.y0 as usize], region_image(doc, d), texture_options());
    }

    /// Trilho vertical de ferramentas (à esquerda da bancada).
    pub(super) fn tool_rail(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            for t in Tool::ALL {
                let active = self.view == View::Edit && self.canvas.tool == t;
                let tip = format!("{} ({:?})\n{}", t.name(), t.key(), t.hint());
                if tool(ui, t.icon(), active).on_hover_text(tip).clicked() {
                    self.canvas.tool = t;
                    self.view = View::Edit;
                }
            }

            ui.add_space(6.0);
            let (line, _) = ui.allocate_exact_size(egui::vec2(26.0, 1.0), egui::Sense::hover());
            ui.painter().rect_filled(line, 0.0, RULE_STRONG);
            ui.add_space(6.0);

            // Cor ativa, como carretel.
            if let Some((i, color, bg)) = self.active_entry()
                && spool(ui, color, 26.0, false, bg).on_hover_text(format!("Cor ativa: {}", hex(color))).clicked()
            {
                self.selected = Some(i);
            }
        });
    }

    /// Barra de opções da ferramenta ativa, acima da bancada.
    pub(super) fn options_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(display(self.canvas.tool.name()).size(16.0).color(INK));
            ui.add_space(12.0);

            if matches!(self.canvas.tool, Tool::Brush | Tool::Eraser) {
                ui.label(muted("Tamanho"));
                ui.spacing_mut().slider_width = 130.0;
                ui.add(
                    egui::Slider::new(&mut self.canvas.brush_radius, 1.0..=200.0)
                        .logarithmic(true)
                        .integer()
                        .suffix(" px"),
                )
                .on_hover_text("Raio do pincel. Atalhos: [ diminui, ] aumenta");
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let image = self.doc_size();
                if ghost(ui, icons::FRAME_CORNERS, "").on_hover_text("Mostrar a peça inteira (tecla 0)").clicked() {
                    self.canvas.view = None;
                }
                if ghost(ui, icons::MAGNIFYING_GLASS_PLUS, "").on_hover_text("Aproximar (tecla +)").clicked() {
                    self.canvas.zoom_by(1.25, image);
                }
                let percent = self.canvas.view.map_or_else(
                    || ViewTransform::fit(self.canvas.area, image).scale,
                    |v| v.scale,
                ) * 100.0;
                ui.label(egui::RichText::new(format!("{percent:.0}%")).color(GRAPHITE));
                if ghost(ui, icons::MAGNIFYING_GLASS_MINUS, "").on_hover_text("Afastar (tecla -)").clicked() {
                    self.canvas.zoom_by(1.0 / 1.25, image);
                }

                ui.add_space(16.0);
                ui.spacing_mut().slider_width = 90.0;
                ui.add(egui::Slider::new(&mut self.canvas.edit_opacity, 0.0..=1.0).show_value(false))
                    .on_hover_text("Diminua para enxergar a imagem original por baixo e pintar por cima dela");
                ui.label(muted(format!("{}  Ver original", icons::EYE)));
            });
        });
    }

    fn doc_size(&self) -> Vec2 {
        self.doc.as_ref().map_or(Vec2::splat(1.0), |d| Vec2::new(d.width as f32, d.height as f32))
    }

    /// Área de edição: desenha a imagem e trata mouse/teclado das ferramentas.
    pub(super) fn editor_canvas(&mut self, ui: &mut egui::Ui) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        self.canvas.area = rect.size();
        if self.doc.is_none() {
            ui.put(rect, egui::Spinner::new().size(32.0));
            return;
        }
        self.ensure_texture(ui.ctx());

        let image = self.doc_size();
        let mut view = self.canvas.view.unwrap_or_else(|| ViewTransform::fit(rect.size(), image));

        // Zoom com a roda do mouse (ou gesto de pinça), mirando no cursor.
        if let Some(hover) = resp.hover_pos() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta().y, i.zoom_delta()));
            let factor = pinch * (scroll * 0.0015).exp();
            if (factor - 1.0).abs() > 1e-4 {
                view.zoom_at(hover - rect.min, factor);
                self.canvas.view = Some(view);
            }
        }

        // Mover: ferramenta Mover, espaço + arrastar, ou botão do meio/direito.
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        let hand = self.canvas.tool == Tool::Move || space;
        let panning = resp.dragged_by(PointerButton::Middle)
            || resp.dragged_by(PointerButton::Secondary)
            || (hand && resp.dragged_by(PointerButton::Primary));
        if panning {
            view.offset += resp.drag_delta();
            self.canvas.view = Some(view);
        }

        // Desenho: base de corte (grade em mm reais), imagem, cotas e réguas.
        // As cores de fundo são transparentes: a base aparece através delas.
        let img_rect = Rect::from_min_size(rect.min + view.offset, image * view.scale);
        let painter = ui.painter_at(rect);
        let piece = self.piece_on_screen(img_rect, view.scale);
        mat::paint_mat(&painter, rect, piece.map(|p| p.scale));
        let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        if self.canvas.edit_opacity < 1.0
            && let Some(img) = &self.image
        {
            painter.image(img.texture.id(), img_rect, uv, Color32::WHITE);
        }
        if let Some(tex) = &self.canvas.texture {
            painter.image(tex.id(), img_rect, uv, Color32::WHITE.gamma_multiply(self.canvas.edit_opacity));
        }
        if let Some(p) = piece {
            mat::paint_dimensions(&painter, p.rect, p.width_mm, p.height_mm);
            mat::paint_rulers(&painter, rect, p.scale);
        }
        pill(&painter, egui::pos2(rect.center().x + mat::RULER / 2.0, rect.max.y - 12.0), self.canvas.tool.hint());

        // Ferramentas.
        let to_image = |p: Pos2| [(p.x - img_rect.min.x) / view.scale, (p.y - img_rect.min.y) / view.scale];
        let primary_down = ui.input(|i| i.pointer.primary_down());
        if !hand && let Some(pos) = resp.interact_pointer_pos() {
            match self.canvas.tool {
                Tool::Brush | Tool::Eraser => {
                    if resp.is_pointer_button_down_on() && primary_down {
                        self.paint_to(to_image(pos));
                    }
                }
                Tool::Fill if resp.clicked_by(PointerButton::Primary) => self.fill_at(to_image(pos)),
                Tool::Select if resp.clicked_by(PointerButton::Primary) => self.select_at(to_image(pos)),
                _ => {}
            }
        }
        if !primary_down {
            self.end_stroke();
        }

        // Cursor: círculo do tamanho do pincel.
        if let Some(hover) = resp.hover_pos() {
            let ctx = ui.ctx();
            if hand {
                ctx.set_cursor_icon(if panning { CursorIcon::Grabbing } else { CursorIcon::Grab });
            } else {
                match self.canvas.tool {
                    Tool::Brush | Tool::Eraser => {
                        ctx.set_cursor_icon(CursorIcon::None);
                        let r = (self.canvas.brush_radius * view.scale).max(2.0);
                        painter.circle_stroke(hover, r + 1.0, egui::Stroke::new(1.0, Color32::BLACK));
                        painter.circle_stroke(hover, r, egui::Stroke::new(1.5, Color32::WHITE));
                    }
                    _ => ctx.set_cursor_icon(CursorIcon::Crosshair),
                }
            }
        }
    }

    /// Onde a peça (área recortada do SVG) aparece na tela e quantos
    /// pixels de tela valem 1 mm. `None` enquanto o SVG não está pronto.
    fn piece_on_screen(&self, img_rect: Rect, view_scale: f32) -> Option<PieceOnScreen> {
        let v = &self.output.as_ref()?.vector;
        let doc = self.doc.as_ref()?;
        // Resultado de uma detecção anterior (outro tamanho): ignora.
        if v.px_width == 0 || v.crop_x + v.px_width > doc.width || v.crop_y + v.px_height > doc.height {
            return None;
        }
        let mm_per_px = (v.width_mm / v.px_width as f64) as f32;
        let origin = img_rect.min + Vec2::new(v.crop_x as f32, v.crop_y as f32) * view_scale;
        let rect = Rect::from_min_size(origin, Vec2::new(v.px_width as f32, v.px_height as f32) * view_scale);
        Some(PieceOnScreen {
            rect,
            scale: mat::Scale { origin, px_per_mm: view_scale / mm_per_px },
            width_mm: v.width_mm,
            height_mm: v.height_mm,
        })
    }

    /// Cor ativa: (índice, cor, é fundo).
    pub(super) fn active_entry(&self) -> Option<(usize, [u8; 3], bool)> {
        let doc = self.doc.as_ref()?;
        let i = self.selected?;
        let e = doc.entries.get(i)?;
        Some((i, e.color, e.background))
    }

    /// Continua (ou começa) o traço do pincel/borracha até `pt`.
    fn paint_to(&mut self, pt: [f32; 2]) {
        let radius = self.canvas.brush_radius;
        if let Some(stroke) = &mut self.canvas.stroke {
            let (from, label) = (stroke.last, stroke.label);
            stroke.last = pt;
            let dirty = self.doc_mut().and_then(|doc| doc.paint_line(from, pt, radius, label));
            self.upload_region(dirty);
            return;
        }

        // Começo do traço: escolhe a cor e registra no histórico.
        if self.canvas.tool == Tool::Brush && self.active_entry().is_none() {
            self.status = "Escolha uma cor na paleta (ou com o conta-gotas) antes de pintar.".into();
            return;
        }
        let tag = EditTag::Unique(self.new_id());
        self.checkpoint(tag);
        let eraser = self.canvas.tool == Tool::Eraser;
        let selected = self.selected;
        let Some(doc) = self.doc_mut() else { return };
        let label = if eraser { doc.ensure_background() } else { selected };
        let Some(label) = label else { return };
        let label = label as u8;
        let dirty = doc.paint_dab(pt[0], pt[1], radius, label);
        self.canvas.stroke = Some(Stroke { last: pt, label });
        self.upload_region(dirty);
    }

    /// Termina o traço: atualiza estatísticas e pede novo SVG.
    pub(super) fn end_stroke(&mut self) {
        if self.canvas.stroke.take().is_none() {
            return;
        }
        if let Some(doc) = self.doc_mut() {
            doc.update_coverage();
        }
        self.edited(false);
    }

    fn fill_at(&mut self, pt: [f32; 2]) {
        let Some((label, ..)) = self.active_entry() else {
            self.status = "Escolha uma cor na paleta antes de usar o balde.".into();
            return;
        };
        let (x, y) = (pt[0].floor() as i64, pt[1].floor() as i64);
        let current = self.doc.as_ref().and_then(|d| d.label_at(x, y));
        if current.is_none() || current == Some(label) {
            return; // fora da imagem, ou já é dessa cor
        }
        let tag = EditTag::Unique(self.new_id());
        self.checkpoint(tag);
        let dirty = self.doc_mut().and_then(|doc| {
            let d = doc.flood_fill(x as u32, y as u32, label as u8);
            doc.update_coverage();
            d
        });
        self.upload_region(dirty);
        self.edited(false);
    }

    fn select_at(&mut self, pt: [f32; 2]) {
        let Some(doc) = &self.doc else { return };
        if let Some(label) = doc.label_at(pt[0].floor() as i64, pt[1].floor() as i64) {
            self.selected = Some(label);
            let e = &doc.entries[label];
            self.status = format!(
                "Cor selecionada: {}{}",
                hex(e.color),
                if e.background { " (fundo)" } else { "" }
            );
        }
    }
}
