use crate::graph::{PortDef, PortKind, PortValue};
use crate::node_trait::{NodeBehavior, RenderContext};
use serde::{Serialize, Deserialize};
use eframe::egui;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleNode {
    #[serde(default)]
    pub messages: Vec<String>,
    #[serde(skip)]
    pub last_logged: String,
    #[serde(skip, default = "std::time::Instant::now")]
    pub start_time: std::time::Instant,
}

impl Default for ConsoleNode {
    fn default() -> Self {
        Self { messages: Vec::new(), last_logged: String::new(), start_time: std::time::Instant::now() }
    }
}

impl NodeBehavior for ConsoleNode {
    fn title(&self) -> &str { "Console" }
    fn inputs(&self) -> Vec<PortDef> { vec![PortDef::new("Log", PortKind::Generic)] }
    fn outputs(&self) -> Vec<PortDef> { vec![] }
    fn color_hint(&self) -> [u8; 3] { [100, 150, 100] }
    fn inline_ports(&self) -> bool { true }

    fn evaluate(&mut self, inputs: &[PortValue]) -> Vec<(usize, PortValue)> {
        if let Some(val) = inputs.first() {
            let text = format!("{}", val);
            if !text.is_empty() && text != "—" && text != self.last_logged {
                let secs = self.start_time.elapsed().as_secs();
                let mins = secs / 60;
                let s = secs % 60;
                self.messages.push(format!("[{:02}:{:02}] {}", mins, s, text));
                if self.messages.len() > 200 {
                    self.messages.drain(..self.messages.len() - 200);
                }
                self.last_logged = text;
            }
        }
        vec![]
    }

    fn type_tag(&self) -> &str { "console" }
    fn save_state(&self) -> serde_json::Value { serde_json::json!({ "messages": self.messages }) }
    fn load_state(&mut self, state: &serde_json::Value) {
        if let Some(msgs) = state.get("messages").and_then(|v| v.as_array()) {
            self.messages = msgs.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
        }
    }

    fn render_with_context(&mut self, ui: &mut egui::Ui, ctx: &mut RenderContext) {
        let dim = ui.visuals().widgets.noninteractive.fg_stroke.color;

        // Input port
        ui.horizontal(|ui| {
            crate::nodes::inline_port_circle(ui, ctx.node_id, 0, true, ctx.connections,
                ctx.port_positions, ctx.dragging_from, ctx.pending_disconnects, PortKind::Generic);
            let connected = ctx.connections.iter().any(|c| c.to_node == ctx.node_id && c.to_port == 0);
            if connected {
                ui.label(egui::RichText::new("Logging").small().color(egui::Color32::from_rgb(80, 200, 80)));
            } else {
                ui.label(egui::RichText::new("Connect to log").small().color(dim));
            }
        });

        ui.horizontal(|ui| {
            if ui.small_button("Clear").clicked() {
                self.messages.clear();
                self.last_logged.clear();
            }
            ui.label(egui::RichText::new(format!("{} msgs", self.messages.len())).small().color(dim));
        });

        ui.separator();

        egui::ScrollArea::vertical().max_height(200.0).stick_to_bottom(true).show(ui, |ui| {
            if self.messages.is_empty() {
                ui.label(egui::RichText::new("No messages yet").small().italics().color(dim));
            }
            for msg in &self.messages {
                let color = if msg.contains("error") || msg.contains("Error") || msg.contains("ERR") {
                    egui::Color32::from_rgb(255, 100, 100)
                } else if msg.contains("warn") || msg.contains("Warning") || msg.contains("WARN") {
                    egui::Color32::from_rgb(255, 200, 100)
                } else if msg.contains("ok") || msg.contains("success") || msg.contains("OK") {
                    egui::Color32::from_rgb(100, 255, 100)
                } else {
                    ui.visuals().text_color()
                };
                ui.label(egui::RichText::new(msg).color(color).monospace().size(11.0));
            }
        });
    }
}

#[allow(dead_code)]
pub fn register(registry: &mut crate::node_trait::NodeRegistryInner) {
    registry.register("console", |state| {
        let mut n = ConsoleNode::default();
        n.load_state(state);
        Box::new(n)
    });
}
