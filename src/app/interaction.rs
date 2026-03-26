use super::*;

impl super::PatchworkApp {
    /// Pan/zoom handling — with set_zoom_factor, pointer is in logical coords.
    pub(super) fn handle_pan_zoom(&mut self, ctx: &egui::Context) {
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
    pub(super) fn handle_canvas_interaction(&mut self, ctx: &egui::Context) {
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

    pub(super) fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
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

    pub(super) fn handle_opt_drag(&mut self, ctx: &egui::Context) {
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

    pub(super) fn duplicate_selected(&mut self) {
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

    pub(super) fn context_menu(&mut self, ctx: &egui::Context) {
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

    pub(super) fn fit_all_nodes(&mut self, ctx: &egui::Context) {
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
}
