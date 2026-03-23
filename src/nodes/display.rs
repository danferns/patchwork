use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    let val = crate::graph::Graph::static_input_value(connections, values, node_id, 0);
    ui.heading(format!("{}", val));
}
