use eframe::egui;
use crate::graph::{NodeId, PortValue, PortKind, Connection, Graph};
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    in_min: &mut f32,
    in_max: &mut f32,
    out_min: &mut f32,
    out_max: &mut f32,
    clamp: &mut bool,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    pending_disconnects: &mut Vec<(NodeId, usize)>,
) {
    let wired: Vec<bool> = (0..5).map(|p| connections.iter().any(|c| c.to_node == node_id && c.to_port == p)).collect();

    // Apply wired values
    if wired[1] { *in_min = Graph::static_input_value(connections, values, node_id, 1).as_float(); }
    if wired[2] { *in_max = Graph::static_input_value(connections, values, node_id, 2).as_float(); }
    if wired[3] { *out_min = Graph::static_input_value(connections, values, node_id, 3).as_float(); }
    if wired[4] { *out_max = Graph::static_input_value(connections, values, node_id, 4).as_float(); }

    // ── Input ports ───────────────────────────────────────────────────
    // Port 0: Value
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 0, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Number);
        ui.label(egui::RichText::new("Value:").small());
        if wired[0] {
            let v = Graph::static_input_value(connections, values, node_id, 0).as_float();
            ui.label(egui::RichText::new(format!("{:.3}", v)).small().color(egui::Color32::from_rgb(80, 170, 255)));
        } else {
            ui.label(egui::RichText::new("—").small().color(egui::Color32::GRAY));
        }
    });

    ui.separator();

    // Input range
    ui.label(egui::RichText::new("Input Range").small().strong());
    // Port 1: In Min
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 1, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Number);
        ui.label(egui::RichText::new("Min:").small());
        if wired[1] {
            ui.label(egui::RichText::new(format!("{:.2}", *in_min)).small().color(egui::Color32::from_rgb(80, 170, 255)));
        } else {
            ui.add(egui::DragValue::new(in_min).speed(0.1));
        }
    });
    // Port 2: In Max
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 2, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Number);
        ui.label(egui::RichText::new("Max:").small());
        if wired[2] {
            ui.label(egui::RichText::new(format!("{:.2}", *in_max)).small().color(egui::Color32::from_rgb(80, 170, 255)));
        } else {
            ui.add(egui::DragValue::new(in_max).speed(0.1));
        }
    });

    // Output range
    ui.label(egui::RichText::new("Output Range").small().strong());
    // Port 3: Out Min
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 3, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Number);
        ui.label(egui::RichText::new("Min:").small());
        if wired[3] {
            ui.label(egui::RichText::new(format!("{:.2}", *out_min)).small().color(egui::Color32::from_rgb(80, 170, 255)));
        } else {
            ui.add(egui::DragValue::new(out_min).speed(0.1));
        }
    });
    // Port 4: Out Max
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 4, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Number);
        ui.label(egui::RichText::new("Max:").small());
        if wired[4] {
            ui.label(egui::RichText::new(format!("{:.2}", *out_max)).small().color(egui::Color32::from_rgb(80, 170, 255)));
        } else {
            ui.add(egui::DragValue::new(out_max).speed(0.1));
        }
    });

    ui.checkbox(clamp, "Clamp output");

    ui.separator();

    // Snapshot values for drawing
    let imin = *in_min;
    let imax = *in_max;
    let omin = *out_min;
    let omax = *out_max;
    let do_clamp = *clamp;

    // ── Visual graph ──────────────────────────────────────────────────
    let graph_size = egui::vec2(140.0, 100.0);
    let (rect, _) = ui.allocate_exact_size(graph_size, egui::Sense::hover());
    let painter = ui.painter();

    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(25, 25, 30));
    painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 70)), egui::StrokeKind::Outside);

    for i in 1..4 {
        let t = i as f32 / 4.0;
        let x = rect.left() + t * rect.width();
        let y = rect.top() + t * rect.height();
        painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(0.5, egui::Color32::from_rgb(40, 40, 50)));
        painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(0.5, egui::Color32::from_rgb(40, 40, 50)));
    }

    let y_range = (omax - omin).abs().max(0.001);
    let o_lo = omin.min(omax);
    let p1 = egui::pos2(rect.left() + 4.0, rect.bottom() - 4.0 - ((omin - o_lo) / y_range) * (rect.height() - 8.0));
    let p2 = egui::pos2(rect.right() - 4.0, rect.bottom() - 4.0 - ((omax - o_lo) / y_range) * (rect.height() - 8.0));
    painter.line_segment([p1, p2], egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 200, 120)));

    let input_val = Graph::static_input_value(connections, values, node_id, 0).as_float();
    let denom = (imax - imin).abs().max(0.001);
    let t = (input_val - imin) / denom;
    let t_clamped = if do_clamp { t.clamp(0.0, 1.0) } else { t };
    let dot_x = rect.left() + 4.0 + t_clamped * (rect.width() - 8.0);
    let mapped = omin + t_clamped * (omax - omin);
    let dot_y = rect.bottom() - 4.0 - ((mapped - o_lo) / y_range) * (rect.height() - 8.0);
    let dot_pos = egui::pos2(dot_x.clamp(rect.left(), rect.right()), dot_y.clamp(rect.top(), rect.bottom()));
    painter.circle_filled(dot_pos, 4.0, egui::Color32::from_rgb(255, 220, 60));

    painter.text(egui::pos2(rect.left() + 2.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM,
        format!("{:.1}", imin), egui::FontId::new(8.0, egui::FontFamily::Proportional), egui::Color32::from_rgb(120, 120, 140));
    painter.text(egui::pos2(rect.right() - 2.0, rect.bottom() - 2.0), egui::Align2::RIGHT_BOTTOM,
        format!("{:.1}", imax), egui::FontId::new(8.0, egui::FontFamily::Proportional), egui::Color32::from_rgb(120, 120, 140));
    painter.text(egui::pos2(rect.left() + 2.0, rect.top() + 2.0), egui::Align2::LEFT_TOP,
        format!("{:.1}", omax), egui::FontId::new(8.0, egui::FontFamily::Proportional), egui::Color32::from_rgb(120, 140, 120));
    painter.text(egui::pos2(rect.left() + 2.0, rect.bottom() - 12.0), egui::Align2::LEFT_BOTTOM,
        format!("{:.1}", omin), egui::FontId::new(8.0, egui::FontFamily::Proportional), egui::Color32::from_rgb(120, 140, 120));

    ui.label(egui::RichText::new(format!("{:.3} → {:.3}", input_val, mapped)).small().monospace().strong());

    ui.separator();

    // ── Output port ───────────────────────────────────────────────────
    crate::nodes::output_port_row(ui, "Out", &format!("{:.3}", mapped), node_id, 0, port_positions, dragging_from, connections, pending_disconnects, PortKind::Number);
}
