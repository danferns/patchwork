use super::*;

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
        let resp = egui::Window::new(egui::RichText::new("\u{2795} Add Node").color(accent).strong())
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
                        .hint_text("\u{1f50d} Search nodes...")
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

    /// Spawn default system nodes if graph is empty.
    pub(super) fn spawn_default_nodes(&mut self) {
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
}
