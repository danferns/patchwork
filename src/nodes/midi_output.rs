use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    channel: &mut u8,
    note: &mut u8,
    velocity: &mut u8,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    ui.horizontal(|ui| {
        ui.label("Ch:");
        ui.add(egui::DragValue::new(channel).range(0..=15));
    });
    ui.horizontal(|ui| {
        ui.label("Note:");
        ui.add(egui::DragValue::new(note).range(0..=127));
    });
    ui.horizontal(|ui| {
        ui.label("Vel:");
        ui.add(egui::DragValue::new(velocity).range(0..=127));
    });

    // Override from inputs if connected
    let in_note = Graph::static_input_value(connections, values, node_id, 0);
    let in_vel  = Graph::static_input_value(connections, values, node_id, 1);
    if let PortValue::Float(v) = in_note {
        *note = v.clamp(0.0, 127.0) as u8;
    }
    if let PortValue::Float(v) = in_vel {
        *velocity = v.clamp(0.0, 127.0) as u8;
    }

    ui.label(egui::RichText::new("(MIDI send — placeholder)").small().color(egui::Color32::GRAY));
}
