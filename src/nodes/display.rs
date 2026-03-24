use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    history: &mut Vec<f32>,
    history_max: &mut usize,
    scope_min: &mut f32,
    scope_max: &mut f32,
    scope_height: &mut f32,
    paused: &mut bool,
) {
    let val = Graph::static_input_value(connections, values, node_id, 0);

    // Current value display
    match &val {
        PortValue::Float(v) => {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{:.3}", v))
                        .size(18.0)
                        .strong()
                        .monospace(),
                );
            });

            // Record history
            if !*paused {
                history.push(*v);
                while history.len() > *history_max {
                    history.remove(0);
                }
            }
        }
        PortValue::Text(s) => {
            egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                ui.label(egui::RichText::new(s.as_str()).monospace());
            });
            return; // No oscilloscope for text
        }
        PortValue::None => {
            ui.label(egui::RichText::new("\u{2014}").color(egui::Color32::GRAY));
            return;
        }
    }

    ui.separator();

    // Oscilloscope display
    let w = ui.available_width().max(100.0);
    let h = *scope_height;
    let (rect, _response) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());

    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(15, 15, 20));

    // Grid lines
    let range = *scope_max - *scope_min;
    if range > 0.0 {
        // Horizontal grid (value)
        for i in 0..=4 {
            let t = i as f32 / 4.0;
            let y = rect.bottom() - t * h;
            let alpha = if i == 0 || i == 4 { 40 } else { 20 };
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(0.5, egui::Color32::from_rgba_premultiplied(100, 100, 100, alpha)),
            );
        }

        // Zero line if visible
        if *scope_min < 0.0 && *scope_max > 0.0 {
            let zero_t = (0.0 - *scope_min) / range;
            let zero_y = rect.bottom() - zero_t * h;
            painter.line_segment(
                [egui::pos2(rect.left(), zero_y), egui::pos2(rect.right(), zero_y)],
                egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(100, 100, 100, 60)),
            );
        }
    }

    // Draw waveform
    if history.len() >= 2 && range > 0.0 {
        let points: Vec<egui::Pos2> = history
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let x = rect.left() + (i as f32 / (history.len() - 1).max(1) as f32) * w;
                let t = ((v - *scope_min) / range).clamp(0.0, 1.0);
                let y = rect.bottom() - t * h;
                egui::pos2(x, y)
            })
            .collect();

        // Draw line segments
        for pair in points.windows(2) {
            painter.line_segment(
                [pair[0], pair[1]],
                egui::Stroke::new(1.5, egui::Color32::from_rgb(80, 220, 120)),
            );
        }

        // Current value dot
        if let Some(&last_pt) = points.last() {
            painter.circle_filled(last_pt, 3.0, egui::Color32::from_rgb(120, 255, 160));
        }
    }

    // Scale labels on scope
    let small = egui::FontId::proportional(9.0);
    painter.text(
        egui::pos2(rect.left() + 2.0, rect.top() + 1.0),
        egui::Align2::LEFT_TOP,
        format!("{:.1}", scope_max),
        small.clone(),
        egui::Color32::from_rgb(100, 100, 100),
    );
    painter.text(
        egui::pos2(rect.left() + 2.0, rect.bottom() - 1.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{:.1}", scope_min),
        small,
        egui::Color32::from_rgb(100, 100, 100),
    );

    ui.add_space(2.0);

    // Controls
    ui.horizontal(|ui| {
        if ui.small_button(if *paused { "▶" } else { "⏸" }).clicked() {
            *paused = !*paused;
        }
        if ui.small_button("⟲").on_hover_text("Clear history").clicked() {
            history.clear();
        }
        if ui.small_button("Auto").on_hover_text("Auto-fit range").clicked() {
            if let (Some(&min_v), Some(&max_v)) = (
                history.iter().min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)),
                history.iter().max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)),
            ) {
                let margin = (max_v - min_v).max(0.1) * 0.1;
                *scope_min = min_v - margin;
                *scope_max = max_v + margin;
            }
        }
    });

    // Settings (collapsible)
    ui.collapsing("Settings", |ui| {
        ui.horizontal(|ui| {
            ui.label("Min:");
            ui.add(egui::DragValue::new(scope_min).speed(0.01));
            ui.label("Max:");
            ui.add(egui::DragValue::new(scope_max).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Samples:");
            ui.add(egui::DragValue::new(history_max).speed(1.0).range(10..=2000));
            ui.label("Height:");
            ui.add(egui::DragValue::new(scope_height).speed(1.0).range(30.0..=300.0));
        });
    });
}
