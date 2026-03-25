use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    node_type: &mut NodeType,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    let points = match node_type {
        NodeType::Curve { points } => points,
        _ => return,
    };

    // Ensure at least 2 points
    if points.len() < 2 {
        *points = vec![[0.0, 0.0], [1.0, 1.0]];
    }

    // Read X input
    let x_input = Graph::static_input_value(connections, values, node_id, 0).as_float();
    let y_output = evaluate_curve(points, x_input);

    // Presets
    ui.horizontal(|ui| {
        if ui.small_button("Linear").clicked() { *points = vec![[0.0, 0.0], [1.0, 1.0]]; }
        if ui.small_button("Ease In").clicked() { *points = vec![[0.0, 0.0], [0.4, 0.0], [1.0, 1.0]]; }
        if ui.small_button("Ease Out").clicked() { *points = vec![[0.0, 0.0], [0.6, 1.0], [1.0, 1.0]]; }
        if ui.small_button("S").clicked() { *points = vec![[0.0, 0.0], [0.3, 0.0], [0.7, 1.0], [1.0, 1.0]]; }
    });

    // Interactive curve editor
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

    // X input indicator
    {
        let ix = rect.left() + x_input.clamp(0.0, 1.0) * size;
        let iy = rect.bottom() - y_output.clamp(0.0, 1.0) * size;
        painter.circle_filled(egui::pos2(ix, iy), 4.0, egui::Color32::from_rgb(255, 200, 80));
    }

    // Control points — draggable
    let mut dragged_idx: Option<usize> = None;
    let drag_id = egui::Id::new(("curve_drag", node_id));
    let active_drag: Option<usize> = ui.ctx().data_mut(|d| d.get_temp(drag_id));

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
                    // Don't allow first/last X to move
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
            // Insert at midpoint
            let mid = points.len() / 2;
            let x = (points[mid.saturating_sub(1)][0] + points[mid.min(points.len()-1)][0]) / 2.0;
            let y = evaluate_curve(points, x);
            points.insert(mid, [x, y]);
        }
        if ui.small_button("- Point").clicked() && points.len() > 2 {
            // Remove middle point
            let mid = points.len() / 2;
            points.remove(mid);
        }
    });

    // Display output
    ui.label(egui::RichText::new(format!("X:{:.2} → Y:{:.2}", x_input, y_output)).monospace().small());
}

/// Evaluate the curve at position x (0-1). Uses piecewise linear interpolation.
pub fn evaluate_curve(points: &[[f32; 2]], x: f32) -> f32 {
    if points.is_empty() { return 0.0; }
    if points.len() == 1 { return points[0][1]; }

    let x = x.clamp(0.0, 1.0);

    // Find the segment
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

    // Outside range
    if x <= points[0][0] { points[0][1] }
    else { points[points.len() - 1][1] }
}
