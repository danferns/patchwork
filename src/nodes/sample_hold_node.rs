//! Sample & Hold — captures a value on trigger, holds it until next trigger.
//!
//! Input 0: Value (any type — float, text, image)
//! Input 1: Trigger (rising edge captures)
//! Output 0: Held value (stays constant between triggers)
//! Output 1: Trigger echo (1.0 on capture frame)
//!
//! Also has a manual "Sample Now" button and a staircase history chart.

use crate::graph::{PortDef, PortKind, PortValue, Graph};
use crate::node_trait::{NodeBehavior, RenderContext};
use serde::{Serialize, Deserialize};
use eframe::egui;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleHoldNode {
    #[serde(default)]
    pub held_float: f32,
    #[serde(default)]
    pub held_text: String,
    #[serde(default)]
    pub is_text: bool,
    #[serde(skip)]
    pub last_trigger: f32,
    #[serde(default)]
    pub history: Vec<f32>,
}

impl Default for SampleHoldNode {
    fn default() -> Self {
        Self {
            held_float: 0.0, held_text: String::new(), is_text: false,
            last_trigger: 0.0, history: Vec::new(),
        }
    }
}

impl NodeBehavior for SampleHoldNode {
    fn title(&self) -> &str { "Sample & Hold" }

    fn inputs(&self) -> Vec<PortDef> {
        vec![PortDef::new("Value", PortKind::Generic), PortDef::new("Trigger", PortKind::Trigger)]
    }

    fn outputs(&self) -> Vec<PortDef> {
        vec![PortDef::new("Out", PortKind::Generic), PortDef::new("Trigger", PortKind::Trigger)]
    }

    fn color_hint(&self) -> [u8; 3] { [120, 200, 160] }
    fn inline_ports(&self) -> bool { true }

    fn evaluate(&mut self, inputs: &[PortValue]) -> Vec<(usize, PortValue)> {
        let trigger_val = inputs.get(1).map(|v| v.as_float()).unwrap_or(0.0);
        let rising_edge = trigger_val > 0.5 && self.last_trigger <= 0.5;
        self.last_trigger = trigger_val;

        if rising_edge {
            if let Some(val) = inputs.first() {
                match val {
                    PortValue::Float(f) => {
                        self.held_float = *f;
                        self.is_text = false;
                        self.history.push(*f);
                        if self.history.len() > 40 { self.history.remove(0); }
                    }
                    PortValue::Text(t) => {
                        self.held_text = t.clone();
                        self.is_text = true;
                    }
                    _ => {}
                }
            }
        }

        let held = if self.is_text {
            PortValue::Text(self.held_text.clone())
        } else {
            PortValue::Float(self.held_float)
        };

        vec![
            (0, held),
            (1, PortValue::Float(if rising_edge { 1.0 } else { 0.0 })),
        ]
    }

    fn type_tag(&self) -> &str { "sample_hold" }
    fn save_state(&self) -> serde_json::Value { serde_json::to_value(self).unwrap_or_default() }
    fn load_state(&mut self, state: &serde_json::Value) {
        if let Ok(l) = serde_json::from_value::<SampleHoldNode>(state.clone()) {
            self.held_float = l.held_float;
            self.held_text = l.held_text;
            self.is_text = l.is_text;
            self.history = l.history;
        }
    }

    fn render_with_context(&mut self, ui: &mut egui::Ui, ctx: &mut RenderContext) {
        let accent = ui.visuals().hyperlink_color;
        let dim = ui.visuals().widgets.noninteractive.fg_stroke.color;

        let val_wired = ctx.connections.iter().any(|c| c.to_node == ctx.node_id && c.to_port == 0);
        let trig_wired = ctx.connections.iter().any(|c| c.to_node == ctx.node_id && c.to_port == 1);

        // Value input
        ui.horizontal(|ui| {
            crate::nodes::inline_port_circle(ui, ctx.node_id, 0, true, ctx.connections,
                ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Generic);
            ui.label(egui::RichText::new("Value:").small());
            if val_wired {
                let v = Graph::static_input_value(ctx.connections, ctx.values, ctx.node_id, 0);
                let s = match &v {
                    PortValue::Float(f) => format!("{:.3}", f),
                    PortValue::Text(t) => if t.len() > 16 { format!("\"{}...\"", &t[..16]) } else { format!("\"{}\"", t) },
                    _ => "—".into(),
                };
                ui.label(egui::RichText::new(s).small().color(accent));
            } else {
                ui.label(egui::RichText::new("—").small().color(dim));
            }
        });

        // Trigger input
        ui.horizontal(|ui| {
            crate::nodes::inline_port_circle(ui, ctx.node_id, 1, true, ctx.connections,
                ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Trigger);
            ui.label(egui::RichText::new("Trigger:").small());
            if trig_wired {
                let t = Graph::static_input_value(ctx.connections, ctx.values, ctx.node_id, 1).as_float();
                let col = if t > 0.5 { egui::Color32::from_rgb(255, 200, 60) } else { dim };
                ui.label(egui::RichText::new(format!("{:.1}", t)).small().color(col));
            } else {
                ui.label(egui::RichText::new("—").small().color(dim));
            }
        });

        ui.separator();

        // Manual sample button + trigger output
        let trigger_val = Graph::static_input_value(ctx.connections, ctx.values, ctx.node_id, 1).as_float();
        let rising_edge = trigger_val > 0.5 && self.last_trigger <= 0.5;

        ui.horizontal(|ui| {
            if ui.button("Sample Now").clicked() && val_wired {
                let v = Graph::static_input_value(ctx.connections, ctx.values, ctx.node_id, 0);
                match &v {
                    PortValue::Float(f) => {
                        self.held_float = *f; self.is_text = false;
                        self.history.push(*f);
                        if self.history.len() > 40 { self.history.remove(0); }
                    }
                    PortValue::Text(t) => { self.held_text = t.clone(); self.is_text = true; }
                    _ => {}
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                crate::nodes::inline_port_circle(ui, ctx.node_id, 1, false, ctx.connections,
                    ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Trigger);
            });
        });

        // Held value display
        ui.horizontal(|ui| {
            let dot_color = if rising_edge { egui::Color32::from_rgb(255, 220, 60) } else { dim };
            let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(dot_rect.center(), if rising_edge { 5.0 } else { 3.0 }, dot_color);

            ui.label(egui::RichText::new("Held:").small().strong());
            if self.is_text {
                let display = if self.held_text.len() > 20 { format!("\"{}...\"", &self.held_text[..20]) }
                    else { format!("\"{}\"", &self.held_text) };
                ui.label(egui::RichText::new(display).small().color(egui::Color32::from_rgb(80, 220, 80)));
            } else {
                ui.label(egui::RichText::new(format!("{:.4}", self.held_float)).strong()
                    .color(egui::Color32::from_rgb(255, 220, 80)));
            }
        });

        ui.separator();

        // Staircase visualization
        if !self.history.is_empty() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{} samples", self.history.len())).small().color(dim));
                if ui.small_button("Clear").clicked() { self.history.clear(); }
            });

            let (rect, _) = ui.allocate_exact_size(egui::vec2(150.0, 60.0), egui::Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

            let min_v = self.history.iter().cloned().fold(f32::INFINITY, f32::min);
            let max_v = self.history.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let range = (max_v - min_v).max(0.01);
            let lo = min_v - range * 0.1;
            let hi = max_v + range * 0.1;
            let pad = 4.0;
            let n = self.history.len();
            let step_w = (rect.width() - pad * 2.0) / n.max(1) as f32;

            let stair_color = egui::Color32::from_rgb(80, 200, 120);
            for (i, val) in self.history.iter().enumerate() {
                let x = rect.left() + pad + i as f32 * step_w;
                let y = rect.bottom() - pad - ((val - lo) / (hi - lo)) * (rect.height() - pad * 2.0);
                let y = y.clamp(rect.top() + pad, rect.bottom() - pad);
                painter.line_segment([egui::pos2(x, y), egui::pos2(x + step_w, y)],
                    egui::Stroke::new(2.0, stair_color));
                if i + 1 < n {
                    let next_y = rect.bottom() - pad - ((self.history[i + 1] - lo) / (hi - lo)) * (rect.height() - pad * 2.0);
                    painter.line_segment([egui::pos2(x + step_w, y), egui::pos2(x + step_w, next_y.clamp(rect.top() + pad, rect.bottom() - pad))],
                        egui::Stroke::new(1.0, stair_color.gamma_multiply(0.4)));
                }
            }
        } else {
            ui.label(egui::RichText::new("No samples yet").small().color(dim));
        }

        ui.separator();

        // Output port
        let out_val = if self.is_text {
            format!("\"{}\"", if self.held_text.len() > 10 { &self.held_text[..10] } else { &self.held_text })
        } else {
            format!("{:.3}", self.held_float)
        };
        crate::nodes::output_port_row(ui, "Out", &out_val, ctx.node_id, 0,
            ctx.port_positions, ctx.dragging_from, ctx.connections, ctx.pending_disconnects, PortKind::Generic);
    }
}

#[allow(dead_code)]
pub fn register(registry: &mut crate::node_trait::NodeRegistryInner) {
    registry.register("sample_hold", |state| {
        if let Ok(n) = serde_json::from_value::<SampleHoldNode>(state.clone()) { Box::new(n) }
        else { Box::new(SampleHoldNode::default()) }
    });
}
