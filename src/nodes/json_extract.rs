use eframe::egui;
use crate::graph::{NodeId, PortValue, Connection, Graph};
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    path: &mut String,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    let json_input = Graph::static_input_value(connections, values, node_id, 0);
    let json_text = match &json_input {
        PortValue::Text(s) => s.clone(),
        _ => String::new(),
    };

    ui.horizontal(|ui| {
        ui.label("Path:");
        ui.text_edit_singleline(path);
    });
    ui.label("(dot-separated, e.g. choices.0.message.content)");

    // Show extracted value
    let output_val = values.get(&(node_id, 0));
    ui.separator();
    match output_val {
        Some(PortValue::Text(s)) if !s.is_empty() => {
            ui.label("Extracted:");
            egui::ScrollArea::vertical().max_height(80.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut s.clone())
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .interactive(false));
            });
        }
        _ => {
            if json_text.is_empty() {
                ui.colored_label(egui::Color32::GRAY, "(no JSON input)");
            } else if path.is_empty() {
                ui.colored_label(egui::Color32::GRAY, "(enter path)");
            } else {
                ui.colored_label(egui::Color32::from_rgb(200, 80, 80), "(no match)");
            }
        }
    }
}
