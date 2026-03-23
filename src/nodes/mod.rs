//! Node definitions — each type in its own file.
//! To add a new node:
//!   1. Create src/nodes/my_node.rs with a pub fn render(...)
//!   2. Add a variant to NodeType in graph.rs (title, inputs, outputs, color_hint)
//!   3. Add evaluation logic in Graph::evaluate() if it produces outputs
//!   4. Register below: pub mod, catalog entry, render_content match arm

pub mod slider;
pub mod display;
pub mod math;
pub mod file;
pub mod text_editor;
pub mod wgsl_viewer;
pub mod mouse_tracker;
pub mod midi_output;
pub mod theme;
pub mod comment;

use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

/// Metadata for the add-node menu.
pub struct NodeCatalogEntry {
    pub label: &'static str,
    pub category: &'static str,
    pub factory: fn() -> NodeType,
}

pub fn catalog() -> Vec<NodeCatalogEntry> {
    vec![
        // ── Input ───────────────────────────────────────────────────────
        NodeCatalogEntry {
            label: "Slider",
            category: "Input",
            factory: || NodeType::Slider { value: 0.5, min: 0.0, max: 1.0 },
        },
        NodeCatalogEntry {
            label: "Mouse Tracker",
            category: "Input",
            factory: || NodeType::MouseTracker { x: 0.0, y: 0.0 },
        },
        // ── Math ────────────────────────────────────────────────────────
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
        // ── IO / Text ───────────────────────────────────────────────────
        NodeCatalogEntry {
            label: "File",
            category: "IO",
            factory: || NodeType::File { path: String::new(), content: String::new() },
        },
        NodeCatalogEntry {
            label: "Text Editor",
            category: "IO",
            factory: || NodeType::TextEditor { content: String::new() },
        },
        NodeCatalogEntry {
            label: "Display",
            category: "Output",
            factory: || NodeType::Display,
        },
        // ── Shader ──────────────────────────────────────────────────────
        NodeCatalogEntry {
            label: "WGSL Viewer",
            category: "Shader",
            factory: || NodeType::WgslViewer,
        },
        // ── MIDI ────────────────────────────────────────────────────────
        NodeCatalogEntry {
            label: "MIDI Output",
            category: "MIDI",
            factory: || NodeType::MidiOutput { channel: 0, note: 60, velocity: 100 },
        },
        // ── Utility ─────────────────────────────────────────────────────
        NodeCatalogEntry {
            label: "Theme",
            category: "Utility",
            factory: || NodeType::Theme { dark_mode: true, accent: [80, 160, 255], font_size: 14.0 },
        },
        NodeCatalogEntry {
            label: "Comment",
            category: "Utility",
            factory: || NodeType::Comment { text: String::new() },
        },
    ]
}

/// Dispatch content rendering to the right node file.
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
        NodeType::Add => math::render(ui, node_id, values),
        NodeType::Multiply => math::render(ui, node_id, values),
        NodeType::File { path, content } => file::render(ui, path, content),
        NodeType::TextEditor { content } => {
            text_editor::render(ui, content, node_id, values, connections)
        }
        NodeType::WgslViewer => wgsl_viewer::render(ui, node_id, values, connections),
        NodeType::MouseTracker { x, y } => mouse_tracker::render(ui, *x, *y),
        NodeType::MidiOutput { channel, note, velocity } => {
            midi_output::render(ui, channel, note, velocity, node_id, values, connections)
        }
        NodeType::Theme { dark_mode, accent, font_size } => {
            theme::render(ui, dark_mode, accent, font_size)
        }
        NodeType::Comment { text } => comment::render(ui, text),
    }
}
