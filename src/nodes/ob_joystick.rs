use crate::graph::*;
use crate::ob::ObManager;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    _node_id: NodeId,
    node_type: &mut NodeType,
    _values: &HashMap<(NodeId, usize), PortValue>,
    _connections: &[Connection],
    ob_manager: &ObManager,
) {
    let (device_id, hub_node_id) = match node_type {
        NodeType::ObJoystick { device_id, hub_node_id } => (device_id, hub_node_id),
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
            .and_then(|h| h.get_device("joystick", did))
    } else {
        ob_manager.find_device("joystick", did).map(|(_, d)| d)
    };

    let (x, y, btn) = if let Some(dev) = device {
        (
            dev.values.get("x").copied().unwrap_or(0.0),
            dev.values.get("y").copied().unwrap_or(0.0),
            dev.values.get("btn").copied().unwrap_or(0.0),
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

    // Joystick visualization (crosshair in a square)
    let viz_size = 80.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(viz_size, viz_size), egui::Sense::hover());
    let painter = ui.painter();

    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 30));
    painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 80)), egui::StrokeKind::Outside);

    let center = rect.center();
    let half = viz_size / 2.0;
    let cross_col = egui::Color32::from_rgb(40, 40, 60);
    painter.line_segment([egui::pos2(rect.left(), center.y), egui::pos2(rect.right(), center.y)], egui::Stroke::new(0.5, cross_col));
    painter.line_segment([egui::pos2(center.x, rect.top()), egui::pos2(center.x, rect.bottom())], egui::Stroke::new(0.5, cross_col));

    let dot_x = center.x + x * half * 0.9;
    let dot_y = center.y + y * half * 0.9;
    let dot_color = if btn > 0.5 {
        egui::Color32::from_rgb(255, 100, 100)
    } else if is_active {
        egui::Color32::from_rgb(80, 170, 255)
    } else {
        egui::Color32::from_rgb(100, 100, 100)
    };
    painter.circle_filled(egui::pos2(dot_x, dot_y), 6.0, dot_color);
    painter.circle_stroke(egui::pos2(dot_x, dot_y), 6.0, egui::Stroke::new(1.0, egui::Color32::WHITE));

    // Values
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("X:{:.2} Y:{:.2}", x, y)).monospace().small());
        if btn > 0.5 {
            ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "BTN");
        }
    });
}
