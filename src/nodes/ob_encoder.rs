use crate::graph::*;
use crate::ob::ObManager;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    node_type: &mut NodeType,
    _values: &HashMap<(NodeId, usize), PortValue>,
    _connections: &[Connection],
    ob_manager: &ObManager,
) {
    let (device_id, hub_node_id) = match node_type {
        NodeType::ObEncoder { device_id, hub_node_id } => (device_id, hub_node_id),
        _ => return,
    };

    // Device ID selector
    ui.horizontal(|ui| {
        ui.label("ID:");
        ui.add(egui::DragValue::new(device_id).range(1..=255));
    });

    let did = *device_id;
    let hid = *hub_node_id;

    // Find device — prefer assigned hub, else search all hubs
    let device = if hid != 0 {
        ob_manager.get_hub(hid)
            .and_then(|h| h.get_device("encoder", did))
    } else {
        ob_manager.find_device("encoder", did).map(|(_, d)| d)
    };

    let (turn, click, position) = if let Some(dev) = device {
        (
            dev.values.get("turn").copied().unwrap_or(0.0),
            dev.values.get("click").copied().unwrap_or(0.0),
            dev.values.get("position").copied().unwrap_or(0.0),
        )
    } else {
        (0.0, 0.0, 0.0)
    };

    let is_active = device.map(|d| d.is_active).unwrap_or(false);

    // Status
    if is_active {
        ui.colored_label(egui::Color32::from_rgb(80, 200, 80), "● Active");
    } else {
        ui.colored_label(egui::Color32::from_rgb(150, 150, 150), "○ Waiting...");
    }

    // Encoder visualization
    let viz_size = 60.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(viz_size, viz_size), egui::Sense::hover());
    let painter = ui.painter();
    let center = rect.center();
    let radius = viz_size * 0.4;

    painter.circle_filled(center, radius, egui::Color32::from_rgb(20, 20, 30));
    painter.circle_stroke(center, radius, egui::Stroke::new(1.5, egui::Color32::from_rgb(60, 60, 80)));

    let angle = position * 0.3;
    let indicator_end = egui::pos2(
        center.x + (angle as f32).cos() * radius * 0.8,
        center.y - (angle as f32).sin() * radius * 0.8,
    );
    let ind_color = if is_active {
        egui::Color32::from_rgb(80, 170, 255)
    } else {
        egui::Color32::from_rgb(100, 100, 100)
    };
    painter.line_segment([center, indicator_end], egui::Stroke::new(2.0, ind_color));
    painter.circle_filled(indicator_end, 3.0, ind_color);

    if click > 0.5 {
        painter.circle_filled(center, 5.0, egui::Color32::from_rgb(255, 100, 100));
    } else {
        painter.circle_filled(center, 3.0, egui::Color32::from_rgb(60, 60, 60));
    }

    // Values
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Pos:{:.0}", position)).monospace().small());
        if turn.abs() > 0.5 {
            let dir = if turn > 0.0 { "↻" } else { "↺" };
            ui.colored_label(egui::Color32::from_rgb(200, 200, 80), dir);
        }
        if click > 0.5 {
            ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "●");
        }
    });
}
