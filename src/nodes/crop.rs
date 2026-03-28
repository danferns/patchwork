use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;
use std::sync::Arc;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    node_type: &mut NodeType,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    pending_disconnects: &mut Vec<(NodeId, usize)>,
) {
    let (top, left, bottom, right) = match node_type {
        NodeType::Crop { top, left, bottom, right } => (top, left, bottom, right),
        _ => return,
    };

    // ── Image input port ──────────────────────────────────────────────
    crate::nodes::inline_port_circle(
        ui, node_id, 0, true, connections,
        port_positions, dragging_from, pending_disconnects, PortKind::Image,
    );

    // Read wired values for crop margins (ports 1–4)
    let wired = |port: usize| -> Option<f32> {
        if connections.iter().any(|c| c.to_node == node_id && c.to_port == port) {
            Some(Graph::static_input_value(connections, values, node_id, port).as_float().clamp(0.0, 0.95))
        } else {
            None
        }
    };

    // ── Preview crop region on input image ────────────────────────────
    let input_val = Graph::static_input_value(connections, values, node_id, 0);
    if let PortValue::Image(img) = &input_val {
        let max_w = ui.available_width().min(200.0);
        let aspect = img.height as f32 / img.width as f32;
        let preview_h = max_w * aspect;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(max_w, preview_h), egui::Sense::hover());
        let painter = ui.painter();

        // Draw dim full image
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(30, 30, 35));

        // Draw bright crop region rectangle
        let crop_rect = egui::Rect::from_min_max(
            egui::pos2(
                rect.left() + rect.width() * *left,
                rect.top() + rect.height() * *top,
            ),
            egui::pos2(
                rect.right() - rect.width() * *right,
                rect.bottom() - rect.height() * *bottom,
            ),
        );
        // Dim overlay on cropped-out regions
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 100));
        painter.rect_filled(crop_rect, 0.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 40));
        painter.rect_stroke(crop_rect, 0.0, egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 200, 255)), egui::StrokeKind::Outside);

        // Show output size
        let out_w = ((1.0 - *left - *right).max(0.01) * img.width as f32) as u32;
        let out_h = ((1.0 - *top - *bottom).max(0.01) * img.height as f32) as u32;
        ui.label(egui::RichText::new(format!("{}×{} → {}×{}", img.width, img.height, out_w, out_h)).small().color(egui::Color32::from_rgb(140, 180, 220)));
    }

    ui.separator();

    // ── Crop margin sliders with inline ports ─────────────────────────
    let mut slider_row = |ui: &mut egui::Ui, label: &str, val: &mut f32, port: usize| {
        ui.horizontal(|ui| {
            crate::nodes::inline_port_circle(
                ui, node_id, port, true, connections,
                port_positions, dragging_from, pending_disconnects, PortKind::Normalized,
            );
            if let Some(wired_val) = wired(port) {
                *val = wired_val;
                ui.label(format!("{}: {:.0}%", label, *val * 100.0));
            } else {
                ui.label(format!("{}:", label));
                ui.add(egui::Slider::new(val, 0.0..=0.95).show_value(false).fixed_decimals(2));
                ui.label(egui::RichText::new(format!("{:.0}%", *val * 100.0)).small());
            }
        });
    };

    slider_row(ui, "Top", top, 1);
    slider_row(ui, "Left", left, 2);
    slider_row(ui, "Bottom", bottom, 3);
    slider_row(ui, "Right", right, 4);

    // Clamp so we don't invert
    if *top + *bottom > 0.95 { *bottom = 0.95 - *top; }
    if *left + *right > 0.95 { *right = 0.95 - *left; }

    // ── Output port ───────────────────────────────────────────────────
    ui.separator();
    crate::nodes::audio_port_row(ui, "Cropped", node_id, 0, false, port_positions, dragging_from, connections, pending_disconnects, PortKind::Image);
}

/// Crop an image by fractional margins. Returns a new ImageData with the cropped region.
pub fn process(img: &ImageData, top: f32, left: f32, bottom: f32, right: f32) -> Arc<ImageData> {
    let w = img.width;
    let h = img.height;

    let x0 = (left * w as f32) as u32;
    let y0 = (top * h as f32) as u32;
    let x1 = w.saturating_sub((right * w as f32) as u32);
    let y1 = h.saturating_sub((bottom * h as f32) as u32);

    let out_w = x1.saturating_sub(x0).max(1);
    let out_h = y1.saturating_sub(y0).max(1);

    let mut pixels = vec![0u8; (out_w * out_h * 4) as usize];
    for y in 0..out_h {
        let src_y = y0 + y;
        if src_y >= h { break; }
        let src_row_start = (src_y * w * 4) as usize;
        let dst_row_start = (y * out_w * 4) as usize;
        for x in 0..out_w {
            let src_x = x0 + x;
            if src_x >= w { break; }
            let si = src_row_start + (src_x * 4) as usize;
            let di = dst_row_start + (x * 4) as usize;
            if si + 3 < img.pixels.len() && di + 3 < pixels.len() {
                pixels[di..di + 4].copy_from_slice(&img.pixels[si..si + 4]);
            }
        }
    }

    Arc::new(ImageData { width: out_w, height: out_h, pixels })
}
