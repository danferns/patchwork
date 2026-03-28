use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

const _SCOPE_W: f32 = 148.0;
const SCOPE_H: f32 = 80.0;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    history: &mut Vec<f32>,
    history_max: &mut usize,
    scope_min: &mut f32,
    scope_max: &mut f32,
    _scope_height: &mut f32,
    paused: &mut bool,
    display_color: &mut [u8; 3],
    label: &mut String,
    auto_fit: &mut bool,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    pending_disconnects: &mut Vec<(NodeId, usize)>,
) {
    let is_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 0);
    let val = Graph::static_input_value(connections, values, node_id, 0);
    let current = val.as_float();
    let wave_color = egui::Color32::from_rgb(display_color[0], display_color[1], display_color[2]);

    // Push to history
    if !*paused && is_wired {
        history.push(current);
        while history.len() > *history_max { history.remove(0); }
    }

    // Auto-fit range
    if *auto_fit && !history.is_empty() {
        let min_v = history.iter().cloned().fold(f32::INFINITY, f32::min);
        let max_v = history.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let margin = (max_v - min_v).max(0.01) * 0.1;
        *scope_min = min_v - margin;
        *scope_max = max_v + margin;
    }

    // ── Input port + value ──────────────────────────────
    ui.horizontal(|ui| {
        super::inline_port_circle(ui, node_id, 0, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Generic);
        ui.label(egui::RichText::new(format!("{:.3}", current)).monospace().strong().color(wave_color));
    });

    // ── Oscilloscope (fills available space, respects resize) ─
    let scope_w = ui.available_width().max(80.0);
    let scope_h = (ui.available_height() - 20.0).max(SCOPE_H); // leave room for label below
    let (rect, body_response) = ui.allocate_exact_size(
        egui::vec2(scope_w, scope_h),
        egui::Sense::click(),
    );
    let painter = ui.painter();

    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(10, 10, 15));

    // Grid lines
    let grid_color = egui::Color32::from_rgba_unmultiplied(60, 60, 70, 40);
    for i in 1..4 {
        let y = rect.top() + rect.height() * i as f32 / 4.0;
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(0.5, grid_color),
        );
    }

    // Waveform
    if history.len() >= 2 {
        let range = (*scope_max - *scope_min).max(0.001);
        let points: Vec<egui::Pos2> = history.iter().enumerate().map(|(i, &v)| {
            let x = rect.left() + (i as f32 / (history.len() - 1).max(1) as f32) * rect.width();
            let t = ((v - *scope_min) / range).clamp(0.0, 1.0);
            let y = rect.bottom() - t * rect.height();
            egui::pos2(x, y)
        }).collect();

        for w in points.windows(2) {
            painter.line_segment([w[0], w[1]], egui::Stroke::new(1.5, wave_color));
        }
        if let Some(&last) = points.last() {
            painter.circle_filled(last, 3.0, wave_color);
        }
    } else if !is_wired {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, "No input",
            egui::FontId::proportional(11.0), egui::Color32::from_rgb(60, 60, 65));
    }

    // Range labels
    painter.text(egui::pos2(rect.left() + 2.0, rect.top() + 2.0), egui::Align2::LEFT_TOP,
        format!("{:.1}", scope_max), egui::FontId::proportional(8.0), egui::Color32::from_rgb(60, 60, 70));
    painter.text(egui::pos2(rect.left() + 2.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM,
        format!("{:.1}", scope_min), egui::FontId::proportional(8.0), egui::Color32::from_rgb(60, 60, 70));

    if *paused {
        painter.text(egui::pos2(rect.right() - 4.0, rect.top() + 2.0), egui::Align2::RIGHT_TOP,
            "⏸", egui::FontId::proportional(10.0), egui::Color32::from_rgb(200, 200, 80));
    }

    // Label
    if !label.is_empty() {
        ui.label(egui::RichText::new(label.as_str()).small().color(egui::Color32::GRAY));
    }

    // ── Popup ───────────────────────────────────────────
    let popup_id = egui::Id::new(("display_popup", node_id));
    let popup_time_id = egui::Id::new(("display_popup_time", node_id));
    let now = ui.ctx().input(|i| i.time);

    if body_response.clicked() {
        let is_open = ui.ctx().data_mut(|d| d.get_temp::<bool>(popup_id).unwrap_or(false));
        if !is_open {
            ui.ctx().data_mut(|d| {
                d.insert_temp(popup_id, true);
                d.insert_temp(popup_time_id, now);
            });
        }
    }

    let popup_open = ui.ctx().data_mut(|d| d.get_temp::<bool>(popup_id).unwrap_or(false));

    if popup_open {
        let accent = ui.ctx().data_mut(|d| d.get_temp::<[u8; 3]>(egui::Id::new("theme_accent"))).unwrap_or([80, 160, 255]);
        let node_rect = ui.min_rect();
        let fg = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new(("display_hl", node_id))));
        fg.rect_stroke(node_rect.expand(3.0), 8.0,
            egui::Stroke::new(2.0, egui::Color32::from_rgb(accent[0], accent[1], accent[2])), egui::StrokeKind::Outside);

        let popup_pos = egui::pos2(rect.right() + 8.0, rect.top());
        let opened_time = ui.ctx().data_mut(|d| d.get_temp::<f64>(popup_time_id).unwrap_or(0.0));

        let area_resp = egui::Area::new(egui::Id::new(("display_opts", node_id)))
            .fixed_pos(popup_pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).corner_radius(10.0).inner_margin(10.0).show(ui, |ui| {
                    ui.set_min_width(170.0);

                    ui.horizontal(|ui| {
                        if ui.selectable_label(*paused, "⏸").on_hover_text("Pause").clicked() { *paused = !*paused; }
                        if ui.small_button("↺").on_hover_text("Reset").clicked() { history.clear(); }
                        if ui.selectable_label(*auto_fit, "Auto").on_hover_text("Auto-fit range").clicked() { *auto_fit = !*auto_fit; }
                    });

                    if !*auto_fit {
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Min"); ui.add(egui::DragValue::new(scope_min).speed(0.1));
                            ui.label("Max"); ui.add(egui::DragValue::new(scope_max).speed(0.1));
                        });
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Samples");
                        let mut hm = *history_max as f32;
                        ui.add(egui::DragValue::new(&mut hm).speed(10.0).range(10.0..=10000.0));
                        *history_max = hm as usize;
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.small_button(crate::icons::PUSH_PIN).on_hover_text("Pin").clicked() {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new(("display_pin_action", node_id)), true);
                                d.insert_temp(popup_id, false);
                            });
                        }
                        let mut color = egui::Color32::from_rgb(display_color[0], display_color[1], display_color[2]);
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            let a = color.to_array();
                            *display_color = [a[0], a[1], a[2]];
                        }
                        ui.add(egui::TextEdit::singleline(label).hint_text(format!("Display #{}", node_id)).desired_width(70.0));
                        if ui.small_button(egui::RichText::new(crate::icons::TRASH).color(egui::Color32::from_rgb(200, 80, 80))).clicked() {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new(("display_delete_action", node_id)), true);
                                d.insert_temp(popup_id, false);
                            });
                        }
                    });
                    ui.label(egui::RichText::new(format!("ID: {}", node_id)).small().color(egui::Color32::from_rgb(80, 80, 80)));
                });
            });

        let popup_rect = area_resp.response.rect;
        let esc = ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));
        let click_outside = if now - opened_time > 0.3 {
            ui.ctx().input(|i| i.pointer.button_clicked(egui::PointerButton::Primary))
                && ui.ctx().pointer_latest_pos().map(|p| !popup_rect.contains(p) && !rect.contains(p)).unwrap_or(false)
        } else { false };

        if esc || click_outside {
            ui.ctx().data_mut(|d| d.insert_temp(popup_id, false));
        }
    }
}
