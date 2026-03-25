use crate::graph::*;
use crate::nodes::curve::evaluate_curve;
use eframe::egui;
use std::collections::HashMap;
use std::sync::Arc;

const CHANNELS: &[&str] = &["Master", "Red", "Green", "Blue"];
const CHANNEL_COLORS: &[[u8; 3]] = &[[200, 200, 200], [255, 80, 80], [80, 255, 80], [80, 120, 255]];

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    node_type: &mut NodeType,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    let (master, red, green, blue, active_channel) = match node_type {
        NodeType::ColorCurves { master, red, green, blue, active_channel } =>
            (master, red, green, blue, active_channel),
        _ => return,
    };

    // Channel selector
    ui.horizontal(|ui| {
        for (i, name) in CHANNELS.iter().enumerate() {
            let is_active = *active_channel == i as u8;
            let col = CHANNEL_COLORS[i];
            let color = if is_active {
                egui::Color32::from_rgb(col[0], col[1], col[2])
            } else {
                egui::Color32::from_rgb(col[0] / 2, col[1] / 2, col[2] / 2)
            };
            if ui.add(egui::Label::new(egui::RichText::new(*name).strong().color(color)).sense(egui::Sense::click())).clicked() {
                *active_channel = i as u8;
            }
        }
    });

    // Presets (applied to active channel)
    let ac = *active_channel;
    ui.horizontal(|ui| {
        if ui.small_button("Reset").clicked() {
            let pts = match ac { 0 => master as &mut Vec<_>, 1 => red, 2 => green, 3 => blue, _ => master };
            *pts = vec![[0.0, 0.0], [1.0, 1.0]];
        }
        if ui.small_button("Contrast").clicked() {
            let pts = match ac { 0 => master as &mut Vec<_>, 1 => red, 2 => green, 3 => blue, _ => master };
            *pts = vec![[0.0, 0.0], [0.25, 0.15], [0.75, 0.85], [1.0, 1.0]];
        }
        if ui.small_button("Bright").clicked() {
            let pts = match ac { 0 => master as &mut Vec<_>, 1 => red, 2 => green, 3 => blue, _ => master };
            *pts = vec![[0.0, 0.1], [0.5, 0.65], [1.0, 1.0]];
        }
    });

    // Ensure all curves have at least 2 points
    for pts in [&mut *master, &mut *red, &mut *green, &mut *blue] {
        if pts.len() < 2 { *pts = vec![[0.0, 0.0], [1.0, 1.0]]; }
    }

    // Clone all curves for drawing (before mutable borrow of active channel)
    let all_curves: [(Vec<[f32; 2]>, [u8; 3], bool); 4] = [
        (master.clone(), CHANNEL_COLORS[0], ac == 0),
        (red.clone(), CHANNEL_COLORS[1], ac == 1),
        (green.clone(), CHANNEL_COLORS[2], ac == 2),
        (blue.clone(), CHANNEL_COLORS[3], ac == 3),
    ];

    // Get active curve for editing (mutable borrow AFTER cloning)
    let active_points = match ac {
        0 => master as &mut Vec<[f32; 2]>,
        1 => red,
        2 => green,
        3 => blue,
        _ => master,
    };

    // Curve editor
    let size = 180.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 30));
    painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 50, 70)), egui::StrokeKind::Outside);

    // Diagonal reference line
    painter.line_segment(
        [egui::pos2(rect.left(), rect.bottom()), egui::pos2(rect.right(), rect.top())],
        egui::Stroke::new(0.5, egui::Color32::from_rgb(40, 40, 50)),
    );

    for (points, col, is_active) in &all_curves {
        let alpha = if *is_active { 255 } else { 60 };
        let width = if *is_active { 2.0 } else { 1.0 };
        let color = egui::Color32::from_rgba_unmultiplied(col[0], col[1], col[2], alpha);

        let steps = 50;
        let mut prev = None;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let y = evaluate_curve(points, t);
            let sx = rect.left() + t * size;
            let sy = rect.bottom() - y.clamp(0.0, 1.0) * size;
            let pt = egui::pos2(sx, sy);
            if let Some(p) = prev {
                painter.line_segment([p, pt], egui::Stroke::new(width, color));
            }
            prev = Some(pt);
        }
    }

    // Control points for active curve (draggable)
    let drag_id = egui::Id::new(("cc_drag", node_id));
    let active_drag: Option<usize> = ui.ctx().data_mut(|d| d.get_temp(drag_id));
    let ac_col = CHANNEL_COLORS[*active_channel as usize];

    for (i, pt) in active_points.iter().enumerate() {
        let sx = rect.left() + pt[0] * size;
        let sy = rect.bottom() - pt[1].clamp(0.0, 1.0) * size;
        let screen_pt = egui::pos2(sx, sy);
        let hit = response.hover_pos().map(|p| p.distance(screen_pt) < 10.0).unwrap_or(false);

        let color = if hit || active_drag == Some(i) {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_rgb(ac_col[0], ac_col[1], ac_col[2])
        };
        painter.circle_filled(screen_pt, 4.0, color);

        if hit && response.drag_started() {
            ui.ctx().data_mut(|d| d.insert_temp(drag_id, i));
        }
    }

    if let Some(idx) = active_drag {
        if response.dragged() {
            if let Some(pos) = response.hover_pos() {
                let nx = ((pos.x - rect.left()) / size).clamp(0.0, 1.0);
                let ny = ((rect.bottom() - pos.y) / size).clamp(0.0, 1.0);
                if idx < active_points.len() {
                    if idx == 0 { active_points[idx] = [0.0, ny]; }
                    else if idx == active_points.len() - 1 { active_points[idx] = [1.0, ny]; }
                    else { active_points[idx] = [nx, ny]; }
                }
            }
        }
        if !response.dragged() {
            ui.ctx().data_mut(|d| d.remove::<usize>(drag_id));
        }
    }

    // Add/remove point
    ui.horizontal(|ui| {
        if ui.small_button("+").clicked() && active_points.len() < 10 {
            let mid = active_points.len() / 2;
            let x = (active_points[mid.saturating_sub(1)][0] + active_points[mid.min(active_points.len()-1)][0]) / 2.0;
            let y = evaluate_curve(active_points, x);
            active_points.insert(mid, [x, y]);
        }
        if ui.small_button("-").clicked() && active_points.len() > 2 {
            active_points.remove(active_points.len() / 2);
        }
    });

    // Input status
    let input_val = Graph::static_input_value(connections, values, node_id, 0);
    if let PortValue::Image(img) = &input_val {
        ui.label(egui::RichText::new(format!("Input: {}x{}", img.width, img.height)).small());
    } else {
        ui.colored_label(egui::Color32::GRAY, "Connect image input");
    }
}

/// Apply color curves to an image. Called during evaluation.
pub fn process(img: &ImageData, master: &[[f32; 2]], red: &[[f32; 2]], green: &[[f32; 2]], blue: &[[f32; 2]]) -> Arc<ImageData> {
    // Build LUTs (256 entries each)
    let master_lut: Vec<f32> = (0..256).map(|i| evaluate_curve(master, i as f32 / 255.0)).collect();
    let red_lut: Vec<f32> = (0..256).map(|i| evaluate_curve(red, i as f32 / 255.0)).collect();
    let green_lut: Vec<f32> = (0..256).map(|i| evaluate_curve(green, i as f32 / 255.0)).collect();
    let blue_lut: Vec<f32> = (0..256).map(|i| evaluate_curve(blue, i as f32 / 255.0)).collect();

    let mut pixels = img.pixels.clone();
    let len = pixels.len();
    let mut i = 0;
    while i + 3 < len {
        let r = pixels[i] as usize;
        let g = pixels[i + 1] as usize;
        let b = pixels[i + 2] as usize;

        // Apply channel curves then master
        let nr = master_lut[(red_lut[r].clamp(0.0, 1.0) * 255.0) as usize].clamp(0.0, 1.0);
        let ng = master_lut[(green_lut[g].clamp(0.0, 1.0) * 255.0) as usize].clamp(0.0, 1.0);
        let nb = master_lut[(blue_lut[b].clamp(0.0, 1.0) * 255.0) as usize].clamp(0.0, 1.0);

        pixels[i] = (nr * 255.0) as u8;
        pixels[i + 1] = (ng * 255.0) as u8;
        pixels[i + 2] = (nb * 255.0) as u8;
        i += 4;
    }
    Arc::new(ImageData::new(img.width, img.height, pixels))
}
