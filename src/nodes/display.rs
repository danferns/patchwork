use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    let val = Graph::static_input_value(connections, values, node_id, 0);
    match &val {
        PortValue::Float(v) => {
            ui.heading(format!("{:.3}", v));
        }
        PortValue::Text(s) => {
            egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                ui.label(egui::RichText::new(s.as_str()).monospace());
            });
        }
        PortValue::None => {
            ui.label(egui::RichText::new("\u{2014}").color(egui::Color32::GRAY));
        }
    }
}
