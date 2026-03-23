//! Node definitions. Each node type lives in its own file for scalability.
//! To add a new node:
//!   1. Create a new file in src/nodes/ (e.g., my_node.rs)
//!   2. Implement `NodeBehavior` for your type
//!   3. Add a variant to `NodeType` in graph.rs
//!   4. Register it in `catalog()` below and `render_content()` / `evaluate()`

pub mod slider;
pub mod display;
pub mod math;
pub mod file_editor;
pub mod mouse_tracker;
pub mod midi_output;
pub mod wgsl_editor;
pub mod comment;

use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

/// Metadata for the node catalog (add-node menu).
pub struct NodeCatalogEntry {
    pub label: &'static str,
    pub category: &'static str,
    pub factory: fn() -> NodeType,
}

/// Returns the full catalog of available node types.
pub fn catalog() -> Vec<NodeCatalogEntry> {
    vec![
        NodeCatalogEntry {
            label: "Slider",
            category: "Input",
            factory: || NodeType::Slider { value: 0.5, min: 0.0, max: 1.0 },
        },
        NodeCatalogEntry {
            label: "Display",
            category: "Output",
            factory: || NodeType::Display,
        },
        NodeCatalogEntry {
            label: "Add",
            category: "Math",
            factory: || NodeType::Add,
        },
        NodeCatalogEntry {
            label: "Multiply",
            category: "Math",
            factory: || NodeType::Multiply,
        },
        NodeCatalogEntry {
            label: "File Editor",
            category: "IO",
            factory: || NodeType::FileEditor { path: String::new(), content: String::new() },
        },
        NodeCatalogEntry {
            label: "Mouse Tracker",
            category: "Input",
            factory: || NodeType::MouseTracker { x: 0.0, y: 0.0 },
        },
        NodeCatalogEntry {
            label: "MIDI Output",
            category: "IO",
            factory: || NodeType::MidiOutput { channel: 0, note: 60, velocity: 100 },
        },
        NodeCatalogEntry {
            label: "WGSL Editor",
            category: "Shader",
            factory: || NodeType::WgslEditor { code: wgsl_editor::DEFAULT_WGSL.to_string(), path: None },
        },
        NodeCatalogEntry {
            label: "Comment",
            category: "Utility",
            factory: || NodeType::Comment { text: String::new() },
        },
    ]
}

/// Dispatch content rendering to the right node module.
pub fn render_content(
    ui: &mut egui::Ui,
    node_type: &mut NodeType,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    match node_type {
        NodeType::Slider { value, min, max } => slider::render(ui, value, min, max),
        NodeType::Display => display::render(ui, node_id, values, connections),
        NodeType::Add => math::render(ui, "Add", node_id, values),
        NodeType::Multiply => math::render(ui, "Multiply", node_id, values),
        NodeType::FileEditor { path, content } => file_editor::render(ui, path, content),
        NodeType::MouseTracker { x, y } => mouse_tracker::render(ui, *x, *y),
        NodeType::MidiOutput { channel, note, velocity } => {
            midi_output::render(ui, channel, note, velocity, node_id, values, connections)
        }
        NodeType::WgslEditor { code, path } => wgsl_editor::render(ui, code, path),
        NodeType::Comment { text } => comment::render(ui, text),
    }
}
