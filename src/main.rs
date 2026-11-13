//! Criador de Modelos — imagem -> preto e branco -> SVG para o slicer.

// Em release no Windows, não abre janela de terminal junto.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod worker;

fn main() -> eframe::Result {
    // Permite abrir direto: `criador_modelos foto.png`
    let initial = std::env::args_os().nth(1).map(std::path::PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Criador de Modelos")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([800.0, 500.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "Criador de Modelos",
        options,
        Box::new(|cc| Ok(Box::new(app::CriadorApp::new(cc, initial)))),
    )
}
