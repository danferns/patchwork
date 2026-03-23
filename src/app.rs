use crate::graph::*;
use crate::nodes;
use eframe::egui;
use std::collections::HashMap;

// ── Constants ───────────────────────────────────────────────────────────────

const PORT_RADIUS: f32 = 5.0;
const PORT_INTERACT: f32 = 14.0;
const CONN_COLOR: egui::Color32 = egui::Color32::from_rgb(180, 180, 180);
const CONN_ACTIVE: egui::Color32 = egui::Color32::from_rgb(80, 170, 255);

// ── App state ───────────────────────────────────────────────────────────────

pub struct PatchworkApp {
    graph: Graph,
    port_positions: HashMap<(NodeId, usize, bool), egui::Pos2>,
    node_rects: HashMap<NodeId, egui::Rect>,
    dragging_from: Option<(NodeId, usize, bool)>,
    show_node_menu: bool,
    node_menu_pos: egui::Pos2,
    project_path: Option<String>,
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
            project_path: None,
        }
    }

    // ── Menu bar ────────────────────────────────────────────────────────────

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Project").clicked() {
                        self.graph = Graph::new();
                        self.project_path = None;
                        ui.close_menu();
                    }
                    if ui.button("Open Project...").clicked() {
                        self.load_project();
                        ui.close_menu();
                    }
                    if ui.button("Save Project...").clicked() {
                        self.save_project();
                        ui.close_menu();
                    }
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

    // ── Canvas background ───────────────────────────────────────────────────

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
            for i in x0..=x1 {
                let x = i as f32 * grid;
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    egui::Stroke::new(0.5, col),
                );
            }
            for i in y0..=y1 {
                let y = i as f32 * grid;
                painter.line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    egui::Stroke::new(0.5, col),
                );
            }

            // Hint when canvas is empty
            if self.graph.nodes.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new("Double-click to add a node")
                            .size(18.0)
                            .color(egui::Color32::from_rgb(100, 100, 100)),
                    );
                });
            }
        });
    }

    // ── Render all nodes ────────────────────────────────────────────────────

    fn render_nodes(&mut self, ctx: &egui::Context, values: &HashMap<(NodeId, usize), PortValue>) {
        let node_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
        let connections = self.graph.connections.clone();
        let mut port_positions: HashMap<(NodeId, usize, bool), egui::Pos2> = HashMap::new();
        let mut node_rects: HashMap<NodeId, egui::Rect> = HashMap::new();
        let mut pending_connections: Vec<(NodeId, usize, NodeId, usize)> = Vec::new();
        let mut nodes_to_delete: Vec<NodeId> = Vec::new();
        let mut dragging_from = self.dragging_from;

        for node_id in node_ids {
            let mut node = match self.graph.nodes.remove(&node_id) {
                Some(n) => n,
                None => continue,
            };

            let input_defs = node.node_type.inputs();
            let output_defs = node.node_type.outputs();
            let [cr, cg, cb] = node.node_type.color_hint();
            let accent = egui::Color32::from_rgb(cr, cg, cb);
            let title = format!("{} #{}", node.node_type.title(), node_id);

            let mut open = true;
            let resp = egui::Window::new(egui::RichText::new(&title).color(accent).strong())
                .id(egui::Id::new(("node", node_id)))
                .default_pos(egui::pos2(node.pos[0], node.pos[1]))
                .default_width(200.0)
                .resizable(true)
                .collapsible(true)
                .open(&mut open)
                .show(ctx, |ui| {
                    // ── Input ports ─────────────────────────────────────
                    for (i, pdef) in input_defs.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(PORT_INTERACT, PORT_INTERACT),
                                egui::Sense::click_and_drag(),
                            );
                            let hovered = response.hovered() || response.dragged();
                            let col = if hovered {
                                egui::Color32::YELLOW
                            } else {
                                egui::Color32::from_rgb(170, 170, 170)
                            };
                            ui.painter().circle_filled(rect.center(), PORT_RADIUS, col);
                            ui.painter().circle_stroke(
                                rect.center(),
                                PORT_RADIUS,
                                egui::Stroke::new(1.0, egui::Color32::WHITE),
                            );
                            port_positions.insert((node_id, i, true), rect.center());

                            let val = Graph::static_input_value(&connections, values, node_id, i);
                            ui.label(format!("{}: {}", pdef.name, val));

                            if response.drag_started() {
                                dragging_from = Some((node_id, i, false));
                            }
                        });
                    }

                    if !input_defs.is_empty() {
                        ui.separator();
                    }

                    // ── Node content (dispatched to nodes/ module) ──────
                    nodes::render_content(ui, &mut node.node_type, node_id, values, &connections);

                    if !output_defs.is_empty() {
                        ui.separator();
                    }

                    // ── Output ports ────────────────────────────────────
                    for (i, pdef) in output_defs.iter().enumerate() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(PORT_INTERACT, PORT_INTERACT),
                                egui::Sense::click_and_drag(),
                            );
                            let hovered = response.hovered() || response.dragged();
                            let col = if hovered {
                                egui::Color32::YELLOW
                            } else {
                                egui::Color32::from_rgb(100, 180, 255)
                            };
                            ui.painter().circle_filled(rect.center(), PORT_RADIUS, col);
                            ui.painter().circle_stroke(
                                rect.center(),
                                PORT_RADIUS,
                                egui::Stroke::new(1.0, egui::Color32::WHITE),
                            );
                            port_positions.insert((node_id, i, false), rect.center());

                            let val = values.get(&(node_id, i)).copied().unwrap_or(PortValue::None);
                            ui.label(format!("{}: {}", pdef.name, val));

                            if response.drag_started() {
                                dragging_from = Some((node_id, i, true));
                            }
                        });
                    }
                });

            if let Some(r) = &resp {
                let rect = r.response.rect;
                node.pos = [rect.left(), rect.top()];
                node_rects.insert(node_id, rect);
            }

            if open {
                self.graph.nodes.insert(node_id, node);
            } else {
                nodes_to_delete.push(node_id);
            }
        }

        // ── Connection drop ─────────────────────────────────────────────
        if let Some((src_node, src_port, is_output)) = dragging_from {
            if ctx.input(|i| i.pointer.any_released()) {
                if let Some(pointer) = ctx.pointer_latest_pos() {
                    for (&(nid, pidx, is_input), &pos) in &port_positions {
                        if pos.distance(pointer) < PORT_INTERACT * 1.5 {
                            if is_output && is_input && nid != src_node {
                                pending_connections.push((src_node, src_port, nid, pidx));
                            } else if !is_output && !is_input && nid != src_node {
                                pending_connections.push((nid, pidx, src_node, src_port));
                            }
                            break;
                        }
                    }
                }
                dragging_from = None;
            }
        }

        // Apply
        self.dragging_from = dragging_from;
        self.port_positions = port_positions;
        self.node_rects = node_rects;
        for id in nodes_to_delete {
            self.graph.remove_node(id);
        }
        for (fn_, fp, tn, tp) in pending_connections {
            self.graph.add_connection(fn_, fp, tn, tp);
        }
    }

    // ── Connection rendering ────────────────────────────────────────────────

    fn render_connections(&self, ctx: &egui::Context) {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("connections"),
        ));

        for conn in &self.graph.connections {
            let from = self.port_positions.get(&(conn.from_node, conn.from_port, false));
            let to = self.port_positions.get(&(conn.to_node, conn.to_port, true));
            if let (Some(&a), Some(&b)) = (from, to) {
                draw_bezier(&painter, a, b, CONN_COLOR, 2.0);
            }
        }

        if let Some((nid, pidx, is_output)) = self.dragging_from {
            let key = (nid, pidx, !is_output);
            if let Some(&from) = self.port_positions.get(&key) {
                if let Some(ptr) = ctx.pointer_latest_pos() {
                    if is_output {
                        draw_bezier(&painter, from, ptr, CONN_ACTIVE, 2.5);
                    } else {
                        draw_bezier(&painter, ptr, from, CONN_ACTIVE, 2.5);
                    }
                }
            }
        }
    }

    // ── Add-node popup ──────────────────────────────────────────────────────

    fn node_menu(&mut self, ctx: &egui::Context) {
        if !self.show_node_menu {
            return;
        }
        let pos = self.node_menu_pos;
        let mut keep_open = true;

        egui::Area::new(egui::Id::new("add_node_popup"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(160.0);
                    ui.label(egui::RichText::new("Add Node").strong());
                    ui.separator();

                    let mut last_cat = "";
                    for entry in nodes::catalog() {
                        if entry.category != last_cat {
                            if !last_cat.is_empty() {
                                ui.separator();
                            }
                            ui.label(
                                egui::RichText::new(entry.category)
                                    .small()
                                    .color(egui::Color32::GRAY),
                            );
                            last_cat = entry.category;
                        }
                        if ui.button(entry.label).clicked() {
                            self.graph.add_node((entry.factory)(), [pos.x, pos.y]);
                            keep_open = false;
                        }
                    }
                });
            });

        if !keep_open {
            self.show_node_menu = false;
        }
    }

    // ── Canvas interaction ──────────────────────────────────────────────────

    fn handle_canvas_interaction(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary)) {
            if let Some(pos) = ctx.pointer_latest_pos() {
                let over_node = self.node_rects.values().any(|r| r.contains(pos));
                if !over_node {
                    self.show_node_menu = true;
                    self.node_menu_pos = pos;
                }
            }
        }
        if self.show_node_menu
            && ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary))
        {
            self.show_node_menu = false;
        }
    }

    fn update_mouse_trackers(&mut self, ctx: &egui::Context) {
        if let Some(pos) = ctx.pointer_latest_pos() {
            for node in self.graph.nodes.values_mut() {
                if let NodeType::MouseTracker { x, y } = &mut node.node_type {
                    *x = pos.x;
                    *y = pos.y;
                }
            }
        }
    }

    // ── Save / Load ─────────────────────────────────────────────────────────

    fn save_project(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Patchwork Project", &["json"])
            .set_file_name("project.json")
            .save_file()
        {
            let json = serde_json::to_string_pretty(&self.graph).unwrap_or_default();
            let _ = std::fs::write(&path, json);
            self.project_path = Some(path.display().to_string());
        }
    }

    fn load_project(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Patchwork Project", &["json"])
            .pick_file()
        {
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

// ── eframe::App ─────────────────────────────────────────────────────────────

impl eframe::App for PatchworkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_mouse_trackers(ctx);
        let values = self.graph.evaluate();

        self.menu_bar(ctx);
        self.canvas(ctx);
        self.render_connections(ctx);
        self.render_nodes(ctx, &values);
        self.node_menu(ctx);
        self.handle_canvas_interaction(ctx);

        ctx.request_repaint();
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn draw_bezier(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, color: egui::Color32, width: f32) {
    let dx = (to.x - from.x).abs().max(50.0) * 0.5;
    let cp1 = egui::pos2(from.x + dx, from.y);
    let cp2 = egui::pos2(to.x - dx, to.y);
    let shape = egui::epaint::CubicBezierShape::from_points_stroke(
        [from, cp1, cp2, to],
        false,
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(width, color),
    );
    painter.add(shape);
}
