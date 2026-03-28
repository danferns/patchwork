use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    node_type: &mut NodeType,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    pending_disconnects: &mut Vec<(NodeId, usize)>,
) {
    let (points, mode, speed, looping, phase, playing) = match node_type {
        NodeType::Curve { points, mode, speed, looping, phase, playing, .. } =>
            (points, mode, speed, looping, phase, playing),
        _ => return,
    };

    // Ensure at least 2 points
    if points.len() < 2 {
        *points = vec![[0.0, 0.0], [1.0, 1.0]];
    }

    // ── Input ports ───────────────────────────────────────────────────
    // Port 0: X (Manual mode)
    let x_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 0);
    let x_input = Graph::static_input_value(connections, values, node_id, 0).as_float().clamp(0.0, 1.0);

    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 0, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Normalized);
        ui.label(egui::RichText::new("X:").small());
        if x_wired || *mode == 0 {
            ui.label(egui::RichText::new(format!("{:.2}", x_input)).small().color(
                if x_wired { egui::Color32::from_rgb(80, 170, 255) } else { egui::Color32::GRAY }
            ));
        } else {
            ui.label(egui::RichText::new("—").small().color(egui::Color32::from_rgb(70, 70, 80)));
        }
    });

    // Port 1: Trigger (Envelope/LFO mode)
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 1, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Trigger);
        ui.label(egui::RichText::new("Trigger").small());
    });

    // Port 2: Speed
    let speed_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 2);
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 2, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Number);
        ui.label(egui::RichText::new("Speed:").small());
        if speed_wired {
            ui.label(egui::RichText::new(format!("{:.1}x", speed)).small().color(egui::Color32::from_rgb(80, 170, 255)));
        } else {
            ui.add(egui::DragValue::new(speed).speed(0.05).range(0.1..=20.0).suffix("x"));
        }
    });

    // Port 3: Gate
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 3, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Gate);
        ui.label(egui::RichText::new("Gate").small());
    });

    ui.separator();

    // ── Mode selector ─────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Mode:").small());
        if ui.selectable_label(*mode == 0, "Manual").clicked() { *mode = 0; }
        if ui.selectable_label(*mode == 1, "Env").clicked() { *mode = 1; }
        if ui.selectable_label(*mode == 2, "LFO").clicked() { *mode = 2; *looping = true; }
    });

    // Loop toggle (Envelope mode)
    if *mode == 1 {
        ui.horizontal(|ui| {
            ui.checkbox(looping, egui::RichText::new("Loop").small());
        });
    }

    // ── Presets ────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        if ui.small_button("Lin").clicked() { *points = vec![[0.0, 0.0], [1.0, 1.0]]; }
        if ui.small_button("Ease").clicked() { *points = vec![[0.0, 0.0], [0.4, 0.0], [0.6, 1.0], [1.0, 1.0]]; }
        if ui.small_button("S").clicked() { *points = vec![[0.0, 0.0], [0.3, 0.0], [0.7, 1.0], [1.0, 1.0]]; }
    });
    ui.horizontal(|ui| {
        if ui.small_button("ADSR").clicked() {
            *points = vec![[0.0, 0.0], [0.1, 1.0], [0.3, 0.7], [0.8, 0.7], [1.0, 0.0]];
        }
        if ui.small_button("Bell").clicked() {
            *points = vec![[0.0, 0.0], [0.3, 0.0], [0.5, 1.0], [0.7, 0.0], [1.0, 0.0]];
        }
        if ui.small_button("Notch").clicked() {
            *points = vec![[0.0, 1.0], [0.3, 1.0], [0.5, 0.0], [0.7, 1.0], [1.0, 1.0]];
        }
    });

    // ── Curve editor ──────────────────────────────────────────────────
    let size = 180.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 30));
    painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 50, 70)), egui::StrokeKind::Outside);

    // Grid
    for i in 1..4 {
        let t = i as f32 / 4.0;
        let x = rect.left() + t * size;
        let y = rect.top() + t * size;
        painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(0.5, egui::Color32::from_rgb(35, 35, 50)));
        painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(0.5, egui::Color32::from_rgb(35, 35, 50)));
    }

    // Draw curve
    let steps = 50;
    let mut prev_screen = None;
    for s in 0..=steps {
        let t = s as f32 / steps as f32;
        let y = evaluate_curve(points, t);
        let sx = rect.left() + t * size;
        let sy = rect.bottom() - y.clamp(0.0, 1.0) * size;
        let screen_pt = egui::pos2(sx, sy);
        if let Some(prev) = prev_screen {
            painter.line_segment([prev, screen_pt], egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 200, 160)));
        }
        prev_screen = Some(screen_pt);
    }

    // ── Playback head (Envelope/LFO mode) ─────────────────────────────
    let current_x = if *mode == 0 { x_input } else { *phase };
    let current_y = evaluate_curve(points, current_x);

    if *mode >= 1 {
        // Vertical playback head line
        let head_x = rect.left() + current_x.clamp(0.0, 1.0) * size;
        painter.line_segment(
            [egui::pos2(head_x, rect.top()), egui::pos2(head_x, rect.bottom())],
            egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(255, 200, 80, 120)),
        );
    }

    // Position dot (orange)
    {
        let ix = rect.left() + current_x.clamp(0.0, 1.0) * size;
        let iy = rect.bottom() - current_y.clamp(0.0, 1.0) * size;
        painter.circle_filled(egui::pos2(ix, iy), 4.0, egui::Color32::from_rgb(255, 200, 80));
    }

    // Control points — draggable
    let drag_id = egui::Id::new(("curve_drag", node_id));
    let active_drag: Option<usize> = ui.ctx().data_mut(|d| d.get_temp(drag_id));
    let mut dragged_idx: Option<usize> = None;

    for (i, pt) in points.iter().enumerate() {
        let sx = rect.left() + pt[0] * size;
        let sy = rect.bottom() - pt[1].clamp(0.0, 1.0) * size;
        let screen_pt = egui::pos2(sx, sy);
        let hit = response.hover_pos().map(|p| p.distance(screen_pt) < 10.0).unwrap_or(false);

        let color = if hit || active_drag == Some(i) {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_rgb(160, 200, 180)
        };
        painter.circle_filled(screen_pt, 5.0, color);
        painter.circle_stroke(screen_pt, 5.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 80, 80)));

        if hit && response.drag_started() {
            dragged_idx = Some(i);
        }
    }

    if let Some(idx) = dragged_idx {
        ui.ctx().data_mut(|d| d.insert_temp(drag_id, idx));
    }

    if let Some(idx) = active_drag {
        if response.dragged() {
            if let Some(pos) = response.hover_pos() {
                let nx = ((pos.x - rect.left()) / size).clamp(0.0, 1.0);
                let ny = ((rect.bottom() - pos.y) / size).clamp(0.0, 1.0);
                if idx < points.len() {
                    if idx == 0 { points[idx] = [0.0, ny]; }
                    else if idx == points.len() - 1 { points[idx] = [1.0, ny]; }
                    else { points[idx] = [nx, ny]; }
                }
            }
        }
        if !response.dragged() {
            ui.ctx().data_mut(|d| d.remove::<usize>(drag_id));
        }
    }

    // Add/remove point buttons
    ui.horizontal(|ui| {
        if ui.small_button("+ Point").clicked() && points.len() < 10 {
            let mid = points.len() / 2;
            let x = (points[mid.saturating_sub(1)][0] + points[mid.min(points.len()-1)][0]) / 2.0;
            let y = evaluate_curve(points, x);
            points.insert(mid, [x, y]);
        }
        if ui.small_button("- Point").clicked() && points.len() > 2 {
            let mid = points.len() / 2;
            points.remove(mid);
        }
    });

    // ── Transport controls (Envelope/LFO mode) ───────────────────────
    if *mode >= 1 {
        ui.separator();
        ui.horizontal(|ui| {
            if ui.small_button(if *playing { "⏸" } else { "▶" }).clicked() {
                if *playing {
                    *playing = false;
                } else {
                    *playing = true;
                    if *phase >= 1.0 { *phase = 0.0; }
                }
            }
            if ui.small_button("⏹").clicked() {
                *playing = false;
                *phase = 0.0;
            }
            if ui.small_button("↺").clicked() {
                *phase = 0.0;
                *playing = true;
            }
            // Phase display
            ui.label(egui::RichText::new(format!("{:.0}%", current_x * 100.0)).small().color(
                if *playing { egui::Color32::from_rgb(80, 200, 120) } else { egui::Color32::GRAY }
            ));
        });
    }

    ui.separator();

    ui.separator();

    // ── Output ports (right-aligned, stable) ──────────────────────────
    crate::nodes::output_port_row(ui, "Y", &format!("{:.2}", current_y), node_id, 0, port_positions, dragging_from, connections, pending_disconnects, PortKind::Normalized);
    crate::nodes::output_port_row(ui, "Phase", &format!("{:.2}", current_x), node_id, 1, port_positions, dragging_from, connections, pending_disconnects, PortKind::Normalized);
    crate::nodes::output_port_row(ui, "End", &format!("{}", if !*playing && *phase >= 1.0 && *mode >= 1 { 1 } else { 0 }), node_id, 2, port_positions, dragging_from, connections, pending_disconnects, PortKind::Trigger);
    // Image output (port 3) handled by standard port system in app.rs

    // Request repaint when animating
    if *playing {
        ui.ctx().request_repaint();
    }
}

/// Evaluate the curve at position x (0-1). Uses cubic Hermite interpolation.
pub fn evaluate_curve(points: &[[f32; 2]], x: f32) -> f32 {
    if points.is_empty() { return 0.0; }
    if points.len() == 1 { return points[0][1]; }

    let x = x.clamp(0.0, 1.0);

    for i in 0..points.len() - 1 {
        let (x0, y0) = (points[i][0], points[i][1]);
        let (x1, y1) = (points[i + 1][0], points[i + 1][1]);
        if x >= x0 && x <= x1 {
            if (x1 - x0).abs() < 1e-6 { return y0; }
            let t = (x - x0) / (x1 - x0);
            // Smooth interpolation (cubic hermite)
            let t2 = t * t;
            let t3 = t2 * t;
            let h = 2.0 * t3 - 3.0 * t2 + 1.0;
            return h * y0 + (1.0 - h) * y1;
        }
    }

    if x <= points[0][0] { points[0][1] }
    else { points[points.len() - 1][1] }
}
