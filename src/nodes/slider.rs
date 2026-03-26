use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    value: &mut f32,
    min: &mut f32,
    max: &mut f32,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
) {
    // Override from inputs if connected
    let in_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 0);
    let min_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 1);
    let max_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 2);

    if in_wired {
        *value = Graph::static_input_value(connections, values, node_id, 0).as_float();
    }
    if min_wired {
        *min = Graph::static_input_value(connections, values, node_id, 1).as_float();
    }
    if max_wired {
        *max = Graph::static_input_value(connections, values, node_id, 2).as_float();
    }

    // Port 0: In — ● In: [value] or wired indicator
    render_input_port(ui, node_id, 0, "In", in_wired, *value, port_positions, dragging_from, connections);

    // Slider (only when In is not wired)
    if !in_wired {
        ui.add(egui::Slider::new(value, *min..=*max));
    }

    // Port 1 & 2: Min / Max as compact row
    ui.horizontal(|ui| {
        input_port_circle(ui, node_id, 1, min_wired, port_positions, dragging_from, connections);
        if min_wired {
            ui.label(egui::RichText::new(format!("min {:.1}", *min)).small().monospace());
        } else {
            ui.add(egui::DragValue::new(min).speed(0.1).prefix("min "));
        }
        ui.add_space(4.0);
        input_port_circle(ui, node_id, 2, max_wired, port_positions, dragging_from, connections);
        if max_wired {
            ui.label(egui::RichText::new(format!("max {:.1}", *max)).small().monospace());
        } else {
            ui.add(egui::DragValue::new(max).speed(0.1).prefix("max "));
        }
    });

    // Output port
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Value: {:.3}", *value)).small().monospace());
        let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
        ui.painter().circle_filled(rect.center(), 5.0,
            if response.hovered() || response.dragged() { egui::Color32::YELLOW } else { egui::Color32::from_rgb(80, 170, 255) });
        ui.painter().circle_stroke(rect.center(), 5.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
        port_positions.insert((node_id, 0, false), rect.center());
        if response.drag_started() { *dragging_from = Some((node_id, 0, true)); }
    });
}

fn render_input_port(
    ui: &mut egui::Ui,
    node_id: NodeId,
    port: usize,
    label: &str,
    is_wired: bool,
    value: f32,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    connections: &[Connection],
) {
    ui.horizontal(|ui| {
        input_port_circle(ui, node_id, port, is_wired, port_positions, dragging_from, connections);
        ui.label(egui::RichText::new(format!("{}:", label)).small());
        if is_wired {
            ui.label(egui::RichText::new(format!("{:.3}", value)).small().monospace().color(egui::Color32::from_rgb(80, 170, 255)));
        } else {
            ui.label(egui::RichText::new("—").small().color(egui::Color32::GRAY));
        }
    });
}

fn input_port_circle(
    ui: &mut egui::Ui,
    node_id: NodeId,
    port: usize,
    is_wired: bool,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    connections: &[Connection],
) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
    let col = if response.hovered() || response.dragged() { egui::Color32::YELLOW }
        else if is_wired { egui::Color32::from_rgb(80, 170, 255) }
        else { egui::Color32::from_rgb(140, 140, 140) };
    ui.painter().circle_filled(rect.center(), 4.0, col);
    ui.painter().circle_stroke(rect.center(), 4.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
    port_positions.insert((node_id, port, true), rect.center());
    if response.drag_started() {
        if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == port) {
            *dragging_from = Some((existing.from_node, existing.from_port, true));
        } else {
            *dragging_from = Some((node_id, port, false));
        }
    }
}
