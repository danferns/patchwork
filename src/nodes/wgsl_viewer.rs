use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

/// WGSL Viewer node.
/// Receives WGSL code from input, displays it with a shader preview placeholder.
/// Connect: File -> WgslViewer, or File -> TextEditor -> WgslViewer, or TextEditor -> WgslViewer.
pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    let input_val = Graph::static_input_value(connections, values, node_id, 0);

    let code = match &input_val {
        PortValue::Text(s) => s.as_str(),
        _ => "",
    };

    let has_code = !code.is_empty();

    // ── Preview area ────────────────────────────────────────────────────
    let preview_size = egui::vec2(ui.available_width().max(180.0), 120.0);
    let (rect, _response) = ui.allocate_exact_size(preview_size, egui::Sense::hover());

    if has_code {
        // Gradient placeholder representing the shader output
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(30, 20, 40));

        // Simple visual: draw colored bars based on code hash for visual feedback
        let hash = code.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
        let r = ((hash >> 16) & 0xFF) as u8;
        let g = ((hash >> 8) & 0xFF) as u8;
        let b = (hash & 0xFF) as u8;

        let inner = rect.shrink(8.0);
        // Triangle placeholder
        let center = inner.center();
        let tri = [
            egui::pos2(center.x, inner.top()),
            egui::pos2(inner.left(), inner.bottom()),
            egui::pos2(inner.right(), inner.bottom()),
        ];
        painter.add(egui::Shape::convex_polygon(
            tri.to_vec(),
            egui::Color32::from_rgb(r, g, b),
            egui::Stroke::new(1.0, egui::Color32::WHITE),
        ));

        painter.text(
            egui::pos2(rect.right() - 4.0, rect.top() + 4.0),
            egui::Align2::RIGHT_TOP,
            "WGSL Preview",
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(150, 150, 150),
        );
    } else {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(40, 40, 40));
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Connect WGSL source\nto see preview",
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(120, 120, 120),
        );
    }

    ui.add_space(4.0);

    // ── Code display ────────────────────────────────────────────────────
    if has_code {
        ui.label(
            egui::RichText::new(format!("{} chars", code.len()))
                .small()
                .color(egui::Color32::GRAY),
        );
        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
            let mut display = code.to_string();
            ui.add(
                egui::TextEdit::multiline(&mut display)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(8)
                    .interactive(false),
            );
        });
    }
}
