use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

/// Text Editor node.
/// - If input is connected: displays upstream text read-only (pass-through).
/// - If input is disconnected: fully editable text area.
/// - Always outputs its current text.
pub fn render(
    ui: &mut egui::Ui,
    content: &mut String,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    let input_val = Graph::static_input_value(connections, values, node_id, 0);
    let has_input = matches!(input_val, PortValue::Text(_));

    // Sync from upstream when connected
    if let PortValue::Text(ref t) = input_val {
        *content = t.clone();
    }

    ui.horizontal(|ui| {
        if has_input {
            ui.label(egui::RichText::new("(input connected)").small().color(egui::Color32::GRAY));
        }
        if !has_input {
            if ui.button("Open...").clicked() {
                if let Some(fp) = rfd::FileDialog::new()
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
        if has_input {
            // Read-only when driven by upstream
            let mut display = content.clone();
            ui.add(
                egui::TextEdit::multiline(&mut display)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(10)
                    .interactive(false),
            );
        } else {
            ui.add(
                egui::TextEdit::multiline(content)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(10),
            );
        }
    });
}
