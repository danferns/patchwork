pub mod slider;
pub mod display;
pub mod math;
pub mod file;
pub mod text_editor;
pub mod wgsl_viewer;
pub mod mouse_tracker;
pub mod midi_out;
pub mod midi_in;
pub mod serial;
pub mod theme;
pub mod comment;

use crate::graph::*;
use crate::midi::MidiAction;
use crate::serial::SerialAction;
use eframe::egui;
use std::collections::HashMap;

pub struct NodeCatalogEntry {
    pub label: &'static str,
    pub category: &'static str,
    pub factory: fn() -> NodeType,
}

pub fn catalog() -> Vec<NodeCatalogEntry> {
    vec![
        NodeCatalogEntry { label: "Slider", category: "Input",
            factory: || NodeType::Slider { value: 0.5, min: 0.0, max: 1.0 } },
        NodeCatalogEntry { label: "Mouse Tracker", category: "Input",
            factory: || NodeType::MouseTracker { x: 0.0, y: 0.0 } },
        NodeCatalogEntry { label: "Add", category: "Math", factory: || NodeType::Add },
        NodeCatalogEntry { label: "Multiply", category: "Math", factory: || NodeType::Multiply },
        NodeCatalogEntry { label: "File", category: "IO",
            factory: || NodeType::File { path: String::new(), content: String::new() } },
        NodeCatalogEntry { label: "Text Editor", category: "IO",
            factory: || NodeType::TextEditor { content: String::new() } },
        NodeCatalogEntry { label: "Display", category: "Output", factory: || NodeType::Display },
        NodeCatalogEntry { label: "WGSL Viewer", category: "Shader", factory: || NodeType::WgslViewer },
        NodeCatalogEntry { label: "MIDI Out", category: "MIDI",
            factory: || NodeType::MidiOut { port_name: String::new(), mode: MidiMode::Note, channel: 0 } },
        NodeCatalogEntry { label: "MIDI In", category: "MIDI",
            factory: || NodeType::MidiIn { port_name: String::new(), channel: 0, note: 0, velocity: 0, log: Vec::new() } },
        NodeCatalogEntry { label: "Serial", category: "Serial",
            factory: || NodeType::Serial { port_name: String::new(), baud_rate: 115200, log: Vec::new(), last_line: String::new(), send_buf: String::new() } },
        NodeCatalogEntry { label: "Theme", category: "Utility",
            factory: || NodeType::Theme { dark_mode: true, accent: [80, 160, 255], font_size: 14.0 } },
        NodeCatalogEntry { label: "Comment", category: "Utility",
            factory: || NodeType::Comment { text: String::new() } },
    ]
}

/// Dispatch content rendering.
pub fn render_content(
    ui: &mut egui::Ui,
    node_type: &mut NodeType,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    midi_out_ports: &[String],
    midi_in_ports: &[String],
    midi_connected_out: bool,
    midi_connected_in: bool,
    midi_actions: &mut Vec<MidiAction>,
    serial_ports: &[String],
    serial_connected: bool,
    serial_actions: &mut Vec<SerialAction>,
) {
    match node_type {
        NodeType::Slider { value, min, max } => slider::render(ui, value, min, max),
        NodeType::Display => display::render(ui, node_id, values, connections),
        NodeType::Add | NodeType::Multiply => math::render(ui, node_id, values),
        NodeType::File { path, content } => file::render(ui, path, content),
        NodeType::TextEditor { content } => text_editor::render(ui, content, node_id, values, connections),
        NodeType::WgslViewer => wgsl_viewer::render(ui, node_id, values, connections),
        NodeType::MouseTracker { x, y } => mouse_tracker::render(ui, *x, *y),
        NodeType::MidiOut { port_name, mode, channel } =>
            midi_out::render(ui, port_name, mode, channel, node_id, values, connections, midi_out_ports, midi_connected_out, midi_actions),
        NodeType::MidiIn { port_name, channel, note, velocity, log } =>
            midi_in::render(ui, port_name, channel, note, velocity, log, node_id, midi_in_ports, midi_connected_in, midi_actions),
        NodeType::Serial { port_name, baud_rate, log, last_line, send_buf } =>
            serial::render(ui, port_name, baud_rate, log, last_line, send_buf, node_id, values, connections, serial_ports, serial_connected, serial_actions),
        NodeType::Theme { dark_mode, accent, font_size } => theme::render(ui, dark_mode, accent, font_size),
        NodeType::Comment { text } => comment::render(ui, text),
    }
}
