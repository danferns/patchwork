use crate::graph::*;
use crate::http::{HttpAction, HttpManager};
use crate::midi::{MidiAction, MidiManager};
use crate::serial::{SerialAction, SerialManager};
use crate::osc::{OscAction, OscManager};
use crate::ob::ObManager;
use crate::audio::AudioManager;
use crate::nodes;
use eframe::egui;
use eframe::egui_wgpu;
use std::collections::HashMap;
use std::sync::Arc;

const PORT_RADIUS: f32 = 5.0;
const PORT_INTERACT: f32 = 14.0;
const CONN_COLOR: egui::Color32 = egui::Color32::from_rgb(180, 180, 180);
const CONN_ACTIVE: egui::Color32 = egui::Color32::from_rgb(80, 170, 255);

// ── Undo / Redo ─────────────────────────────────────────────────────────────

struct UndoSnapshot {
    graph: Graph,
    pinned_nodes: std::collections::HashSet<NodeId>,
}

struct UndoHistory {
    undo_stack: Vec<UndoSnapshot>,
    redo_stack: Vec<UndoSnapshot>,
    max: usize,
}

impl UndoHistory {
    fn new(max: usize) -> Self {
        Self { undo_stack: Vec::new(), redo_stack: Vec::new(), max }
    }

    fn push(&mut self, snap: UndoSnapshot) {
        self.redo_stack.clear();
        self.undo_stack.push(snap);
        if self.undo_stack.len() > self.max {
            self.undo_stack.remove(0);
        }
    }

    fn undo(&mut self, current: UndoSnapshot) -> Option<UndoSnapshot> {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(current);
            Some(prev)
        } else {
            None
        }
    }

    fn redo(&mut self, current: UndoSnapshot) -> Option<UndoSnapshot> {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(current);
            Some(next)
        } else {
            None
        }
    }

    fn can_undo(&self) -> bool { !self.undo_stack.is_empty() }
    fn can_redo(&self) -> bool { !self.redo_stack.is_empty() }
    fn clear(&mut self) { self.undo_stack.clear(); self.redo_stack.clear(); }
}

pub struct PatchworkApp {
    graph: Graph,
    port_positions: HashMap<(NodeId, usize, bool), egui::Pos2>,
    node_rects: HashMap<NodeId, egui::Rect>,
    dragging_from: Option<(NodeId, usize, bool)>,
    show_node_menu: bool,
    node_menu_pos: egui::Pos2,
    node_menu_search: String,
    project_path: Option<String>,
    midi: MidiManager,
    serial: SerialManager,
    frame_count: u64,
    console_messages: Vec<String>,
    monitor: nodes::monitor::MonitorState,
    osc: OscManager,
    // Selection & clipboard
    selected_nodes: std::collections::HashSet<NodeId>,
    selected_connection: Option<usize>,
    // Box selection
    box_select_start: Option<egui::Pos2>,
    box_select_end: Option<egui::Pos2>,
    clipboard: Option<NodeType>,
    context_menu_node: Option<NodeId>,
    show_context_menu: bool,
    context_menu_pos: egui::Pos2,
    // Option+drag duplication
    opt_drag_source: Option<NodeId>,
    opt_drag_created: Option<NodeId>,
    // Canvas pan & zoom
    canvas_offset: egui::Vec2,
    canvas_zoom: f32,
    prev_zoom: f32,
    panning: bool,
    pinned_nodes: std::collections::HashSet<NodeId>,
    // HTTP & API
    http: HttpManager,
    api_keys: HashMap<String, String>,
    // OB Hardware
    ob: ObManager,
    // Audio
    audio: AudioManager,
    // MCP server
    mcp_rx: Option<std::sync::mpsc::Receiver<crate::mcp::McpRequest>>,
    mcp_log: crate::mcp::McpLog,
    // ML inference
    ml_rx: std::sync::mpsc::Receiver<crate::nodes::ml_model::MlInferenceResult>,
    ml_tx: std::sync::mpsc::Sender<crate::nodes::ml_model::MlInferenceResult>,
    // Background device refresh
    device_refresh_rx: std::sync::mpsc::Receiver<DeviceRefreshResult>,
    // WGPU render state for shader nodes
    wgpu_render_state: Option<egui_wgpu::RenderState>,
    // Undo / Redo
    undo_history: UndoHistory,
    drag_undo_pushed: bool,
}

/// Results from background device enumeration thread
struct DeviceRefreshResult {
    midi_in: Vec<String>,
    midi_out: Vec<String>,
    serial: Vec<String>,
    audio_output: Vec<String>,
    audio_input: Vec<String>,
}

impl PatchworkApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let wgpu_render_state = cc.wgpu_render_state.clone();
        let (ml_tx, ml_rx) = std::sync::mpsc::channel();
        let mut app = Self {
            graph: Graph::new(),
            port_positions: HashMap::new(),
            node_rects: HashMap::new(),
            dragging_from: None,
            show_node_menu: false,
            node_menu_pos: egui::Pos2::ZERO,
            node_menu_search: String::new(),
            project_path: None,
            midi: MidiManager::new(),
            serial: SerialManager::new(),
            frame_count: 0,
            console_messages: Vec::new(),
            monitor: nodes::monitor::MonitorState::default(),
            osc: OscManager::new(),
            selected_nodes: std::collections::HashSet::new(),
            selected_connection: None,
            box_select_start: None,
            box_select_end: None,
            clipboard: None,
            context_menu_node: None,
            show_context_menu: false,
            context_menu_pos: egui::Pos2::ZERO,
            opt_drag_source: None,
            opt_drag_created: None,
            canvas_offset: egui::Vec2::ZERO,
            canvas_zoom: 1.0,
            prev_zoom: 1.0,
            panning: false,
            pinned_nodes: std::collections::HashSet::new(),
            http: HttpManager::new(),
            api_keys: HashMap::new(),
            ob: ObManager::new(),
            audio: AudioManager::new(),
            mcp_log: crate::mcp::new_log(),
            mcp_rx: None,
            ml_rx: ml_rx,
            ml_tx: ml_tx,
            device_refresh_rx: {
                let (tx, rx) = std::sync::mpsc::channel();
                // Background thread enumerates devices every 5 seconds — never blocks UI
                std::thread::spawn(move || {
                    loop {
                        let midi_in = midir::MidiInput::new("patchwork-scan")
                            .map(|m| m.ports().iter().filter_map(|p| m.port_name(p).ok()).collect())
                            .unwrap_or_default();
                        let midi_out = midir::MidiOutput::new("patchwork-scan")
                            .map(|m| m.ports().iter().filter_map(|p| m.port_name(p).ok()).collect())
                            .unwrap_or_default();
                        let serial = serialport::available_ports()
                            .unwrap_or_default().into_iter().map(|p| p.port_name).collect();
                        let (audio_output, audio_input) = {
                            use cpal::traits::{HostTrait, DeviceTrait};
                            let host = cpal::default_host();
                            let out = host.output_devices().map(|devs|
                                devs.filter_map(|d| d.name().ok()).collect()).unwrap_or_default();
                            let inp = host.input_devices().map(|devs|
                                devs.filter_map(|d| d.name().ok()).collect()).unwrap_or_default();
                            (out, inp)
                        };
                        let _ = tx.send(DeviceRefreshResult { midi_in, midi_out, serial, audio_output, audio_input });
                        std::thread::sleep(std::time::Duration::from_secs(5));
                    }
                });
                rx
            },
            wgpu_render_state,
            undo_history: UndoHistory::new(100),
            drag_undo_pushed: false,
        };
        // Always start MCP server thread — auto-detects if stdin is a pipe (Claude Desktop)
        // vs terminal (normal launch). If stdin is a terminal, the thread exits immediately.
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let log = app.mcp_log.clone();
            std::thread::spawn(move || crate::mcp::run_mcp_thread(tx, log));
            app.mcp_rx = Some(rx);
        }

        app.spawn_default_nodes();
        app
    }

    // ── Undo helpers ──────────────────────────────────────────────────────

    fn push_undo(&mut self) {
        let snap = UndoSnapshot {
            graph: self.graph.clone(),
            pinned_nodes: self.pinned_nodes.clone(),
        };
        self.undo_history.push(snap);
    }

    fn perform_undo(&mut self) {
        let current = UndoSnapshot {
            graph: self.graph.clone(),
            pinned_nodes: self.pinned_nodes.clone(),
        };
        if let Some(prev) = self.undo_history.undo(current) {
            self.graph = prev.graph;
            self.pinned_nodes = prev.pinned_nodes;
            self.port_positions.clear();
            self.node_rects.clear();
            self.selected_nodes.clear();
            self.selected_connection = None;
        }
    }

    fn perform_redo(&mut self) {
        let current = UndoSnapshot {
            graph: self.graph.clone(),
            pinned_nodes: self.pinned_nodes.clone(),
        };
        if let Some(next) = self.undo_history.redo(current) {
            self.graph = next.graph;
            self.pinned_nodes = next.pinned_nodes;
            self.port_positions.clear();
            self.node_rects.clear();
            self.selected_nodes.clear();
            self.selected_connection = None;
        }
    }

    fn primary_selected(&self) -> Option<NodeId> {
        self.selected_nodes.iter().next().copied()
    }

    fn log_message(&mut self, msg: String) {
        self.console_messages.push(msg);
        if self.console_messages.len() > 200 {
            self.console_messages.remove(0);
        }
    }

    fn sync_console_messages(&mut self) {
        for node in self.graph.nodes.values_mut() {
            if let NodeType::Console { messages } = &mut node.node_type {
                *messages = self.console_messages.clone();
            }
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        for node in self.graph.nodes.values() {
            if let NodeType::Theme { dark_mode, accent, font_size, bg_color, text_color, window_bg, window_alpha, grid_color: _, rounding, spacing, .. } = &node.node_type {
                nodes::theme::apply(ctx, *dark_mode, *accent, *font_size, *bg_color, *text_color, *window_bg, *window_alpha, *rounding, *spacing);
                return;
            }
        }
    }

    // Pinned nodes: pos is screen pixels, computed to egui coords inline in render

    fn handle_file_drop(&mut self, ctx: &egui::Context) {
        let dropped: Vec<_> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() { self.push_undo(); }
        let image_exts = ["png", "jpg", "jpeg", "gif", "bmp", "webp"];
        let video_exts = ["mp4", "mov", "avi", "webm", "mkv"];
        for path in dropped {
            let pos = ctx.pointer_latest_pos().unwrap_or(egui::pos2(200.0, 200.0));
            let off_e = self.canvas_offset / self.canvas_zoom;
            let canvas_x = pos.x - off_e.x;
            let canvas_y = pos.y - off_e.y;

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if image_exts.contains(&ext.as_str()) {
                let image_data = crate::nodes::image_node::load_image_from_path(&path.display().to_string());
                self.graph.add_node(NodeType::ImageNode {
                    path: path.display().to_string(),
                    save_path: String::new(),
                    image_data,
                    preview_size: 150.0,
                    last_save_hash: 0,
                }, [canvas_x, canvas_y]);
            } else if video_exts.contains(&ext.as_str()) {
                self.graph.add_node(NodeType::VideoPlayer {
                    path: path.display().to_string(),
                    playing: false, looping: false,
                    res_w: 640, res_h: 480,
                    current_frame: None,
                    duration: 0.0, speed: 1.0,
                    status: "Loaded".into(),
                }, [canvas_x, canvas_y]);
            } else {
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                self.graph.add_node(NodeType::File { path: path.display().to_string(), content }, [canvas_x, canvas_y]);
            }
        }
    }

    fn poll_midi_inputs(&mut self) {
        let node_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
        for nid in node_ids {
            if let Some(msg) = self.midi.poll_input(nid) {
                if let Some(node) = self.graph.nodes.get_mut(&nid) {
                    if let NodeType::MidiIn { channel, note, velocity, log, .. } = &mut node.node_type {
                        if msg.len() >= 3 {
                            *channel = msg[0] & 0x0F;
                            let status = msg[0] & 0xF0;
                            match status {
                                0x80 | 0x90 | 0xA0 | 0xB0 => { *note = msg[1]; *velocity = msg[2]; }
                                _ => {}
                            }
                        }
                        log.push(nodes::midi_in::format_midi_message(&msg));
                    }
                }
            }
        }
    }

    fn poll_serial_inputs(&mut self) {
        let node_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
        for nid in node_ids {
            let lines = self.serial.poll(nid);
            if !lines.is_empty() {
                if let Some(node) = self.graph.nodes.get_mut(&nid) {
                    if let NodeType::Serial { log, last_line, .. } = &mut node.node_type {
                        for line in lines {
                            *last_line = line.clone();
                            log.push(line);
                        }
                    }
                }
            }
        }
    }

    fn poll_osc_inputs(&mut self) {
        let node_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
        for nid in node_ids {
            let messages = self.osc.poll(nid);
            if !messages.is_empty() {
                if let Some(node) = self.graph.nodes.get_mut(&nid) {
                    if let NodeType::OscIn { address_filter, arg_count, last_args, log, .. } = &mut node.node_type {
                        for (addr, args) in messages {
                            if !address_filter.is_empty() && !addr.contains(address_filter.as_str()) {
                                continue;
                            }
                            // Update last_args from received message
                            for (i, &val) in args.iter().enumerate() {
                                if i < *arg_count {
                                    if i >= last_args.len() { last_args.push(0.0); }
                                    last_args[i] = val;
                                }
                            }
                            let args_str = args.iter().map(|v| format!("{:.3}", v)).collect::<Vec<_>>().join(", ");
                            log.push(format!("{} [{}]", addr, args_str));
                            if log.len() > 200 { log.remove(0); }
                        }
                    }
                }
            }
        }
    }

    fn poll_http_responses(&mut self) {
        let node_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
        for nid in node_ids {
            if let Some(resp) = self.http.poll(nid) {
                if let Some(node) = self.graph.nodes.get_mut(&nid) {
                    match &mut node.node_type {
                        NodeType::HttpRequest { response, status, .. } => {
                            *status = format!("{}", resp.status);
                            *response = resp.body;
                        }
                        NodeType::AiRequest { provider, response, status, .. } => {
                            if resp.status >= 200 && resp.status < 300 {
                                // Try to detect provider from response shape if not set
                                let prov = if provider.is_empty() {
                                    // Auto-detect: Anthropic has "content" array, OpenAI has "choices"
                                    if resp.body.contains("\"content\":[{\"type\"") { "anthropic" }
                                    else { "openai" }
                                } else {
                                    provider.as_str()
                                };
                                *response = crate::nodes::ai_request::extract_ai_response(prov, &resp.body);
                                *status = "done".into();
                            } else {
                                *response = resp.body;
                                *status = format!("error: {}", resp.status);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Spawn default system nodes if graph is empty.
    fn spawn_default_nodes(&mut self) {
        if self.graph.nodes.is_empty() {
            // File menu — pinned top-left
            let file_id = self.graph.add_node(NodeType::FileMenu, [10.0, 10.0]);
            self.pinned_nodes.insert(file_id);

            // Zoom control — pinned top-right
            let zoom_id = self.graph.add_node(
                NodeType::ZoomControl { zoom_value: 1.0 },
                [1100.0, 10.0],
            );
            self.pinned_nodes.insert(zoom_id);

            // Node Palette — pinned bottom-left (higher up so list is visible)
            let palette_id = self.graph.add_node(
                NodeType::Palette { search: String::new() },
                [10.0, 200.0],
            );
            self.pinned_nodes.insert(palette_id);

            // Monitor — pinned bottom-right (higher up)
            let monitor_id = self.graph.add_node(NodeType::Monitor, [1100.0, 500.0]);
            self.pinned_nodes.insert(monitor_id);
        }
    }

    /// Poll ML inference results and dispatch new requests
    fn poll_ml_inference(&mut self, ctx: &egui::Context) {
        // Receive completed results
        while let Ok(result) = self.ml_rx.try_recv() {
            if let Some(node) = self.graph.nodes.get_mut(&result.node_id) {
                if let NodeType::MlModel { result_text, status, .. } = &mut node.node_type {
                    *result_text = result.result_text;
                    *status = result.status;
                }
            }
        }

        // Check for new inference requests (stored in egui temp data by ml_model::render)
        let node_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
        for nid in node_ids {
            let inference_id = egui::Id::new(("ml_inference", nid));
            if let Some(req) = ctx.data_mut(|d| d.get_temp::<crate::nodes::ml_model::MlInferenceRequest>(inference_id)) {
                ctx.data_mut(|d| d.remove::<crate::nodes::ml_model::MlInferenceRequest>(inference_id));
                let tx = self.ml_tx.clone();
                std::thread::spawn(move || {
                    let result = crate::nodes::ml_model::run_inference(&req);
                    let _ = tx.send(result);
                });
            }
        }
    }

    /// Process pending MCP commands from the MCP server thread
    fn process_mcp_commands(&mut self, values: &HashMap<(NodeId, usize), PortValue>) {
        let rx = match &self.mcp_rx {
            Some(rx) => rx,
            None => return,
        };
        // Drain all pending requests (non-blocking)
        while let Ok(request) = rx.try_recv() {
            let result = crate::mcp::execute_command(request.command, &mut self.graph, values);
            let _ = request.response_tx.send(result);
        }
    }

    /// Check for file/zoom actions from system nodes (communicated via egui temp data).
    fn handle_system_node_actions(&mut self, ctx: &egui::Context) {
        let new_project = ctx.data_mut(|d| d.get_temp::<bool>(egui::Id::new("file_action_new")).unwrap_or(false));
        let load_project = ctx.data_mut(|d| d.get_temp::<bool>(egui::Id::new("file_action_load")).unwrap_or(false));
        let save_project = ctx.data_mut(|d| d.get_temp::<bool>(egui::Id::new("file_action_save")).unwrap_or(false));

        // Clear flags
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("file_action_new"), false);
            d.insert_temp(egui::Id::new("file_action_load"), false);
            d.insert_temp(egui::Id::new("file_action_save"), false);
        });

        if new_project {
            self.graph = Graph::new();
            self.project_path = None;
            self.pinned_nodes.clear();
            self.undo_history.clear();
            self.spawn_default_nodes();
        }
        if load_project { self.load_project(); }
        if save_project { self.save_project(); }

        // Zoom control
        let zoom_action: Option<f32> = ctx.data_mut(|d| d.get_temp(egui::Id::new("zoom_action")));
        if let Some(new_zoom) = zoom_action {
            self.canvas_zoom = new_zoom.clamp(0.1, 5.0);
            ctx.data_mut(|d| d.remove::<f32>(egui::Id::new("zoom_action")));
        }

        // Provide current zoom to ZoomControl nodes
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("current_zoom"), self.canvas_zoom));
    }

    fn canvas(&self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let painter = ui.painter();
            let rect = ui.max_rect();
            let zoom = self.canvas_zoom;
            let grid = 25.0; // Logical units; set_zoom_factor scales to screen
            let gc = self.graph.nodes.values()
                .find_map(|n| if let NodeType::Theme { grid_color, .. } = &n.node_type { Some(*grid_color) } else { None })
                .unwrap_or([12, 12, 12]);
            let col = egui::Color32::from_rgba_premultiplied(gc[0], gc[1], gc[2], 35);
            let off = self.canvas_offset / zoom; // Offset in logical coords
            let x_start = ((rect.left() - off.x) / grid).floor() as i32;
            let x_end = ((rect.right() - off.x) / grid).ceil() as i32;
            let y_start = ((rect.top() - off.y) / grid).floor() as i32;
            let y_end = ((rect.bottom() - off.y) / grid).ceil() as i32;
            for i in x_start..=x_end {
                let x = i as f32 * grid + off.x;
                painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], egui::Stroke::new(0.5, col));
            }
            for i in y_start..=y_end {
                let y = i as f32 * grid + off.y;
                painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], egui::Stroke::new(0.5, col));
            }
            // Origin crosshair
            if rect.contains(egui::pos2(off.x, off.y)) {
                painter.circle_filled(egui::pos2(off.x, off.y), 3.0, egui::Color32::from_rgba_premultiplied(gc[0], gc[1], gc[2], 80));
            }
            // Zoom indicator
            if (zoom - 1.0).abs() > 0.01 {
                painter.text(
                    egui::pos2(rect.right() - 8.0, rect.bottom() - 8.0),
                    egui::Align2::RIGHT_BOTTOM,
                    format!("{:.0}%", zoom * 100.0),
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(100, 100, 100),
                );
            }
            if self.graph.nodes.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("Double-click to add a node  \u{2022}  Drag & drop a file").size(16.0).color(egui::Color32::from_rgb(100, 100, 100)));
                });
            }

            // Draw box selection rectangle
            if let (Some(start), Some(end)) = (self.box_select_start, self.box_select_end) {
                let sel_rect = egui::Rect::from_two_pos(start, end);
                let painter = ui.painter();
                painter.rect_filled(sel_rect, 0.0, egui::Color32::from_rgba_unmultiplied(80, 170, 255, 25));
                painter.rect_stroke(sel_rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(80, 170, 255, 150)), egui::StrokeKind::Outside);
            }
        });
    }

    /// Render nodes. If `pinned_only` is true, only render pinned nodes (at zoom 1.0).
    /// If false, only render non-pinned nodes (at canvas zoom).
    fn render_nodes_filtered(&mut self, ctx: &egui::Context, values: &HashMap<(NodeId, usize), PortValue>, pinned_only: bool) {
        let node_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
        let connections = self.graph.connections.clone();
        let midi_out_ports = self.midi.cached_output_ports.clone();
        let midi_in_ports = self.midi.cached_input_ports.clone();
        let serial_ports = self.serial.cached_ports.clone();
        let monitor_state = std::mem::take(&mut self.monitor);
        let offset = self.canvas_offset;
        let zoom = self.canvas_zoom;
        // Don't create fresh — extend existing to preserve positions from other passes
        let mut port_positions: HashMap<(NodeId, usize, bool), egui::Pos2> = if pinned_only {
            std::mem::take(&mut self.port_positions)
        } else {
            HashMap::new()
        };
        let mut node_rects: HashMap<NodeId, egui::Rect> = if pinned_only {
            std::mem::take(&mut self.node_rects)
        } else {
            HashMap::new()
        };
        let mut pending_connections: Vec<(NodeId, usize, NodeId, usize)> = Vec::new();
        let mut nodes_to_delete: Vec<NodeId> = Vec::new();
        let mut osc_actions: Vec<OscAction> = Vec::new();
        let mut dragging_from = self.dragging_from;
        let mut palette_spawns: Vec<([f32; 2], NodeType)> = Vec::new();
        let mut midi_actions: Vec<MidiAction> = Vec::new();
        let mut serial_actions: Vec<SerialAction> = Vec::new();
        let mut pending_disconnects: Vec<(NodeId, usize)> = Vec::new();
        let mut ob_manager = std::mem::replace(&mut self.ob, ObManager::new());
        let mut audio_manager = std::mem::replace(&mut self.audio, AudioManager::placeholder());
        let mut http_actions: Vec<HttpAction> = Vec::new();
        let mut multi_drag: Option<(NodeId, egui::Vec2)> = None; // (dragged_node, delta)
        let api_keys = self.api_keys.clone();
        let wgpu_render_state = self.wgpu_render_state.clone();

        // With set_zoom_factor, normal nodes scale automatically via GPU.
        // We only need the original style to restore after pinned nodes' inverse-zoom.
        let original_style = ctx.style();

        for node_id in node_ids {
            let is_pinned = self.pinned_nodes.contains(&node_id);
            // Skip nodes not in the current pass
            if pinned_only != is_pinned {
                continue;
            }
            let mut node = match self.graph.nodes.remove(&node_id) { Some(n) => n, None => continue };
            let input_defs = node.node_type.inputs();
            let output_defs = node.node_type.outputs();
            let n_inputs = input_defs.len();
            let n_outputs = output_defs.len();
            let [cr, cg, cb] = node.node_type.color_hint();
            let accent = egui::Color32::from_rgb(cr, cg, cb);
            let title = format!("{} #{}", node.node_type.title(), node_id);
            let midi_conn_out = self.midi.is_output_connected(node_id);
            let midi_conn_in = self.midi.is_input_connected(node_id);
            let serial_conn = self.serial.is_connected(node_id);
            let osc_listening = self.osc.is_listening(node_id);
            let http_pending = self.http.is_pending(node_id);

            let mut open = true;
            let inline = node.node_type.inline_ports();
            // With set_zoom_factor: logical = screen / zoom.
            // Normal: egui_pos = canvas_pos + offset/zoom
            // Pinned: egui_pos = screen_pos / zoom (fixed screen position)
            let offset_egui = offset / zoom;
            let (egui_x, egui_y) = if is_pinned {
                (node.pos[0] / zoom, node.pos[1] / zoom)
            } else {
                (node.pos[0] + offset_egui.x, node.pos[1] + offset_egui.y)
            };
            // Pinned: inverse zoom on sizes so they appear at native screen size.
            // Zoom-bucketed ID resets egui's cached window size on zoom changes.
            let inv = 1.0 / zoom;
            let node_width = if is_pinned { 180.0 * inv } else { 180.0 };
            let port_sz = if is_pinned { PORT_INTERACT * inv } else { PORT_INTERACT };
            let port_r = if is_pinned { PORT_RADIUS * inv } else { PORT_RADIUS };
            let title_size = if is_pinned { 14.0 * inv } else { 14.0 };
            let title_rt = egui::RichText::new(&title).color(accent).strong().size(title_size);
            // Pinned windows use zoom-bucketed ID to reset cached size on zoom change
            let zoom_bucket = if is_pinned { (zoom * 10.0).round() as i32 } else { 0 };
            // Apply inverse-zoom style for pinned nodes
            if is_pinned {
                let mut style = ctx.style().as_ref().clone();
                for (_, font_id) in style.text_styles.iter_mut() {
                    font_id.size *= inv;
                }
                style.spacing.item_spacing *= inv;
                style.spacing.button_padding *= inv;
                style.spacing.interact_size *= inv;
                style.spacing.window_margin *= inv;
                ctx.set_style(std::sync::Arc::new(style));
            }
            let resp = egui::Window::new(title_rt)
                .id(egui::Id::new(("node", node_id, zoom_bucket)))
                .current_pos(egui::pos2(egui_x, egui_y))
                .default_width(node_width)
                .resizable(true)
                .constrain(false)
                .collapsible(true)
                .open(&mut open)
                .show(ctx, |ui| {
                    // Top input ports (skip for inline-port nodes)
                    if !inline {
                        for (i, pdef) in input_defs.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let (rect, response) = ui.allocate_exact_size(egui::vec2(port_sz, port_sz), egui::Sense::click_and_drag());
                                let col = if response.hovered() || response.dragged() { egui::Color32::YELLOW } else { egui::Color32::from_rgb(170, 170, 170) };
                                ui.painter().circle_filled(rect.center(), port_r, col);
                                ui.painter().circle_stroke(rect.center(), port_r, egui::Stroke::new(1.0, egui::Color32::WHITE));
                                port_positions.insert((node_id, i, true), rect.center());
                                let val = Graph::static_input_value(&connections, values, node_id, i);
                                ui.label(format!("{}: {}", pdef.name, val));
                                if response.drag_started() {
                                    if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == i) {
                                        let src_node = existing.from_node;
                                        let src_port = existing.from_port;
                                        dragging_from = Some((src_node, src_port, true));
                                        pending_disconnects.push((node_id, i));
                                    } else {
                                        dragging_from = Some((node_id, i, false));
                                    }
                                }
                            });
                        }
                        if !input_defs.is_empty() { ui.separator(); }
                    }

                    nodes::render_content(ui, &mut node.node_type, node_id, values, &connections,
                        &midi_out_ports, &midi_in_ports, midi_conn_out, midi_conn_in, &mut midi_actions,
                        &serial_ports, serial_conn, &mut serial_actions, &monitor_state,
                        osc_listening, &mut osc_actions, &mut port_positions, &mut dragging_from,
                        &mut http_actions, http_pending, &api_keys, &wgpu_render_state,
                        &mut pending_disconnects, &mut ob_manager, &mut audio_manager,
                        &self.mcp_log, self.mcp_rx.is_some());

                    // Check if a Palette node wants to spawn new nodes
                    if matches!(node.node_type, NodeType::Palette { .. }) {
                        for spawn_nt in nodes::palette_actions(ui) {
                            palette_spawns.push((node.pos, spawn_nt));
                        }
                    }

                    // Bottom output ports (skip for inline-port nodes)
                    if !inline {
                        if !output_defs.is_empty() { ui.separator(); }
                        for (i, pdef) in output_defs.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let val = values.get(&(node_id, i)).cloned().unwrap_or(PortValue::None);
                                ui.label(format!("{}: {}", pdef.name, val));
                                let (rect, response) = ui.allocate_exact_size(egui::vec2(port_sz, port_sz), egui::Sense::click_and_drag());
                                let col = if response.hovered() || response.dragged() { egui::Color32::YELLOW } else { egui::Color32::from_rgb(100, 180, 255) };
                                ui.painter().circle_filled(rect.center(), port_r, col);
                                ui.painter().circle_stroke(rect.center(), port_r, egui::Stroke::new(1.0, egui::Color32::WHITE));
                                port_positions.insert((node_id, i, false), rect.center());
                                if response.drag_started() { dragging_from = Some((node_id, i, true)); }
                            });
                        }
                    }
                });

            // Restore original style after pinned window's inverse-zoom style
            if is_pinned {
                ctx.set_style(original_style.clone());
            }

            if let Some(r) = &resp {
                if is_pinned {
                    // Pinned: drag delta is logical (screen/zoom), store screen pos
                    if r.response.dragged() {
                        let delta = r.response.drag_delta();
                        node.pos[0] += delta.x * zoom;
                        node.pos[1] += delta.y * zoom;
                    }
                } else {
                    // Normal: convert logical pos back to canvas coords
                    let offset_egui = offset / zoom;
                    node.pos = [
                        r.response.rect.left() - offset_egui.x,
                        r.response.rect.top() - offset_egui.y,
                    ];
                }
                node_rects.insert(node_id, r.response.rect);

                // Selection on click (Shift = toggle, no Shift = replace)
                if r.response.clicked() || r.response.drag_started() {
                    let shift = ctx.input(|i| i.modifiers.shift);
                    if shift {
                        if self.selected_nodes.contains(&node_id) {
                            self.selected_nodes.remove(&node_id);
                        } else {
                            self.selected_nodes.insert(node_id);
                        }
                    } else if !self.selected_nodes.contains(&node_id) {
                        self.selected_nodes.clear();
                        self.selected_nodes.insert(node_id);
                    }
                    self.selected_connection = None;
                }

                // Right-click context menu (check raw secondary click within node rect)
                let secondary_in_rect = ctx.input(|i| {
                    i.pointer.button_clicked(egui::PointerButton::Secondary)
                }) && ctx.pointer_latest_pos().map(|p| r.response.rect.contains(p)).unwrap_or(false);
                if r.response.secondary_clicked() || secondary_in_rect {
                    if !self.selected_nodes.contains(&node_id) {
                        self.selected_nodes.clear();
                        self.selected_nodes.insert(node_id);
                    }
                    self.context_menu_node = Some(node_id);
                    self.show_context_menu = true;
                    self.context_menu_pos = ctx.pointer_latest_pos().unwrap_or(r.response.rect.center());
                }

                // Draw selection highlight
                if self.selected_nodes.contains(&node_id) {
                    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("selection")));
                    painter.rect_stroke(r.response.rect.expand(2.0), 4.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 170, 255)), egui::StrokeKind::Outside);
                }
                // Pin indicator
                if is_pinned {
                    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("pins")));
                    painter.text(
                        egui::pos2(r.response.rect.left() + 4.0, r.response.rect.top() - 12.0),
                        egui::Align2::LEFT_TOP,
                        "Pinned",
                        egui::FontId::proportional(9.0),
                        egui::Color32::from_rgb(255, 200, 80),
                    );
                }

                // Collapsed node: populate fallback port positions so wires still draw
                let is_collapsed = r.inner.is_none();
                if is_collapsed {
                    let rect = r.response.rect;
                    let n_in = n_inputs;
                    let n_out = n_outputs;
                    let fg = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("collapsed_ports")));
                    for i in 0..n_in {
                        let y_off = if n_in > 1 { (i as f32 - (n_in - 1) as f32 / 2.0) * 4.0 } else { 0.0 };
                        let pos = egui::pos2(rect.left() - PORT_RADIUS, rect.center().y + y_off);
                        port_positions.insert((node_id, i, true), pos);
                        fg.circle_filled(pos, 3.0, egui::Color32::from_rgb(170, 170, 170));
                    }
                    for i in 0..n_out {
                        let y_off = if n_out > 1 { (i as f32 - (n_out - 1) as f32 / 2.0) * 4.0 } else { 0.0 };
                        let pos = egui::pos2(rect.right() + PORT_RADIUS, rect.center().y + y_off);
                        port_positions.insert((node_id, i, false), pos);
                        fg.circle_filled(pos, 3.0, egui::Color32::from_rgb(100, 180, 255));
                    }
                }

                // Undo snapshot on drag start (coalesced — one per gesture)
                if r.response.drag_started() && !ctx.input(|i| i.modifiers.alt) && !self.drag_undo_pushed {
                    self.push_undo();
                    self.drag_undo_pushed = true;
                }
                if r.response.drag_stopped() {
                    self.drag_undo_pushed = false;
                }

                // Track drag delta for multi-node movement
                if r.response.dragged() && self.selected_nodes.contains(&node_id) && self.selected_nodes.len() > 1 {
                    multi_drag = Some((node_id, r.response.drag_delta()));
                }

                // Option+drag to duplicate
                if r.response.drag_started() && ctx.input(|i| i.modifiers.alt) {
                    self.opt_drag_source = Some(node_id);
                }
            }
            if open { self.graph.nodes.insert(node_id, node); } else { nodes_to_delete.push(node_id); }
        }

        // Apply multi-node drag: move all other selected nodes by the same delta
        if let Some((dragged_id, delta)) = multi_drag {
            if delta.length() > 0.0 {
                for &nid in &self.selected_nodes {
                    if nid != dragged_id {
                        if let Some(node) = self.graph.nodes.get_mut(&nid) {
                            if self.pinned_nodes.contains(&nid) {
                                node.pos[0] += delta.x * zoom;
                                node.pos[1] += delta.y * zoom;
                            } else {
                                // delta is in egui logical coords; convert to canvas coords
                                node.pos[0] += delta.x;
                                node.pos[1] += delta.y;
                            }
                        }
                    }
                }
            }
        }

        if let Some((src_node, src_port, is_output)) = dragging_from {
            if ctx.input(|i| i.pointer.any_released()) {
                if let Some(pointer) = ctx.pointer_latest_pos() {
                    for (&(nid, pidx, is_input), &pos) in &port_positions {
                        if pos.distance(pointer) < PORT_INTERACT * 1.5 {
                            if is_output && is_input && nid != src_node { pending_connections.push((src_node, src_port, nid, pidx)); }
                            else if !is_output && !is_input && nid != src_node { pending_connections.push((nid, pidx, src_node, src_port)); }
                            break;
                        }
                    }
                }
                dragging_from = None;
            }
        }

        self.dragging_from = dragging_from;
        self.port_positions = port_positions;
        self.node_rects = node_rects;
        self.monitor = monitor_state;
        self.ob = ob_manager;
        self.audio = audio_manager;
        // Push undo snapshot if any graph mutations are pending
        if !nodes_to_delete.is_empty() || !pending_disconnects.is_empty()
            || !pending_connections.is_empty() || !palette_spawns.is_empty() {
            self.push_undo();
        }
        for id in nodes_to_delete { self.midi.cleanup_node(id); self.serial.cleanup_node(id); self.osc.cleanup_node(id); self.ob.cleanup_node(id); self.audio.cleanup_node(id); crate::nodes::video_player::cleanup_node(id); self.graph.remove_node(id); }
        for (nid, port) in pending_disconnects { self.graph.remove_connections_to_port(nid, port); }
        for (fn_, fp, tn, tp) in pending_connections { self.graph.add_connection(fn_, fp, tn, tp); }
        // Spawn nodes from Palette clicks (place to the right of the palette node)
        for (palette_pos, nt) in palette_spawns {
            self.graph.add_node(nt, [palette_pos[0] + 250.0, palette_pos[1]]);
        }
        self.midi.process(midi_actions);
        self.serial.process(serial_actions);
        self.osc.process(osc_actions);
        self.http.process(http_actions);
    }

    fn render_connections(&mut self, ctx: &egui::Context) {
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("connections")));
        let pointer = ctx.pointer_latest_pos();
        let clicked = ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));
        let mut hovered_conn: Option<usize> = None;

        for (idx, conn) in self.graph.connections.iter().enumerate() {
            let from = self.port_positions.get(&(conn.from_node, conn.from_port, false));
            let to = self.port_positions.get(&(conn.to_node, conn.to_port, true));
            if let (Some(&a), Some(&b)) = (from, to) {
                let is_selected = self.selected_connection == Some(idx);
                let is_hovered = pointer.map(|p| point_near_bezier(p, a, b, 8.0)).unwrap_or(false);
                if is_hovered { hovered_conn = Some(idx); }

                let (color, width) = if is_selected {
                    (egui::Color32::from_rgb(255, 170, 0), 3.0) // amber
                } else if is_hovered {
                    (egui::Color32::from_rgb(220, 200, 120), 2.5)
                } else {
                    (CONN_COLOR, 2.0)
                };
                draw_bezier(&painter, a, b, color, width);
            }
        }

        // Click on connection to select (only if not clicking on a node or port)
        if clicked {
            if let Some(idx) = hovered_conn {
                // Check we're not near any port (don't select wire if clicking a port)
                let near_port = pointer.map(|p| {
                    self.port_positions.values().any(|&pp| pp.distance(p) < PORT_INTERACT)
                }).unwrap_or(false);
                if !near_port {
                    self.selected_connection = Some(idx);
                    self.selected_nodes.clear();
                }
            }
        }

        if let Some((nid, pidx, is_output)) = self.dragging_from {
            if let Some(&from) = self.port_positions.get(&(nid, pidx, !is_output)) {
                if let Some(ptr) = ctx.pointer_latest_pos() {
                    if is_output { draw_bezier(&painter, from, ptr, CONN_ACTIVE, 2.5); }
                    else { draw_bezier(&painter, ptr, from, CONN_ACTIVE, 2.5); }
                }
            }
        }

        // Draw dashed association lines between OB Hub and its device nodes
        let dash_color = egui::Color32::from_rgba_unmultiplied(80, 200, 120, 100);
        for (&dev_id, dev_node) in &self.graph.nodes {
            let hub_id = match &dev_node.node_type {
                NodeType::ObJoystick { hub_node_id, .. } if *hub_node_id != 0 => *hub_node_id,
                NodeType::ObEncoder { hub_node_id, .. } if *hub_node_id != 0 => *hub_node_id,
                _ => continue,
            };
            if let (Some(hub_rect), Some(dev_rect)) = (self.node_rects.get(&hub_id), self.node_rects.get(&dev_id)) {
                let from = egui::pos2(hub_rect.right(), hub_rect.center().y);
                let to = egui::pos2(dev_rect.left(), dev_rect.center().y);
                // Draw dashed line
                let total = from.distance(to);
                let dash_len = 6.0;
                let gap_len = 4.0;
                let dir = (to - from) / total;
                let mut d = 0.0;
                while d < total {
                    let seg_start = from + dir * d;
                    let seg_end = from + dir * (d + dash_len).min(total);
                    painter.line_segment([seg_start, seg_end], egui::Stroke::new(1.5, dash_color));
                    d += dash_len + gap_len;
                }
            }
        }
    }

    // ── Add-node menu with search ───────────────────────────────────

    /// Apply inverse-zoom style so menus appear at native screen size
    fn apply_inverse_zoom_style(&self, ctx: &egui::Context) -> std::sync::Arc<egui::Style> {
        let original = ctx.style();
        let inv = 1.0 / self.canvas_zoom;
        if (self.canvas_zoom - 1.0).abs() > 0.001 {
            let mut style = original.as_ref().clone();
            for (_, font_id) in style.text_styles.iter_mut() {
                font_id.size *= inv;
            }
            style.spacing.item_spacing *= inv;
            style.spacing.button_padding *= inv;
            style.spacing.interact_size *= inv;
            style.spacing.window_margin *= inv;
            ctx.set_style(std::sync::Arc::new(style));
        }
        original
    }

    fn node_menu(&mut self, ctx: &egui::Context) {
        if !self.show_node_menu { return; }
        // Menus at native screen size regardless of zoom
        let orig_style = self.apply_inverse_zoom_style(ctx);
        let pos = self.node_menu_pos;
        let mut keep_open = true;
        let menu_id = egui::Id::new("add_node_menu_window");

        let accent_rgb = self.graph.nodes.values()
            .find_map(|n| if let NodeType::Theme { accent, .. } = &n.node_type { Some(*accent) } else { None })
            .unwrap_or([80, 160, 255]);
        let accent = egui::Color32::from_rgb(accent_rgb[0], accent_rgb[1], accent_rgb[2]);

        let inv = 1.0 / self.canvas_zoom;
        let resp = egui::Window::new(egui::RichText::new("➕ Add Node").color(accent).strong())
            .id(menu_id)
            .fixed_pos(pos)
            .default_width(200.0 * inv)
            .resizable(false)
            .collapsible(false)
            .title_bar(true)
            .scroll([false, true])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                // Search box (auto-focused)
                let search_re = ui.add(
                    egui::TextEdit::singleline(&mut self.node_menu_search)
                        .hint_text("🔍 Search nodes...")
                        .desired_width(180.0),
                );
                if search_re.gained_focus() || self.node_menu_search.is_empty() {
                    search_re.request_focus();
                }

                ui.separator();

                let query = self.node_menu_search.to_lowercase();
                let catalog = nodes::catalog();

                let mut last_cat = "";
                let mut any_shown = false;

                // Place new node at the original double-click position
                // canvas_pos = (screen_pos - offset) / zoom
                let spawn_x = (self.node_menu_pos.x - self.canvas_offset.x) / self.canvas_zoom;
                let mut spawn_y_base = (self.node_menu_pos.y - self.canvas_offset.y) / self.canvas_zoom;

                for entry in &catalog {
                    // Hide system nodes (auto-created, not user-addable)
                    if entry.category == "System" { continue; }

                    if !query.is_empty()
                        && !entry.label.to_lowercase().contains(&query)
                        && !entry.category.to_lowercase().contains(&query)
                    {
                        continue;
                    }

                    // Category header
                    if entry.category != last_cat {
                        if !last_cat.is_empty() {
                            ui.add_space(4.0);
                            ui.separator();
                            ui.add_space(2.0);
                        }
                        ui.label(egui::RichText::new(entry.category).small().strong().color(egui::Color32::GRAY));
                        last_cat = entry.category;
                    }

                    // Each node as a small styled button
                    let btn = ui.add_sized(
                        [ui.available_width(), 24.0],
                        egui::Button::new(egui::RichText::new(entry.label).size(13.0))
                            .frame(true),
                    );

                    if btn.clicked() {
                        self.push_undo();
                        self.graph.add_node((entry.factory)(), [spawn_x, spawn_y_base]);
                        spawn_y_base += 40.0;
                        keep_open = false;
                    }

                    // Tooltip with category
                    btn.on_hover_text(format!("Add {} node", entry.label));

                    any_shown = true;
                }

                if !any_shown {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("No matches").color(egui::Color32::GRAY).italics());
                    ui.add_space(8.0);
                }
            });

        // Close on click outside the menu window
        if let Some(inner) = &resp {
            let menu_rect = inner.response.rect;
            let clicked_outside = ctx.input(|i| {
                i.pointer.button_clicked(egui::PointerButton::Primary)
                    || i.pointer.button_clicked(egui::PointerButton::Secondary)
            }) && ctx.pointer_latest_pos().map(|p| !menu_rect.contains(p)).unwrap_or(false);

            // Also close on Escape
            let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));

            if clicked_outside || esc {
                keep_open = false;
            }
        }

        if !keep_open {
            self.show_node_menu = false;
            self.node_menu_search.clear();
        }
        ctx.set_style(orig_style);
    }

    /// Pan/zoom handling — with set_zoom_factor, pointer is in logical coords.
    fn handle_pan_zoom(&mut self, ctx: &egui::Context) {
        let z = self.canvas_zoom;
        let space_held = ctx.input(|i| i.key_down(egui::Key::Space));
        let middle_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
        let modifiers = ctx.input(|i| i.modifiers);

        let on_node = ctx.pointer_latest_pos().map(|p| {
            self.node_rects.values().any(|r| r.contains(p))
        }).unwrap_or(false);

        let dragging_node = ctx.pointer_latest_pos().map(|p| {
            ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary))
                && self.node_rects.values().any(|r| r.contains(p))
        }).unwrap_or(false);

        if self.show_context_menu {
            self.panning = false;
            return;
        }

        // Pan: delta is logical, multiply by zoom to get screen pixels for offset
        if middle_down || (space_held && ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary))) {
            self.panning = true;
            let delta = ctx.input(|i| i.pointer.delta());
            self.canvas_offset += delta * z;
        } else {
            self.panning = false;
        }

        // Trackpad scroll pan
        if !modifiers.command {
            let scroll = ctx.input(|i| i.smooth_scroll_delta);
            if scroll.length() > 0.5 {
                self.canvas_offset += scroll * z;
            }
        }

        // Zoom
        let min_zoom: f32 = 0.08;
        let max_zoom: f32 = 4.0;

        // Pinch-to-zoom — pointer is logical, convert to screen
        let pinch = ctx.input(|i| i.zoom_delta());
        if (pinch - 1.0).abs() > 0.001 {
            let old_zoom = self.canvas_zoom;
            self.canvas_zoom = (self.canvas_zoom * pinch).clamp(min_zoom, max_zoom);
            if let Some(pointer) = ctx.pointer_latest_pos() {
                let screen_ptr = pointer.to_vec2() * old_zoom;
                let ratio = self.canvas_zoom / old_zoom;
                self.canvas_offset = screen_ptr - (screen_ptr - self.canvas_offset) * ratio;
            }
        }

        // Cmd+scroll → zoom
        if modifiers.command && !on_node {
            let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.5 {
                let old_zoom = self.canvas_zoom;
                self.canvas_zoom = (self.canvas_zoom + scroll * 0.003).clamp(min_zoom, max_zoom);
                if let Some(pointer) = ctx.pointer_latest_pos() {
                    let screen_ptr = pointer.to_vec2() * old_zoom;
                    let ratio = self.canvas_zoom / old_zoom;
                    self.canvas_offset = screen_ptr - (screen_ptr - self.canvas_offset) * ratio;
                }
            }
        }

        // Clamp pan boundary
        let z = self.canvas_zoom;
        let boundary: f32 = 2000.0;
        let screen = ctx.screen_rect();
        // No conversion needed — screen rect is already in screen pixels
        let real_w = screen.width();
        let real_h = screen.height();
        let (mut min_cx, mut min_cy, mut max_cx, mut max_cy): (f32, f32, f32, f32) = (-boundary, -boundary, boundary, boundary);
        for node in self.graph.nodes.values() {
            if self.pinned_nodes.contains(&node.id) { continue; }
            min_cx = min_cx.min(node.pos[0] - 200.0);
            min_cy = min_cy.min(node.pos[1] - 200.0);
            max_cx = max_cx.max(node.pos[0] + 400.0);
            max_cy = max_cy.max(node.pos[1] + 400.0);
        }
        let max_off_x = -min_cx * z + real_w;
        let min_off_x = -max_cx * z;
        let max_off_y = -min_cy * z + real_h;
        let min_off_y = -max_cy * z;
        self.canvas_offset.x = self.canvas_offset.x.clamp(min_off_x, max_off_x);
        self.canvas_offset.y = self.canvas_offset.y.clamp(min_off_y, max_off_y);

        // Reset / fit
        if (modifiers.mac_cmd || modifiers.ctrl) && ctx.input(|i| i.key_pressed(egui::Key::Num0)) {
            self.canvas_offset = egui::Vec2::ZERO;
            self.canvas_zoom = 1.0;
        }
        if (modifiers.mac_cmd || modifiers.ctrl) && ctx.input(|i| i.key_pressed(egui::Key::Num1)) {
            self.fit_all_nodes(ctx);
        }

        // Cursor icon: grab when panning, move when dragging a node
        if self.panning {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if dragging_node && !self.panning {
            ctx.set_cursor_icon(egui::CursorIcon::Move);
        } else if space_held {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
        }
    }

    /// Node interaction — pointer and node_rects are in screen pixel space.
    fn handle_canvas_interaction(&mut self, ctx: &egui::Context) {
        // Double-click to add node
        if ctx.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary)) {
            if let Some(pos) = ctx.pointer_latest_pos() {
                if !self.node_rects.values().any(|r| r.contains(pos)) {
                    self.show_node_menu = true;
                    self.node_menu_pos = pos;
                    self.node_menu_search.clear();
                }
            }
        }
        // Box selection & click-on-empty deselect
        let primary_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
        let primary_pressed = ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
        let primary_released = ctx.input(|i| i.pointer.any_released());
        let on_empty = ctx.pointer_latest_pos().map(|p| !self.node_rects.values().any(|r| r.contains(p))).unwrap_or(false);
        let not_dragging_wire = self.dragging_from.is_none();
        let not_panning = !self.panning;
        let not_menu = !self.show_node_menu && !self.show_context_menu;

        if primary_pressed && on_empty && not_dragging_wire && not_panning && not_menu {
            if let Some(pos) = ctx.pointer_latest_pos() {
                self.box_select_start = Some(pos);
                self.box_select_end = Some(pos);
            }
        }

        if let Some(_start) = self.box_select_start {
            if primary_down {
                if let Some(pos) = ctx.pointer_latest_pos() {
                    self.box_select_end = Some(pos);
                }
            }
            if primary_released {
                // Compute selection rect
                if let (Some(start), Some(end)) = (self.box_select_start, self.box_select_end) {
                    let sel_rect = egui::Rect::from_two_pos(start, end);
                    let shift = ctx.input(|i| i.modifiers.shift);
                    // Only select if drag was meaningful (> 4px), otherwise treat as click-deselect
                    if sel_rect.width() > 4.0 || sel_rect.height() > 4.0 {
                        let hits: std::collections::HashSet<NodeId> = self.node_rects.iter()
                            .filter(|(_, rect)| sel_rect.intersects(**rect))
                            .map(|(id, _)| *id)
                            .collect();
                        if shift {
                            for id in hits { self.selected_nodes.insert(id); }
                        } else {
                            self.selected_nodes = hits;
                        }
                    } else {
                        // Small click on empty = deselect
                        if !ctx.input(|i| i.modifiers.shift) {
                            self.selected_nodes.clear();
                        }
                        self.selected_connection = None;
                    }
                }
                self.box_select_start = None;
                self.box_select_end = None;
            }
        }
        if self.show_node_menu && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_node_menu = false;
            self.node_menu_search.clear();
        }
        if self.show_node_menu && ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary)) {
            self.show_node_menu = false;
            self.node_menu_search.clear();
        }
        if self.show_context_menu && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_context_menu = false;
        }

        // Keyboard shortcuts
        self.handle_keyboard_shortcuts(ctx);

        // Option+drag completion
        self.handle_opt_drag(ctx);
    }

    fn fit_all_nodes(&mut self, ctx: &egui::Context) {
        if self.graph.nodes.is_empty() { return; }
        let mut min_x = f32::MAX; let mut min_y = f32::MAX;
        let mut max_x = f32::MIN; let mut max_y = f32::MIN;
        for node in self.graph.nodes.values() {
            if self.pinned_nodes.contains(&node.id) { continue; }
            min_x = min_x.min(node.pos[0]);
            min_y = min_y.min(node.pos[1]);
            max_x = max_x.max(node.pos[0] + 200.0);
            max_y = max_y.max(node.pos[1] + 150.0);
        }
        if min_x >= max_x { return; }
        let screen = ctx.screen_rect();
        let margin = 80.0;
        // Screen rect is now in real pixels (no zoom factor applied)
        let available_w = screen.width() - margin * 2.0;
        let available_h = screen.height() - margin * 2.0;
        let content_w = max_x - min_x;
        let content_h = max_y - min_y;
        let zoom_x = available_w / content_w;
        let zoom_y = available_h / content_h;
        self.canvas_zoom = zoom_x.min(zoom_y).clamp(0.08, 2.0);
        self.canvas_offset = egui::vec2(
            margin - min_x * self.canvas_zoom,
            margin - min_y * self.canvas_zoom,
        );
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        let modifiers = ctx.input(|i| i.modifiers);
        let cmd = modifiers.mac_cmd || modifiers.ctrl;
        let text_focused = ctx.wants_keyboard_input();

        // Cmd+Z = undo, Cmd+Shift+Z = redo
        if cmd && !modifiers.shift && !text_focused && ctx.input(|i| i.key_pressed(egui::Key::Z)) {
            self.perform_undo();
        }
        if cmd && modifiers.shift && !text_focused && ctx.input(|i| i.key_pressed(egui::Key::Z)) {
            self.perform_redo();
        }

        // Cmd+C = copy node (only when no text field is focused)
        if cmd && !text_focused && ctx.input(|i| i.key_pressed(egui::Key::C)) {
            if let Some(id) = self.primary_selected() {
                if let Some(node) = self.graph.nodes.get(&id) {
                    self.clipboard = Some(node.node_type.clone());
                }
            }
        }
        // Cmd+V = paste node (only when no text field is focused)
        if cmd && !text_focused && ctx.input(|i| i.key_pressed(egui::Key::V)) {
            if let Some(nt) = self.clipboard.clone() {
                let (cx, cy) = if let Some(id) = self.primary_selected() {
                    if let Some(node) = self.graph.nodes.get(&id) {
                        (node.pos[0] + 30.0, node.pos[1] + 30.0)
                    } else {
                        let pos = ctx.pointer_latest_pos().unwrap_or(egui::pos2(200.0, 200.0));
                        let off_e = self.canvas_offset / self.canvas_zoom;
                        (pos.x - off_e.x + 20.0, pos.y - off_e.y + 20.0)
                    }
                } else {
                    let pos = ctx.pointer_latest_pos().unwrap_or(egui::pos2(200.0, 200.0));
                    let off_e = self.canvas_offset / self.canvas_zoom;
                    (pos.x - off_e.x + 20.0, pos.y - off_e.y + 20.0)
                };
                self.push_undo();
                let new_id = self.graph.add_node(nt, [cx, cy]);
                self.selected_nodes.clear();
                self.selected_nodes.insert(new_id);
            }
        }
        // Cmd+D = duplicate (only when no text field is focused)
        if cmd && !text_focused && ctx.input(|i| i.key_pressed(egui::Key::D)) {
            self.duplicate_selected();
        }
        // Delete / Backspace = delete selected (only if no text input is focused)
        if !text_focused && ctx.input(|i| i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete)) {
            if let Some(conn_idx) = self.selected_connection.take() {
                if conn_idx < self.graph.connections.len() {
                    self.push_undo();
                    self.graph.connections.remove(conn_idx);
                }
            } else if !self.selected_nodes.is_empty() {
                self.push_undo();
                let to_delete: Vec<NodeId> = self.selected_nodes.drain().collect();
                for id in to_delete {
                    self.midi.cleanup_node(id);
                    self.serial.cleanup_node(id);
                    self.osc.cleanup_node(id);
                    self.ob.cleanup_node(id);
                    self.audio.cleanup_node(id);
                    crate::nodes::video_player::cleanup_node(id);
                    self.graph.remove_node(id);
                }
            }
        }
    }

    fn handle_opt_drag(&mut self, ctx: &egui::Context) {
        if let Some(source_id) = self.opt_drag_source {
            if self.opt_drag_created.is_none() {
                // Create duplicate at same position
                if let Some(node) = self.graph.nodes.get(&source_id) {
                    let nt = node.node_type.clone();
                    let pos = node.pos;
                    self.push_undo();
                    let new_id = self.graph.add_node(nt, [pos[0] + 30.0, pos[1] + 30.0]);
                    self.opt_drag_created = Some(new_id);
                    self.selected_nodes.clear();
                    self.selected_nodes.insert(new_id);
                }
            }
            if ctx.input(|i| i.pointer.any_released()) {
                self.opt_drag_source = None;
                self.opt_drag_created = None;
            }
        }
    }

    fn duplicate_selected(&mut self) {
        let ids: Vec<NodeId> = self.selected_nodes.iter().copied().collect();
        if ids.is_empty() { return; }
        self.push_undo();
        let mut new_ids = std::collections::HashSet::new();
        for id in ids {
            if let Some(node) = self.graph.nodes.get(&id) {
                let nt = node.node_type.clone();
                let pos = node.pos;
                let new_id = self.graph.add_node(nt, [pos[0] + 30.0, pos[1] + 30.0]);
                new_ids.insert(new_id);
            }
        }
        self.selected_nodes = new_ids;
    }

    fn context_menu(&mut self, ctx: &egui::Context) {
        if !self.show_context_menu { return; }
        let orig_style = self.apply_inverse_zoom_style(ctx);
        let pos = self.context_menu_pos;
        let mut keep_open = true;

        egui::Area::new(egui::Id::new("node_context_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(120.0 / self.canvas_zoom);

                    // Undo / Redo
                    ui.add_enabled_ui(self.undo_history.can_undo(), |ui| {
                        if ui.button("Undo  ⌘Z").clicked() {
                            self.perform_undo();
                            keep_open = false;
                        }
                    });
                    ui.add_enabled_ui(self.undo_history.can_redo(), |ui| {
                        if ui.button("Redo  ⌘⇧Z").clicked() {
                            self.perform_redo();
                            keep_open = false;
                        }
                    });
                    ui.separator();

                    if ui.button("Copy").clicked() {
                        if let Some(id) = self.context_menu_node {
                            if let Some(node) = self.graph.nodes.get(&id) {
                                self.clipboard = Some(node.node_type.clone());
                            }
                        }
                        keep_open = false;
                    }
                    if self.clipboard.is_some() {
                        if ui.button("Paste").clicked() {
                            if let Some(nt) = self.clipboard.clone() {
                                self.push_undo();
                                let off_e = self.canvas_offset / self.canvas_zoom;
                                let new_id = self.graph.add_node(nt, [pos.x - off_e.x + 20.0, pos.y - off_e.y + 20.0]);
                                self.selected_nodes.clear();
                                self.selected_nodes.insert(new_id);
                            }
                            keep_open = false;
                        }
                    }
                    if ui.button("Duplicate").clicked() {
                        if let Some(id) = self.context_menu_node {
                            if let Some(node) = self.graph.nodes.get(&id) {
                                let nt = node.node_type.clone();
                                let p = node.pos;
                                self.push_undo();
                                let new_id = self.graph.add_node(nt, [p[0] + 30.0, p[1] + 30.0]);
                                self.selected_nodes.clear();
                                self.selected_nodes.insert(new_id);
                            }
                        }
                        keep_open = false;
                    }
                    ui.separator();
                    // Pin/Unpin toggle
                    if let Some(id) = self.context_menu_node {
                        let is_pinned = self.pinned_nodes.contains(&id);
                        let pin_label = if is_pinned { "Unpin from screen" } else { "Pin to screen" };
                        if ui.button(pin_label).clicked() {
                            self.push_undo();
                            if is_pinned {
                                // Unpin: pos is currently screen pixels, convert to canvas
                                // canvas = (screen - offset) / zoom
                                if let Some(node) = self.graph.nodes.get_mut(&id) {
                                    let cx = (node.pos[0] - self.canvas_offset.x) / self.canvas_zoom;
                                    let cy = (node.pos[1] - self.canvas_offset.y) / self.canvas_zoom;
                                    node.pos = [cx, cy];
                                }
                                self.pinned_nodes.remove(&id);
                            } else {
                                // Pin: pos is currently canvas coords, convert to screen pixels
                                // screen = canvas * zoom + offset
                                if let Some(node) = self.graph.nodes.get_mut(&id) {
                                    let sx = node.pos[0] * self.canvas_zoom + self.canvas_offset.x;
                                    let sy = node.pos[1] * self.canvas_zoom + self.canvas_offset.y;
                                    node.pos = [sx, sy];
                                }
                                self.pinned_nodes.insert(id);
                            }
                            keep_open = false;
                        }
                    }
                    ui.separator();
                    let del_label = if self.selected_nodes.len() > 1 {
                        format!("Delete {} nodes", self.selected_nodes.len())
                    } else {
                        "Delete".to_string()
                    };
                    if ui.button(egui::RichText::new(del_label).color(egui::Color32::from_rgb(255, 100, 100))).clicked() {
                        self.push_undo();
                        // Delete all selected nodes (or just the context menu node if not in selection)
                        let to_delete: Vec<NodeId> = if self.selected_nodes.len() > 1 {
                            self.selected_nodes.drain().collect()
                        } else if let Some(id) = self.context_menu_node {
                            self.selected_nodes.remove(&id);
                            vec![id]
                        } else {
                            vec![]
                        };
                        for id in to_delete {
                            self.midi.cleanup_node(id);
                            self.serial.cleanup_node(id);
                            self.osc.cleanup_node(id);
                            self.ob.cleanup_node(id);
                            self.audio.cleanup_node(id);
                            self.graph.remove_node(id);
                        }
                        keep_open = false;
                    }
                });
            });

        if !keep_open {
            self.show_context_menu = false;
            self.context_menu_node = None;
        }
        // Click elsewhere to close (primary or secondary click outside the menu)
        if ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary)
            || i.pointer.button_clicked(egui::PointerButton::Secondary))
        {
            if let Some(ptr) = ctx.pointer_latest_pos() {
                // Only dismiss if click is outside the context menu area
                let menu_rect = egui::Rect::from_min_size(pos, egui::vec2(140.0, 200.0));
                if !menu_rect.contains(ptr) {
                    self.show_context_menu = false;
                    self.context_menu_node = None;
                }
            }
        }
        ctx.set_style(orig_style);
    }

    fn update_mouse_trackers(&mut self, ctx: &egui::Context) {
        if let Some(pos) = ctx.pointer_latest_pos() {
            for node in self.graph.nodes.values_mut() {
                if let NodeType::MouseTracker { x, y } = &mut node.node_type { *x = pos.x; *y = pos.y; }
            }
        }
    }

    fn update_key_inputs(&mut self, ctx: &egui::Context) {
        // Don't capture keys when a text field is focused
        if ctx.wants_keyboard_input() { return; }

        for node in self.graph.nodes.values_mut() {
            if let NodeType::KeyInput { key_name, pressed, toggle_mode, toggled_on } = &mut node.node_type {
                if let Some(key) = nodes::key_input::parse_key(key_name) {
                    let is_down = ctx.input(|i| i.key_down(key));
                    let just_pressed = ctx.input(|i| i.key_pressed(key));

                    if *toggle_mode {
                        if just_pressed {
                            *toggled_on = !*toggled_on;
                        }
                        *pressed = just_pressed;
                    } else {
                        *pressed = is_down;
                    }
                } else {
                    *pressed = false;
                }
            }
        }
    }

    fn save_project(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().set_title("Save Project Folder").pick_folder() {
            let project_file = dir.join("project.json");
            let json = serde_json::to_string_pretty(&self.graph).unwrap_or_default();
            let _ = std::fs::write(&project_file, json);
            // Also save api_keys if any exist
            if !self.api_keys.is_empty() {
                let keys_file = dir.join("api_keys.json");
                let keys_json = serde_json::to_string_pretty(&self.api_keys).unwrap_or_default();
                let _ = std::fs::write(&keys_file, keys_json);
            }
            self.project_path = Some(dir.display().to_string());
        }
    }

    fn load_project(&mut self) {
        // Try picking a folder first, then fall back to file
        if let Some(path) = rfd::FileDialog::new().add_filter("Patchwork", &["json"]).pick_file() {
            let dir = if path.file_name().map(|f| f == "project.json").unwrap_or(false) {
                path.parent().map(|p| p.to_path_buf())
            } else {
                None
            };
            // Load graph from the file
            if let Ok(json) = std::fs::read_to_string(&path) {
                if let Ok(graph) = serde_json::from_str::<Graph>(&json) {
                    self.graph = graph;
                    self.port_positions.clear();
                    self.node_rects.clear();
                    self.undo_history.clear();
                }
            }
            // Load api_keys from the same folder
            if let Some(dir) = &dir {
                let keys_file = dir.join("api_keys.json");
                if let Ok(json) = std::fs::read_to_string(&keys_file) {
                    if let Ok(keys) = serde_json::from_str::<HashMap<String, String>>(&json) {
                        self.api_keys = keys;
                    }
                }
                self.project_path = Some(dir.display().to_string());
            } else {
                self.project_path = Some(path.display().to_string());
            }
        }
    }

    fn project_dir(&self) -> Option<std::path::PathBuf> {
        self.project_path.as_ref().map(|p| std::path::PathBuf::from(p))
    }

    fn load_api_keys(&mut self) {
        if let Some(dir) = self.project_dir() {
            let keys_file = dir.join("api_keys.json");
            if let Ok(json) = std::fs::read_to_string(&keys_file) {
                if let Ok(keys) = serde_json::from_str::<HashMap<String, String>>(&json) {
                    self.api_keys = keys;
                }
            }
        }
    }
}

impl eframe::App for PatchworkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // set_zoom_factor scales everything (nodes, text, chrome) via GPU — zero overhead.
        // Pinned nodes compensate with inverse zoom style + zoom-keyed window IDs.
        ctx.set_zoom_factor(self.canvas_zoom);

        self.apply_theme(ctx);
        self.handle_file_drop(ctx);
        self.update_mouse_trackers(ctx);
        self.update_key_inputs(ctx);
        self.poll_midi_inputs();
        self.poll_serial_inputs();
        self.poll_osc_inputs();
        self.poll_http_responses();
        self.ob.poll_all();
        self.poll_ml_inference(ctx);

        self.frame_count += 1;
        // Receive device lists from background enumeration thread (non-blocking)
        if let Ok(devices) = self.device_refresh_rx.try_recv() {
            self.midi.set_port_lists(devices.midi_in, devices.midi_out);
            self.serial.set_port_list(devices.serial);
            self.audio.set_device_lists(devices.audio_output, devices.audio_input);
        }

        let node_count = self.graph.nodes.len();
        let conn_count = self.graph.connections.len();
        self.monitor.tick(node_count, conn_count);

        let mut values = self.graph.evaluate();

        // Inject OB hardware values into the graph (before they're used by downstream nodes)
        // These need a second evaluation pass to propagate through Add/Multiply/etc.
        {
            let mut ob_injected = false;
            for (&id, node) in &self.graph.nodes {
                match &node.node_type {
                    NodeType::ObHub { detected_devices, .. } => {
                        let mut port_idx = 0usize;
                        let mut sorted = detected_devices.clone();
                        sorted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                        for (dtype, did) in &sorted {
                            if let Some(hub) = self.ob.get_hub(id) {
                                match dtype.as_str() {
                                    "joystick" => {
                                        values.insert((id, port_idx), PortValue::Float(hub.get_value("joystick", *did, "x")));
                                        values.insert((id, port_idx + 1), PortValue::Float(hub.get_value("joystick", *did, "y")));
                                        values.insert((id, port_idx + 2), PortValue::Float(hub.get_value("joystick", *did, "btn")));
                                        port_idx += 3;
                                    }
                                    "encoder" => {
                                        values.insert((id, port_idx), PortValue::Float(hub.get_value("encoder", *did, "turn")));
                                        values.insert((id, port_idx + 1), PortValue::Float(hub.get_value("encoder", *did, "click")));
                                        values.insert((id, port_idx + 2), PortValue::Float(hub.get_value("encoder", *did, "position")));
                                        port_idx += 3;
                                    }
                                    _ => { port_idx += 1; }
                                }
                            }
                        }
                        ob_injected = true;
                    }
                    NodeType::ObJoystick { device_id, hub_node_id } => {
                        let find = if *hub_node_id != 0 {
                            self.ob.get_hub(*hub_node_id).and_then(|h| h.get_device("joystick", *device_id))
                        } else {
                            self.ob.find_device("joystick", *device_id).map(|(_, d)| d)
                        };
                        if let Some(dev) = find {
                            values.insert((id, 0), PortValue::Float(dev.values.get("x").copied().unwrap_or(0.0)));
                            values.insert((id, 1), PortValue::Float(dev.values.get("y").copied().unwrap_or(0.0)));
                            values.insert((id, 2), PortValue::Float(dev.values.get("btn").copied().unwrap_or(0.0)));
                        }
                        ob_injected = true;
                    }
                    NodeType::ObEncoder { device_id, hub_node_id } => {
                        let find = if *hub_node_id != 0 {
                            self.ob.get_hub(*hub_node_id).and_then(|h| h.get_device("encoder", *device_id))
                        } else {
                            self.ob.find_device("encoder", *device_id).map(|(_, d)| d)
                        };
                        if let Some(dev) = find {
                            values.insert((id, 0), PortValue::Float(dev.values.get("turn").copied().unwrap_or(0.0)));
                            values.insert((id, 1), PortValue::Float(dev.values.get("click").copied().unwrap_or(0.0)));
                            values.insert((id, 2), PortValue::Float(dev.values.get("position").copied().unwrap_or(0.0)));
                        }
                        ob_injected = true;
                    }
                    _ => {}
                }
            }
            // Re-evaluate to propagate OB values through downstream nodes (Add, Multiply, etc.)
            if ob_injected {
                self.graph.evaluate_with_existing(&mut values);
            }
        }

        // Profiler output values
        for (&id, node) in &self.graph.nodes {
            if matches!(node.node_type, NodeType::Profiler) {
                let profiler_id = egui::Id::new(("profiler_state", id));
                let state = ctx.data_mut(|d| {
                    d.get_temp_mut_or_insert_with::<std::sync::Arc<std::sync::Mutex<crate::nodes::profiler::ProfilerState>>>(
                        profiler_id,
                        || std::sync::Arc::new(std::sync::Mutex::new(crate::nodes::profiler::ProfilerState::new()))
                    ).clone()
                });
                if let Ok(s) = state.lock() {
                    let fps = s.fps_history.back().copied().unwrap_or(0.0);
                    if let Ok(m) = s.metrics.lock() {
                        values.insert((id, 0), PortValue::Float(fps));
                        values.insert((id, 1), PortValue::Float(m.cpu_usage));
                        values.insert((id, 2), PortValue::Float(m.mem_percent));
                        values.insert((id, 3), PortValue::Float(m.process_mem_mb));
                    }
                }
            }
        }

        // Evaluate Rust Plugin nodes
        {
            let plugin_ids: Vec<NodeId> = self.graph.nodes.iter()
                .filter(|(_, n)| matches!(n.node_type, NodeType::RustPlugin { .. }))
                .map(|(&id, _)| id)
                .collect();
            for id in plugin_ids {
                let inputs: Vec<PortValue> = self.graph.collect_inputs(id, &values);
                let node = match self.graph.nodes.get_mut(&id) { Some(n) => n, None => continue };
                let outputs = nodes::rust_plugin::evaluate(id, &mut node.node_type, &inputs, ctx);
                for (port, val) in outputs {
                    values.insert((id, port), val);
                }
            }
        }

        // Evaluate image nodes (with caching — only reprocess when inputs change)
        // Run 2 passes so downstream nodes (e.g., Image receiver) see upstream results (e.g., Effects output)
        for _img_pass in 0..2 {
            let image_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
            for id in image_ids {
                let inputs: Vec<PortValue> = self.graph.collect_inputs(id, &values);
                let node = match self.graph.nodes.get(&id) { Some(n) => n, None => continue };
                match &node.node_type {
                    NodeType::ImageNode { image_data, .. } => {
                        let out = if let Some(PortValue::Image(img)) = inputs.first() {
                            PortValue::Image(img.clone())
                        } else if let Some(img) = image_data {
                            PortValue::Image(img.clone())
                        } else {
                            PortValue::None
                        };
                        values.insert((id, 0), out);
                    }
                    NodeType::ImageEffects { brightness, contrast, saturation, hue, exposure, gamma } => {
                        if let Some(PortValue::Image(img)) = inputs.first() {
                            // Cache key: param hash + image pointer
                            let param_hash = ((*brightness * 1000.0) as u64) ^ ((*contrast * 1000.0) as u64) << 8
                                ^ ((*saturation * 1000.0) as u64) << 16 ^ ((*hue * 10.0) as u64) << 24
                                ^ ((*exposure * 1000.0) as u64) << 32 ^ ((*gamma * 1000.0) as u64) << 40;
                            let img_ptr = std::sync::Arc::as_ptr(img) as u64;
                            let cache_key = param_hash ^ img_ptr;
                            let cache_id = egui::Id::new(("img_fx_cache", id));
                            let cached: Option<(u64, std::sync::Arc<ImageData>)> = ctx.data_mut(|d| d.get_temp(cache_id));
                            if let Some((prev_key, prev_result)) = cached {
                                if prev_key == cache_key {
                                    values.insert((id, 0), PortValue::Image(prev_result));
                                    continue;
                                }
                            }
                            let result = nodes::image_effects::process(img, *brightness, *contrast, *saturation, *hue, *exposure, *gamma);
                            ctx.data_mut(|d| d.insert_temp(cache_id, (cache_key, result.clone())));
                            values.insert((id, 0), PortValue::Image(result));
                        }
                    }
                    NodeType::Blend { mode, mix } => {
                        let a = inputs.first().and_then(|v| v.as_image());
                        let b = inputs.get(1).and_then(|v| v.as_image());
                        if let (Some(a), Some(b)) = (a, b) {
                            let cache_key = (std::sync::Arc::as_ptr(a) as u64) ^ (std::sync::Arc::as_ptr(b) as u64)
                                ^ (*mode as u64) ^ ((*mix * 1000.0) as u64) << 8;
                            let cache_id = egui::Id::new(("blend_cache", id));
                            let cached: Option<(u64, std::sync::Arc<ImageData>)> = ctx.data_mut(|d| d.get_temp(cache_id));
                            if let Some((prev_key, prev_result)) = cached {
                                if prev_key == cache_key {
                                    values.insert((id, 0), PortValue::Image(prev_result));
                                    continue;
                                }
                            }
                            let result = nodes::blend::process(a, b, *mode, *mix);
                            ctx.data_mut(|d| d.insert_temp(cache_id, (cache_key, result.clone())));
                            values.insert((id, 0), PortValue::Image(result));
                        }
                    }
                    NodeType::Curve { points } => {
                        let x = inputs.first().map(|v| v.as_float()).unwrap_or(0.0);
                        let y = nodes::curve::evaluate_curve(points, x);
                        values.insert((id, 0), PortValue::Float(y));
                    }
                    NodeType::Draw { strokes, canvas_size, .. } => {
                        // Only regenerate if strokes changed
                        let stroke_hash = strokes.len() as u64 ^ (*canvas_size as u64);
                        let cache_id = egui::Id::new(("draw_cache", id));
                        let cached: Option<(u64, std::sync::Arc<ImageData>)> = ctx.data_mut(|d| d.get_temp(cache_id));
                        if let Some((prev_hash, prev_img)) = cached {
                            if prev_hash == stroke_hash {
                                values.insert((id, 0), PortValue::Image(prev_img));
                            } else {
                                let img = nodes::draw::render_to_image(strokes, *canvas_size as u32);
                                ctx.data_mut(|d| d.insert_temp(cache_id, (stroke_hash, img.clone())));
                                values.insert((id, 0), PortValue::Image(img));
                            }
                        } else {
                            let img = nodes::draw::render_to_image(strokes, *canvas_size as u32);
                            ctx.data_mut(|d| d.insert_temp(cache_id, (stroke_hash, img.clone())));
                            values.insert((id, 0), PortValue::Image(img));
                        }
                        let pts_json = serde_json::to_string(strokes).unwrap_or_default();
                        values.insert((id, 1), PortValue::Text(pts_json));
                    }
                    NodeType::Noise { noise_type, mode, scale, seed } => {
                        let x = inputs.get(2).map(|v| v.as_float()).unwrap_or(0.0);
                        let val = nodes::noise::perlin_1d(x * *scale, *seed);
                        values.insert((id, 0), PortValue::Float(val));
                        if *mode == 1 {
                            let cache_key = (*seed as u64) ^ ((*scale * 100.0) as u64) << 16 ^ (*noise_type as u64) << 32;
                            let cache_id = egui::Id::new(("noise_cache", id));
                            let cached: Option<(u64, std::sync::Arc<ImageData>)> = ctx.data_mut(|d| d.get_temp(cache_id));
                            if let Some((prev_key, prev_img)) = cached {
                                if prev_key == cache_key {
                                    values.insert((id, 1), PortValue::Image(prev_img));
                                    continue;
                                }
                            }
                            let img = nodes::noise::generate_2d(*scale, *seed, *noise_type, 128);
                            ctx.data_mut(|d| d.insert_temp(cache_id, (cache_key, img.clone())));
                            values.insert((id, 1), PortValue::Image(img));
                        }
                    }
                    NodeType::ColorCurves { master, red, green, blue, .. } => {
                        if let Some(PortValue::Image(img)) = inputs.first() {
                            let img_ptr = std::sync::Arc::as_ptr(img) as u64;
                            let curve_hash = master.len() as u64 ^ red.len() as u64 ^ green.len() as u64 ^ blue.len() as u64;
                            let cache_key = img_ptr ^ curve_hash;
                            let cache_id = egui::Id::new(("cc_cache", id));
                            let cached: Option<(u64, std::sync::Arc<ImageData>)> = ctx.data_mut(|d| d.get_temp(cache_id));
                            if let Some((prev_key, prev_result)) = cached {
                                if prev_key == cache_key {
                                    values.insert((id, 0), PortValue::Image(prev_result));
                                    continue;
                                }
                            }
                            let result = nodes::color_curves::process(img, master, red, green, blue);
                            ctx.data_mut(|d| d.insert_temp(cache_id, (cache_key, result.clone())));
                            values.insert((id, 0), PortValue::Image(result));
                        }
                    }
                    NodeType::VideoPlayer { current_frame, duration, .. } => {
                        if let Some(frame) = current_frame {
                            values.insert((id, 0), PortValue::Image(frame.clone()));
                            if *duration > 0.0 {
                                // Progress output would need frame counting — skip for now
                                values.insert((id, 1), PortValue::Float(0.0));
                            }
                        }
                    }
                    NodeType::Camera { current_frame, .. } => {
                        if let Some(frame) = current_frame {
                            values.insert((id, 0), PortValue::Image(frame.clone()));
                        }
                    }
                    NodeType::MlModel { result_text, .. } => {
                        if !result_text.is_empty() {
                            values.insert((id, 0), PortValue::Text(result_text.clone()));
                        }
                    }
                    _ => {}
                }
            }
        } // end img_pass loop

        // Sync OB Hub detected_devices from ObManager + auto-connect saved ports
        {
            let hub_nodes: Vec<(NodeId, String)> = self.graph.nodes.iter()
                .filter_map(|(&id, n)| {
                    if let NodeType::ObHub { port_name, .. } = &n.node_type {
                        Some((id, port_name.clone()))
                    } else {
                        None
                    }
                })
                .collect();

            for (hub_id, port_name) in &hub_nodes {
                // Auto-connect: if port_name is set but hub not connected, try to reconnect
                if !port_name.is_empty() && self.ob.get_hub(*hub_id).is_none() {
                    let _ = self.ob.connect_hub(*hub_id, port_name);
                }

                // Sync detected_devices into NodeType
                let detected: Vec<(String, u8)> = self.ob.get_hub(*hub_id)
                    .map(|h| h.devices.keys().cloned().collect())
                    .unwrap_or_default();
                if let Some(node) = self.graph.nodes.get_mut(hub_id) {
                    if let NodeType::ObHub { detected_devices, .. } = &mut node.node_type {
                        *detected_devices = detected;
                    }
                }
            }
        }

        // Inject Monitor + Audio node values
        for (&id, node) in &self.graph.nodes {
            match &node.node_type {
                NodeType::Monitor => {
                    values.insert((id, 0), PortValue::Float(self.monitor.fps));
                    values.insert((id, 1), PortValue::Float(self.monitor.frame_ms));
                    values.insert((id, 2), PortValue::Float(self.monitor.node_count as f32));
                }
                // Synth and AudioPlayer output their NodeId so FX nodes can reference them
                NodeType::Synth { .. } | NodeType::AudioPlayer { .. } => {
                    values.insert((id, 0), PortValue::Float(id as f32));
                }
                _ => {}
            }
        }

        // Process MCP commands (if MCP server is active)
        self.process_mcp_commands(&values);

        self.canvas(ctx);
        self.render_connections(ctx);
        self.render_nodes_filtered(ctx, &values, false);
        self.render_nodes_filtered(ctx, &values, true);
        self.sync_console_messages();
        self.handle_system_node_actions(ctx);

        // Handle OB Hub spawn requests (create device node auto-connected to hub)
        {
            let hub_ids: Vec<NodeId> = self.graph.nodes.keys().copied()
                .filter(|id| matches!(self.graph.nodes.get(id).map(|n| &n.node_type), Some(NodeType::ObHub { .. })))
                .collect();
            for hub_id in hub_ids {
                let spawn_id = egui::Id::new(("ob_spawn", hub_id));
                if let Some((dtype, did)) = ctx.data_mut(|d| d.get_temp::<(String, u8)>(spawn_id)) {
                    ctx.data_mut(|d| d.remove::<(String, u8)>(spawn_id));
                    let hub_pos = self.graph.nodes.get(&hub_id).map(|n| n.pos).unwrap_or([200.0, 200.0]);
                    let nt = match dtype.as_str() {
                        "joystick" => NodeType::ObJoystick { device_id: did, hub_node_id: hub_id },
                        "encoder" => NodeType::ObEncoder { device_id: did, hub_node_id: hub_id },
                        _ => continue,
                    };
                    self.push_undo();
                    let new_id = self.graph.add_node(nt, [hub_pos[0] + 250.0, hub_pos[1]]);
                    self.selected_nodes.clear();
                    self.selected_nodes.insert(new_id);
                }
            }
        }
        self.node_menu(ctx);
        self.context_menu(ctx);
        self.handle_pan_zoom(ctx);
        self.handle_canvas_interaction(ctx);

        ctx.request_repaint();
    }
}

fn point_near_bezier(p: egui::Pos2, from: egui::Pos2, to: egui::Pos2, threshold: f32) -> bool {
    let dx = (to.x - from.x).abs().max(50.0) * 0.5;
    let cp1 = egui::pos2(from.x + dx, from.y);
    let cp2 = egui::pos2(to.x - dx, to.y);
    for i in 0..=20 {
        let t = i as f32 / 20.0;
        let it = 1.0 - t;
        let pt = egui::pos2(
            it*it*it*from.x + 3.0*it*it*t*cp1.x + 3.0*it*t*t*cp2.x + t*t*t*to.x,
            it*it*it*from.y + 3.0*it*it*t*cp1.y + 3.0*it*t*t*cp2.y + t*t*t*to.y,
        );
        if pt.distance(p) < threshold { return true; }
    }
    false
}

fn draw_bezier(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, color: egui::Color32, width: f32) {
    let dx = (to.x - from.x).abs().max(50.0) * 0.5;
    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
        [from, egui::pos2(from.x + dx, from.y), egui::pos2(to.x - dx, to.y), to],
        false, egui::Color32::TRANSPARENT, egui::Stroke::new(width, color),
    ));
}
