use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type NodeId = u64;

// ── Values ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PortValue {
    Float(f32),
    None,
}

impl std::fmt::Display for PortValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortValue::Float(v) => write!(f, "{:.3}", v),
            PortValue::None => write!(f, "\u{2014}"),
        }
    }
}

// ── Port definitions ────────────────────────────────────────────────────────

pub struct PortDef {
    pub name: &'static str,
}

// ── Node types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Slider { value: f32, min: f32, max: f32 },
    Display,
    Add,
    Multiply,
    FileEditor { path: String, content: String },
    MouseTracker { x: f32, y: f32 },
    MidiOutput { channel: u8, note: u8, velocity: u8 },
    WgslEditor { code: String, path: Option<String> },
    Comment { text: String },
}

impl NodeType {
    pub fn title(&self) -> &str {
        match self {
            NodeType::Slider { .. } => "Slider",
            NodeType::Display => "Display",
            NodeType::Add => "Add",
            NodeType::Multiply => "Multiply",
            NodeType::FileEditor { .. } => "File Editor",
            NodeType::MouseTracker { .. } => "Mouse Tracker",
            NodeType::MidiOutput { .. } => "MIDI Output",
            NodeType::WgslEditor { .. } => "WGSL Editor",
            NodeType::Comment { .. } => "Comment",
        }
    }

    pub fn inputs(&self) -> Vec<PortDef> {
        match self {
            NodeType::Slider { .. } => vec![],
            NodeType::Display => vec![PortDef { name: "Value" }],
            NodeType::Add => vec![PortDef { name: "A" }, PortDef { name: "B" }],
            NodeType::Multiply => vec![PortDef { name: "A" }, PortDef { name: "B" }],
            NodeType::FileEditor { .. } => vec![],
            NodeType::MouseTracker { .. } => vec![],
            NodeType::MidiOutput { .. } => vec![
                PortDef { name: "Note" },
                PortDef { name: "Velocity" },
            ],
            NodeType::WgslEditor { .. } => vec![PortDef { name: "Param" }],
            NodeType::Comment { .. } => vec![],
        }
    }

    pub fn outputs(&self) -> Vec<PortDef> {
        match self {
            NodeType::Slider { .. } => vec![PortDef { name: "Value" }],
            NodeType::Display => vec![],
            NodeType::Add => vec![PortDef { name: "Result" }],
            NodeType::Multiply => vec![PortDef { name: "Result" }],
            NodeType::FileEditor { .. } => vec![],
            NodeType::MouseTracker { .. } => vec![
                PortDef { name: "X" },
                PortDef { name: "Y" },
            ],
            NodeType::MidiOutput { .. } => vec![],
            NodeType::WgslEditor { .. } => vec![],
            NodeType::Comment { .. } => vec![],
        }
    }

    pub fn color_hint(&self) -> [u8; 3] {
        match self {
            NodeType::Slider { .. } => [80, 160, 255],
            NodeType::Display => [100, 200, 100],
            NodeType::Add | NodeType::Multiply => [200, 160, 80],
            NodeType::FileEditor { .. } => [180, 120, 200],
            NodeType::MouseTracker { .. } => [200, 100, 100],
            NodeType::MidiOutput { .. } => [100, 180, 180],
            NodeType::WgslEditor { .. } => [220, 140, 60],
            NodeType::Comment { .. } => [140, 140, 140],
        }
    }
}

// ── Node & Connection ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub node_type: NodeType,
    pub pos: [f32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub from_node: NodeId,
    pub from_port: usize,
    pub to_node: NodeId,
    pub to_port: usize,
}

// ── Graph ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: HashMap<NodeId, Node>,
    pub connections: Vec<Connection>,
    next_id: u64,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            connections: Vec::new(),
            next_id: 1,
        }
    }

    pub fn add_node(&mut self, node_type: NodeType, pos: [f32; 2]) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.insert(id, Node { id, node_type, pos });
        id
    }

    pub fn remove_node(&mut self, id: NodeId) {
        self.nodes.remove(&id);
        self.connections.retain(|c| c.from_node != id && c.to_node != id);
    }

    pub fn add_connection(&mut self, from_node: NodeId, from_port: usize, to_node: NodeId, to_port: usize) {
        self.connections.retain(|c| !(c.to_node == to_node && c.to_port == to_port));
        self.connections.push(Connection { from_node, from_port, to_node, to_port });
    }

    /// Evaluate the graph, returning output values keyed by (node_id, output_port_index).
    pub fn evaluate(&self) -> HashMap<(NodeId, usize), PortValue> {
        let mut values: HashMap<(NodeId, usize), PortValue> = HashMap::new();

        for _ in 0..5 {
            for (&id, node) in &self.nodes {
                let inputs = self.collect_inputs(id, &values);
                match &node.node_type {
                    NodeType::Slider { value, .. } => {
                        values.insert((id, 0), PortValue::Float(*value));
                    }
                    NodeType::Add => {
                        let a = as_float(&inputs, 0);
                        let b = as_float(&inputs, 1);
                        values.insert((id, 0), PortValue::Float(a + b));
                    }
                    NodeType::Multiply => {
                        let a = as_float(&inputs, 0);
                        let b = as_float(&inputs, 1);
                        values.insert((id, 0), PortValue::Float(a * b));
                    }
                    NodeType::MouseTracker { x, y } => {
                        values.insert((id, 0), PortValue::Float(*x));
                        values.insert((id, 1), PortValue::Float(*y));
                    }
                    _ => {}
                }
            }
        }
        values
    }

    fn collect_inputs(&self, node_id: NodeId, values: &HashMap<(NodeId, usize), PortValue>) -> Vec<PortValue> {
        let num = self.nodes.get(&node_id).map(|n| n.node_type.inputs().len()).unwrap_or(0);
        let mut inputs = vec![PortValue::None; num];
        for conn in &self.connections {
            if conn.to_node == node_id && conn.to_port < num {
                if let Some(&val) = values.get(&(conn.from_node, conn.from_port)) {
                    inputs[conn.to_port] = val;
                }
            }
        }
        inputs
    }

    /// Standalone helper usable from node rendering code without a Graph instance.
    pub fn static_input_value(
        connections: &[Connection],
        values: &HashMap<(NodeId, usize), PortValue>,
        node_id: NodeId,
        port_idx: usize,
    ) -> PortValue {
        for c in connections {
            if c.to_node == node_id && c.to_port == port_idx {
                return values.get(&(c.from_node, c.from_port)).copied().unwrap_or(PortValue::None);
            }
        }
        PortValue::None
    }
}

fn as_float(inputs: &[PortValue], idx: usize) -> f32 {
    match inputs.get(idx) {
        Some(PortValue::Float(v)) => *v,
        _ => 0.0,
    }
}
