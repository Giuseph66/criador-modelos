//! Interface gráfica (egui). egui é "immediate mode": a função `ui` roda
//! a cada frame e redesenha tudo a partir do estado em `CriadorApp`.
//!
//! Organização:
//! - `app.rs` (este): estado, ações (abrir, desfazer, exportar) e painéis.
//! - `app/canvas.rs`: a tela de edição com as ferramentas.
//! - `app/mat.rs`: a base de corte (grade em mm, réguas, cotas).
//! - `app/widgets.rs`: componentes visuais (botões, abas, carretéis...).
//! - `app/theme.rs`: cores, fontes, ícones e estilo.

mod canvas;
mod mat;
mod theme;
mod widgets;

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;
use image::RgbaImage;

use criador_modelos::colors::{self, Document, Rgb8, hex};
use criador_modelos::history::History;
use criador_modelos::mesh::{self, MeshParams};
use criador_modelos::processing::{self, BwParams, ThresholdMode};
use criador_modelos::vectorize::{CurveMode, VectorParams, VectorResult};

use crate::worker::{DetectJob, Detection, Job, JobResult, RenderJob, RenderSettings, Worker};
use canvas::{CanvasState, Tool};
use theme::{ALUMINIUM, CUTTER, GRAPHITE, INK, MAT, MAT_INK, PAPER, PLATE, SIGNAL, bold, display, icons, muted, num};
use widgets::{add_spool, field, ghost, inset, primary, secondary, section, segmented, spool, tab, wordmark};

/// Maior lado da textura usada para mostrar a imagem original.
const ORIGINAL_PREVIEW_MAX: u32 = 2048;
/// Passos de desfazer guardados.
const HISTORY_LIMIT: usize = 60;
/// Cores sugeridas para "Nova cor" (vai alternando).
const NEW_COLORS: [Rgb8; 8] = [
    [229, 57, 53],
    [30, 136, 229],
    [67, 160, 71],
    [253, 216, 53],
    [142, 36, 170],
    [251, 140, 0],
    [0, 172, 193],
    [109, 76, 65],
];

/// Como a imagem vira documento.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Colors,
    BlackWhite,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Edit,
    Original,
    Vector,
    Compare,
}

#[derive(Clone, Copy)]
enum Preset {
    Logo,
    Photo,
    PixelArt,
}

/// Identifica uma alteração para o histórico. Alterações seguidas com a
/// mesma etiqueta viram um passo só (ex.: arrastar o seletor de cor).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EditTag {
    Unique(u64),
    Recolor(usize),
}

struct LoadedImage {
    id: u64,
    path: PathBuf,
    rgba: Arc<RgbaImage>,
    texture: egui::TextureHandle,
}

/// Último SVG pronto.
struct Output {
    vector: VectorResult,
    texture: Option<egui::TextureHandle>,
    elapsed_ms: u128,
}

pub struct CriadorApp {
    worker: Worker,
    image: Option<LoadedImage>,

    // --- Detecção (imagem -> documento) ---
    mode: Mode,
    color_count: usize,
    bw: BwParams,
    max_side: u32,
    /// Última detecção pedida: (id da tarefa, imagem, resolução, método).
    detect_sent: Option<(u64, u64, u32, Detection)>,
    detecting: bool,
    threshold_used: Option<u8>,

    // --- Documento editável ---
    doc: Option<Arc<Document>>,
    history: History<Document>,
    /// Aumenta a cada alteração do documento (para saber quando refazer o SVG).
    doc_version: u64,
    last_edit: Option<EditTag>,
    /// Cor selecionada na paleta (cor ativa do pincel/balde).
    selected: Option<usize>,

    // --- SVG / STL ---
    render: RenderSettings,
    mesh: MeshParams,
    stl_per_color: bool,
    render_sent: Option<(u64, RenderSettings)>,
    pending_render: Option<u64>,
    output: Option<Output>,

    canvas: CanvasState,
    view: View,
    next_id: u64,
    status: String,
}

impl CriadorApp {
    /// `initial`: imagem passada na linha de comando (opcional).
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        theme::apply(&cc.egui_ctx);
        let mut app = Self {
            worker: Worker::spawn(cc.egui_ctx.clone()),
            image: None,
            mode: Mode::Colors,
            color_count: 4,
            bw: BwParams::default(),
            max_side: processing::DEFAULT_MAX_SIDE,
            detect_sent: None,
            detecting: false,
            threshold_used: None,
            doc: None,
            history: History::new(HISTORY_LIMIT),
            doc_version: 0,
            last_edit: None,
            selected: None,
            render: default_render(),
            mesh: MeshParams::default(),
            stl_per_color: false,
            render_sent: None,
            pending_render: None,
            output: None,
            canvas: CanvasState::default(),
            view: View::Edit,
            next_id: 1,
            status: "Abra ou arraste uma imagem para começar.".into(),
        };
        if let Some(path) = initial {
            app.load_image(&cc.egui_ctx, &path);
        }
        app
    }

    fn new_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn detection(&self) -> Detection {
        match self.mode {
            Mode::Colors => Detection::Colors { count: self.color_count },
            Mode::BlackWhite => Detection::BlackWhite(self.bw.clone()),
        }
    }

    // ================= Documento e histórico =================

    /// Registra o estado atual no histórico antes de uma alteração.
    fn checkpoint(&mut self, tag: EditTag) {
        if self.last_edit == Some(tag) {
            return; // continuação da mesma alteração
        }
        if let Some(doc) = &self.doc {
            self.history.push(Arc::clone(doc));
        }
        self.last_edit = Some(tag);
    }

    /// Acesso para alterar o documento. `Arc::make_mut` copia o documento
    /// só se o histórico (ou o worker) ainda estiver usando esta versão.
    fn doc_mut(&mut self) -> Option<&mut Document> {
        self.doc.as_mut().map(Arc::make_mut)
    }

    /// Marca o documento como alterado.
    fn edited(&mut self, full_texture: bool) {
        self.doc_version += 1;
        if full_texture {
            self.canvas.texture_dirty = true;
        }
    }

    /// Garante que a cor selecionada ainda existe.
    fn fix_selection(&mut self) {
        let len = self.doc.as_ref().map_or(0, |d| d.entries.len());
        if self.selected.is_some_and(|i| i >= len) {
            self.selected = None;
        }
    }

    fn undo(&mut self) {
        self.canvas.cancel_stroke();
        let Some(current) = self.doc.clone() else { return };
        if let Some(previous) = self.history.undo(current) {
            self.doc = Some(previous);
            self.after_history_move("Desfeito");
        }
    }

    fn redo(&mut self) {
        self.canvas.cancel_stroke();
        let Some(current) = self.doc.clone() else { return };
        if let Some(next) = self.history.redo(current) {
            self.doc = Some(next);
            self.after_history_move("Refeito");
        }
    }

    fn after_history_move(&mut self, what: &str) {
        self.last_edit = None;
        self.fix_selection();
        self.edited(true);
        self.status = what.into();
    }

    // ================= Ações =================

    fn open_dialog(&mut self, ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .add_filter("Imagens", &["png", "jpg", "jpeg", "webp", "bmp", "gif"])
            .pick_file();
        if let Some(path) = picked {
            self.load_image(ctx, &path);
        }
    }

    fn load_image(&mut self, ctx: &egui::Context, path: &Path) {
        // `match` em `Result`: trata sucesso e erro explicitamente.
        let rgba = match image::open(path) {
            Ok(img) => img.to_rgba8(),
            Err(e) => {
                self.status = format!("Erro ao abrir {}: {e}", path.display());
                return;
            }
        };

        let preview_w = rgba.width().min(ORIGINAL_PREVIEW_MAX);
        let preview_h = ((rgba.height() as u64 * preview_w as u64) / rgba.width().max(1) as u64)
            .clamp(1, ORIGINAL_PREVIEW_MAX as u64) as u32;
        let thumb = image::imageops::thumbnail(&rgba, preview_w, preview_h);
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [thumb.width() as usize, thumb.height() as usize],
            thumb.as_raw(),
        );
        let texture = ctx.load_texture("original", color, egui::TextureOptions::LINEAR);

        self.status = format!(
            "{} ({}×{} px)",
            path.file_name().unwrap_or_default().to_string_lossy(),
            rgba.width(),
            rgba.height()
        );
        let id = self.new_id();
        self.image = Some(LoadedImage { id, path: path.to_path_buf(), rgba: Arc::new(rgba), texture });
        self.doc = None;
        self.history.clear();
        self.last_edit = None;
        self.selected = None;
        self.output = None;
        self.detect_sent = None;
        self.render_sent = None;
        self.canvas.view = None;
        self.canvas.cancel_stroke();
        self.canvas.texture_dirty = true;
    }

    /// Envia ao worker o que mudou (detecção ou SVG). Chamado todo frame.
    fn sync_jobs(&mut self) {
        if let Some(img) = &self.image {
            let detection = self.detection();
            let same = self.detect_sent.as_ref().is_some_and(|(_, image_id, side, det)| {
                *image_id == img.id && *side == self.max_side && *det == detection
            });
            if !same {
                let (image_id, image) = (img.id, Arc::clone(&img.rgba));
                let id = self.new_id();
                self.worker.submit(Job::Detect(DetectJob {
                    id,
                    image_id,
                    image,
                    max_side: self.max_side,
                    detection: detection.clone(),
                }));
                self.detect_sent = Some((id, image_id, self.max_side, detection));
                self.detecting = true;
            }
        }

        // SVG: só quando não está no meio de um traço nem detectando.
        if let Some(doc) = &self.doc
            && !self.canvas.is_stroking()
            && !self.detecting
        {
            let key = (self.doc_version, self.render.clone());
            if self.render_sent.as_ref() != Some(&key) {
                let doc = Arc::clone(doc);
                let id = self.new_id();
                self.worker.submit(Job::Render(RenderJob { id, doc, settings: self.render.clone() }));
                self.pending_render = Some(id);
                self.render_sent = Some(key);
            }
        }
    }

    fn receive_results(&mut self, ctx: &egui::Context) {
        for result in self.worker.poll() {
            match result {
                JobResult::Detected { id, image_id, doc, threshold_used } => {
                    // Ignora resultados velhos (outra imagem ou opções antigas).
                    let latest = self.detect_sent.as_ref().map(|d| d.0);
                    if latest != Some(id) || self.image.as_ref().map(|i| i.id) != Some(image_id) {
                        continue;
                    }
                    self.detecting = false;
                    self.threshold_used = threshold_used;
                    // Re-detecção substitui as edições, mas dá pra desfazer.
                    if let Some(old) = self.doc.take() {
                        self.history.push(old);
                    }
                    let same_size = self.canvas.texture.as_ref().is_some_and(|t| {
                        t.size() == [doc.width as usize, doc.height as usize]
                    });
                    if !same_size {
                        self.canvas.view = None;
                    }
                    self.selected = doc.entries.iter().position(|e| !e.background);
                    self.doc = Some(Arc::new(doc));
                    self.last_edit = None;
                    self.canvas.cancel_stroke();
                    self.edited(true);
                }
                JobResult::Rendered { id, vector, preview, elapsed } => {
                    if self.pending_render == Some(id) {
                        self.pending_render = None;
                    }
                    let texture = preview.map(|img| ctx.load_texture("vetor", img, egui::TextureOptions::LINEAR));
                    self.output = Some(Output { vector, texture, elapsed_ms: elapsed.as_millis() });
                }
            }
        }
    }

    /// Resultado pronto e atualizado, ou mensagem de espera.
    fn ready_output(&mut self) -> bool {
        if self.output.is_none() || self.pending_render.is_some() || self.detecting {
            self.status = "Aguarde o processamento terminar e tente de novo.".into();
            return false;
        }
        true
    }

    fn default_save_name(&self, ext: &str) -> String {
        let stem = self
            .image
            .as_ref()
            .and_then(|i| i.path.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "modelo".into());
        format!("{stem}.{ext}")
    }

    fn save_dialog(&self, filter: &str, ext: &str, name_ext: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter(filter, &[ext])
            .set_file_name(self.default_save_name(name_ext))
            .save_file()
    }

    fn save_svg(&mut self) {
        if !self.ready_output() {
            return;
        }
        let Some(path) = self.save_dialog("SVG", "svg", "svg") else { return };
        let Some(out) = &self.output else { return };
        self.status = match std::fs::write(&path, &out.vector.svg) {
            Ok(()) => format!("SVG salvo em {}", path.display()),
            Err(e) => format!("Erro ao salvar SVG: {e}"),
        };
    }

    /// Um SVG por camada: `nome_camada1_rrggbb.svg`, ...
    fn save_layer_svgs(&mut self) {
        if !self.ready_output() {
            return;
        }
        let Some(path) = self.save_dialog("SVG", "svg", "svg") else { return };
        let Some(out) = &self.output else { return };
        let files: Vec<(String, &str)> = out
            .vector
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| (layer_file_name(&path, i, l.color, "svg"), l.svg.as_str()))
            .collect();
        self.status = write_files(&path, &files, "camadas SVG");
    }

    fn save_png(&mut self) {
        let Some(doc) = self.doc.clone() else { return };
        let Some(path) = self.save_dialog("PNG", "png", "editada.png") else { return };
        self.status = match doc.to_rgb().save(&path) {
            Ok(()) => format!("PNG salvo em {}", path.display()),
            Err(e) => format!("Erro ao salvar PNG: {e}"),
        };
    }

    fn export_stl(&mut self) {
        if !self.ready_output() {
            return;
        }
        let Some(out) = &self.output else { return };
        if out.vector.geometry.iter().all(|g| g.shapes.is_empty()) {
            self.status = "Nada para exportar: todas as cores estão como fundo.".into();
            return;
        }
        let Some(path) = self.save_dialog("STL", "stl", "stl") else { return };
        let Some(out) = &self.output else { return };

        let params = MeshParams { stacked: self.render.stacked, ..self.mesh.clone() };
        let meshes = mesh::build_meshes(&out.vector, &params);
        let total: usize = meshes.iter().map(|m| m.triangles.len()).sum();
        let size = format!(
            "{:.1} × {:.1} × {:.1} mm",
            out.vector.width_mm,
            out.vector.height_mm,
            params.total_height(meshes.len())
        );

        let result = if self.stl_per_color {
            meshes.iter().enumerate().try_for_each(|(i, m)| {
                write_stl_file(Path::new(&layer_file_name(&path, i, m.color, "stl")), &m.triangles)
            })
        } else {
            let all: Vec<_> = meshes.iter().flat_map(|m| m.triangles.iter().copied()).collect();
            write_stl_file(&path, &all)
        };
        self.status = match result {
            Ok(()) => format!("STL salvo ({total} triângulos, {size}) em {}", path.display()),
            Err(e) => format!("Erro ao salvar STL: {e}"),
        };
    }

    fn apply_preset(&mut self, preset: Preset) {
        let width_mm = self.render.vector.width_mm; // tamanho é escolha do usuário, mantém.
        match preset {
            Preset::Logo => {
                self.bw = BwParams { mode: ThresholdMode::Otsu, blur_sigma: 0.8, ..Default::default() };
                self.render.vector = VectorParams::default();
            }
            Preset::Photo => {
                self.bw = BwParams {
                    mode: ThresholdMode::Adaptive,
                    blur_sigma: 1.5,
                    adaptive_sensitivity: 12.0,
                    ..Default::default()
                };
                self.render.vector = VectorParams { filter_speckle: 8, length_threshold: 6.0, ..Default::default() };
            }
            Preset::PixelArt => {
                self.bw = BwParams { mode: ThresholdMode::Otsu, blur_sigma: 0.0, ..Default::default() };
                self.render.vector = VectorParams { mode: CurveMode::Pixel, filter_speckle: 1, ..Default::default() };
            }
        }
        self.render.vector.width_mm = width_mm;
    }

    // ================= Paleta =================

    fn add_color(&mut self) {
        let Some(doc) = &self.doc else { return };
        let color = NEW_COLORS[doc.entries.len() % NEW_COLORS.len()];
        let tag = EditTag::Unique(self.new_id());
        self.checkpoint(tag);
        match self.doc_mut().and_then(|d| d.add_color(color, false)) {
            Some(i) => {
                self.selected = Some(i);
                self.canvas.tool = Tool::Brush;
                self.edited(true);
                self.status = "Nova cor criada: pinte com o pincel ou o balde. Troque a cor no seletor.".into();
            }
            None => self.status = format!("A paleta já tem o máximo de {} cores.", colors::MAX_COLORS),
        }
    }

    fn palette_ui(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = self.doc.clone() else {
            if self.image.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(muted("Detectando cores…"));
                });
            }
            return;
        };

        ui.label(muted("Cada carretel é uma cor da peça. Clique para pintar com ela ou editá-la.").size(12.0));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
            for (i, e) in doc.entries.iter().enumerate() {
                let mut tip = format!("{}\n{}% da imagem", hex(e.color).to_uppercase(), num(e.coverage as f64 * 100.0));
                if e.background {
                    tip.push_str("\nFundo: não entra no modelo");
                }
                if spool(ui, e.color, 30.0, self.selected == Some(i), e.background).on_hover_text(tip).clicked() {
                    self.selected = Some(i);
                }
            }
            if add_spool(ui, 30.0).on_hover_text("Adicionar uma cor nova").clicked() {
                self.add_color();
            }
        });

        let Some(i) = self.selected.filter(|&i| i < doc.entries.len()) else { return };
        let entry = doc.entries[i].clone();
        ui.add_space(4.0);

        inset(ui, |ui| {
            // Editamos uma cópia; se mudou, registra no histórico e aplica.
            let mut color = entry.color;
            ui.horizontal(|ui| {
                ui.color_edit_button_srgb(&mut color).on_hover_text("Trocar a cor");
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(bold(hex(color).to_uppercase()).color(INK));
                    ui.label(muted(format!("{}% da imagem", num(entry.coverage as f64 * 100.0))).size(11.5));
                });
                if entry.color != entry.original {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ghost(ui, icons::ARROW_COUNTER_CLOCKWISE, "")
                            .on_hover_text("Voltar para a cor detectada")
                            .clicked()
                        {
                            color = entry.original;
                        }
                    });
                }
            });
            if color != entry.color {
                self.checkpoint(EditTag::Recolor(i));
                if let Some(d) = self.doc_mut() {
                    d.entries[i].color = color;
                }
                self.edited(true);
            }

            let mut background = entry.background;
            segmented(
                ui,
                &mut background,
                &[(false, icons::CUBE, "Na peça"), (true, icons::EYE_SLASH, "Fundo")],
                true,
            );
            if background != entry.background {
                let tag = EditTag::Unique(self.new_id());
                self.checkpoint(tag);
                if let Some(d) = self.doc_mut() {
                    d.entries[i].background = background;
                }
                self.edited(true);
            }

            ui.horizontal_wrapped(|ui| {
                if secondary(ui, icons::PAINT_BRUSH, "Pincel").on_hover_text("Pintar com esta cor (B)").clicked() {
                    self.canvas.tool = Tool::Brush;
                    self.view = View::Edit;
                }
                if secondary(ui, icons::PAINT_BUCKET, "Balde")
                    .on_hover_text("Pintar áreas inteiras com esta cor (G)")
                    .clicked()
                {
                    self.canvas.tool = Tool::Fill;
                    self.view = View::Edit;
                }
                if !entry.background
                    && ghost(ui, icons::TRASH, "Apagar").on_hover_text("Os pixels desta cor viram fundo").clicked()
                {
                    let tag = EditTag::Unique(self.new_id());
                    self.checkpoint(tag);
                    if let Some(d) = self.doc_mut() {
                        d.remove_color(i);
                    }
                    self.selected = None;
                    self.edited(true);
                }
            });

            ui.horizontal(|ui| {
                ui.label(muted("Juntar com"));
                let mut merge_into = None;
                egui::ComboBox::from_id_salt("juntar")
                    .selected_text("escolher cor…")
                    .show_ui(ui, |ui| {
                        for (j, other) in doc.entries.iter().enumerate().filter(|&(j, _)| j != i) {
                            ui.horizontal(|ui| {
                                spool(ui, other.color, 14.0, false, other.background);
                                let label = format!(
                                    "{}{}",
                                    hex(other.color).to_uppercase(),
                                    if other.background { " (fundo)" } else { "" }
                                );
                                if ui.selectable_label(false, label).clicked() {
                                    merge_into = Some(j);
                                }
                            });
                        }
                    })
                    .response
                    .on_hover_text("Os pixels desta cor passam para a outra (ex.: contorno fino junto com o preto)");
                if let Some(j) = merge_into {
                    let tag = EditTag::Unique(self.new_id());
                    self.checkpoint(tag);
                    if let Some(d) = self.doc_mut() {
                        d.merge(i, j);
                    }
                    // Índices depois de `i` andaram uma posição.
                    self.selected = Some(if j > i { j - 1 } else { j });
                    self.edited(true);
                }
            });
        });
    }

    /// Lista das camadas que vão para o SVG/STL, com a altura de cada uma.
    fn layers_ui(&mut self, ui: &mut egui::Ui) {
        let groups = match &self.doc {
            Some(doc) => colors::layer_groups(&doc.entries),
            None => return,
        };
        let params = self.mesh_params();

        let select = section(ui, icons::STACK, "Camadas", |ui| {
            if groups.is_empty() {
                ui.colored_label(SIGNAL, "Nenhuma camada: todas as cores estão como fundo.");
                return None;
            }
            ui.label(muted("De baixo para cima, com a altura de cada uma no STL.").size(12.0));
            let mut select = None;
            for (n, g) in groups.iter().enumerate() {
                let (z0, z1) = params.z_range(n);
                ui.horizontal(|ui| {
                    if spool(ui, g.color, 18.0, false, false).on_hover_text("Selecionar esta cor").clicked() {
                        select = g.members.first().copied();
                    }
                    ui.label(if n == 0 { bold("Base") } else { bold(format!("Camada {}", n + 1)) });
                    ui.label(muted(format!("{}%", num(g.coverage as f64 * 100.0))).size(12.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(muted(format!("{} a {} mm", num(z0), num(z1))).size(12.0));
                    });
                });
            }
            select
        });
        if select.is_some() {
            self.selected = select;
        }
    }

    fn mesh_params(&self) -> MeshParams {
        MeshParams { stacked: self.render.stacked, ..self.mesh.clone() }
    }

    // ================= Painéis =================

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            wordmark(ui);
            ui.add_space(20.0);

            if ghost(ui, icons::FOLDER_OPEN, "Abrir")
                .on_hover_text("Abrir imagem (Ctrl+O). Também dá para arrastar o arquivo para a janela.")
                .clicked()
            {
                self.open_dialog(ui.ctx());
            }
            let (can_undo, can_redo) = (self.history.can_undo(), self.history.can_redo());
            ui.add_enabled_ui(can_undo, |ui| {
                if ghost(ui, icons::ARROW_COUNTER_CLOCKWISE, "").on_hover_text("Desfazer (Ctrl+Z)").clicked() {
                    self.undo();
                }
            });
            ui.add_enabled_ui(can_redo, |ui| {
                if ghost(ui, icons::ARROW_CLOCKWISE, "").on_hover_text("Refazer (Ctrl+Y)").clicked() {
                    self.redo();
                }
            });

            if self.image.is_some() {
                ui.add_space(20.0);
                let tabs = [
                    (View::Edit, icons::PENCIL_SIMPLE, "Editar"),
                    (View::Original, icons::IMAGE, "Original"),
                    (View::Vector, icons::BEZIER_CURVE, "Vetor"),
                    (View::Compare, icons::COLUMNS, "Comparar"),
                ];
                for (view, icon, label) in tabs {
                    if tab(ui, icon, label, self.view == view).clicked() {
                        self.view = view;
                    }
                }
            }

            // Grupo da direita (a ordem é de fora para dentro).
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.add_enabled_ui(self.doc.is_some(), |ui| {
                    if primary(ui, icons::CUBE, "Exportar STL")
                        .on_hover_text("Modelo 3D para impressão (Ctrl+E)")
                        .clicked()
                    {
                        self.export_stl();
                    }
                    if secondary(ui, icons::FILE_SVG, "Salvar SVG")
                        .on_hover_text("Vetor para o slicer (Ctrl+S)")
                        .clicked()
                    {
                        self.save_svg();
                    }
                    ui.menu_button(egui::RichText::new(icons::DOTS_THREE).size(18.0).color(GRAPHITE), |ui| {
                        if ui.button(format!("{}  Um SVG por cor", icons::FILES)).clicked() {
                            self.save_layer_svgs();
                            ui.close();
                        }
                        if ui.button(format!("{}  Imagem editada (PNG)", icons::IMAGE)).clicked() {
                            self.save_png();
                            ui.close();
                        }
                    });
                });
            });
        });
    }

    /// Inspetor: propriedades à direita, seções separadas por linhas finas.
    fn inspector(&mut self, ui: &mut egui::Ui) {
        section(ui, icons::MAGIC_WAND, "Detecção", |ui| {
            segmented(
                ui,
                &mut self.mode,
                &[(Mode::Colors, icons::PALETTE, "Cores"), (Mode::BlackWhite, icons::CIRCLE_HALF, "Preto e branco")],
                true,
            );
            match self.mode {
                Mode::Colors => {
                    ui.add(egui::Slider::new(&mut self.color_count, 2..=12).text("cores")).on_hover_text(
                        "Quantas cores detectar. Aumente se algum detalhe sumiu; diminua se uma cor se dividiu em várias",
                    );
                }
                Mode::BlackWhite => self.bw_controls(ui),
            }
            ui.add(egui::Slider::new(&mut self.max_side, 200..=3000).text("resolução").suffix(" px").step_by(100.0))
                .on_hover_text("Maior lado usado para editar e vetorizar. Menos = mais simples e rápido");
            if self.history.can_undo() {
                ui.label(muted("Mudar a detecção refaz as cores e substitui suas edições. Ctrl+Z desfaz.").size(11.5));
            }
        });

        section(ui, icons::PALETTE, "Cores", |ui| self.palette_ui(ui));

        section(ui, icons::BEZIER_CURVE, "Contorno", |ui| {
            ui.add(egui::Slider::new(&mut self.render.smooth_sigma, 0.0..=5.0).text("suavizar"))
                .on_hover_text("Arredonda serrilhados e fecha falhas pequenas nas bordas");
            ui.checkbox(&mut self.render.stacked, "Empilhar camadas (base sólida)").on_hover_text(
                "Cada camada também preenche a área das camadas acima dela.\n\
                 Ideal para chaveiro: a base fica inteira, sem buraco onde fica o texto.",
            );
            let v = &mut self.render.vector;
            segmented(
                ui,
                &mut v.mode,
                &[
                    (CurveMode::Spline, icons::BEZIER_CURVE, "Curvas"),
                    (CurveMode::Polygon, icons::POLYGON, "Retas"),
                    (CurveMode::Pixel, icons::SQUARES_FOUR, "Pixel"),
                ],
                true,
            );
            ui.add(egui::Slider::new(&mut v.filter_speckle, 0..=64).text("manchas"))
                .on_hover_text("Ignora manchas e fecha furos menores que N×N pixels");
            if v.mode == CurveMode::Spline {
                ui.add(egui::Slider::new(&mut v.corner_threshold_deg, 0.0..=180.0).text("quinas").suffix("°"))
                    .on_hover_text("Menor = mais quinas vivas; maior = mais arredondado");
            }
            if v.mode != CurveMode::Pixel {
                ui.add(egui::Slider::new(&mut v.length_threshold, 3.5..=10.0).text("simplificar"))
                    .on_hover_text("Maior = menos nós, contorno mais simples");
            }
        });

        self.layers_ui(ui);

        section(ui, icons::RULER, "Medidas", |ui| {
            if let Some(out) = &self.output {
                let height = self.mesh_params().total_height(out.vector.geometry.len());
                ui.horizontal(|ui| {
                    ui.label(
                        display(format!(
                            "{} × {} × {}",
                            num(out.vector.width_mm),
                            num(out.vector.height_mm),
                            num(height)
                        ))
                        .size(22.0)
                        .color(INK),
                    );
                    ui.label(muted("mm"));
                });
                ui.label(muted("largura × altura × espessura da peça").size(11.5));
                ui.add_space(4.0);
            }
            egui::Grid::new("medidas").num_columns(2).spacing([16.0, 7.0]).show(ui, |ui| {
                ui.label("Largura");
                ui.add(egui::DragValue::new(&mut self.render.vector.width_mm).range(1.0..=1000.0).speed(0.5).suffix(" mm"))
                    .on_hover_text("Largura do desenho (as margens vazias são cortadas)");
                ui.end_row();
                ui.label("Base");
                ui.add(egui::DragValue::new(&mut self.mesh.base_mm).range(0.2..=50.0).speed(0.1).suffix(" mm"))
                    .on_hover_text("Espessura da camada de baixo (a base do chaveiro)");
                ui.end_row();
                ui.label("Relevo");
                ui.add(egui::DragValue::new(&mut self.mesh.relief_mm).range(0.2..=20.0).speed(0.1).suffix(" mm"))
                    .on_hover_text("Quanto cada camada de cima sobe em relação à de baixo");
                ui.end_row();
            });
            ui.checkbox(&mut self.stl_per_color, "Um STL por cor").on_hover_text(
                "Um arquivo por camada. Importe todos juntos no slicer e escolha um filamento para cada.",
            );
        });

        if ghost(ui, icons::ARROW_COUNTER_CLOCKWISE, "Restaurar padrões")
            .on_hover_text("Volta as opções de contorno e medidas; suas edições continuam")
            .clicked()
        {
            self.render = RenderSettings {
                vector: VectorParams { width_mm: self.render.vector.width_mm, ..Default::default() },
                ..default_render()
            };
            self.mesh = MeshParams::default();
        }
        ui.add_space(10.0);
    }

    fn bw_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(muted("Partir de"));
            if secondary(ui, "", "Logo").clicked() {
                self.apply_preset(Preset::Logo);
            }
            if secondary(ui, "", "Foto").clicked() {
                self.apply_preset(Preset::Photo);
            }
            if secondary(ui, "", "Pixel art").clicked() {
                self.apply_preset(Preset::PixelArt);
            }
        });
        segmented(
            ui,
            &mut self.bw.mode,
            &[
                (ThresholdMode::Otsu, "", "Automático"),
                (ThresholdMode::Fixed, "", "Manual"),
                (ThresholdMode::Adaptive, "", "Adaptativo"),
            ],
            true,
        );
        match self.bw.mode {
            ThresholdMode::Fixed => {
                ui.add(egui::Slider::new(&mut self.bw.threshold, 1..=254).text("limiar"));
            }
            ThresholdMode::Adaptive => {
                ui.add(egui::Slider::new(&mut self.bw.adaptive_sensitivity, 1.0..=40.0).text("sensibilidade").suffix("%"));
            }
            ThresholdMode::Otsu => {
                if let Some(t) = self.threshold_used {
                    ui.label(muted(format!("Limiar calculado: {t}")).size(12.0));
                }
            }
        }
        ui.add(egui::Slider::new(&mut self.bw.blur_sigma, 0.0..=5.0).text("desfocar"))
            .on_hover_text("Desfoque antes do corte: menos ruído");
        ui.checkbox(&mut self.bw.invert, "Inverter claro e escuro");
    }

    /// Tela inicial: a bancada vazia, esperando uma imagem.
    fn welcome(&mut self, ui: &mut egui::Ui) {
        let area = ui.available_rect_before_wrap();
        mat::paint_mat(ui.painter(), area, None);
        ui.vertical_centered(|ui| {
            ui.add_space((area.height() * 0.32).max(24.0));
            ui.label(egui::RichText::new(icons::IMAGE_SQUARE).size(44.0).color(CUTTER));
            ui.add_space(6.0);
            ui.label(display("Solte uma imagem na bancada").size(30.0).color(PAPER));
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("Detecte as cores, ajuste à mão e exporte SVG ou STL no tamanho certo.")
                    .size(14.5)
                    .color(MAT_INK),
            );
            ui.add_space(20.0);
            if primary(ui, icons::FOLDER_OPEN, "Abrir imagem").clicked() {
                self.open_dialog(ui.ctx());
            }
            ui.add_space(10.0);
            ui.label(egui::RichText::new("PNG, JPG, WebP, BMP ou GIF").size(12.0).color(MAT_INK));
        });
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        if self.image.is_none() {
            self.welcome(ui);
            return;
        }
        match self.view {
            View::Edit => {
                self.options_bar(ui);
                ui.add_space(6.0);
                self.editor_canvas(ui);
            }
            View::Original => mat::board(ui, self.image.as_ref().map(|i| &i.texture)),
            View::Vector => mat::board(ui, self.output.as_ref().and_then(|o| o.texture.as_ref())),
            View::Compare => {
                let boards = [
                    ("Original", self.image.as_ref().map(|i| &i.texture)),
                    ("Cores editadas", self.canvas.texture.as_ref()),
                    ("Vetor", self.output.as_ref().and_then(|o| o.texture.as_ref())),
                ];
                ui.columns(3, |cols| {
                    for (col, (title, tex)) in cols.iter_mut().zip(boards) {
                        col.label(display(title).size(14.0).color(INK));
                        mat::board(col, tex);
                    }
                });
            }
        }
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            if self.detecting {
                ui.add(egui::Spinner::new().size(12.0));
                ui.label(muted("Detectando cores").size(12.0));
                ui.add_space(14.0);
            } else if self.pending_render.is_some() || self.canvas.is_stroking() {
                ui.add(egui::Spinner::new().size(12.0));
                ui.label(muted("Atualizando vetor").size(12.0));
                ui.add_space(14.0);
            }
            if let Some(out) = &self.output {
                let v = &out.vector;
                let height = self.mesh_params().total_height(v.geometry.len());
                field(ui, "Peça", &format!("{} × {} × {} mm", num(v.width_mm), num(v.height_mm), num(height)));
                ui.add_space(14.0);
                field(ui, "Camadas", &v.layers.len().to_string());
                ui.add_space(14.0);
                field(ui, "Formas", &v.path_count.to_string());
                ui.add_space(14.0);
                field(ui, "SVG", &format!("{} KB", v.svg.len().div_ceil(1024)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(muted(&self.status).size(12.0));
            });
        });
    }

    // ================= Teclado e arquivos arrastados =================

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;

        if ctx.input_mut(|i| i.consume_key(cmd, Key::O)) {
            self.open_dialog(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::S)) {
            self.save_svg();
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::E)) {
            self.export_stl();
        }

        // Com um campo de texto ativo, as teclas são dele.
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        // Mais específico primeiro: Ctrl+Shift+Z antes de Ctrl+Z.
        if ctx.input_mut(|i| i.consume_key(cmd_shift, Key::Z) || i.consume_key(cmd, Key::Y)) {
            self.redo();
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::Z)) {
            self.undo();
        }

        let none = Modifiers::NONE;
        for tool in Tool::ALL {
            if ctx.input_mut(|i| i.consume_key(none, tool.key())) {
                self.canvas.tool = tool;
                self.view = View::Edit;
            }
        }
        if ctx.input_mut(|i| i.consume_key(none, Key::OpenBracket)) {
            self.canvas.grow_brush(1.0 / 1.2);
        }
        if ctx.input_mut(|i| i.consume_key(none, Key::CloseBracket)) {
            self.canvas.grow_brush(1.2);
        }
        let image = self.doc.as_ref().map_or(egui::Vec2::splat(1.0), |d| egui::vec2(d.width as f32, d.height as f32));
        if ctx.input_mut(|i| i.consume_key(none, Key::Plus) || i.consume_key(none, Key::Equals)) {
            self.canvas.zoom_by(1.25, image);
        }
        if ctx.input_mut(|i| i.consume_key(none, Key::Minus)) {
            self.canvas.zoom_by(1.0 / 1.25, image);
        }
        if ctx.input_mut(|i| i.consume_key(none, Key::Num0)) {
            self.canvas.view = None;
        }
    }

    /// Aviso por cima de tudo enquanto um arquivo está sendo arrastado.
    fn drop_overlay(&self, ctx: &egui::Context) {
        if ctx.input(|i| i.raw.hovered_files.is_empty()) {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("drop-overlay")));
        let rect = ctx.content_rect();
        painter.rect_filled(rect, 0.0, MAT.gamma_multiply(0.94));
        let frame = rect.shrink(20.0);
        let outline = vec![frame.left_top(), frame.right_top(), frame.right_bottom(), frame.left_bottom(), frame.left_top()];
        painter.extend(egui::Shape::dashed_line(&outline, egui::Stroke::new(2.0, CUTTER), 10.0, 7.0));
        painter.text(
            rect.center() - egui::vec2(0.0, 28.0),
            egui::Align2::CENTER_CENTER,
            icons::DOWNLOAD_SIMPLE,
            egui::FontId::proportional(46.0),
            CUTTER,
        );
        painter.text(
            rect.center() + egui::vec2(0.0, 24.0),
            egui::Align2::CENTER_CENTER,
            "Solte para abrir na bancada",
            egui::FontId::new(26.0, theme::display_family()),
            PAPER,
        );
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped.first() {
            let path = file.path().to_path_buf();
            self.load_image(ctx, &path);
        }
    }
}

fn default_render() -> RenderSettings {
    RenderSettings { smooth_sigma: 1.0, stacked: true, vector: VectorParams::default() }
}

/// `pasta/nome_camada2_ff00aa.ext` a partir de `pasta/nome.ext`.
fn layer_file_name(base: &Path, index: usize, color: Rgb8, ext: &str) -> String {
    let dir = base.parent().unwrap_or(Path::new("."));
    let stem = base.file_stem().unwrap_or_default().to_string_lossy();
    let name = format!("{stem}_camada{}_{}.{ext}", index + 1, hex(color).trim_start_matches('#'));
    dir.join(name).to_string_lossy().into_owned()
}

/// Grava vários arquivos de texto; devolve a mensagem de status.
fn write_files(base: &Path, files: &[(String, &str)], what: &str) -> String {
    for (path, content) in files {
        if let Err(e) = std::fs::write(path, content) {
            return format!("Erro ao salvar {path}: {e}");
        }
    }
    let dir = base.parent().unwrap_or(Path::new("."));
    format!("{} {what} salvas em {}", files.len(), dir.display())
}

fn write_stl_file(path: &Path, triangles: &[mesh::Triangle]) -> std::io::Result<()> {
    mesh::write_stl(triangles, BufWriter::new(File::create(path)?))
}

impl eframe::App for CriadorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_dropped_files(&ctx);
        self.receive_results(&ctx);
        self.handle_shortcuts(&ctx);
        // Soltou o botão (mesmo fora da área de edição): termina o traço.
        if !ctx.input(|i| i.pointer.primary_down()) {
            self.end_stroke();
        }

        let frame = |fill, x, y| egui::Frame::new().fill(fill).inner_margin(egui::Margin::symmetric(x, y));

        egui::Panel::top("barra").frame(frame(ALUMINIUM, 14, 0)).show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status").frame(frame(ALUMINIUM, 14, 5)).show(ui, |ui| self.status_bar(ui));
        if self.image.is_some() {
            egui::Panel::left("ferramentas")
                .frame(frame(ALUMINIUM, 8, 10))
                .resizable(false)
                .exact_size(56.0)
                .show(ui, |ui| self.tool_rail(ui));
            egui::Panel::right("inspetor")
                .frame(frame(PLATE, 16, 6))
                .resizable(true)
                .default_size(330.0)
                .min_size(300.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| self.inspector(ui));
                });
        }
        egui::CentralPanel::default().frame(frame(ALUMINIUM, 10, 10)).show(ui, |ui| self.central(ui));

        self.drop_overlay(&ctx);
        self.sync_jobs();
    }
}
