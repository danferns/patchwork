use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

const MODES: &[&str] = &[">", "<", "≥", "≤", "=", "≠"];

pub fn render(
    ui: &mut egui::Ui,
    mode: &mut u8,
    threshold: &mut f32,
    else_value: &mut f32,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    pending_disconnects: &mut Vec<(NodeId, usize)>,
) {
    let dim = egui::Color32::from_rgb(140, 140, 140);
    let _accent = egui::Color32::from_rgb(220, 180, 60);

    // ── Input ports ───────────────────────────────────────────────────
    // Port 0: Value (Number)
    let val_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 0);
    let val = Graph::static_input_value(connections, values, node_id, 0).as_float();

    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 0, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Number);
        ui.label(egui::RichText::new("Value:").small());
        if val_wired {
            ui.label(egui::RichText::new(format!("{:.2}", val)).small().color(egui::Color32::from_rgb(80, 170, 255)));
        } else {
            ui.label(egui::RichText::new("—").small().color(egui::Color32::GRAY));
        }
    });

    // Port 1: Threshold (Number)
    let thresh_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 1);
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 1, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Number);
        ui.label(egui::RichText::new("Thresh:").small());
        if thresh_wired {
            ui.label(egui::RichText::new(format!("{:.2}", threshold)).small().color(egui::Color32::from_rgb(80, 170, 255)));
        } else {
            ui.add(egui::DragValue::new(threshold).speed(0.1));
        }
    });

    ui.separator();

    // ── Mode selector ─────────────────────────────────────────────────
    ui.horizontal(|ui| {
        for (i, label) in MODES.iter().enumerate() {
            let selected = *mode == i as u8;
            if ui.selectable_label(selected, egui::RichText::new(*label).strong()).clicked() {
                *mode = i as u8;
            }
        }
    });

    // Else value
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Else:").small().color(dim));
        ui.add(egui::DragValue::new(else_value).speed(0.1));
    });

    ui.separator();

    // ── Live status ───────────────────────────────────────────────────
    let pass = match *mode {
        0 => val > *threshold,
        1 => val < *threshold,
        2 => val >= *threshold,
        3 => val <= *threshold,
        4 => (val - *threshold).abs() < f32::EPSILON,
        5 => (val - *threshold).abs() >= f32::EPSILON,
        _ => val > *threshold,
    };

    let status_color = if pass { egui::Color32::from_rgb(80, 255, 120) } else { egui::Color32::from_rgb(255, 80, 80) };
    let status_icon = if pass { "✓ Pass" } else { "✗ Block" };

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(status_icon).color(status_color).strong());
        ui.label(egui::RichText::new(format!("{:.2} {} {:.2}", val, MODES[*mode as usize], *threshold))
            .small().monospace().color(dim));
    });

    let out = if pass { val } else { *else_value };

    ui.separator();

    // ── Output ports ──────────────────────────────────────────────────
    crate::nodes::output_port_row(ui, "Out", &format!("{:.3}", out), node_id, 0, port_positions, dragging_from, connections, pending_disconnects, PortKind::Number);
    crate::nodes::output_port_row(ui, "Pass", if pass { "1" } else { "0" }, node_id, 1, port_positions, dragging_from, connections, pending_disconnects, PortKind::Gate);
}
