use crate::graph::{PortDef, PortKind, PortValue, Graph};
use crate::node_trait::{NodeBehavior, RenderContext};
use serde::{Serialize, Deserialize};
use eframe::egui;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayNode {
    #[serde(default)]
    pub history: Vec<f32>,
    #[serde(default = "default_history_max")]
    pub history_max: usize,
    #[serde(default)]
    pub scope_min: f32,
    #[serde(default = "default_one")]
    pub scope_max: f32,
    #[serde(default = "default_scope_height")]
    pub scope_height: f32,
    #[serde(default)]
    pub paused: bool,
    #[serde(default = "default_display_color")]
    pub display_color: [u8; 3],
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_true")]
    pub auto_fit: bool,
}

fn default_history_max() -> usize { 200 }
fn default_one() -> f32 { 1.0 }
fn default_scope_height() -> f32 { 80.0 }
fn default_display_color() -> [u8; 3] { [80, 200, 120] }
fn default_true() -> bool { true }

impl Default for DisplayNode {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            history_max: 200,
            scope_min: 0.0,
            scope_max: 1.0,
            scope_height: 80.0,
            paused: false,
            display_color: [80, 200, 120],
            label: String::new(),
            auto_fit: true,
        }
    }
}

impl NodeBehavior for DisplayNode {
    fn title(&self) -> &str { "Display" }

    fn inputs(&self) -> Vec<PortDef> {
        vec![PortDef::new("Value", PortKind::Generic)]
    }

    fn outputs(&self) -> Vec<PortDef> { vec![] }

    fn color_hint(&self) -> [u8; 3] { self.display_color }
    fn inline_ports(&self) -> bool { true }
    fn custom_render(&self) -> bool { true }
    fn no_title(&self) -> bool { true }
    fn min_width(&self) -> Option<f32> { Some(160.0) }

    fn render_background(&self, painter: &egui::Painter, rect: egui::Rect) -> Option<egui::Frame> {
        painter.rect_filled(rect, 8.0, egui::Color32::from_rgb(15, 15, 20));
        Some(egui::Frame::NONE.inner_margin(6.0))
    }

    fn evaluate(&mut self, inputs: &[PortValue]) -> Vec<(usize, PortValue)> {
        if let Some(v) = inputs.first() {
            vec![(0, v.clone())]
        } else {
            vec![]
        }
    }

    fn type_tag(&self) -> &str { "display" }

    fn save_state(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    fn load_state(&mut self, state: &serde_json::Value) {
        if let Ok(loaded) = serde_json::from_value::<DisplayNode>(state.clone()) {
            *self = loaded;
        }
    }

    fn render_with_context(&mut self, ui: &mut egui::Ui, ctx: &mut RenderContext) {
        let is_wired = ctx.connections.iter().any(|c| c.to_node == ctx.node_id && c.to_port == 0);
        let val = Graph::static_input_value(ctx.connections, ctx.values, ctx.node_id, 0);
        let current = val.as_float();
        let wave_color = egui::Color32::from_rgb(self.display_color[0], self.display_color[1], self.display_color[2]);

        // Push to history
        if !self.paused && is_wired {
            self.history.push(current);
            while self.history.len() > self.history_max { self.history.remove(0); }
        }

        // Auto-fit range
        if self.auto_fit && !self.history.is_empty() {
            let min_v = self.history.iter().cloned().fold(f32::INFINITY, f32::min);
            let max_v = self.history.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let margin = (max_v - min_v).max(0.01) * 0.1;
            self.scope_min = min_v - margin;
            self.scope_max = max_v + margin;
        }

        // Input port + value
        ui.horizontal(|ui| {
            crate::nodes::inline_port_circle(ui, ctx.node_id, 0, true, ctx.connections, ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Generic);
            ui.label(egui::RichText::new(format!("{:.3}", current)).monospace().strong().color(wave_color));
        });

        // Oscilloscope — FIXED height (not ui.available_height which grows to fill screen)
        let scope_w = ui.available_width().max(80.0);
        let scope_h = self.scope_height.max(60.0);
        let (rect, body_response) = ui.allocate_exact_size(
            egui::vec2(scope_w, scope_h), egui::Sense::click(),
        );
        let painter = ui.painter();
        painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(10, 10, 15));

        // Grid lines
        let grid_color = egui::Color32::from_rgba_unmultiplied(60, 60, 70, 40);
        for i in 1..4 {
            let y = rect.top() + rect.height() * i as f32 / 4.0;
            painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], egui::Stroke::new(0.5, grid_color));
        }

        // Waveform
        if self.history.len() >= 2 {
            let range = (self.scope_max - self.scope_min).max(0.001);
            let points: Vec<egui::Pos2> = self.history.iter().enumerate().map(|(i, &v)| {
                let x = rect.left() + (i as f32 / (self.history.len() - 1).max(1) as f32) * rect.width();
                let t = ((v - self.scope_min) / range).clamp(0.0, 1.0);
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
            format!("{:.1}", self.scope_max), egui::FontId::proportional(8.0), egui::Color32::from_rgb(60, 60, 70));
        painter.text(egui::pos2(rect.left() + 2.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM,
            format!("{:.1}", self.scope_min), egui::FontId::proportional(8.0), egui::Color32::from_rgb(60, 60, 70));

        if self.paused {
            painter.text(egui::pos2(rect.right() - 4.0, rect.top() + 2.0), egui::Align2::RIGHT_TOP,
                "⏸", egui::FontId::proportional(10.0), egui::Color32::from_rgb(200, 200, 80));
        }

        // Label
        if !self.label.is_empty() {
            let dim = ui.visuals().widgets.noninteractive.fg_stroke.color;
            ui.label(egui::RichText::new(self.label.as_str()).small().color(dim));
        }

        // Popup on click
        let popup_id = egui::Id::new(("display_popup_d", ctx.node_id));
        let popup_time_id = egui::Id::new(("display_popup_time_d", ctx.node_id));
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
            let accent = ui.visuals().hyperlink_color;
            let node_rect = ui.min_rect();
            let fg = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new(("display_hl_d", ctx.node_id))));
            fg.rect_stroke(node_rect.expand(3.0), 8.0, egui::Stroke::new(2.0, accent), egui::StrokeKind::Outside);

            let popup_pos = egui::pos2(rect.right() + 8.0, rect.top());
            let opened_time = ui.ctx().data_mut(|d| d.get_temp::<f64>(popup_time_id).unwrap_or(0.0));

            let area_resp = egui::Area::new(egui::Id::new(("display_opts_d", ctx.node_id)))
                .fixed_pos(popup_pos).order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).corner_radius(10.0).inner_margin(10.0).show(ui, |ui| {
                        ui.set_min_width(170.0);

                        ui.horizontal(|ui| {
                            if ui.selectable_label(self.paused, "⏸").on_hover_text("Pause").clicked() { self.paused = !self.paused; }
                            if ui.small_button("↺").on_hover_text("Reset").clicked() { self.history.clear(); }
                            if ui.selectable_label(self.auto_fit, "Auto").on_hover_text("Auto-fit range").clicked() { self.auto_fit = !self.auto_fit; }
                        });

                        if !self.auto_fit {
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label("Min"); ui.add(egui::DragValue::new(&mut self.scope_min).speed(0.1));
                                ui.label("Max"); ui.add(egui::DragValue::new(&mut self.scope_max).speed(0.1));
                            });
                        }

                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Samples");
                            let mut hm = self.history_max as f32;
                            ui.add(egui::DragValue::new(&mut hm).speed(10.0).range(10.0..=10000.0));
                            self.history_max = hm as usize;
                        });
                        ui.horizontal(|ui| {
                            ui.label("Height");
                            ui.add(egui::DragValue::new(&mut self.scope_height).speed(1.0).range(40.0..=400.0).suffix("px"));
                        });

                        ui.separator();
                        ui.horizontal(|ui| {
                            let mut color = egui::Color32::from_rgb(self.display_color[0], self.display_color[1], self.display_color[2]);
                            if ui.color_edit_button_srgba(&mut color).changed() {
                                let a = color.to_array();
                                self.display_color = [a[0], a[1], a[2]];
                            }
                            ui.add(egui::TextEdit::singleline(&mut self.label).hint_text("Label").desired_width(70.0));
                        });
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
}

#[allow(dead_code)]
pub fn register(registry: &mut crate::node_trait::NodeRegistryInner) {
    registry.register("display", |state| {
        if let Ok(node) = serde_json::from_value::<DisplayNode>(state.clone()) {
            Box::new(node)
        } else {
            Box::new(DisplayNode::default())
        }
    });
}
