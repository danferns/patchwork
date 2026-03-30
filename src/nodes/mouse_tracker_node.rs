use crate::graph::{PortDef, PortKind, PortValue};
use crate::node_trait::NodeBehavior;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MouseTrackerNode {
    #[serde(skip)]
    pub x: f32,
    #[serde(skip)]
    pub y: f32,
}

impl NodeBehavior for MouseTrackerNode {
    fn title(&self) -> &str { "Mouse Tracker" }
    fn inputs(&self) -> Vec<PortDef> { vec![] }
    fn outputs(&self) -> Vec<PortDef> {
        vec![PortDef::new("X", PortKind::Number), PortDef::new("Y", PortKind::Number)]
    }
    fn color_hint(&self) -> [u8; 3] { [200, 100, 100] }

    fn evaluate(&mut self, _inputs: &[PortValue]) -> Vec<(usize, PortValue)> {
        vec![
            (0, PortValue::Float(self.x)),
            (1, PortValue::Float(self.y)),
        ]
    }

    fn type_tag(&self) -> &str { "mouse_tracker" }
    fn save_state(&self) -> serde_json::Value { serde_json::json!({}) }

    fn render_ui(&mut self, ui: &mut eframe::egui::Ui) {
        // Read pointer position from egui
        if let Some(pos) = ui.ctx().pointer_latest_pos() {
            self.x = pos.x;
            self.y = pos.y;
        }
        let dim = ui.visuals().widgets.noninteractive.fg_stroke.color;
        ui.label(format!("X: {:.1}", self.x));
        ui.label(format!("Y: {:.1}", self.y));
        ui.label(eframe::egui::RichText::new("(tracks pointer position)").small().color(dim));
    }
}

#[allow(dead_code)]
pub fn register(registry: &mut crate::node_trait::NodeRegistryInner) {
    registry.register("mouse_tracker", |_| Box::new(MouseTrackerNode::default()));
}
