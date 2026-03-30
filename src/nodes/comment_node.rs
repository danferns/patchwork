//! CommentNode — standalone sticky note node with custom rendering.
//!
//! Demonstrates: custom_render, custom_frame, no_title, color picker,
//! font size control, paper fold effect. This is the proof that plugins
//! can have fully custom visuals.

use crate::graph::{PortDef, PortValue};
use crate::node_trait::NodeBehavior;
use serde::{Serialize, Deserialize};
use eframe::egui;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentNode {
    pub text: String,
    pub bg_color: [u8; 3],
    #[serde(default = "default_font_size")]
    pub font_size: f32,
}

fn default_font_size() -> f32 { 14.0 }

impl Default for CommentNode {
    fn default() -> Self {
        Self {
            text: String::new(),
            bg_color: [45, 45, 50],
            font_size: 14.0,
        }
    }
}

impl NodeBehavior for CommentNode {
    fn title(&self) -> &str { "Comment" }
    fn inputs(&self) -> Vec<PortDef> { vec![] }
    fn outputs(&self) -> Vec<PortDef> { vec![] }
    fn color_hint(&self) -> [u8; 3] { self.bg_color }

    fn custom_render(&self) -> bool { true }
    fn no_title(&self) -> bool { true }
    fn min_width(&self) -> Option<f32> { Some(160.0) }

    fn render_background(&self, painter: &egui::Painter, rect: egui::Rect) -> Option<egui::Frame> {
        let bg = egui::Color32::from_rgb(self.bg_color[0], self.bg_color[1], self.bg_color[2]);
        painter.rect_filled(rect, 8.0, bg);
        // Return frame for margins only (fill already drawn above)
        Some(egui::Frame::NONE.inner_margin(12.0))
    }

    fn evaluate(&mut self, _inputs: &[PortValue]) -> Vec<(usize, PortValue)> { vec![] }

    fn type_tag(&self) -> &str { "comment" }

    fn save_state(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    fn load_state(&mut self, state: &serde_json::Value) {
        if let Ok(loaded) = serde_json::from_value::<CommentNode>(state.clone()) {
            *self = loaded;
        }
    }

    fn render_ui(&mut self, ui: &mut egui::Ui) {
        let node_id = ui.id(); // use egui's widget ID as proxy
        let fold_size = 12.0;

        let popup_id = egui::Id::new(("comment_popup_d", node_id));
        let popup_time_id = egui::Id::new(("comment_popup_time_d", node_id));
        let editing_id = egui::Id::new(("comment_editing_d", node_id));
        let just_entered_id = egui::Id::new(("comment_just_entered_d", node_id));
        let now = ui.ctx().input(|i| i.time);

        let is_editing = ui.ctx().data_mut(|d| d.get_temp::<bool>(editing_id).unwrap_or(false));

        // Allocate body area
        let avail_w = ui.available_width();
        let estimated_h = 80.0_f32.max(self.text.lines().count() as f32 * (self.font_size + 4.0) + 30.0);
        let (body_rect, body_response) = ui.allocate_exact_size(
            egui::vec2(avail_w, estimated_h), egui::Sense::click(),
        );

        let text_rect = body_rect.shrink2(egui::vec2(4.0, 4.0));
        let just_entered = ui.ctx().data_mut(|d| d.get_temp::<bool>(just_entered_id).unwrap_or(false));

        if is_editing {
            let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
            let text_resp = child_ui.add(
                egui::TextEdit::multiline(&mut self.text)
                    .desired_width(text_rect.width())
                    .desired_rows(3)
                    .frame(false)
                    .font(egui::FontId::proportional(self.font_size))
                    .hint_text("Write a note..."),
            );
            if just_entered {
                text_resp.request_focus();
                ui.ctx().data_mut(|d| d.insert_temp(just_entered_id, false));
            }
            if text_resp.lost_focus() && !just_entered {
                ui.ctx().data_mut(|d| d.insert_temp(editing_id, false));
            }
        } else {
            let painter = ui.painter();
            let display_text = if self.text.is_empty() { "Write a note..." } else { self.text.as_str() };
            let color = if self.text.is_empty() {
                ui.visuals().widgets.noninteractive.fg_stroke.color
            } else {
                ui.visuals().text_color()
            };
            painter.text(text_rect.left_top(), egui::Align2::LEFT_TOP, display_text,
                egui::FontId::proportional(self.font_size), color);
        }

        // Click = start editing
        if body_response.clicked() && !is_editing {
            ui.ctx().data_mut(|d| {
                d.insert_temp(editing_id, true);
                d.insert_temp(just_entered_id, true);
            });
            let is_open = ui.ctx().data_mut(|d| d.get_temp::<bool>(popup_id).unwrap_or(false));
            if !is_open {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(popup_id, true);
                    d.insert_temp(popup_time_id, now);
                });
            }
        }

        // "Comment" label bottom-left
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground, egui::Id::new(("comment_overlay_d", node_id))));
        let node_rect = body_rect.expand(12.0);
        let dim = ui.visuals().widgets.noninteractive.fg_stroke.color;
        painter.text(egui::pos2(node_rect.left() + 14.0, node_rect.bottom() - 6.0),
            egui::Align2::LEFT_BOTTOM, "Comment", egui::FontId::proportional(10.0), dim);

        // Paper fold (top-right corner)
        let tr = node_rect.right_top();
        let fold_dark = egui::Color32::from_rgb(
            self.bg_color[0].saturating_sub(12), self.bg_color[1].saturating_sub(12), self.bg_color[2].saturating_sub(12));
        let fold_light = egui::Color32::from_rgb(
            self.bg_color[0].saturating_add(20), self.bg_color[1].saturating_add(20), self.bg_color[2].saturating_add(20));
        painter.add(egui::Shape::convex_polygon(
            vec![egui::pos2(tr.x - fold_size, tr.y), egui::pos2(tr.x, tr.y), egui::pos2(tr.x, tr.y + fold_size)],
            fold_dark, egui::Stroke::NONE));
        painter.add(egui::Shape::convex_polygon(
            vec![egui::pos2(tr.x - fold_size, tr.y), egui::pos2(tr.x, tr.y + fold_size), egui::pos2(tr.x - fold_size, tr.y + fold_size)],
            fold_light, egui::Stroke::NONE));

        // Accent border when editing
        let popup_open = ui.ctx().data_mut(|d| d.get_temp::<bool>(popup_id).unwrap_or(false));
        if popup_open || is_editing {
            let accent = ui.visuals().hyperlink_color;
            painter.rect_stroke(node_rect.expand(2.0), 8.0,
                egui::Stroke::new(2.0, accent), egui::StrokeKind::Outside);
        }

        // Options popup
        if popup_open {
            let popup_pos = egui::pos2(node_rect.right() + 6.0, node_rect.top());
            let opened_time = ui.ctx().data_mut(|d| d.get_temp::<f64>(popup_time_id).unwrap_or(0.0));

            let area_resp = egui::Area::new(egui::Id::new(("comment_opts_d", node_id)))
                .fixed_pos(popup_pos)
                .order(egui::Order::Tooltip)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).corner_radius(10.0).inner_margin(8.0).show(ui, |ui| {
                        ui.set_min_width(130.0);
                        ui.horizontal(|ui| {
                            let mut color = egui::Color32::from_rgb(self.bg_color[0], self.bg_color[1], self.bg_color[2]);
                            if ui.color_edit_button_srgba(&mut color).changed() {
                                let a = color.to_array();
                                self.bg_color = [a[0], a[1], a[2]];
                            }
                            ui.label("Color");
                        });
                        ui.horizontal(|ui| {
                            ui.label("Size");
                            for (label, size) in [("S", 11.0), ("M", 14.0), ("L", 18.0)] {
                                let selected = (self.font_size - size).abs() < 0.5;
                                if ui.selectable_label(selected, label).clicked() {
                                    self.font_size = size;
                                }
                            }
                        });
                    });
                });

            let popup_rect = area_resp.response.rect;
            let esc = ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));
            let click_outside = if now - opened_time > 0.3 {
                ui.ctx().input(|i| i.pointer.button_clicked(egui::PointerButton::Primary))
                    && ui.ctx().pointer_latest_pos().map(|p| !popup_rect.contains(p) && !node_rect.contains(p)).unwrap_or(false)
            } else { false };

            if esc || click_outside {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(popup_id, false);
                    d.insert_temp(editing_id, false);
                });
            }
        }
    }
}

#[allow(dead_code)]
pub fn register(registry: &mut crate::node_trait::NodeRegistryInner) {
    registry.register("comment", |state| {
        if let Ok(node) = serde_json::from_value::<CommentNode>(state.clone()) {
            Box::new(node)
        } else {
            Box::new(CommentNode::default())
        }
    });
}
