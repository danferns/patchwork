use crate::graph::*;
use eframe::egui;

pub fn render(
    ui: &mut egui::Ui,
    text: &mut String,
    bg_color: &mut [u8; 3],
    node_id: NodeId,
) {
    let fold_size = 12.0;

    let font_size_id = egui::Id::new(("comment_font_size", node_id));
    let font_size = ui.ctx().data_mut(|d| d.get_temp::<f32>(font_size_id).unwrap_or(14.0));

    let popup_id = egui::Id::new(("comment_popup", node_id));
    let popup_time_id = egui::Id::new(("comment_popup_time", node_id));
    let editing_id = egui::Id::new(("comment_editing", node_id));
    let now = ui.ctx().input(|i| i.time);

    // Check if we're in editing mode (text cursor active)
    let is_editing = ui.ctx().data_mut(|d| d.get_temp::<bool>(editing_id).unwrap_or(false));

    // ── Allocate the full body area for drag detection ─────────
    let avail_w = ui.available_width();
    let estimated_h = 80.0_f32.max(text.lines().count() as f32 * (font_size + 4.0) + 30.0);
    let (body_rect, body_response) = ui.allocate_exact_size(
        egui::vec2(avail_w, estimated_h),
        if is_editing { egui::Sense::click() } else { egui::Sense::click() },
    );

    // ── Render text on top of the allocated area ──────────────
    let text_rect = body_rect.shrink2(egui::vec2(4.0, 4.0));

    // Track if we just entered editing mode this frame
    let just_entered_id = egui::Id::new(("comment_just_entered", node_id));
    let just_entered = ui.ctx().data_mut(|d| d.get_temp::<bool>(just_entered_id).unwrap_or(false));

    if is_editing {
        // Interactive text editor — user is typing
        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
        let text_resp = child_ui.add(
            egui::TextEdit::multiline(text)
                .desired_width(text_rect.width())
                .desired_rows(3)
                .frame(false)
                .font(egui::FontId::proportional(font_size))
                .hint_text("Write a note..."),
        );

        // Auto-focus on first frame of editing
        if just_entered {
            text_resp.request_focus();
            ui.ctx().data_mut(|d| d.insert_temp(just_entered_id, false));
        }

        // If text editor lost focus, exit editing mode
        if text_resp.lost_focus() && !just_entered {
            ui.ctx().data_mut(|d| d.insert_temp(editing_id, false));
        }
    } else {
        // Non-interactive display — just paint the text, allow drag for movement
        let painter = ui.painter();
        let display_text = if text.is_empty() { "Write a note..." } else { text.as_str() };
        let color = if text.is_empty() {
            egui::Color32::from_rgb(80, 80, 85)
        } else {
            egui::Color32::from_rgb(200, 200, 205)
        };
        painter.text(
            text_rect.left_top(),
            egui::Align2::LEFT_TOP,
            display_text,
            egui::FontId::proportional(font_size),
            color,
        );
    }

    // ── Click = start editing + open popup, Drag = move node ──
    if body_response.clicked() && !is_editing {
        // Enter editing mode + flag to auto-focus on next frame
        ui.ctx().data_mut(|d| {
            d.insert_temp(editing_id, true);
            d.insert_temp(just_entered_id, true);
        });
        // Open popup
        let is_open = ui.ctx().data_mut(|d| d.get_temp::<bool>(popup_id).unwrap_or(false));
        if !is_open {
            ui.ctx().data_mut(|d| {
                d.insert_temp(popup_id, true);
                d.insert_temp(popup_time_id, now);
            });
        }
    }

    // ── "Comment" label bottom-left ───────────────────────────
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new(("comment_overlay", node_id)),
    ));
    let node_rect = body_rect.expand(12.0); // account for frame margin
    painter.text(
        egui::pos2(node_rect.left() + 14.0, node_rect.bottom() - 6.0),
        egui::Align2::LEFT_BOTTOM,
        "Comment",
        egui::FontId::proportional(10.0),
        egui::Color32::from_rgb(70, 70, 75),
    );

    // ── Paper fold (top-right corner) ─────────────────────────
    let tr = node_rect.right_top();
    let fold_dark = egui::Color32::from_rgb(
        bg_color[0].saturating_sub(12),
        bg_color[1].saturating_sub(12),
        bg_color[2].saturating_sub(12),
    );
    let fold_light = egui::Color32::from_rgb(
        bg_color[0].saturating_add(20),
        bg_color[1].saturating_add(20),
        bg_color[2].saturating_add(20),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(tr.x - fold_size, tr.y),
            egui::pos2(tr.x, tr.y),
            egui::pos2(tr.x, tr.y + fold_size),
        ],
        fold_dark,
        egui::Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(tr.x - fold_size, tr.y),
            egui::pos2(tr.x, tr.y + fold_size),
            egui::pos2(tr.x - fold_size, tr.y + fold_size),
        ],
        fold_light,
        egui::Stroke::NONE,
    ));

    // ── Accent border when popup is open or editing ───────────
    let popup_open = ui.ctx().data_mut(|d| d.get_temp::<bool>(popup_id).unwrap_or(false));
    if popup_open || is_editing {
        let accent = ui.ctx().data_mut(|d| d.get_temp::<[u8; 3]>(egui::Id::new("theme_accent")))
            .unwrap_or([80, 160, 255]);
        painter.rect_stroke(node_rect.expand(2.0), 8.0,
            egui::Stroke::new(2.0, egui::Color32::from_rgb(accent[0], accent[1], accent[2])),
            egui::StrokeKind::Outside);
    }

    // ── Options popup ─────────────────────────────────────────
    if popup_open {
        let popup_pos = egui::pos2(node_rect.right() + 6.0, node_rect.top());
        let opened_time = ui.ctx().data_mut(|d| d.get_temp::<f64>(popup_time_id).unwrap_or(0.0));

        let area_resp = egui::Area::new(egui::Id::new(("comment_opts", node_id)))
            .fixed_pos(popup_pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).rounding(10.0).inner_margin(8.0).show(ui, |ui| {
                    ui.set_min_width(130.0);

                    // Color picker
                    ui.horizontal(|ui| {
                        let mut color = egui::Color32::from_rgb(bg_color[0], bg_color[1], bg_color[2]);
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            let a = color.to_array();
                            *bg_color = [a[0], a[1], a[2]];
                        }
                        ui.label("Color");
                    });

                    // Font size S/M/L
                    ui.horizontal(|ui| {
                        ui.label("Size");
                        for (label, size) in [("S", 11.0), ("M", 14.0), ("L", 18.0)] {
                            let selected = (font_size - size).abs() < 0.5;
                            if ui.selectable_label(selected, label).clicked() {
                                ui.ctx().data_mut(|d| d.insert_temp(font_size_id, size));
                            }
                        }
                    });

                    ui.separator();

                    if ui.add(egui::Button::new(
                        egui::RichText::new(format!("{} Copy", crate::icons::COPY))
                    ).frame(false)).clicked() {
                        ui.ctx().copy_text(text.clone());
                        ui.ctx().data_mut(|d| d.insert_temp(popup_id, false));
                    }

                    if ui.add(egui::Button::new(
                        egui::RichText::new(format!("{} Delete", crate::icons::TRASH))
                            .color(egui::Color32::from_rgb(200, 80, 80))
                    ).frame(false)).clicked() {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new(("comment_delete_action", node_id)), true);
                            d.insert_temp(popup_id, false);
                        });
                    }

                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(format!("ID: {}", node_id)).small().color(egui::Color32::from_rgb(80, 80, 80)));
                });
            });

        // Close logic
        let popup_rect = area_resp.response.rect;
        let esc = ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));
        let click_outside = if now - opened_time > 0.3 {
            ui.ctx().input(|i| i.pointer.button_clicked(egui::PointerButton::Primary))
                && ui.ctx().pointer_latest_pos().map(|p| {
                    !popup_rect.contains(p) && !node_rect.contains(p)
                }).unwrap_or(false)
        } else { false };

        if esc || click_outside {
            ui.ctx().data_mut(|d| {
                d.insert_temp(popup_id, false);
                d.insert_temp(editing_id, false);
            });
        }
    }
}
