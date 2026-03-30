use crate::graph::{PortDef, PortKind, PortValue};
use crate::node_trait::{NodeBehavior, RenderContext};
use serde::{Serialize, Deserialize};
use eframe::egui;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonExtractNode {
    pub path: String,
}

impl Default for JsonExtractNode {
    fn default() -> Self {
        Self { path: String::new() }
    }
}

impl NodeBehavior for JsonExtractNode {
    fn title(&self) -> &str { "JSON Extract" }

    fn inputs(&self) -> Vec<PortDef> {
        vec![PortDef::new("JSON", PortKind::Text)]
    }

    fn outputs(&self) -> Vec<PortDef> {
        vec![PortDef::new("Value", PortKind::Generic)]
    }

    fn color_hint(&self) -> [u8; 3] { [200, 160, 60] }

    fn evaluate(&mut self, inputs: &[PortValue]) -> Vec<(usize, PortValue)> {
        let json_text = match inputs.first() {
            Some(PortValue::Text(s)) => s.clone(),
            _ => String::new(),
        };
        let extracted = if !json_text.is_empty() && !self.path.is_empty() {
            crate::graph::extract_json_path_pub(&json_text, &self.path)
        } else {
            String::new()
        };
        vec![(0, PortValue::Text(extracted))]
    }

    fn type_tag(&self) -> &str { "json_extract" }

    fn save_state(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    fn load_state(&mut self, state: &serde_json::Value) {
        if let Ok(loaded) = serde_json::from_value::<JsonExtractNode>(state.clone()) {
            *self = loaded;
        }
    }

    fn render_with_context(&mut self, ui: &mut egui::Ui, ctx: &mut RenderContext) {
        ui.horizontal(|ui| {
            ui.label("Path:");
            ui.text_edit_singleline(&mut self.path);
        });
        ui.label(egui::RichText::new("(dot-separated, e.g. data.items.0.name)").small()
            .color(ui.visuals().widgets.noninteractive.fg_stroke.color));

        let output_val = ctx.values.get(&(ctx.node_id, 0));
        ui.separator();
        match output_val {
            Some(PortValue::Text(s)) if !s.is_empty() => {
                ui.label("Extracted:");
                egui::ScrollArea::vertical().max_height(80.0).show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut s.clone())
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .interactive(false));
                });
            }
            _ => {
                let dim = ui.visuals().widgets.noninteractive.fg_stroke.color;
                if self.path.is_empty() {
                    ui.colored_label(dim, "(enter path)");
                } else {
                    ui.colored_label(egui::Color32::from_rgb(200, 80, 80), "(no match)");
                }
            }
        }
    }
}

#[allow(dead_code)]
pub fn register(registry: &mut crate::node_trait::NodeRegistryInner) {
    registry.register("json_extract", |state| {
        if let Ok(node) = serde_json::from_value::<JsonExtractNode>(state.clone()) {
            Box::new(node)
        } else {
            Box::new(JsonExtractNode::default())
        }
    });
}
