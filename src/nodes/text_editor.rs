use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

/// Text Editor node.
/// - If input is connected: text is selectable/copyable. Editing disconnects the input.
/// - If input is disconnected: fully editable text area.
/// - Always outputs its current text.
pub fn render(
    ui: &mut egui::Ui,
    content: &mut String,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    pending_disconnects: &mut Vec<(NodeId, usize)>,
) {
    let input_val = Graph::static_input_value(connections, values, node_id, 0);
    let has_input = matches!(input_val, PortValue::Text(_));

    // Get upstream text if connected
    let upstream = if let PortValue::Text(ref t) = input_val { Some(t.clone()) } else { None };

    ui.horizontal(|ui| {
        if has_input {
            ui.label(egui::RichText::new("(edit to disconnect)").small().color(egui::Color32::from_rgb(200, 180, 100)));
        }
        if !has_input {
            if ui.button("Open...").clicked() {
                if let Some(fp) = rfd::FileDialog::new()
                    .add_filter("All files", &["*"])
                    .add_filter("Text", &["txt", "json", "csv", "wgsl", "toml", "yaml", "rs", "py"])
                    .pick_file()
                {
                    *content = std::fs::read_to_string(&fp)
                        .unwrap_or_else(|e| format!("Error: {e}"));
                }
            }
        }
        if ui.button("Save as...").clicked() {
            if let Some(fp) = rfd::FileDialog::new().save_file() {
                let _ = std::fs::write(&fp, content.as_str());
            }
        }
    });

    ui.label(
        egui::RichText::new(format!("{} chars", content.len()))
            .small()
            .color(egui::Color32::GRAY),
    );

    egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
        if let Some(ref upstream_text) = upstream {
            // Connected: sync content from upstream, but let user interact
            // If user edits, disconnect
            *content = upstream_text.clone();
            let before = content.clone();
            ui.add(
                egui::TextEdit::multiline(content)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(10),
            );
            if *content != before {
                // User edited — disconnect the input
                pending_disconnects.push((node_id, 0));
            }
        } else {
            // Not connected: fully editable
            ui.add(
                egui::TextEdit::multiline(content)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(10),
            );
        }
    });
}
