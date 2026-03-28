pub mod slider;
pub mod display;
pub mod math;
pub mod math_formula;
pub mod file;
pub mod text_editor;
pub mod wgsl_viewer;
pub mod mouse_tracker;
pub mod midi_out;
pub mod midi_in;
pub mod serial;
pub mod theme;
pub mod comment;
pub mod script;
pub mod console;
pub mod monitor;
pub mod osc_out;
pub mod osc_in;
pub mod key_input;
pub mod time;
pub mod color;
pub mod palette;
pub mod http_request;
pub mod ai_request;
pub mod json_extract;
pub mod file_menu;
pub mod zoom_control;
pub mod ob_hub;
pub mod ob_joystick;
pub mod ob_encoder;
pub mod html_viewer;
pub mod mcp_server;
pub mod profiler;
pub mod rust_plugin;
pub mod synth;
pub mod audio_player;
pub mod audio_device;
pub mod audio_fx;
pub mod audio_delay;
pub mod audio_distortion;
pub mod audio_filter;
pub mod audio_gain;
pub mod speaker;
pub mod audio_mixer;
pub mod audio_input;
pub mod audio_analyzer;
pub mod crop;
pub mod folder_browser;
pub mod image_node;
pub mod image_effects;
pub mod blend;
pub mod curve;
pub mod draw;
pub mod noise;
pub mod color_curves;
pub mod ml_model;
pub mod video_player;
pub mod gate;
pub mod timer;
pub mod map_range;
pub mod string_format;
pub mod sample_hold;
pub mod select;

use crate::graph::*;
use crate::midi::MidiAction;
use crate::serial::SerialAction;
use crate::osc::OscAction;
use crate::http::HttpAction;
use crate::ob::ObManager;
use crate::audio::AudioManager;
use eframe::egui;
use std::collections::HashMap;

pub struct NodeCatalogEntry {
    pub label: &'static str,
    pub category: &'static str,
    pub factory: fn() -> NodeType,
}

/// Draw a small inline port using the PortKind visual system.
/// Uses `draw_shaped_port` from `app/mod.rs` for consistent rendering across all ports.
pub fn inline_port_circle(
    ui: &mut egui::Ui, node_id: NodeId, port: usize, is_input: bool,
    connections: &[Connection],
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    pending_disconnects: &mut Vec<(NodeId, usize)>,
    kind: PortKind,
) {
    let is_wired = if is_input {
        connections.iter().any(|c| c.to_node == node_id && c.to_port == port)
    } else {
        connections.iter().any(|c| c.from_node == node_id && c.from_port == port)
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click_and_drag());

    // Colors based on PortKind
    let base = kind.base_color();
    let (fill, border) = if response.hovered() || response.dragged() {
        (egui::Color32::YELLOW, egui::Color32::WHITE)
    } else if is_wired {
        (
            egui::Color32::from_rgb(base[0], base[1], base[2]),
            egui::Color32::from_rgb(
                (base[0] as u16 + 60).min(255) as u8,
                (base[1] as u16 + 60).min(255) as u8,
                (base[2] as u16 + 60).min(255) as u8,
            ),
        )
    } else {
        (
            egui::Color32::from_rgb(
                (base[0] as f32 * 0.35) as u8,
                (base[1] as f32 * 0.35) as u8,
                (base[2] as f32 * 0.35) as u8,
            ),
            egui::Color32::from_rgb(
                (base[0] as f32 * 0.6) as u8,
                (base[1] as f32 * 0.6) as u8,
                (base[2] as f32 * 0.6) as u8,
            ),
        )
    };

    // Use the shared draw_shaped_port for consistent visuals
    crate::app::draw_shaped_port(
        ui.painter(), rect.center(), 7.0, fill, border, 2.0, kind, is_wired,
    );

    port_positions.insert((node_id, port, is_input), rect.center());
    if response.drag_started() {
        if is_input {
            if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == port) {
                *dragging_from = Some((existing.from_node, existing.from_port, true));
                pending_disconnects.push((node_id, port));
            } else {
                *dragging_from = Some((node_id, port, false));
            }
        } else {
            *dragging_from = Some((node_id, port, true));
        }
    }
    // Click-to-connect support for inline ports
    let click_wiring_id = egui::Id::new("click_wiring_active");
    let is_click_wiring = ui.ctx().data_mut(|d| d.get_temp::<bool>(click_wiring_id).unwrap_or(false));
    if response.clicked() {
        if let Some((src_node, src_port, src_is_output)) = *dragging_from {
            if is_click_wiring && node_id != src_node {
                // Complete click-wiring connection
                if src_is_output && is_input {
                    ui.ctx().data_mut(|d| d.insert_temp(
                        egui::Id::new("click_wire_complete"),
                        (src_node, src_port, node_id, port),
                    ));
                } else if !src_is_output && !is_input {
                    ui.ctx().data_mut(|d| d.insert_temp(
                        egui::Id::new("click_wire_complete"),
                        (node_id, port, src_node, src_port),
                    ));
                }
                *dragging_from = None;
                ui.ctx().data_mut(|d| d.insert_temp(click_wiring_id, false));
            }
        } else {
            // Start click-wiring from this inline port
            if is_input {
                if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == port) {
                    *dragging_from = Some((existing.from_node, existing.from_port, true));
                    pending_disconnects.push((node_id, port));
                } else {
                    *dragging_from = Some((node_id, port, false));
                }
            } else {
                *dragging_from = Some((node_id, port, true));
            }
            ui.ctx().data_mut(|d| d.insert_temp(click_wiring_id, true));
        }
    }
}

/// Draw a labeled audio port row (input or output)
pub fn audio_port_row(
    ui: &mut egui::Ui, label: &str, node_id: NodeId, port: usize, is_input: bool,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    connections: &[Connection],
    pending_disconnects: &mut Vec<(NodeId, usize)>,
    kind: PortKind,
) {
    ui.horizontal(|ui| {
        if is_input {
            inline_port_circle(ui, node_id, port, true, connections, port_positions, dragging_from, pending_disconnects, kind);
            ui.label(egui::RichText::new(label).small());
        } else {
            let remaining = ui.available_width() - 30.0;
            if remaining > 0.0 { ui.add_space(remaining); }
            ui.label(egui::RichText::new(label).small());
            inline_port_circle(ui, node_id, port, false, connections, port_positions, dragging_from, pending_disconnects, kind);
        }
    });
}

/// Draw a right-aligned output port row: label + value right-aligned, port flush at edge.
/// Uses fixed-width monospace for values to prevent jitter when values change.
pub fn output_port_row(
    ui: &mut egui::Ui, label: &str, value: &str, node_id: NodeId, port: usize,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    connections: &[Connection],
    pending_disconnects: &mut Vec<(NodeId, usize)>,
    kind: PortKind,
) {
    ui.horizontal(|ui| {
        // Use right-to-left layout for clean right-alignment
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Port circle (rightmost)
            inline_port_circle(ui, node_id, port, false, connections, port_positions, dragging_from, pending_disconnects, kind);
            // Value in monospace (fixed width, no jitter)
            ui.label(egui::RichText::new(value).small().monospace());
            // Label
            ui.label(egui::RichText::new(label).small().color(egui::Color32::from_rgb(140, 140, 155)));
        });
    });
}

pub fn catalog() -> Vec<NodeCatalogEntry> {
    vec![
        // ── Input ────────────────────────────────────────────
        NodeCatalogEntry { label: "Slider", category: "Input",
            factory: || NodeType::Slider { value: 0.5, min: 0.0, max: 1.0, step: 0.01, slider_color: [80, 160, 255], label: String::new() } },
        NodeCatalogEntry { label: "Time", category: "Input",
            factory: || NodeType::Time { elapsed: 0.0, speed: 1.0, running: true } },
        NodeCatalogEntry { label: "Color", category: "Input",
            factory: || NodeType::Color { r: 128, g: 128, b: 255 } },
        NodeCatalogEntry { label: "Mouse Tracker", category: "Input",
            factory: || NodeType::MouseTracker { x: 0.0, y: 0.0 } },
        NodeCatalogEntry { label: "Keyboard Input", category: "Input",
            factory: || NodeType::KeyInput { key_name: String::new(), pressed: false, toggle_mode: false, toggled_on: false } },

        // ── Math ─────────────────────────────────────────────
        NodeCatalogEntry { label: "Add", category: "Math", factory: || NodeType::Add },
        NodeCatalogEntry { label: "Multiply", category: "Math", factory: || NodeType::Multiply },
        NodeCatalogEntry { label: "Math", category: "Math", factory: || NodeType::Math {
            formula: "A + B".into(), variables: vec!['A', 'B'], result: 0.0, error: String::new(),
        } },
        NodeCatalogEntry { label: "Gate", category: "Logic",
            factory: || NodeType::Gate { mode: 0, threshold: 0.5, else_value: 0.0 } },
        NodeCatalogEntry { label: "Timer", category: "Input",
            factory: || NodeType::Timer { interval: 1.0, elapsed: 0.0, running: true, pulse_width: 0.1, ref_time: 0.0, paused_elapsed: 0.0, time_initialized: false } },
        NodeCatalogEntry { label: "Map/Range", category: "Math",
            factory: || NodeType::MapRange { in_min: 0.0, in_max: 1.0, out_min: 0.0, out_max: 1.0, clamp: false } },
        NodeCatalogEntry { label: "String Format", category: "IO",
            factory: || NodeType::StringFormat { template: String::new(), arg_count: 2 } },
        NodeCatalogEntry { label: "Sample & Hold", category: "Logic",
            factory: || NodeType::SampleHold { held_float: 0.0, held_text: String::new(), is_text: false, last_trigger: 0.0, history: Vec::new() } },
        NodeCatalogEntry { label: "Select", category: "Logic",
            factory: || NodeType::Select { mode: 0 } },

        // ── IO ───────────────────────────────────────────────
        NodeCatalogEntry { label: "File", category: "IO",
            factory: || NodeType::File { path: String::new(), content: String::new() } },
        NodeCatalogEntry { label: "Folder", category: "IO",
            factory: || NodeType::FolderBrowser { path: String::new(), selected_file: String::new(), search: String::new() } },
        NodeCatalogEntry { label: "Text Editor", category: "IO",
            factory: || NodeType::TextEditor { content: String::new() } },

        // ── Output ───────────────────────────────────────────
        NodeCatalogEntry { label: "Display", category: "Output", factory: || NodeType::Display {
            history: Vec::new(), history_max: 200, scope_min: 0.0, scope_max: 1.0, scope_height: 80.0, paused: false,
            display_color: [80, 200, 120], label: String::new(), auto_fit: true,
        } },
        NodeCatalogEntry { label: "Visual Output", category: "Output",
            factory: || NodeType::VisualOutput { preview_size: 200.0 } },
        NodeCatalogEntry { label: "HTML Viewer", category: "Output",
            factory: || NodeType::HtmlViewer },

        // ── Shader ───────────────────────────────────────────
        NodeCatalogEntry { label: "WGSL Viewer", category: "Shader", factory: || NodeType::WgslViewer {
            wgsl_code: String::new(),
            uniform_names: vec![], uniform_types: vec![], uniform_values: vec![], uniform_min: vec![], uniform_max: vec![],
            canvas_w: 400.0, canvas_h: 300.0, resolution: 120, expanded: false,
        } },

        // ── Image ────────────────────────────────────────────
        NodeCatalogEntry { label: "Image", category: "Image",
            factory: || NodeType::ImageNode { path: String::new(), save_path: String::new(), image_data: None, preview_size: 150.0, last_save_hash: 0 } },
        NodeCatalogEntry { label: "Image Effects", category: "Image",
            factory: || NodeType::ImageEffects { brightness: 1.0, contrast: 1.0, saturation: 1.0, hue: 0.0, exposure: 0.0, gamma: 1.0 } },
        NodeCatalogEntry { label: "Blend", category: "Image",
            factory: || NodeType::Blend { mode: 0, mix: 0.5 } },
        NodeCatalogEntry { label: "Crop", category: "Image",
            factory: || NodeType::Crop { top: 0.0, left: 0.0, bottom: 0.0, right: 0.0 } },
        NodeCatalogEntry { label: "Color Curves", category: "Image",
            factory: || NodeType::ColorCurves { master: vec![[0.0, 0.0], [1.0, 1.0]], red: vec![[0.0, 0.0], [1.0, 1.0]], green: vec![[0.0, 0.0], [1.0, 1.0]], blue: vec![[0.0, 0.0], [1.0, 1.0]], active_channel: 0 } },

        // ── Signal ───────────────────────────────────────────
        NodeCatalogEntry { label: "Curve", category: "Signal",
            factory: || NodeType::Curve { points: vec![[0.0, 0.0], [1.0, 1.0]], mode: 0, speed: 1.0, looping: false, phase: 0.0, playing: false, last_trigger: 0.0 } },
        NodeCatalogEntry { label: "Draw", category: "Signal",
            factory: || NodeType::Draw { strokes: vec![], canvas_size: 200.0, color: [255, 255, 255], line_width: 2.0 } },
        NodeCatalogEntry { label: "Noise", category: "Signal",
            factory: || NodeType::Noise { noise_type: 0, mode: 1, scale: 5.0, seed: 0 } },

        // ── Video ────────────────────────────────────────────
        NodeCatalogEntry { label: "Video Player", category: "Video",
            factory: || NodeType::VideoPlayer { path: String::new(), playing: false, looping: false, res_w: 640, res_h: 480, current_frame: None, duration: 0.0, speed: 1.0, status: String::new() } },
        NodeCatalogEntry { label: "Camera", category: "Video",
            factory: || NodeType::Camera { device_index: 0, res_w: 640, res_h: 480, active: false, current_frame: None, status: String::new() } },

        // ── Audio ────────────────────────────────────────────
        NodeCatalogEntry { label: "Synth", category: "Audio",
            factory: || NodeType::Synth { waveform: crate::audio::Waveform::Sine, frequency: 440.0, amplitude: 0.5, active: true, fm_depth: 0.0 } },
        NodeCatalogEntry { label: "Audio FX", category: "Audio",
            factory: || NodeType::AudioFx { effects: Vec::new() } },
        NodeCatalogEntry { label: "Delay", category: "Audio",
            factory: || NodeType::AudioDelay { time_ms: 250.0, feedback: 0.5 } },
        NodeCatalogEntry { label: "Distortion", category: "Audio",
            factory: || NodeType::AudioDistortion { drive: 4.0 } },
        NodeCatalogEntry { label: "Low Pass", category: "Audio",
            factory: || NodeType::AudioLowPass { cutoff: 1000.0 } },
        NodeCatalogEntry { label: "High Pass", category: "Audio",
            factory: || NodeType::AudioHighPass { cutoff: 200.0 } },
        NodeCatalogEntry { label: "Gain", category: "Audio",
            factory: || NodeType::AudioGain { level: 1.0 } },
        NodeCatalogEntry { label: "Speaker", category: "Audio",
            factory: || NodeType::Speaker { active: true, volume: 0.8 } },
        NodeCatalogEntry { label: "Mixer", category: "Audio",
            factory: || NodeType::AudioMixer { channel_count: 2, gains: vec![0.8, 0.8] } },
        NodeCatalogEntry { label: "Audio Player", category: "Audio",
            factory: || NodeType::AudioPlayer { file_path: String::new(), volume: 1.0, looping: false, duration_secs: 0.0 } },
        NodeCatalogEntry { label: "Audio Input", category: "Audio",
            factory: || NodeType::AudioInput { selected_device: String::new(), gain: 1.0, active: false } },
        NodeCatalogEntry { label: "Audio Analyzer", category: "Audio",
            factory: || NodeType::AudioAnalyzer },
        NodeCatalogEntry { label: "Audio Device", category: "Audio",
            factory: || NodeType::AudioDevice { selected_output: String::new(), selected_input: String::new(), master_volume: 0.8 } },

        // ── MIDI ─────────────────────────────────────────────
        NodeCatalogEntry { label: "MIDI Out", category: "MIDI",
            factory: || NodeType::MidiOut { port_name: String::new(), mode: MidiMode::Note, channel: 0, manual_d1: 0, manual_d2: 64 } },
        NodeCatalogEntry { label: "MIDI In", category: "MIDI",
            factory: || NodeType::MidiIn { port_name: String::new(), channel: 0, note: 0, velocity: 0, log: Vec::new() } },

        // ── Serial ───────────────────────────────────────────
        NodeCatalogEntry { label: "Serial", category: "Serial",
            factory: || NodeType::Serial { port_name: String::new(), baud_rate: 115200, log: Vec::new(), last_line: String::new(), send_buf: String::new() } },

        // ── OSC ──────────────────────────────────────────────
        NodeCatalogEntry { label: "OSC Out", category: "OSC",
            factory: || NodeType::OscOut { host: "127.0.0.1".to_string(), port: 9000, address: "/patchwork".to_string(), arg_count: 1 } },
        NodeCatalogEntry { label: "OSC In", category: "OSC",
            factory: || NodeType::OscIn { port: 8000, address_filter: String::new(), arg_count: 1, last_args: vec![0.0], last_args_text: Vec::new(), log: Vec::new(), listening: false, discovered: Vec::new() } },

        // ── Network / AI ─────────────────────────────────────
        NodeCatalogEntry { label: "HTTP Request", category: "Network",
            factory: || NodeType::HttpRequest {
                url: String::new(), method: "POST".into(), headers: String::new(),
                response: String::new(), status: String::new(), auto_send: false, last_hash: 0,
            } },
        NodeCatalogEntry { label: "AI Request", category: "Network",
            factory: || NodeType::AiRequest {
                provider: "anthropic".into(), model: "claude-sonnet-4-20250514".into(),
                system_prompt: String::new(), user_prompt: String::new(),
                response: String::new(), status: String::new(),
                max_tokens: 1024, temperature: 0.7, api_key: String::new(),
                response_type: 0, last_trigger: 0.0,
                api_key_name: String::new(), custom_url: String::new(),
            } },
        NodeCatalogEntry { label: "JSON Extract", category: "Network",
            factory: || NodeType::JsonExtract { path: String::new() } },

        // ── Hardware ─────────────────────────────────────────
        NodeCatalogEntry { label: "OB Hub", category: "Hardware",
            factory: || NodeType::ObHub { port_name: String::new(), selected_port: String::new(), detected_devices: Vec::new() } },
        NodeCatalogEntry { label: "OB Joystick", category: "Hardware",
            factory: || NodeType::ObJoystick { device_id: 1, hub_node_id: 0 } },
        NodeCatalogEntry { label: "OB Encoder", category: "Hardware",
            factory: || NodeType::ObEncoder { device_id: 1, hub_node_id: 0 } },

        // ── Custom ───────────────────────────────────────────
        NodeCatalogEntry { label: "Script", category: "Custom",
            factory: || NodeType::Script { name: "Custom Script".to_string(), input_names: vec![], output_names: vec![], code: String::new(), last_values: vec![], error: String::new(), continuous: true, trigger: false } },
        NodeCatalogEntry { label: "Rust Plugin", category: "Custom",
            factory: || NodeType::RustPlugin { input_names: vec!["in0".into()], output_names: vec!["out0".into()], code: String::new(), last_values: vec![0.0], error: String::new() } },

        // ── ML / AI ──────────────────────────────────────────
        NodeCatalogEntry { label: "ML Model", category: "ML",
            factory: || NodeType::MlModel { model_path: String::new(), labels_path: String::new(), confidence: 0.05, preset: MlPreset::default(), result_text: String::new(), result_json: String::new(), annotated_frame: None, status: String::new(), last_input_hash: 0 } },

        // ── Utility ──────────────────────────────────────────
        NodeCatalogEntry { label: "Comment", category: "Utility",
            factory: || NodeType::Comment { text: String::new(), bg_color: [45, 45, 50] } },
        NodeCatalogEntry { label: "Theme", category: "Utility",
            factory: || NodeType::Theme {
                dark_mode: true, accent: [80, 160, 255], font_size: 14.0,
                bg_color: [30, 30, 30], text_color: [220, 220, 220],
                window_bg: [40, 40, 40], window_alpha: 240,
                grid_color: [12, 12, 12], grid_style: 2, wire_style: 0,
                wiggle_gravity: 0.0, wiggle_range: 1.0, wiggle_speed: 1.0,
                rounding: 16.0, spacing: 4.0, use_hsl: false,
                wire_thickness: 6.0, background_path: String::new(),
            } },
        NodeCatalogEntry { label: "Console", category: "Utility",
            factory: || NodeType::Console { messages: Vec::new() } },
        NodeCatalogEntry { label: "Monitor", category: "Utility",
            factory: || NodeType::Monitor },
        NodeCatalogEntry { label: "System Profiler", category: "Utility",
            factory: || NodeType::Profiler },

        // ── System (hidden from palette, visible in full catalog) ──
        NodeCatalogEntry { label: "File Menu", category: "System",
            factory: || NodeType::FileMenu },
        NodeCatalogEntry { label: "Zoom Control", category: "System",
            factory: || NodeType::ZoomControl { zoom_value: 1.0 } },
        NodeCatalogEntry { label: "Node Palette", category: "System",
            factory: || NodeType::Palette { search: String::new() } },
        NodeCatalogEntry { label: "MCP Server", category: "System",
            factory: || NodeType::McpServer },
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
    monitor_state: &monitor::MonitorState,
    osc_listening: bool,
    osc_actions: &mut Vec<OscAction>,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    http_actions: &mut Vec<HttpAction>,
    http_pending: bool,
    _api_keys: &HashMap<String, String>,
    wgpu_render_state: &Option<eframe::egui_wgpu::RenderState>,
    pending_disconnects: &mut Vec<(NodeId, usize)>,
    ob_manager: &mut ObManager,
    audio_manager: &mut AudioManager,
    mcp_log: &crate::mcp::McpLog,
    mcp_active: bool,
) {
    // ── Property-edit undo detection ────────────────────────────────────
    // Snapshot egui interaction state BEFORE rendering widgets.
    // After the match, compare to detect if a new drag or focus began — which means the
    // user started editing a property.  The caller (app/mod.rs) reads this signal and
    // pushes an undo snapshot (coalesced per gesture via property_undo_pushed).
    let focused_before = ui.ctx().memory(|mem| mem.focused());
    let dragged_before = ui.ctx().dragged_id().is_some();

    match node_type {
        NodeType::Slider { value, min, max, step, slider_color, label } => slider::render(ui, value, min, max, step, slider_color, label, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::Display { history, history_max, scope_min, scope_max, scope_height, paused, display_color, label, auto_fit } =>
            display::render(ui, node_id, values, connections, history, history_max, scope_min, scope_max, scope_height, paused, display_color, label, auto_fit, port_positions, dragging_from, pending_disconnects),
        NodeType::Add | NodeType::Multiply => math::render(ui, node_id, values),
        NodeType::Math { formula, variables, result, error, .. } =>
            math_formula::render(ui, formula, variables, result, error, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::File { path, content } => file::render(ui, path, content),
        NodeType::TextEditor { content } => text_editor::render(ui, content, node_id, values, connections, pending_disconnects),
        NodeType::WgslViewer { wgsl_code, uniform_names, uniform_types, uniform_values, canvas_w, canvas_h, .. } =>
            wgsl_viewer::render(ui, wgsl_code, uniform_names, uniform_types, uniform_values, canvas_w, canvas_h, node_id, values, connections, wgpu_render_state, pending_disconnects, port_positions, dragging_from),
        NodeType::Time { elapsed, speed, running } => time::render(ui, elapsed, speed, running),
        NodeType::Color { r, g, b } => color::render(ui, r, g, b, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::MouseTracker { x, y } => mouse_tracker::render(ui, *x, *y),
        NodeType::MidiOut { port_name, mode, channel, manual_d1, manual_d2 } =>
            midi_out::render(ui, port_name, mode, channel, node_id, values, connections, midi_out_ports, midi_connected_out, midi_actions, port_positions, dragging_from, pending_disconnects, manual_d1, manual_d2),
        NodeType::MidiIn { port_name, channel, note, velocity, log } =>
            midi_in::render(ui, port_name, channel, note, velocity, log, node_id, midi_in_ports, midi_connected_in, midi_actions),
        NodeType::Serial { port_name, baud_rate, log, last_line, send_buf } =>
            serial::render(ui, port_name, baud_rate, log, last_line, send_buf, node_id, values, connections, serial_ports, serial_connected, serial_actions),
        NodeType::Theme { dark_mode, accent, font_size, bg_color, text_color, window_bg, window_alpha, grid_color, grid_style, wire_style, wiggle_gravity, wiggle_range, wiggle_speed, rounding, spacing, use_hsl, wire_thickness, background_path } =>
            theme::render(ui, dark_mode, accent, font_size, bg_color, text_color, window_bg, window_alpha, grid_color, grid_style, wire_style, wiggle_gravity, wiggle_range, wiggle_speed, rounding, spacing, use_hsl, wire_thickness, background_path, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::Comment { text, bg_color } => comment::render(ui, text, bg_color, node_id),
        NodeType::Console { messages } => console::render(ui, messages),
        NodeType::Monitor => monitor::render(ui, monitor_state),
        NodeType::OscOut { host, port, address, arg_count } =>
            osc_out::render(ui, host, port, address, arg_count, node_id, values, osc_actions),
        NodeType::OscIn { port, address_filter, arg_count, last_args, last_args_text, log, listening, discovered, .. } =>
            osc_in::render(ui, port, address_filter, arg_count, last_args, last_args_text, log, listening, discovered, node_id, osc_listening, osc_actions),
        NodeType::KeyInput { key_name, pressed, toggle_mode, toggled_on } =>
            key_input::render(ui, key_name, pressed, toggle_mode, toggled_on),
        NodeType::Script { name, input_names, output_names, code, last_values, error, continuous, trigger } =>
            script::render(ui, name, input_names, output_names, code, last_values, error, continuous, trigger, values, node_id),
        NodeType::Palette { search } =>
            palette::render(ui, search, node_id),
        NodeType::HttpRequest { url, method, headers, response, status, auto_send, last_hash } =>
            http_request::render(ui, url, method, headers, response, status, auto_send, last_hash, node_id, values, connections, http_pending, http_actions, port_positions, dragging_from, pending_disconnects),
        NodeType::AiRequest { provider, model, system_prompt, user_prompt, response, status, max_tokens, temperature, api_key, response_type, last_trigger, .. } =>
            ai_request::render(ui, provider, model, system_prompt, user_prompt, response, status, max_tokens, temperature, api_key, response_type, last_trigger, node_id, values, connections, http_pending, http_actions, port_positions, dragging_from, pending_disconnects),
        NodeType::JsonExtract { path } =>
            json_extract::render(ui, path, node_id, values, connections),
        NodeType::FileMenu => {
            let action = file_menu::render(ui);
            // Store actions in temp data for app.rs to pick up
            if action.new_project { ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("file_action_new"), true)); }
            if action.load_project { ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("file_action_load"), true)); }
            if action.save_project { ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("file_action_save"), true)); }
        }
        NodeType::ZoomControl { zoom_value } => {
            let current_zoom = ui.ctx().data_mut(|d| d.get_temp::<f32>(egui::Id::new("current_zoom")).unwrap_or(1.0));
            if let Some(new_zoom) = zoom_control::render(ui, zoom_value, node_id, values, connections, current_zoom) {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("zoom_action"), new_zoom));
            }
        }
        NodeType::ObHub { .. } => ob_hub::render(ui, node_id, node_type, ob_manager),
        NodeType::ObJoystick { .. } => ob_joystick::render(ui, node_id, node_type, values, connections, ob_manager),
        NodeType::ObEncoder { .. } => ob_encoder::render(ui, node_id, node_type, values, connections, ob_manager),
        NodeType::Synth { .. } => synth::render(ui, node_id, node_type, values, connections, audio_manager, port_positions, dragging_from, pending_disconnects),
        NodeType::AudioPlayer { .. } => audio_player::render(ui, node_id, node_type, values, connections, audio_manager, port_positions, dragging_from, pending_disconnects),
        NodeType::AudioInput { .. } => audio_input::render(ui, node_id, node_type, values, connections, audio_manager, port_positions, dragging_from, pending_disconnects),
        NodeType::AudioAnalyzer => audio_analyzer::render(ui, node_id, values, audio_manager, port_positions, dragging_from, connections, pending_disconnects),
        NodeType::AudioDevice { .. } => audio_device::render(ui, node_id, node_type, audio_manager),
        NodeType::AudioFx { .. } => audio_fx::render(ui, node_id, node_type, values, connections, audio_manager),
        NodeType::AudioDelay { time_ms, feedback } => audio_delay::render(ui, time_ms, feedback, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::AudioDistortion { drive } => audio_distortion::render(ui, drive, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::AudioLowPass { cutoff } => audio_filter::render_lpf(ui, cutoff, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::AudioHighPass { cutoff } => audio_filter::render_hpf(ui, cutoff, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::AudioGain { level } => audio_gain::render(ui, level, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::Speaker { active, volume } => speaker::render(ui, active, volume, node_id, values, connections, audio_manager, port_positions, dragging_from, pending_disconnects),
        NodeType::AudioMixer { channel_count, gains } =>
            audio_mixer::render(ui, channel_count, gains, node_id, values, connections, port_positions, dragging_from, pending_disconnects, audio_manager),
        NodeType::RustPlugin { .. } => rust_plugin::render(ui, node_id, node_type, values, connections),
        NodeType::HtmlViewer => html_viewer::render(ui, node_id, node_type, values, connections),
        NodeType::McpServer => mcp_server::render(ui, mcp_log, mcp_active),
        NodeType::Profiler => {
            // Profiler state is managed externally via egui temp data
            let profiler_id = egui::Id::new(("profiler_state", node_id));
            let state = ui.ctx().data_mut(|d| {
                d.get_temp_mut_or_insert_with::<std::sync::Arc<std::sync::Mutex<profiler::ProfilerState>>>(
                    profiler_id,
                    || std::sync::Arc::new(std::sync::Mutex::new(profiler::ProfilerState::new()))
                ).clone()
            });
            if let Ok(mut s) = state.lock() {
                s.tick();
                profiler::render(ui, &s);
            }
        }
        NodeType::ImageNode { .. } => image_node::render(ui, node_id, node_type, values, connections),
        NodeType::Crop { .. } => crop::render(ui, node_id, node_type, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::ImageEffects { .. } => image_effects::render(ui, node_id, node_type, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::Blend { .. } => blend::render(ui, node_id, node_type, values, connections, wgpu_render_state, port_positions, dragging_from, pending_disconnects),
        NodeType::Curve { .. } => curve::render(ui, node_id, node_type, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::Draw { .. } => draw::render(ui, node_id, node_type),
        NodeType::Noise { .. } => noise::render(ui, node_id, node_type, values, connections),
        NodeType::ColorCurves { .. } => color_curves::render(ui, node_id, node_type, values, connections),
        NodeType::VideoPlayer { .. } => video_player::render_video(ui, node_id, node_type, values, connections),
        NodeType::Camera { .. } => video_player::render_camera(ui, node_id, node_type, values, connections),
        NodeType::MlModel { .. } => ml_model::render(ui, node_id, node_type, values, connections),
        NodeType::Gate { mode, threshold, else_value } =>
            gate::render(ui, mode, threshold, else_value, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::Timer { interval, elapsed, running, pulse_width, ref_time, paused_elapsed, time_initialized } =>
            timer::render(ui, interval, elapsed, running, pulse_width, ref_time, paused_elapsed, time_initialized, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::MapRange { in_min, in_max, out_min, out_max, clamp } =>
            map_range::render(ui, in_min, in_max, out_min, out_max, clamp, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::StringFormat { template, arg_count } =>
            string_format::render(ui, template, arg_count, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::SampleHold { held_float, held_text, is_text, last_trigger, history } =>
            sample_hold::render(ui, held_float, held_text, is_text, last_trigger, history, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::Select { mode } =>
            select::render(ui, mode, node_id, values, connections, port_positions, dragging_from, pending_disconnects),
        NodeType::FolderBrowser { path, selected_file, search } =>
            folder_browser::render(ui, path, selected_file, search, node_id),
        NodeType::VisualOutput { preview_size } => {
            // Input port
            ui.horizontal(|ui| {
                inline_port_circle(ui, node_id, 0, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Image);
                ui.label(egui::RichText::new("Image").small());
            });

            let input_val = Graph::static_input_value(connections, values, node_id, 0);
            if let PortValue::Image(img) = &input_val {
                ui.label(egui::RichText::new(format!("{}×{}", img.width, img.height)).small().color(egui::Color32::GRAY));
                image_node::show_image_preview(ui, node_id, img, *preview_size);

                // Size slider
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Size").small());
                    ui.add(egui::Slider::new(preview_size, 80.0..=600.0).show_value(false));
                });

                // Fullscreen pop-out
                let popout_id = egui::Id::new(("visual_popout", node_id));
                let is_popout = ui.ctx().data_mut(|d| d.get_temp::<bool>(popout_id).unwrap_or(false));
                if ui.button(if is_popout { "Close Window" } else { "⛶ Fullscreen" }).clicked() {
                    ui.ctx().data_mut(|d| d.insert_temp(popout_id, !is_popout));
                }

                // Pop-out viewport
                if is_popout {
                    let img_clone = img.clone();
                    let nid = node_id;
                    ui.ctx().show_viewport_immediate(
                        egui::ViewportId::from_hash_of(("visual_popout_vp", node_id)),
                        egui::ViewportBuilder::default()
                            .with_title(format!("Visual Output #{}", node_id))
                            .with_inner_size([img_clone.width as f32, img_clone.height as f32]),
                        |ctx, _class| {
                            egui::CentralPanel::default()
                                .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
                                .show(ctx, |ui| {
                                    image_node::show_image_preview(ui, nid, &img_clone, ui.available_width());
                                });
                            if ctx.input(|i| i.viewport().close_requested()) {
                                ctx.data_mut(|d| d.insert_temp(popout_id, false));
                            }
                        },
                    );
                }
            } else {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::from_rgb(100, 100, 110), "No image connected");
                ui.add_space(8.0);
            }
        }
    }

    // ── Post-render: detect if a NEW widget interaction started ──────────
    let focused_after = ui.ctx().memory(|mem| mem.focused());
    let dragged_after = ui.ctx().dragged_id().is_some();
    let new_interaction =
        (!dragged_before && dragged_after) ||
        (focused_before != focused_after && focused_after.is_some());
    if new_interaction {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("_patchwork_prop_edit_signal"), true));
    }
}

/// Returns node types to create from Palette clicks (checked after render_content)
pub fn palette_actions(ui: &egui::Ui) -> Vec<NodeType> {
    ui.memory_mut(|mem| {
        let v: Vec<NodeType> = mem.data.get_temp(egui::Id::new("palette_spawn")).unwrap_or_default();
        if !v.is_empty() {
            mem.data.insert_temp::<Vec<NodeType>>(egui::Id::new("palette_spawn"), vec![]);
        }
        v
    })
}
