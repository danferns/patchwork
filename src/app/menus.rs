use super::*;

/// Check if two port kinds are compatible for the wire+Tab filtered menu.
fn port_kinds_compatible(a: PortKind, b: PortKind) -> bool {
    use PortKind::*;
    if a == b { return true; }
    // Number-like types are interchangeable
    let number_like = |k: PortKind| matches!(k, Number | Normalized | Trigger | Gate | Color);
    if number_like(a) && number_like(b) { return true; }
    // Generic matches number-like and text, but NOT image or audio
    if a == Generic { return number_like(b) || b == Text; }
    if b == Generic { return number_like(a) || a == Text; }
    false
}

impl super::PatchworkApp {
    /// Apply inverse-zoom style so menus appear at native screen size
    pub(super) fn apply_inverse_zoom_style(&self, ctx: &egui::Context) -> std::sync::Arc<egui::Style> {
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

    pub(super) fn node_menu(&mut self, ctx: &egui::Context) {
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
        let menu_w = 180.0 * inv;
        let resp = egui::Window::new(egui::RichText::new("\u{2795} Add Node").color(accent).strong())
            .id(menu_id)
            .fixed_pos(pos)
            .default_width(menu_w)
            .resizable(false)
            .collapsible(false)
            .title_bar(true)
            .scroll([false, true])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_max_width(menu_w);

                // Search box (auto-focused)
                let prev_search = self.node_menu_search.clone();
                let search_re = ui.add(
                    egui::TextEdit::singleline(&mut self.node_menu_search)
                        .hint_text("\u{1f50d} Search...")
                        .desired_width(menu_w - 8.0),
                );
                if search_re.gained_focus() || prev_search.is_empty() {
                    search_re.request_focus();
                }
                if self.node_menu_search != prev_search {
                    self.node_menu_selected = 0;
                }

                // Category filter pills
                // Category pills — some map to multiple catalog categories
                let categories: &[(&str, &[&str])] = &[
                    ("All", &[]),
                    ("Math", &["Math", "Logic"]),
                    ("Signal", &["Signal", "Input"]),
                    ("Visual", &["Image", "Video", "Shader", "ML"]),
                    ("Audio", &["Audio"]),
                    ("I/O", &["IO", "Network", "Serial", "OSC", "MIDI", "Hardware"]),
                    ("Utility", &["Utility", "Output", "Custom"]),
                ];
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    for &(label, _) in categories {
                        let is_active = if label == "All" { self.node_menu_category.is_empty() } else { self.node_menu_category == label };
                        let text = egui::RichText::new(label).small();
                        let btn = if is_active {
                            egui::Button::new(text.strong().color(egui::Color32::WHITE))
                                .fill(egui::Color32::from_rgba_unmultiplied(accent_rgb[0], accent_rgb[1], accent_rgb[2], 180))
                                .corner_radius(10.0)
                        } else {
                            egui::Button::new(text.color(egui::Color32::GRAY))
                                .fill(egui::Color32::TRANSPARENT)
                                .corner_radius(10.0)
                        };
                        if ui.add(btn).clicked() {
                            self.node_menu_category = if label == "All" { String::new() } else { label.to_string() };
                            self.node_menu_selected = 0;
                        }
                    }
                });

                ui.separator();

                let query = self.node_menu_search.to_lowercase();
                let catalog = nodes::catalog();
                let wire_ctx = self.wire_menu_context;
                let cat_filter = &self.node_menu_category;

                // Show filter hint if opened via Tab
                if wire_ctx.is_some() {
                    ui.label(egui::RichText::new("⚡ Compatible nodes").small().color(accent));
                    ui.separator();
                }

                // Collect visible entries (filtered by search + category + wire compat)
                let mut visible: Vec<usize> = Vec::new();
                for (i, entry) in catalog.iter().enumerate() {
                    if entry.category == "System" { continue; }
                    // Category pill filter — match against grouped categories
                    if !cat_filter.is_empty() {
                        let matched = categories.iter().any(|&(label, cats)| {
                            label == cat_filter.as_str() && cats.contains(&entry.category)
                        });
                        if !matched { continue; }
                    }
                    // Text search
                    if !query.is_empty()
                        && !entry.label.to_lowercase().contains(&query)
                        && !entry.category.to_lowercase().contains(&query)
                    { continue; }
                    // Wire compatibility filter
                    if let Some((_src_nid, _src_port, src_is_output, src_kind)) = wire_ctx {
                        let candidate = (entry.factory)();
                        let target_ports = if src_is_output { candidate.inputs() } else { candidate.outputs() };
                        if !target_ports.iter().any(|p| port_kinds_compatible(src_kind, p.kind)) { continue; }
                    }
                    visible.push(i);
                }

                // Keyboard navigation (arrow keys + enter)
                let up = ui.ctx().input(|i| i.key_pressed(egui::Key::ArrowUp));
                let down = ui.ctx().input(|i| i.key_pressed(egui::Key::ArrowDown));
                let enter = ui.ctx().input(|i| i.key_pressed(egui::Key::Enter));
                if up && self.node_menu_selected > 0 {
                    self.node_menu_selected -= 1;
                }
                if down && self.node_menu_selected + 1 < visible.len() {
                    self.node_menu_selected += 1;
                }
                // Clamp selection
                if !visible.is_empty() {
                    self.node_menu_selected = self.node_menu_selected.min(visible.len() - 1);
                }

                let off_e = self.canvas_offset / self.canvas_zoom;
                let spawn_x = self.node_menu_pos.x - off_e.x;
                let spawn_y_base = self.node_menu_pos.y - off_e.y;

                // Spawn helper (used by both click and Enter)
                let mut spawn_idx: Option<usize> = None;
                if enter && !visible.is_empty() {
                    spawn_idx = Some(visible[self.node_menu_selected]);
                }

                let mut last_cat = "";
                let mut visible_pos = 0usize;
                for &cat_idx in &visible {
                    let entry = &catalog[cat_idx];
                    let is_selected = visible_pos == self.node_menu_selected;

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

                    let text = egui::RichText::new(entry.label).size(13.0);
                    let btn = ui.add_sized(
                        [ui.available_width(), 24.0],
                        egui::Button::new(if is_selected { text.strong().color(accent) } else { text })
                            .fill(if is_selected {
                                egui::Color32::from_rgba_unmultiplied(accent_rgb[0], accent_rgb[1], accent_rgb[2], 30)
                            } else {
                                egui::Color32::TRANSPARENT
                            })
                            .frame(true),
                    );

                    // Scroll selected item into view
                    if is_selected && (up || down) {
                        btn.scroll_to_me(Some(egui::Align::Center));
                    }

                    if btn.clicked() {
                        spawn_idx = Some(cat_idx);
                    }

                    btn.on_hover_text(format!("Add {} node", entry.label));
                    visible_pos += 1;
                }

                // Spawn the selected node (from click or Enter)
                if let Some(cat_idx) = spawn_idx {
                    let entry = &catalog[cat_idx];
                    self.push_undo();
                    let nt = (entry.factory)();
                    let target_port_idx = if let Some((_, _, src_is_output, src_kind)) = wire_ctx {
                        let target_ports = if src_is_output { nt.inputs() } else { nt.outputs() };
                        target_ports.iter().position(|p| port_kinds_compatible(src_kind, p.kind))
                    } else { None };

                    let new_id = self.graph.add_node(nt, [spawn_x, spawn_y_base]);
                    if let Some((src_nid, src_port, src_is_output, _)) = wire_ctx {
                        if let Some(tp) = target_port_idx {
                            if src_is_output {
                                self.graph.add_connection(src_nid, src_port, new_id, tp);
                            } else {
                                self.graph.add_connection(new_id, tp, src_nid, src_port);
                            }
                        }
                        self.dragging_from = None;
                        self.wire_menu_context = None;
                    }
                    let _ = spawn_y_base;
                    keep_open = false;
                }

                if visible.is_empty() {
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
            self.node_menu_category.clear();
            self.node_menu_selected = 0;
            self.wire_menu_context = None;
        }
        ctx.set_style(orig_style);
    }

    /// Check for file/zoom actions from system nodes (communicated via egui temp data).
    pub(super) fn handle_system_node_actions(&mut self, ctx: &egui::Context) {
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
            self.session_accent = crate::nodes::theme::random_accent();
            self.spawn_default_nodes();
        }
        if load_project { self.load_project(); }
        if save_project { self.save_project(); }

        // Zoom control
        let zoom_action: Option<f32> = ctx.data_mut(|d| d.get_temp(egui::Id::new("zoom_action")));
        if let Some(new_zoom) = zoom_action {
            let z = new_zoom.clamp(0.1, 5.0);
            self.canvas_zoom = z;
            self.target_zoom = z;
            ctx.data_mut(|d| d.remove::<f32>(egui::Id::new("zoom_action")));
        }

        // Provide current zoom to ZoomControl nodes
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("current_zoom"), self.canvas_zoom));
    }

    /// Spawn default system nodes if they don't already exist.
    pub(super) fn spawn_default_nodes(&mut self) {
        let has = |nodes: &std::collections::HashMap<NodeId, crate::graph::Node>, check: &dyn Fn(&NodeType) -> bool| -> bool {
            nodes.values().any(|n| check(&n.node_type))
        };
        if !has(&self.graph.nodes, &|t| t.title() == "File" && matches!(t, NodeType::Dynamic { .. } | NodeType::FileMenu)) {
            let id = self.graph.add_node(
                NodeType::Dynamic { inner: crate::graph::DynNode { node: Box::new(crate::nodes::file_menu_node::FileMenuNode) } },
                [10.0, 10.0]);
            self.pinned_nodes.insert(id);
        }
        if !has(&self.graph.nodes, &|t| t.title() == "Zoom") {
            let id = self.graph.add_node(
                NodeType::Dynamic { inner: crate::graph::DynNode { node: Box::new(crate::nodes::zoom_control_node::ZoomControlNode::default()) } },
                [1100.0, 10.0]);
            self.pinned_nodes.insert(id);
        }
        if !has(&self.graph.nodes, &|t| matches!(t, NodeType::Palette { .. })) {
            let id = self.graph.add_node(NodeType::Palette { search: String::new() }, [10.0, 200.0]);
            self.pinned_nodes.insert(id);
        }
        if !has(&self.graph.nodes, &|t| matches!(t, NodeType::Dynamic { .. }) && t.title() == "Monitor") {
            let id = self.graph.add_node(
                NodeType::Dynamic { inner: crate::graph::DynNode { node: Box::new(crate::nodes::monitor_node::MonitorNode::default()) } },
                [1100.0, 500.0],
            );
            self.pinned_nodes.insert(id);
        }
        if !has(&self.graph.nodes, &|t| matches!(t, NodeType::AudioDevice { .. })) {
            let id = self.graph.add_node(NodeType::AudioDevice {
                selected_output: String::new(), selected_input: String::new(),
                master_volume: 0.8, enabled: false,
            }, [1100.0, 350.0]);
            self.pinned_nodes.insert(id);
        }
        // Theme node is optional — not added by default.
        // Users can add it from the palette when needed.
    }
}
