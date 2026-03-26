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
pub mod image_node;
pub mod image_effects;
pub mod blend;
pub mod curve;
pub mod draw;
pub mod noise;
pub mod color_curves;
pub mod ml_model;
pub mod video_player;

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

pub fn catalog() -> Vec<NodeCatalogEntry> {
    vec![
        // ── Input ────────────────────────────────────────────
        NodeCatalogEntry { label: "Slider", category: "Input",
            factory: || NodeType::Slider { value: 0.5, min: 0.0, max: 1.0 } },
        NodeCatalogEntry { label: "Time", category: "Input",
            factory: || NodeType::Time { elapsed: 0.0, speed: 1.0, running: true } },
        NodeCatalogEntry { label: "Color", category: "Input",
            factory: || NodeType::Color { r: 128, g: 128, b: 255 } },
        NodeCatalogEntry { label: "Mouse Tracker", category: "Input",
            factory: || NodeType::MouseTracker { x: 0.0, y: 0.0 } },
        NodeCatalogEntry { label: "Key Input", category: "Input",
            factory: || NodeType::KeyInput { key_name: String::new(), pressed: false, toggle_mode: false, toggled_on: false } },

        // ── Math ─────────────────────────────────────────────
        NodeCatalogEntry { label: "Add", category: "Math", factory: || NodeType::Add },
        NodeCatalogEntry { label: "Multiply", category: "Math", factory: || NodeType::Multiply },

        // ── IO ───────────────────────────────────────────────
        NodeCatalogEntry { label: "File", category: "IO",
            factory: || NodeType::File { path: String::new(), content: String::new() } },
        NodeCatalogEntry { label: "Text Editor", category: "IO",
            factory: || NodeType::TextEditor { content: String::new() } },

        // ── Output ───────────────────────────────────────────
        NodeCatalogEntry { label: "Display", category: "Output", factory: || NodeType::Display {
            history: Vec::new(), history_max: 200, scope_min: 0.0, scope_max: 1.0, scope_height: 80.0, paused: false,
        } },
        NodeCatalogEntry { label: "HTML Viewer", category: "Output",
            factory: || NodeType::HtmlViewer },

        // ── Shader ───────────────────────────────────────────
        NodeCatalogEntry { label: "WGSL Viewer", category: "Shader", factory: || NodeType::WgslViewer {
            wgsl_code: String::new(),
            uniform_names: vec![], uniform_types: vec![], uniform_values: vec![], uniform_min: vec![], uniform_max: vec![],
            canvas_w: 800.0, canvas_h: 600.0, resolution: 120, expanded: false,
        } },

        // ── Image ────────────────────────────────────────────
        NodeCatalogEntry { label: "Image", category: "Image",
            factory: || NodeType::ImageNode { path: String::new(), save_path: String::new(), image_data: None, preview_size: 150.0, last_save_hash: 0 } },
        NodeCatalogEntry { label: "Image Effects", category: "Image",
            factory: || NodeType::ImageEffects { brightness: 1.0, contrast: 1.0, saturation: 1.0, hue: 0.0, exposure: 0.0, gamma: 1.0 } },
        NodeCatalogEntry { label: "Blend", category: "Image",
            factory: || NodeType::Blend { mode: 0, mix: 0.5 } },
        NodeCatalogEntry { label: "Color Curves", category: "Image",
            factory: || NodeType::ColorCurves { master: vec![[0.0, 0.0], [1.0, 1.0]], red: vec![[0.0, 0.0], [1.0, 1.0]], green: vec![[0.0, 0.0], [1.0, 1.0]], blue: vec![[0.0, 0.0], [1.0, 1.0]], active_channel: 0 } },

        // ── Signal ───────────────────────────────────────────
        NodeCatalogEntry { label: "Curve", category: "Signal",
            factory: || NodeType::Curve { points: vec![[0.0, 0.0], [1.0, 1.0]] } },
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
            factory: || NodeType::Synth { waveform: crate::audio::Waveform::Sine, frequency: 440.0, amplitude: 0.5, active: true } },
        NodeCatalogEntry { label: "Audio FX", category: "Audio",
            factory: || NodeType::AudioFx { effects: Vec::new() } },
        NodeCatalogEntry { label: "Audio Player", category: "Audio",
            factory: || NodeType::AudioPlayer { file_path: String::new(), volume: 1.0, looping: false } },
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
            factory: || NodeType::OscIn { port: 8000, address_filter: String::new(), arg_count: 1, last_args: vec![0.0], log: Vec::new(), listening: false } },

        // ── Network / AI ─────────────────────────────────────
        NodeCatalogEntry { label: "HTTP Request", category: "Network",
            factory: || NodeType::HttpRequest {
                url: String::new(), method: "POST".into(), headers: String::new(),
                response: String::new(), status: String::new(), auto_send: false, last_hash: 0,
            } },
        NodeCatalogEntry { label: "AI Request", category: "Network",
            factory: || NodeType::AiRequest {
                provider: "anthropic".into(), model: "claude-sonnet-4-20250514".into(),
                response: String::new(), status: String::new(),
                max_tokens: 1024, temperature: 0.7, api_key_name: String::new(), custom_url: String::new(),
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
            factory: || NodeType::MlModel { model_path: String::new(), labels_path: String::new(), confidence: 0.05, result_text: String::new(), status: String::new(), last_input_hash: 0 } },

        // ── Utility ──────────────────────────────────────────
        NodeCatalogEntry { label: "Comment", category: "Utility",
            factory: || NodeType::Comment { text: String::new() } },
        NodeCatalogEntry { label: "Theme", category: "Utility",
            factory: || NodeType::Theme {
                dark_mode: true, accent: [80, 160, 255], font_size: 14.0,
                bg_color: [30, 30, 30], text_color: [220, 220, 220],
                window_bg: [40, 40, 40], window_alpha: 240,
                grid_color: [12, 12, 12], rounding: 4.0, spacing: 4.0, use_hsl: false,
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
    api_keys: &HashMap<String, String>,
    wgpu_render_state: &Option<eframe::egui_wgpu::RenderState>,
    pending_disconnects: &mut Vec<(NodeId, usize)>,
    ob_manager: &mut ObManager,
    audio_manager: &mut AudioManager,
    mcp_log: &crate::mcp::McpLog,
    mcp_active: bool,
) {
    match node_type {
        NodeType::Slider { value, min, max } => slider::render(ui, value, min, max),
        NodeType::Display { history, history_max, scope_min, scope_max, scope_height, paused } =>
            display::render(ui, node_id, values, connections, history, history_max, scope_min, scope_max, scope_height, paused),
        NodeType::Add | NodeType::Multiply => math::render(ui, node_id, values),
        NodeType::File { path, content } => file::render(ui, path, content),
        NodeType::TextEditor { content } => text_editor::render(ui, content, node_id, values, connections, pending_disconnects),
        NodeType::WgslViewer { wgsl_code, uniform_names, uniform_types, uniform_values, canvas_w, canvas_h, .. } =>
            wgsl_viewer::render(ui, wgsl_code, uniform_names, uniform_types, uniform_values, canvas_w, canvas_h, node_id, values, connections, wgpu_render_state, pending_disconnects),
        NodeType::Time { elapsed, speed, running } => time::render(ui, elapsed, speed, running),
        NodeType::Color { r, g, b } => color::render(ui, r, g, b),
        NodeType::MouseTracker { x, y } => mouse_tracker::render(ui, *x, *y),
        NodeType::MidiOut { port_name, mode, channel, manual_d1, manual_d2 } =>
            midi_out::render(ui, port_name, mode, channel, node_id, values, connections, midi_out_ports, midi_connected_out, midi_actions, port_positions, dragging_from, pending_disconnects, manual_d1, manual_d2),
        NodeType::MidiIn { port_name, channel, note, velocity, log } =>
            midi_in::render(ui, port_name, channel, note, velocity, log, node_id, midi_in_ports, midi_connected_in, midi_actions),
        NodeType::Serial { port_name, baud_rate, log, last_line, send_buf } =>
            serial::render(ui, port_name, baud_rate, log, last_line, send_buf, node_id, values, connections, serial_ports, serial_connected, serial_actions),
        NodeType::Theme { dark_mode, accent, font_size, bg_color, text_color, window_bg, window_alpha, grid_color, rounding, spacing, use_hsl } =>
            theme::render(ui, dark_mode, accent, font_size, bg_color, text_color, window_bg, window_alpha, grid_color, rounding, spacing, use_hsl, node_id, values, connections, port_positions, dragging_from),
        NodeType::Comment { text } => comment::render(ui, text),
        NodeType::Console { messages } => console::render(ui, messages),
        NodeType::Monitor => monitor::render(ui, monitor_state),
        NodeType::OscOut { host, port, address, arg_count } =>
            osc_out::render(ui, host, port, address, arg_count, node_id, values, osc_actions),
        NodeType::OscIn { port, address_filter, arg_count, last_args, log, listening, .. } =>
            osc_in::render(ui, port, address_filter, arg_count, last_args, log, listening, node_id, osc_listening, osc_actions),
        NodeType::KeyInput { key_name, pressed, toggle_mode, toggled_on } =>
            key_input::render(ui, key_name, pressed, toggle_mode, toggled_on),
        NodeType::Script { name, input_names, output_names, code, last_values, error, continuous, trigger } =>
            script::render(ui, name, input_names, output_names, code, last_values, error, continuous, trigger, values, node_id),
        NodeType::Palette { search } =>
            palette::render(ui, search, node_id),
        NodeType::HttpRequest { url, method, headers, response, status, auto_send, last_hash } =>
            http_request::render(ui, url, method, headers, response, status, auto_send, last_hash, node_id, values, connections, http_pending, http_actions),
        NodeType::AiRequest { provider, model, response, status, max_tokens, temperature, api_key_name, custom_url } =>
            ai_request::render(ui, provider, model, response, status, max_tokens, temperature, api_key_name, custom_url, node_id, values, connections, http_pending, http_actions, api_keys),
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
        NodeType::AudioPlayer { .. } => audio_player::render(ui, node_id, node_type, values, connections, audio_manager),
        NodeType::AudioDevice { .. } => audio_device::render(ui, node_id, node_type, audio_manager),
        NodeType::AudioFx { .. } => audio_fx::render(ui, node_id, node_type, values, connections, audio_manager),
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
        NodeType::ImageEffects { .. } => image_effects::render(ui, node_id, node_type, values, connections),
        NodeType::Blend { .. } => blend::render(ui, node_id, node_type, values, connections, wgpu_render_state),
        NodeType::Curve { .. } => curve::render(ui, node_id, node_type, values, connections),
        NodeType::Draw { .. } => draw::render(ui, node_id, node_type),
        NodeType::Noise { .. } => noise::render(ui, node_id, node_type, values, connections),
        NodeType::ColorCurves { .. } => color_curves::render(ui, node_id, node_type, values, connections),
        NodeType::VideoPlayer { .. } => video_player::render_video(ui, node_id, node_type, values, connections),
        NodeType::Camera { .. } => video_player::render_camera(ui, node_id, node_type, values, connections),
        NodeType::MlModel { .. } => ml_model::render(ui, node_id, node_type, values, connections),
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
