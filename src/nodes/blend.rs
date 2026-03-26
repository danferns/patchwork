use crate::graph::*;
use crate::gpu_image::GpuBlendCallback;
use eframe::egui;
use eframe::egui_wgpu;
use std::collections::HashMap;
use std::sync::Arc;

const BLEND_MODES: &[&str] = &["Normal", "Multiply", "Screen", "Overlay", "Add", "Difference", "Soft Light", "Hard Light"];

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    node_type: &mut NodeType,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    wgpu_render_state: &Option<egui_wgpu::RenderState>,
) {
    let (mode, mix) = match node_type {
        NodeType::Blend { mode, mix } => (mode, mix),
        _ => return,
    };

    // Read mix from input port if connected
    if connections.iter().any(|c| c.to_node == node_id && c.to_port == 2) {
        *mix = Graph::static_input_value(connections, values, node_id, 2).as_float();
    }

    ui.horizontal(|ui| {
        ui.label("Mode:");
        egui::ComboBox::from_id_salt(egui::Id::new(("blend_mode", node_id)))
            .selected_text(*BLEND_MODES.get(*mode as usize).unwrap_or(&"Normal"))
            .show_ui(ui, |ui| {
                for (i, name) in BLEND_MODES.iter().enumerate() {
                    if ui.selectable_label(*mode == i as u8, *name).clicked() {
                        *mode = i as u8;
                    }
                }
            });
    });
    ui.horizontal(|ui| { ui.label("Mix:"); ui.add(egui::Slider::new(mix, 0.0..=1.0)); });

    // Show input status
    let a = Graph::static_input_value(connections, values, node_id, 0);
    let b = Graph::static_input_value(connections, values, node_id, 1);
    let has_a = matches!(&a, PortValue::Image(_));
    let has_b = matches!(&b, PortValue::Image(_));
    if has_a && has_b {
        ui.colored_label(egui::Color32::from_rgb(80, 200, 80), "A + B connected");

        // GPU-accelerated inline preview via wgpu callback
        if let (PortValue::Image(img_a), PortValue::Image(img_b)) = (&a, &b) {
            let preview_w = ui.available_width().min(250.0);
            let aspect = img_a.height as f32 / img_a.width as f32;
            let preview_h = preview_w * aspect;

            let (rect, _) = ui.allocate_exact_size(egui::vec2(preview_w, preview_h), egui::Sense::hover());

            // Schedule GPU blend + render via egui callback
            let target_format = wgpu_render_state.as_ref()
                .map(|rs| rs.target_format)
                .unwrap_or(eframe::egui_wgpu::wgpu::TextureFormat::Bgra8UnormSrgb);
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                rect,
                GpuBlendCallback {
                    node_id,
                    mode: *mode as u32,
                    mix: *mix,
                    img_a: img_a.clone(),
                    img_b: img_b.clone(),
                    target_format,
                },
            ));
        }
    } else {
        if !has_a { ui.colored_label(egui::Color32::GRAY, "Connect Image A"); }
        if !has_b { ui.colored_label(egui::Color32::GRAY, "Connect Image B"); }
    }
}

/// Blend two images. Called during evaluation.
pub fn process(a: &ImageData, b: &ImageData, mode: u8, mix: f32) -> Arc<ImageData> {
    let w = a.width.min(b.width);
    let h = a.height.min(b.height);
    let mut pixels = vec![0u8; (w * h * 4) as usize];

    for y in 0..h {
        for x in 0..w {
            let ai = ((y * a.width + x) * 4) as usize;
            let bi = ((y * b.width + x) * 4) as usize;
            let oi = ((y * w + x) * 4) as usize;

            if ai + 3 >= a.pixels.len() || bi + 3 >= b.pixels.len() { continue; }

            for c in 0..3 {
                let va = a.pixels[ai + c] as f32 / 255.0;
                let vb = b.pixels[bi + c] as f32 / 255.0;

                let blended = match mode {
                    0 => va * (1.0 - mix) + vb * mix,                    // Normal
                    1 => va * vb,                                         // Multiply
                    2 => 1.0 - (1.0 - va) * (1.0 - vb),                 // Screen
                    3 => if va < 0.5 { 2.0 * va * vb } else { 1.0 - 2.0 * (1.0 - va) * (1.0 - vb) }, // Overlay
                    4 => (va + vb).min(1.0),                             // Add
                    5 => (va - vb).abs(),                                // Difference
                    6 => {                                                // Soft Light
                        if vb <= 0.5 { va - (1.0 - 2.0 * vb) * va * (1.0 - va) }
                        else { va + (2.0 * vb - 1.0) * (va.sqrt() - va) }
                    }
                    7 => if vb < 0.5 { 2.0 * va * vb } else { 1.0 - 2.0 * (1.0 - va) * (1.0 - vb) }, // Hard Light
                    _ => va * (1.0 - mix) + vb * mix,
                };

                // Apply mix for non-Normal modes
                let final_val = if mode != 0 {
                    va * (1.0 - mix) + blended * mix
                } else {
                    blended
                };

                pixels[oi + c] = (final_val.clamp(0.0, 1.0) * 255.0) as u8;
            }
            pixels[oi + 3] = 255; // Alpha
        }
    }
    Arc::new(ImageData::new(w, h, pixels))
}
