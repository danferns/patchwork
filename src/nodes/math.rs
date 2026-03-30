#![allow(dead_code)]
use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
) {
    let result = values.get(&(node_id, 0)).cloned().unwrap_or(PortValue::None);
    ui.label(format!("= {}", result));
}
