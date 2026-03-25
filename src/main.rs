mod app;
mod audio;
mod graph;
mod http;
mod mcp;
mod midi;
mod nodes;
mod ob;
mod osc;
mod serial;

use eframe::egui;
use std::sync::Arc;

fn main() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../icons/icon.png"))
        .expect("Failed to load icon");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(Arc::new(icon))
            .with_inner_size([1280.0, 800.0])
            .with_title("Patchwork"),
        renderer: eframe::Renderer::Wgpu,
        vsync: false,
        ..Default::default()
    };

    eframe::run_native(
        "Patchwork",
        options,
        Box::new(|cc| Ok(Box::new(app::PatchworkApp::new(cc)))),
    )
}
