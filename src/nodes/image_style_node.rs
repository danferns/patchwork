//! ImageStyleNode — Blur, Pixelate, Sharpen for images.

use crate::graph::{PortDef, PortKind, PortValue, ImageData};
use crate::node_trait::{NodeBehavior, RenderContext};
use serde::{Serialize, Deserialize};
use eframe::egui;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StyleMode {
    Blur,
    Pixelate,
    Sharpen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageStyleNode {
    #[serde(default)]
    pub mode: StyleMode,
    #[serde(default = "default_amount")]
    pub amount: f32,
}

fn default_amount() -> f32 { 3.0 }

impl Default for StyleMode {
    fn default() -> Self { StyleMode::Blur }
}

impl Default for ImageStyleNode {
    fn default() -> Self {
        Self { mode: StyleMode::Blur, amount: 3.0 }
    }
}

impl ImageStyleNode {
    fn process(&self, img: &ImageData) -> Arc<ImageData> {
        match self.mode {
            StyleMode::Blur => self.blur(img),
            StyleMode::Pixelate => self.pixelate(img),
            StyleMode::Sharpen => self.sharpen(img),
        }
    }

    fn blur(&self, img: &ImageData) -> Arc<ImageData> {
        let w = img.width as usize;
        let h = img.height as usize;
        let r = (self.amount as usize).max(1).min(20);
        if w == 0 || h == 0 { return Arc::new(img.clone()); }

        // Two-pass separable box blur for performance
        let mut temp = img.pixels.clone();
        let mut out = img.pixels.clone();

        // Horizontal pass
        for y in 0..h {
            for x in 0..w {
                let mut sum = [0u32; 4];
                let mut count = 0u32;
                let x0 = (x as i32 - r as i32).max(0) as usize;
                let x1 = (x + r + 1).min(w);
                for sx in x0..x1 {
                    let si = (y * w + sx) * 4;
                    if si + 3 < img.pixels.len() {
                        sum[0] += img.pixels[si] as u32;
                        sum[1] += img.pixels[si + 1] as u32;
                        sum[2] += img.pixels[si + 2] as u32;
                        sum[3] += img.pixels[si + 3] as u32;
                        count += 1;
                    }
                }
                let di = (y * w + x) * 4;
                if count > 0 && di + 3 < temp.len() {
                    temp[di] = (sum[0] / count) as u8;
                    temp[di + 1] = (sum[1] / count) as u8;
                    temp[di + 2] = (sum[2] / count) as u8;
                    temp[di + 3] = (sum[3] / count) as u8;
                }
            }
        }

        // Vertical pass
        for y in 0..h {
            for x in 0..w {
                let mut sum = [0u32; 4];
                let mut count = 0u32;
                let y0 = (y as i32 - r as i32).max(0) as usize;
                let y1 = (y + r + 1).min(h);
                for sy in y0..y1 {
                    let si = (sy * w + x) * 4;
                    if si + 3 < temp.len() {
                        sum[0] += temp[si] as u32;
                        sum[1] += temp[si + 1] as u32;
                        sum[2] += temp[si + 2] as u32;
                        sum[3] += temp[si + 3] as u32;
                        count += 1;
                    }
                }
                let di = (y * w + x) * 4;
                if count > 0 && di + 3 < out.len() {
                    out[di] = (sum[0] / count) as u8;
                    out[di + 1] = (sum[1] / count) as u8;
                    out[di + 2] = (sum[2] / count) as u8;
                    out[di + 3] = (sum[3] / count) as u8;
                }
            }
        }

        Arc::new(ImageData { width: img.width, height: img.height, pixels: out })
    }

    fn pixelate(&self, img: &ImageData) -> Arc<ImageData> {
        let w = img.width as usize;
        let h = img.height as usize;
        let block = (self.amount as usize).max(2).min(64);
        if w == 0 || h == 0 { return Arc::new(img.clone()); }

        let mut pixels = img.pixels.clone();

        for by in (0..h).step_by(block) {
            for bx in (0..w).step_by(block) {
                let mut sum = [0u32; 4];
                let mut count = 0u32;
                let bw = block.min(w - bx);
                let bh = block.min(h - by);

                for dy in 0..bh {
                    for dx in 0..bw {
                        let si = ((by + dy) * w + (bx + dx)) * 4;
                        if si + 3 < pixels.len() {
                            sum[0] += img.pixels[si] as u32;
                            sum[1] += img.pixels[si + 1] as u32;
                            sum[2] += img.pixels[si + 2] as u32;
                            sum[3] += img.pixels[si + 3] as u32;
                            count += 1;
                        }
                    }
                }

                if count > 0 {
                    let avg = [(sum[0] / count) as u8, (sum[1] / count) as u8,
                               (sum[2] / count) as u8, (sum[3] / count) as u8];
                    for dy in 0..bh {
                        for dx in 0..bw {
                            let di = ((by + dy) * w + (bx + dx)) * 4;
                            if di + 3 < pixels.len() {
                                pixels[di..di + 4].copy_from_slice(&avg);
                            }
                        }
                    }
                }
            }
        }

        Arc::new(ImageData { width: img.width, height: img.height, pixels })
    }

    fn sharpen(&self, img: &ImageData) -> Arc<ImageData> {
        // Unsharp mask: output = original + amount * (original - blurred)
        let strength = self.amount.clamp(0.1, 5.0);
        let mut blur_node = self.clone();
        blur_node.amount = 2.0; // fixed blur radius for unsharp mask
        let blurred = blur_node.blur(img);

        let mut pixels = img.pixels.clone();
        for i in (0..pixels.len()).step_by(4) {
            if i + 2 < pixels.len() && i + 2 < blurred.pixels.len() {
                for c in 0..3 {
                    let orig = img.pixels[i + c] as f32;
                    let blur_val = blurred.pixels[i + c] as f32;
                    let sharpened = orig + strength * (orig - blur_val);
                    pixels[i + c] = sharpened.clamp(0.0, 255.0) as u8;
                }
            }
        }

        Arc::new(ImageData { width: img.width, height: img.height, pixels })
    }
}

impl NodeBehavior for ImageStyleNode {
    fn title(&self) -> &str { "Image Style" }
    fn inputs(&self) -> Vec<PortDef> {
        vec![
            PortDef::new("Image", PortKind::Image),
            PortDef::new("Amount", PortKind::Number),
        ]
    }
    fn outputs(&self) -> Vec<PortDef> { vec![PortDef::new("Image", PortKind::Image)] }
    fn color_hint(&self) -> [u8; 3] { [200, 120, 180] }
    fn inline_ports(&self) -> bool { true }

    fn evaluate(&mut self, inputs: &[PortValue]) -> Vec<(usize, PortValue)> {
        if let Some(PortValue::Float(v)) = inputs.get(1) {
            self.amount = match self.mode {
                StyleMode::Blur => v.clamp(1.0, 20.0),
                StyleMode::Pixelate => v.clamp(2.0, 64.0),
                StyleMode::Sharpen => v.clamp(0.1, 5.0),
            };
        }

        let result = match inputs.first() {
            Some(PortValue::Image(img)) => PortValue::Image(self.process(img)),
            _ => PortValue::None,
        };
        vec![(0, result)]
    }

    fn type_tag(&self) -> &str { "image_style" }
    fn save_state(&self) -> serde_json::Value { serde_json::to_value(self).unwrap_or_default() }
    fn load_state(&mut self, state: &serde_json::Value) {
        if let Ok(l) = serde_json::from_value::<ImageStyleNode>(state.clone()) { *self = l; }
    }

    fn render_with_context(&mut self, ui: &mut egui::Ui, ctx: &mut RenderContext) {
        // Image input
        ui.horizontal(|ui| {
            crate::nodes::inline_port_circle(ui, ctx.node_id, 0, true, ctx.connections,
                ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Image);
            ui.label(egui::RichText::new("Image").small());
        });

        // Mode selector
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Mode").small());
            egui::ComboBox::from_id_salt(egui::Id::new(("style_mode", ctx.node_id)))
                .selected_text(match self.mode {
                    StyleMode::Blur => "Blur",
                    StyleMode::Pixelate => "Pixelate",
                    StyleMode::Sharpen => "Sharpen",
                })
                .width(80.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.mode, StyleMode::Blur, "Blur");
                    ui.selectable_value(&mut self.mode, StyleMode::Pixelate, "Pixelate");
                    ui.selectable_value(&mut self.mode, StyleMode::Sharpen, "Sharpen");
                });
        });

        // Amount slider with port
        let amt_wired = ctx.connections.iter().any(|c| c.to_node == ctx.node_id && c.to_port == 1);
        let (range, step, label) = match self.mode {
            StyleMode::Blur => (1.0..=20.0, 1.0, "Radius"),
            StyleMode::Pixelate => (2.0..=64.0, 1.0, "Block"),
            StyleMode::Sharpen => (0.1..=5.0, 0.1, "Strength"),
        };
        ui.horizontal(|ui| {
            crate::nodes::inline_port_circle(ui, ctx.node_id, 1, true, ctx.connections,
                ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Number);
            ui.label(egui::RichText::new(label).small());
            if amt_wired {
                ui.label(egui::RichText::new(format!("{:.1}", self.amount)).small()
                    .color(egui::Color32::from_rgb(80, 170, 255)));
            } else {
                ui.add(egui::Slider::new(&mut self.amount, range).step_by(step as f64).show_value(true));
            }
        });

        ui.separator();
        crate::nodes::audio_port_row(ui, "Image", ctx.node_id, 0, false, ctx.port_positions,
            ctx.dragging_from, ctx.connections, ctx.pending_disconnects, PortKind::Image);
    }
}

pub fn register(registry: &mut crate::node_trait::NodeRegistryInner) {
    registry.register("image_style", |state| {
        if let Ok(n) = serde_json::from_value::<ImageStyleNode>(state.clone()) { Box::new(n) }
        else { Box::new(ImageStyleNode::default()) }
    });
}
