//! Roda o trabalho pesado numa thread separada para a janela não travar.
//!
//! Dois tipos de tarefa:
//! - `Detect`: imagem -> documento editável (detecta cores ou faz P&B).
//! - `Render`: documento -> camadas -> SVG + geometria + prévia.
//!
//! Comunicação por canais (`mpsc`). Se chegarem várias tarefas enquanto uma
//! roda, só a mais nova de cada tipo é feita.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use image::{RgbImage, RgbaImage};

use criador_modelos::colors::{self, Document};
use criador_modelos::processing::{self, BwParams};
use criador_modelos::vectorize::{self, VectorParams, VectorResult};

/// Tamanho máximo (px) da prévia renderizada do SVG.
const PREVIEW_MAX_PX: f32 = 1400.0;

/// Como transformar a imagem em documento.
#[derive(Clone, PartialEq, Debug)]
pub enum Detection {
    /// Detecta `count` cores (k-means).
    Colors { count: usize },
    /// Corte claro/escuro: duas cores (fundo branco + preto).
    BlackWhite(BwParams),
}

pub struct DetectJob {
    pub id: u64,
    pub image_id: u64,
    /// `Arc` = ponteiro com contagem de referência: compartilha a imagem
    /// entre threads sem copiar os bytes.
    pub image: Arc<RgbaImage>,
    pub max_side: u32,
    pub detection: Detection,
}

/// Configurações que afetam o SVG.
#[derive(Clone, PartialEq, Debug)]
pub struct RenderSettings {
    /// Suavização das bordas de cada camada.
    pub smooth_sigma: f32,
    /// Camadas empilhadas (base sólida por baixo das de cima).
    pub stacked: bool,
    pub vector: VectorParams,
}

pub struct RenderJob {
    pub id: u64,
    pub doc: Arc<Document>,
    pub settings: RenderSettings,
}

pub enum Job {
    Detect(DetectJob),
    Render(RenderJob),
}

pub enum JobResult {
    Detected {
        id: u64,
        image_id: u64,
        doc: Document,
        /// Limiar usado no modo P&B automático (para mostrar na tela).
        threshold_used: Option<u8>,
    },
    Rendered {
        id: u64,
        vector: VectorResult,
        preview: Option<egui::ColorImage>,
        elapsed: Duration,
    },
}

pub struct Worker {
    jobs: Sender<Job>,
    results: Receiver<JobResult>,
}

impl Worker {
    pub fn spawn(ctx: egui::Context) -> Self {
        let (job_tx, job_rx) = channel::<Job>();
        let (res_tx, res_rx) = channel::<JobResult>();

        thread::spawn(move || {
            // Imagem redimensionada da última detecção: (image_id, max_side).
            let mut rgb_cache: Option<((u64, u32), RgbImage)> = None;

            // `recv` bloqueia até chegar tarefa; termina quando a UI fecha.
            while let Ok(first) = job_rx.recv() {
                // Junta tudo que está na fila e fica só com o mais novo de cada tipo.
                let mut detect = None;
                let mut render = None;
                for job in std::iter::once(first).chain(job_rx.try_iter()) {
                    match job {
                        Job::Detect(j) => detect = Some(j),
                        Job::Render(j) => render = Some(j),
                    }
                }

                // Detecção gera documento novo; a UI pede outro render depois.
                let result = match (detect, render) {
                    (Some(j), _) => run_detect(j, &mut rgb_cache),
                    (None, Some(j)) => run_render(j),
                    (None, None) => continue,
                };
                if res_tx.send(result).is_err() {
                    break; // UI fechou.
                }
                ctx.request_repaint();
            }
        });

        Self { jobs: job_tx, results: res_rx }
    }

    pub fn submit(&self, job: Job) {
        self.jobs.send(job).ok();
    }

    /// Resultados prontos (sem bloquear).
    pub fn poll(&self) -> Vec<JobResult> {
        self.results.try_iter().collect()
    }
}

fn run_detect(job: DetectJob, cache: &mut Option<((u64, u32), RgbImage)>) -> JobResult {
    let key = (job.image_id, job.max_side);
    if cache.as_ref().map(|(k, _)| *k) != Some(key) {
        *cache = Some((key, processing::prepare_rgb(&job.image, job.max_side)));
    }
    let rgb = &cache.as_ref().expect("preenchido acima").1;

    let (doc, threshold_used) = match &job.detection {
        Detection::Colors { count } => (Document::from_quantized(colors::quantize(rgb, *count)), None),
        Detection::BlackWhite(params) => {
            let bw = processing::to_black_and_white(&processing::to_gray(rgb), params);
            (Document::from_mask(&bw.mask), bw.threshold_used)
        }
    };
    JobResult::Detected { id: job.id, image_id: job.image_id, doc, threshold_used }
}

fn run_render(job: RenderJob) -> JobResult {
    let start = Instant::now();
    let s = &job.settings;
    let layers = colors::build_layers(&job.doc, s.stacked, s.smooth_sigma);
    let refs: Vec<_> = layers.iter().map(|l| (l.color, &l.mask)).collect();
    let vector = vectorize::vectorize_layers(job.doc.width, job.doc.height, &refs, &s.vector);
    let preview = render_svg(&vector.svg);
    JobResult::Rendered { id: job.id, vector, preview, elapsed: start.elapsed() }
}

/// Renderiza o SVG gerado em pixels (com `resvg`) para mostrar na tela.
/// Assim a prévia mostra exatamente o que vai para o slicer.
fn render_svg(svg: &str) -> Option<egui::ColorImage> {
    use resvg::{tiny_skia, usvg};

    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = PREVIEW_MAX_PX / size.width().max(size.height());
    let pw = ((size.width() * scale).ceil() as u32).max(1);
    let ph = ((size.height() * scale).ceil() as u32).max(1);

    let mut pixmap = tiny_skia::Pixmap::new(pw, ph)?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());

    Some(egui::ColorImage::from_rgba_premultiplied(
        [pw as usize, ph as usize],
        pixmap.data(),
    ))
}
