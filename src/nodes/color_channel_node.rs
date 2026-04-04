//! ColorChannelNode — Split & adjust R/G/B channels with per-channel level controls.
//! Output 0: Combined image (levels applied). Outputs 1-3: individual R/G/B as grayscale.

use crate::graph::{PortDef, PortKind, PortValue, ImageData};
use crate::node_trait::{NodeBehavior, RenderContext};
use serde::{Serialize, Deserialize};
use eframe::egui;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorChannelNode {
    #[serde(default = "default_one")]
    pub r_level: f32,
    #[serde(default = "default_one")]
    pub g_level: f32,
    #[serde(default = "default_one")]
    pub b_level: f32,
}

fn default_one() -> f32 { 1.0 }

impl Default for ColorChannelNode {
    fn default() -> Self {
        Self { r_level: 1.0, g_level: 1.0, b_level: 1.0 }
    }
}

impl ColorChannelNode {
    fn process(&self, img: &ImageData) -> (Arc<ImageData>, Arc<ImageData>, Arc<ImageData>, Arc<ImageData>) {
        let len = img.pixels.len();
        let mut combined = img.pixels.clone();
        let mut r_img = vec![0u8; len];
        let mut g_img = vec![0u8; len];
        let mut b_img = vec![0u8; len];

        for i in (0..len).step_by(4) {
            if i + 3 >= len { break; }

            let r = (img.pixels[i] as f32 * self.r_level).clamp(0.0, 255.0) as u8;
            let g = (img.pixels[i + 1] as f32 * self.g_level).clamp(0.0, 255.0) as u8;
            let b = (img.pixels[i + 2] as f32 * self.b_level).clamp(0.0, 255.0) as u8;
            let a = img.pixels[i + 3];

            // Combined output
            combined[i] = r;
            combined[i + 1] = g;
            combined[i + 2] = b;
            combined[i + 3] = a;

            // Individual channels as grayscale
            r_img[i] = r; r_img[i + 1] = r; r_img[i + 2] = r; r_img[i + 3] = a;
            g_img[i] = g; g_img[i + 1] = g; g_img[i + 2] = g; g_img[i + 3] = a;
            b_img[i] = b; b_img[i + 1] = b; b_img[i + 2] = b; b_img[i + 3] = a;
        }

        (
            Arc::new(ImageData { width: img.width, height: img.height, pixels: combined }),
            Arc::new(ImageData { width: img.width, height: img.height, pixels: r_img }),
            Arc::new(ImageData { width: img.width, height: img.height, pixels: g_img }),
            Arc::new(ImageData { width: img.width, height: img.height, pixels: b_img }),
        )
    }
}

impl NodeBehavior for ColorChannelNode {
    fn title(&self) -> &str { "Color Channel" }
    fn inputs(&self) -> Vec<PortDef> {
        vec![
            PortDef::new("Image", PortKind::Image),
            PortDef::new("R Level", PortKind::Number),
            PortDef::new("G Level", PortKind::Number),
            PortDef::new("B Level", PortKind::Number),
        ]
    }
    fn outputs(&self) -> Vec<PortDef> {
        vec![
            PortDef::new("Image", PortKind::Image),
            PortDef::new("R", PortKind::Image),
            PortDef::new("G", PortKind::Image),
            PortDef::new("B", PortKind::Image),
        ]
    }
    fn color_hint(&self) -> [u8; 3] { [200, 160, 120] }
    fn inline_ports(&self) -> bool { true }

    fn evaluate(&mut self, inputs: &[PortValue]) -> Vec<(usize, PortValue)> {
        if let Some(PortValue::Float(v)) = inputs.get(1) { self.r_level = v.clamp(0.0, 2.0); }
        if let Some(PortValue::Float(v)) = inputs.get(2) { self.g_level = v.clamp(0.0, 2.0); }
        if let Some(PortValue::Float(v)) = inputs.get(3) { self.b_level = v.clamp(0.0, 2.0); }

        match inputs.first() {
            Some(PortValue::Image(img)) => {
                let (combined, r, g, b) = self.process(img);
                vec![
                    (0, PortValue::Image(combined)),
                    (1, PortValue::Image(r)),
                    (2, PortValue::Image(g)),
                    (3, PortValue::Image(b)),
                ]
            }
            _ => vec![
                (0, PortValue::None),
                (1, PortValue::None),
                (2, PortValue::None),
                (3, PortValue::None),
            ],
        }
    }

    fn type_tag(&self) -> &str { "color_channel" }
    fn save_state(&self) -> serde_json::Value { serde_json::to_value(self).unwrap_or_default() }
    fn load_state(&mut self, state: &serde_json::Value) {
        if let Ok(l) = serde_json::from_value::<ColorChannelNode>(state.clone()) { *self = l; }
    }

    fn render_with_context(&mut self, ui: &mut egui::Ui, ctx: &mut RenderContext) {
        // Image input
        ui.horizontal(|ui| {
            crate::nodes::inline_port_circle(ui, ctx.node_id, 0, true, ctx.connections,
                ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Image);
            ui.label(egui::RichText::new("Image").small());
        });

        ui.separator();

        // R level
        let r_wired = ctx.connections.iter().any(|c| c.to_node == ctx.node_id && c.to_port == 1);
        ui.horizontal(|ui| {
            crate::nodes::inline_port_circle(ui, ctx.node_id, 1, true, ctx.connections,
                ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Number);
            ui.label(egui::RichText::new("R").small().color(egui::Color32::from_rgb(255, 100, 100)));
            if r_wired {
                ui.label(egui::RichText::new(format!("{:.2}", self.r_level)).small().color(egui::Color32::from_rgb(255, 100, 100)));
            } else {
                ui.add(egui::Slider::new(&mut self.r_level, 0.0..=2.0).step_by(0.01).show_value(true));
            }
            // R output port on the right
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                crate::nodes::inline_port_circle(ui, ctx.node_id, 1, false, ctx.connections,
                    ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Image);
            });
        });

        // G level
        let g_wired = ctx.connections.iter().any(|c| c.to_node == ctx.node_id && c.to_port == 2);
        ui.horizontal(|ui| {
            crate::nodes::inline_port_circle(ui, ctx.node_id, 2, true, ctx.connections,
                ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Number);
            ui.label(egui::RichText::new("G").small().color(egui::Color32::from_rgb(100, 255, 100)));
            if g_wired {
                ui.label(egui::RichText::new(format!("{:.2}", self.g_level)).small().color(egui::Color32::from_rgb(100, 255, 100)));
            } else {
                ui.add(egui::Slider::new(&mut self.g_level, 0.0..=2.0).step_by(0.01).show_value(true));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                crate::nodes::inline_port_circle(ui, ctx.node_id, 2, false, ctx.connections,
                    ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Image);
            });
        });

        // B level
        let b_wired = ctx.connections.iter().any(|c| c.to_node == ctx.node_id && c.to_port == 3);
        ui.horizontal(|ui| {
            crate::nodes::inline_port_circle(ui, ctx.node_id, 3, true, ctx.connections,
                ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Number);
            ui.label(egui::RichText::new("B").small().color(egui::Color32::from_rgb(100, 100, 255)));
            if b_wired {
                ui.label(egui::RichText::new(format!("{:.2}", self.b_level)).small().color(egui::Color32::from_rgb(100, 100, 255)));
            } else {
                ui.add(egui::Slider::new(&mut self.b_level, 0.0..=2.0).step_by(0.01).show_value(true));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                crate::nodes::inline_port_circle(ui, ctx.node_id, 3, false, ctx.connections,
                    ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Image);
            });
        });

        ui.separator();

        // Combined output
        crate::nodes::audio_port_row(ui, "Image", ctx.node_id, 0, false, ctx.port_positions,
            ctx.dragging_from, ctx.connections, ctx.pending_disconnects, PortKind::Image);
    }
}

pub fn register(registry: &mut crate::node_trait::NodeRegistryInner) {
    registry.register("color_channel", |state| {
        if let Ok(n) = serde_json::from_value::<ColorChannelNode>(state.clone()) { Box::new(n) }
        else { Box::new(ColorChannelNode::default()) }
    });
}
