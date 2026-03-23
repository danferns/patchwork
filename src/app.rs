use crate::graph::*;
use crate::midi::{MidiAction, MidiManager};
use crate::serial::{SerialAction, SerialManager};
use crate::osc::{OscAction, OscManager};
use crate::nodes;
use eframe::egui;
use std::collections::HashMap;

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
}

impl PatchworkApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
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
        }
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
            if let NodeType::Theme { dark_mode, accent, font_size } = &node.node_type {
                nodes::theme::apply(ctx, *dark_mode, *accent, *font_size);
                return;
            }
        }
    }

    fn handle_file_drop(&mut self, ctx: &egui::Context) {
        let dropped: Vec<_> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        for path in dropped {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let pos = ctx.pointer_latest_pos().unwrap_or(egui::pos2(200.0, 200.0));
            self.graph.add_node(NodeType::File { path: path.display().to_string(), content }, [pos.x, pos.y]);
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

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Project").clicked() { self.graph = Graph::new(); self.project_path = None; ui.close_menu(); }
                    if ui.button("Open Project...").clicked() { self.load_project(); ui.close_menu(); }
                    if ui.button("Save Project...").clicked() { self.save_project(); ui.close_menu(); }
                });
                ui.separator();
                let count = self.graph.nodes.len();
                ui.label(egui::RichText::new(format!("{count} nodes")).small().color(egui::Color32::GRAY));
                if let Some(path) = &self.project_path {
                    ui.separator();
                    ui.label(egui::RichText::new(path.as_str()).small().color(egui::Color32::GRAY));
                }
            });
        });
    }

    fn canvas(&self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let painter = ui.painter();
            let rect = ui.max_rect();
            let grid = 25.0;
            let col = egui::Color32::from_rgba_premultiplied(12, 12, 12, 35);
            let x0 = (rect.left() / grid).floor() as i32;
            let x1 = (rect.right() / grid).ceil() as i32;
            let y0 = (rect.top() / grid).floor() as i32;
            let y1 = (rect.bottom() / grid).ceil() as i32;
            for i in x0..=x1 { let x = i as f32 * grid; painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], egui::Stroke::new(0.5, col)); }
            for i in y0..=y1 { let y = i as f32 * grid; painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], egui::Stroke::new(0.5, col)); }
            if self.graph.nodes.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("Double-click to add a node  \u{2022}  Drag & drop a file").size(16.0).color(egui::Color32::from_rgb(100, 100, 100)));
                });
            }
        });
    }

    fn render_nodes(&mut self, ctx: &egui::Context, values: &HashMap<(NodeId, usize), PortValue>) {
        let node_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
        let connections = self.graph.connections.clone();
        let midi_out_ports = self.midi.cached_output_ports.clone();
        let midi_in_ports = self.midi.cached_input_ports.clone();
        let serial_ports = self.serial.cached_ports.clone();
        // Snapshot monitor state before the mutable borrow loop
        let monitor_state = std::mem::take(&mut self.monitor);
        let mut port_positions: HashMap<(NodeId, usize, bool), egui::Pos2> = HashMap::new();
        let mut node_rects: HashMap<NodeId, egui::Rect> = HashMap::new();
        let mut pending_connections: Vec<(NodeId, usize, NodeId, usize)> = Vec::new();
        let mut nodes_to_delete: Vec<NodeId> = Vec::new();
        let mut osc_actions: Vec<OscAction> = Vec::new();
        let mut dragging_from = self.dragging_from;
        let mut midi_actions: Vec<MidiAction> = Vec::new();
        let mut serial_actions: Vec<SerialAction> = Vec::new();

        for node_id in node_ids {
            let mut node = match self.graph.nodes.remove(&node_id) { Some(n) => n, None => continue };
            let input_defs = node.node_type.inputs();
            let output_defs = node.node_type.outputs();
            let [cr, cg, cb] = node.node_type.color_hint();
            let accent = egui::Color32::from_rgb(cr, cg, cb);
            let title = format!("{} #{}", node.node_type.title(), node_id);
            let midi_conn_out = self.midi.is_output_connected(node_id);
            let midi_conn_in = self.midi.is_input_connected(node_id);
            let serial_conn = self.serial.is_connected(node_id);
            let osc_listening = self.osc.is_listening(node_id);

            let mut open = true;
            let resp = egui::Window::new(egui::RichText::new(&title).color(accent).strong())
                .id(egui::Id::new(("node", node_id)))
                .default_pos(egui::pos2(node.pos[0], node.pos[1]))
                .default_width(180.0)
                .resizable(true)
                .constrain(false)
                .collapsible(true)
                .scroll([false, true])
                .open(&mut open)
                .show(ctx, |ui| {
                    for (i, pdef) in input_defs.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let (rect, response) = ui.allocate_exact_size(egui::vec2(PORT_INTERACT, PORT_INTERACT), egui::Sense::click_and_drag());
                            let col = if response.hovered() || response.dragged() { egui::Color32::YELLOW } else { egui::Color32::from_rgb(170, 170, 170) };
                            ui.painter().circle_filled(rect.center(), PORT_RADIUS, col);
                            ui.painter().circle_stroke(rect.center(), PORT_RADIUS, egui::Stroke::new(1.0, egui::Color32::WHITE));
                            port_positions.insert((node_id, i, true), rect.center());
                            let val = Graph::static_input_value(&connections, values, node_id, i);
                            ui.label(format!("{}: {}", pdef.name, val));
                            if response.drag_started() { dragging_from = Some((node_id, i, false)); }
                        });
                    }
                    if !input_defs.is_empty() { ui.separator(); }

                    nodes::render_content(ui, &mut node.node_type, node_id, values, &connections,
                        &midi_out_ports, &midi_in_ports, midi_conn_out, midi_conn_in, &mut midi_actions,
                        &serial_ports, serial_conn, &mut serial_actions, &monitor_state,
                        osc_listening, &mut osc_actions);

                    if !output_defs.is_empty() { ui.separator(); }
                    for (i, pdef) in output_defs.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let val = values.get(&(node_id, i)).cloned().unwrap_or(PortValue::None);
                            ui.label(format!("{}: {}", pdef.name, val));
                            let (rect, response) = ui.allocate_exact_size(egui::vec2(PORT_INTERACT, PORT_INTERACT), egui::Sense::click_and_drag());
                            let col = if response.hovered() || response.dragged() { egui::Color32::YELLOW } else { egui::Color32::from_rgb(100, 180, 255) };
                            ui.painter().circle_filled(rect.center(), PORT_RADIUS, col);
                            ui.painter().circle_stroke(rect.center(), PORT_RADIUS, egui::Stroke::new(1.0, egui::Color32::WHITE));
                            port_positions.insert((node_id, i, false), rect.center());
                            if response.drag_started() { dragging_from = Some((node_id, i, true)); }
                        });
                    }
                });

            if let Some(r) = &resp { node.pos = [r.response.rect.left(), r.response.rect.top()]; node_rects.insert(node_id, r.response.rect); }
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
        for (fn_, fp, tn, tp) in pending_connections { self.graph.add_connection(fn_, fp, tn, tp); }
        self.midi.process(midi_actions);
        self.serial.process(serial_actions);
        self.osc.process(osc_actions);
    }

    fn render_connections(&self, ctx: &egui::Context) {
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("connections")));
        for conn in &self.graph.connections {
            let from = self.port_positions.get(&(conn.from_node, conn.from_port, false));
            let to = self.port_positions.get(&(conn.to_node, conn.to_port, true));
            if let (Some(&a), Some(&b)) = (from, to) { draw_bezier(&painter, a, b, CONN_COLOR, 2.0); }
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

    fn node_menu(&mut self, ctx: &egui::Context) {
        if !self.show_node_menu { return; }
        let pos = self.node_menu_pos;
        let mut keep_open = true;

        egui::Area::new(egui::Id::new("add_node_popup"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(180.0);
                    ui.set_max_height(400.0);

                    // Search box (auto-focused)
                    let search_re = ui.add(
                        egui::TextEdit::singleline(&mut self.node_menu_search)
                            .hint_text("Search nodes...")
                            .desired_width(160.0),
                    );
                    if search_re.gained_focus() || self.node_menu_search.is_empty() {
                        search_re.request_focus();
                    }

                    ui.separator();

                    let query = self.node_menu_search.to_lowercase();
                    let catalog = nodes::catalog();

                    egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                        let mut last_cat = "";
                        let mut any_shown = false;

                        for entry in &catalog {
                            // Filter by search
                            if !query.is_empty()
                                && !entry.label.to_lowercase().contains(&query)
                                && !entry.category.to_lowercase().contains(&query)
                            {
                                continue;
                            }

                            // Category header
                            if entry.category != last_cat {
                                if !last_cat.is_empty() { ui.separator(); }
                                ui.label(egui::RichText::new(entry.category).small().color(egui::Color32::GRAY));
                                last_cat = entry.category;
                            }

                            if ui.button(entry.label).clicked() {
                                self.graph.add_node((entry.factory)(), [pos.x, pos.y]);
                                keep_open = false;
                            }
                            any_shown = true;
                        }

                        if !any_shown {
                            ui.label(egui::RichText::new("No matches").color(egui::Color32::GRAY));
                        }
                    });
                });
            });

        if !keep_open {
            self.show_node_menu = false;
            self.node_menu_search.clear();
        }
    }

    fn handle_canvas_interaction(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary)) {
            if let Some(pos) = ctx.pointer_latest_pos() {
                if !self.node_rects.values().any(|r| r.contains(pos)) {
                    self.show_node_menu = true;
                    self.node_menu_pos = pos;
                    self.node_menu_search.clear();
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
    }

    fn update_mouse_trackers(&mut self, ctx: &egui::Context) {
        if let Some(pos) = ctx.pointer_latest_pos() {
            for node in self.graph.nodes.values_mut() {
                if let NodeType::MouseTracker { x, y } = &mut node.node_type { *x = pos.x; *y = pos.y; }
            }
        }
    }

    fn save_project(&mut self) {
        if let Some(path) = rfd::FileDialog::new().add_filter("Patchwork", &["json"]).set_file_name("project.json").save_file() {
            let json = serde_json::to_string_pretty(&self.graph).unwrap_or_default();
            let _ = std::fs::write(&path, json);
            self.project_path = Some(path.display().to_string());
        }
    }

    fn load_project(&mut self) {
        if let Some(path) = rfd::FileDialog::new().add_filter("Patchwork", &["json"]).pick_file() {
            if let Ok(json) = std::fs::read_to_string(&path) {
                if let Ok(graph) = serde_json::from_str::<Graph>(&json) {
                    self.graph = graph;
                    self.project_path = Some(path.display().to_string());
                    self.port_positions.clear();
                    self.node_rects.clear();
                }
            }
        }
    }
}

impl eframe::App for PatchworkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_theme(ctx);
        self.handle_file_drop(ctx);
        self.update_mouse_trackers(ctx);
        self.poll_midi_inputs();
        self.poll_serial_inputs();
        self.poll_osc_inputs();

        self.frame_count += 1;
        if self.frame_count % 60 == 0 {
            self.midi.refresh_ports();
            self.serial.refresh_ports();
        }

        let node_count = self.graph.nodes.len();
        let conn_count = self.graph.connections.len();
        self.monitor.tick(node_count, conn_count);

        let mut values = self.graph.evaluate();

        // Inject monitor outputs so they can be connected to other nodes
        for (&id, node) in &self.graph.nodes {
            if matches!(node.node_type, NodeType::Monitor) {
                values.insert((id, 0), PortValue::Float(self.monitor.fps));
                values.insert((id, 1), PortValue::Float(self.monitor.frame_ms));
                values.insert((id, 2), PortValue::Float(self.monitor.node_count as f32));
            }
        }

        self.menu_bar(ctx);
        self.canvas(ctx);
        self.render_connections(ctx);
        self.render_nodes(ctx, &values);
        self.sync_console_messages();
        self.node_menu(ctx);
        self.handle_canvas_interaction(ctx);
        ctx.request_repaint();
    }
}

fn draw_bezier(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, color: egui::Color32, width: f32) {
    let dx = (to.x - from.x).abs().max(50.0) * 0.5;
    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
        [from, egui::pos2(from.x + dx, from.y), egui::pos2(to.x - dx, to.y), to],
        false, egui::Color32::TRANSPARENT, egui::Stroke::new(width, color),
    ));
}
