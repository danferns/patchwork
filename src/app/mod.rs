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


const PORT_RADIUS: f32 = 6.0;    // 12px diameter
const PORT_INTERACT: f32 = 18.0;
const PORT_BORDER: f32 = 3.0;    // 3px border stroke
const CONN_COLOR: egui::Color32 = egui::Color32::from_rgb(180, 180, 180);
const CONN_ACTIVE: egui::Color32 = egui::Color32::from_rgb(80, 170, 255);

/// Get port fill + border colors based on data type.
/// Fill is a darker shade, border is a lighter shade — both matching the wire hue.
fn port_colors_for_value(val: &PortValue, is_output: bool) -> (egui::Color32, egui::Color32) {
    let base: [u8; 3] = match val {
        PortValue::Float(_) => [80, 100, 230],      // matches wire blue
        PortValue::Text(_) => [60, 220, 80],         // matches wire green
        PortValue::Image(_) => [200, 30, 255],       // matches wire purple
        PortValue::None => [140, 140, 140],           // gray
    };
    let fill = if is_output {
        egui::Color32::from_rgb(base[0], base[1], base[2])
    } else {
        // Input: darker fill
        egui::Color32::from_rgb(
            (base[0] as f32 * 0.5) as u8,
            (base[1] as f32 * 0.5) as u8,
            (base[2] as f32 * 0.5) as u8,
        )
    };
    // Border: lighter shade of the wire color
    let border = egui::Color32::from_rgb(
        (base[0] as u16 + 80).min(255) as u8,
        (base[1] as u16 + 80).min(255) as u8,
        (base[2] as u16 + 80).min(255) as u8,
    );
    (fill, border)
}

mod undo;
mod canvas;
mod interaction;
mod io;
mod menus;

use undo::UndoHistory;

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
    wire_menu_conn: Option<usize>,      // connection index for wire context menu
    wire_menu_pos: egui::Pos2,
    // Box selection
    box_select_start: Option<egui::Pos2>,
    box_select_end: Option<egui::Pos2>,
    clipboard: Option<NodeType>,
    context_menu_node: Option<NodeId>,
    show_context_menu: bool,
    context_menu_pos: egui::Pos2,
    context_menu_opened_at: f64, // time when menu was opened (for auto-dismiss)
    // Option+drag duplication
    opt_drag_source: Option<NodeId>,
    opt_drag_created: Option<NodeId>,
    // Canvas pan & zoom
    canvas_offset: egui::Vec2,
    canvas_zoom: f32,
    _prev_zoom: f32,
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
        crate::icons::setup(&cc.egui_ctx);
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
            wire_menu_conn: None,
            wire_menu_pos: egui::Pos2::ZERO,
            box_select_start: None,
            box_select_end: None,
            clipboard: None,
            context_menu_node: None,
            show_context_menu: false,
            context_menu_pos: egui::Pos2::ZERO,
            context_menu_opened_at: 0.0,
            opt_drag_source: None,
            opt_drag_created: None,
            canvas_offset: egui::Vec2::ZERO,
            canvas_zoom: 1.0,
            _prev_zoom: 1.0,
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

    fn primary_selected(&self) -> Option<NodeId> {
        self.selected_nodes.iter().next().copied()
    }

    // Pinned nodes: pos is screen pixels, computed to egui coords inline in render

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
            let node_width = if node.node_type.custom_render() { 50.0 } else if is_pinned { 180.0 * inv } else { 180.0 };
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
            let is_custom_render = node.node_type.custom_render();
            let resp = egui::Window::new(title_rt)
                .id(egui::Id::new(("node", node_id, zoom_bucket)))
                .current_pos(egui::pos2(egui_x, egui_y))
                .default_width(node_width)
                .resizable(!is_custom_render)
                .constrain(false)
                .collapsible(!is_custom_render)
                .title_bar(!is_custom_render)
                .show(ctx, |ui| {
                    // Top input ports (skip for inline-port nodes)
                    if !inline {
                        for (i, pdef) in input_defs.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let (rect, response) = ui.allocate_exact_size(egui::vec2(port_sz, port_sz), egui::Sense::click_and_drag());
                                let val = Graph::static_input_value(&connections, values, node_id, i);
                                let (type_fill, type_border) = port_colors_for_value(&val, false);
                                let (fill, border) = if response.hovered() || response.dragged() {
                                    (egui::Color32::YELLOW, egui::Color32::WHITE)
                                } else { (type_fill, type_border) };
                                ui.painter().circle_filled(rect.center(), port_r, fill);
                                ui.painter().circle_stroke(rect.center(), port_r, egui::Stroke::new(PORT_BORDER, border));
                                port_positions.insert((node_id, i, true), rect.center());
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
                                // Push port circle to right edge
                                let remaining = ui.available_width() - port_sz - 2.0;
                                if remaining > 0.0 { ui.add_space(remaining); }
                                let (rect, response) = ui.allocate_exact_size(egui::vec2(port_sz, port_sz), egui::Sense::click_and_drag());
                                let (type_fill, type_border) = port_colors_for_value(&val, true);
                                let (fill, border) = if response.hovered() || response.dragged() {
                                    (egui::Color32::YELLOW, egui::Color32::WHITE)
                                } else { (type_fill, type_border) };
                                ui.painter().circle_filled(rect.center(), port_r, fill);
                                ui.painter().circle_stroke(rect.center(), port_r, egui::Stroke::new(PORT_BORDER, border));
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
                // For custom_render nodes (Slider), only select on drag — click opens their popup
                let select_trigger = if is_custom_render {
                    r.response.drag_started()
                } else {
                    r.response.clicked() || r.response.drag_started()
                };
                if select_trigger {
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

                // Right-click context menu — skip for custom_render nodes (they handle their own UI)
                let secondary_in_rect = !is_custom_render && ctx.input(|i| {
                    i.pointer.button_clicked(egui::PointerButton::Secondary)
                }) && ctx.pointer_latest_pos().map(|p| r.response.rect.contains(p)).unwrap_or(false);
                if !is_custom_render && (r.response.secondary_clicked() || secondary_in_rect) {
                    if !self.selected_nodes.contains(&node_id) {
                        self.selected_nodes.clear();
                        self.selected_nodes.insert(node_id);
                    }
                    self.context_menu_node = Some(node_id);
                    self.show_context_menu = true;
                    self.context_menu_pos = ctx.pointer_latest_pos().unwrap_or(r.response.rect.center());
                    self.context_menu_opened_at = ctx.input(|i| i.time);
                }

                // Draw selection highlight (skip for custom_render nodes — they handle their own visual feedback)
                if self.selected_nodes.contains(&node_id) && !is_custom_render {
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
                        fg.circle_filled(pos, 3.0, egui::Color32::from_rgb(70, 75, 85));
                    }
                    for i in 0..n_out {
                        let y_off = if n_out > 1 { (i as f32 - (n_out - 1) as f32 / 2.0) * 4.0 } else { 0.0 };
                        let pos = egui::pos2(rect.right() + PORT_RADIUS, rect.center().y + y_off);
                        port_positions.insert((node_id, i, false), pos);
                        fg.circle_filled(pos, 3.0, egui::Color32::from_rgb(60, 140, 255));
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
            // Process slider popup actions (pin/delete)
            let pin_id = egui::Id::new(("slider_pin_action", node_id));
            if ctx.data_mut(|d| d.get_temp::<bool>(pin_id).unwrap_or(false)) {
                ctx.data_mut(|d| d.remove::<bool>(pin_id));
                self.push_undo();
                if self.pinned_nodes.contains(&node_id) {
                    // Unpin: current pos is screen pixels stored by pinned rendering
                    // Convert to canvas coords: canvas = (screen - offset) / zoom
                    let sx = node.pos[0];
                    let sy = node.pos[1];
                    node.pos = [(sx - offset.x) / zoom, (sy - offset.y) / zoom];
                    self.pinned_nodes.remove(&node_id);
                } else {
                    // Pin: current pos is canvas coords
                    // Convert to screen pixels: screen = canvas * zoom + offset
                    let cx = node.pos[0];
                    let cy = node.pos[1];
                    node.pos = [cx * zoom + offset.x, cy * zoom + offset.y];
                    self.pinned_nodes.insert(node_id);
                }
            }
            let del_id = egui::Id::new(("slider_delete_action", node_id));
            if ctx.data_mut(|d| d.get_temp::<bool>(del_id).unwrap_or(false)) {
                ctx.data_mut(|d| d.remove::<bool>(del_id));
                nodes_to_delete.push(node_id);
            }

            self.graph.nodes.insert(node_id, node);
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
        self.render_connections(ctx, &values);
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
