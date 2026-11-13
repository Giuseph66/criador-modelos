//! Converte uma imagem em SVG pela linha de comando (sem janela).
//!
//! Uso: cargo run --release --example converter -- entrada.png saida.svg [largura_mm]

use criador_modelos::processing::{self, BwParams};
use criador_modelos::vectorize::{self, VectorParams};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Uso: converter <entrada> <saida.svg> [largura_mm]");
        std::process::exit(2);
    }

    let img = image::open(&args[1]).expect("não consegui abrir a imagem").to_rgba8();
    let bw_params = BwParams::default();
    let mut vec_params = VectorParams::default();
    if let Some(mm) = args.get(3) {
        vec_params.width_mm = mm.parse().expect("largura_mm deve ser número");
    }

    let rgb = processing::prepare_rgb(&img, processing::DEFAULT_MAX_SIDE);
    let gray = processing::to_gray(&rgb);
    let bw = processing::to_black_and_white(&gray, &bw_params);
    let result = vectorize::vectorize(&bw.mask, &vec_params);

    std::fs::write(&args[2], &result.svg).expect("não consegui salvar o SVG");
    println!(
        "{} formas, {:.1} x {:.1} mm, limiar {:?} -> {}",
        result.path_count, result.width_mm, result.height_mm, bw.threshold_used, args[2]
    );
}
