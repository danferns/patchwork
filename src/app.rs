use crate::graph::*;
use crate::http::{HttpAction, HttpManager};
use crate::midi::{MidiAction, MidiManager};
use crate::serial::{SerialAction, SerialManager};
use crate::osc::{OscAction, OscManager};
use crate::nodes;
use eframe::egui;
use eframe::egui_wgpu;
use std::collections::HashMap;
use std::sync::Arc;

const PORT_RADIUS: f32 = 5.0;
const PORT_INTERACT: f32 = 14.0;
const CONN_COLOR: egui::Color32 = egui::Color32::from_rgb(180, 180, 180);
const CONN_ACTIVE: egui::Color32 = egui::Color32::from_rgb(80, 170, 255);

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
    selected_node: Option<NodeId>,
    selected_connection: Option<usize>,
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
    // WGPU render state for shader nodes
    wgpu_render_state: Option<egui_wgpu::RenderState>,
}

impl PatchworkApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let wgpu_render_state = cc.wgpu_render_state.clone();
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
            selected_node: None,
            selected_connection: None,
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
            wgpu_render_state,
        };
        app.spawn_default_nodes();
        app
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
        for path in dropped {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let pos = ctx.pointer_latest_pos().unwrap_or(egui::pos2(200.0, 200.0));
            // pos is in zoomed space; convert to canvas
            let off_e = self.canvas_offset / self.canvas_zoom;
            let canvas_x = pos.x - off_e.x;
            let canvas_y = pos.y - off_e.y;
            self.graph.add_node(NodeType::File { path: path.display().to_string(), content }, [canvas_x, canvas_y]);
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
        let mut http_actions: Vec<HttpAction> = Vec::new();
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
                        &mut pending_disconnects);

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

                // Selection on click
                if r.response.clicked() || r.response.drag_started() {
                    self.selected_node = Some(node_id);
                    self.selected_connection = None;
                }

                // Right-click context menu (check raw secondary click within node rect)
                let secondary_in_rect = ctx.input(|i| {
                    i.pointer.button_clicked(egui::PointerButton::Secondary)
                }) && ctx.pointer_latest_pos().map(|p| r.response.rect.contains(p)).unwrap_or(false);
                if r.response.secondary_clicked() || secondary_in_rect {
                    self.selected_node = Some(node_id);
                    self.context_menu_node = Some(node_id);
                    self.show_context_menu = true;
                    self.context_menu_pos = ctx.pointer_latest_pos().unwrap_or(r.response.rect.center());
                }

                // Draw selection highlight
                if self.selected_node == Some(node_id) {
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

                // Option+drag to duplicate
                if r.response.drag_started() && ctx.input(|i| i.modifiers.alt) {
                    self.opt_drag_source = Some(node_id);
                }
            }
            if open { self.graph.nodes.insert(node_id, node); } else { nodes_to_delete.push(node_id); }
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
        for id in nodes_to_delete { self.midi.cleanup_node(id); self.serial.cleanup_node(id); self.osc.cleanup_node(id); self.graph.remove_node(id); }
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
                    self.selected_node = None;
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

                // Place new nodes to the right of the menu
                // canvas_pos = (screen_pos - offset) / zoom
                let spawn_x = (pos.x - self.canvas_offset.x + 220.0) / self.canvas_zoom;
                let mut spawn_y_base = (pos.y - self.canvas_offset.y) / self.canvas_zoom;

                for entry in &catalog {
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
        // Click on empty canvas deselects
        if ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary)) {
            if let Some(pos) = ctx.pointer_latest_pos() {
                if !self.node_rects.values().any(|r| r.contains(pos)) {
                    self.selected_node = None;
                    self.selected_connection = None;
                }
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

        // Cmd+C = copy node (only when no text field is focused)
        if cmd && !text_focused && ctx.input(|i| i.key_pressed(egui::Key::C)) {
            if let Some(id) = self.selected_node {
                if let Some(node) = self.graph.nodes.get(&id) {
                    self.clipboard = Some(node.node_type.clone());
                }
            }
        }
        // Cmd+V = paste node (only when no text field is focused)
        if cmd && !text_focused && ctx.input(|i| i.key_pressed(egui::Key::V)) {
            if let Some(nt) = &self.clipboard {
                // Place near the selected node if one exists, otherwise at pointer
                let (cx, cy) = if let Some(id) = self.selected_node {
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
                let new_id = self.graph.add_node(nt.clone(), [cx, cy]);
                self.selected_node = Some(new_id);
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
                    self.graph.connections.remove(conn_idx);
                }
            } else if let Some(id) = self.selected_node.take() {
                self.midi.cleanup_node(id);
                self.serial.cleanup_node(id);
                self.osc.cleanup_node(id);
                self.graph.remove_node(id);
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
                    let new_id = self.graph.add_node(nt, [pos[0] + 30.0, pos[1] + 30.0]);
                    self.opt_drag_created = Some(new_id);
                    self.selected_node = Some(new_id);
                }
            }
            if ctx.input(|i| i.pointer.any_released()) {
                self.opt_drag_source = None;
                self.opt_drag_created = None;
            }
        }
    }

    fn duplicate_selected(&mut self) {
        if let Some(id) = self.selected_node {
            if let Some(node) = self.graph.nodes.get(&id) {
                let nt = node.node_type.clone();
                let pos = node.pos;
                let new_id = self.graph.add_node(nt, [pos[0] + 30.0, pos[1] + 30.0]);
                self.selected_node = Some(new_id);
            }
        }
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
                            if let Some(nt) = &self.clipboard {
                                let off_e = self.canvas_offset / self.canvas_zoom;
                                let new_id = self.graph.add_node(nt.clone(), [pos.x - off_e.x + 20.0, pos.y - off_e.y + 20.0]);
                                self.selected_node = Some(new_id);
                            }
                            keep_open = false;
                        }
                    }
                    if ui.button("Duplicate").clicked() {
                        if let Some(id) = self.context_menu_node {
                            if let Some(node) = self.graph.nodes.get(&id) {
                                let nt = node.node_type.clone();
                                let p = node.pos;
                                let new_id = self.graph.add_node(nt, [p[0] + 30.0, p[1] + 30.0]);
                                self.selected_node = Some(new_id);
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
                    if ui.button(egui::RichText::new("Delete").color(egui::Color32::from_rgb(255, 100, 100))).clicked() {
                        if let Some(id) = self.context_menu_node {
                            self.midi.cleanup_node(id);
                            self.serial.cleanup_node(id);
                            self.osc.cleanup_node(id);
                            self.graph.remove_node(id);
                            if self.selected_node == Some(id) { self.selected_node = None; }
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

        self.frame_count += 1;
        if self.frame_count % 60 == 0 {
            self.midi.refresh_ports();
            self.serial.refresh_ports();
        }

        let node_count = self.graph.nodes.len();
        let conn_count = self.graph.connections.len();
        self.monitor.tick(node_count, conn_count);

        let mut values = self.graph.evaluate();

        for (&id, node) in &self.graph.nodes {
            if matches!(node.node_type, NodeType::Monitor) {
                values.insert((id, 0), PortValue::Float(self.monitor.fps));
                values.insert((id, 1), PortValue::Float(self.monitor.frame_ms));
                values.insert((id, 2), PortValue::Float(self.monitor.node_count as f32));
            }
        }

        self.canvas(ctx);
        self.render_connections(ctx);
        self.render_nodes_filtered(ctx, &values, false);
        self.render_nodes_filtered(ctx, &values, true);
        self.sync_console_messages();
        self.handle_system_node_actions(ctx);
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
