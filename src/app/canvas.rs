use super::*;

pub(super) fn point_near_bezier(p: egui::Pos2, from: egui::Pos2, to: egui::Pos2, threshold: f32) -> bool {
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

pub(super) fn draw_bezier(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, color: egui::Color32, width: f32) {
    let dx = (to.x - from.x).abs().max(50.0) * 0.5;
    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
        [from, egui::pos2(from.x + dx, from.y), egui::pos2(to.x - dx, to.y), to],
        false, egui::Color32::TRANSPARENT, egui::Stroke::new(width, color),
    ));
}

impl super::PatchworkApp {
    pub(super) fn canvas(&self, ctx: &egui::Context) {
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

    pub(super) fn render_connections(&mut self, ctx: &egui::Context, values: &HashMap<(NodeId, usize), PortValue>) {
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("connections")));
        let pointer = ctx.pointer_latest_pos();
        let clicked = ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));
        let mut hovered_conn: Option<usize> = None;

        // ── Wire color palette (by data type) ──────────────────────────
        let color_float = egui::Color32::from_rgb(80, 100, 230);    // blue
        let color_text  = egui::Color32::from_rgb(60, 220, 80);     // green
        let color_image = egui::Color32::from_rgb(200, 30, 255);    // purple
        let color_audio = egui::Color32::from_rgb(255, 220, 40);    // yellow-orange
        let color_none  = egui::Color32::from_rgb(160, 160, 160);   // gray

        // Wire thickness from Theme (default 6px, minimum 1px)
        let wire_thickness: f32 = self.graph.nodes.values()
            .find_map(|n| if let NodeType::Theme { wire_thickness, .. } = &n.node_type { Some(*wire_thickness) } else { None })
            .unwrap_or(6.0)
            .max(1.0);

        // Connection endpoint dot size
        let endpoint_radius: f32 = 6.0;

        // Pre-compute wire colors for all connections + drag
        let compute_wire_color = |nodes: &HashMap<NodeId, Node>, from_node: NodeId, from_port: usize| -> egui::Color32 {
            let is_audio = nodes.get(&from_node).map(|n| {
                matches!(n.node_type, NodeType::Synth { .. } | NodeType::AudioFx { .. } | NodeType::AudioPlayer { .. })
            }).unwrap_or(false);
            if is_audio { return color_audio; }
            match values.get(&(from_node, from_port)) {
                Some(PortValue::Float(_)) => color_float,
                Some(PortValue::Text(_)) => color_text,
                Some(PortValue::Image(_)) => color_image,
                _ => color_none,
            }
        };
        // Pre-compute all wire colors (so we don't borrow self.graph during rendering)
        let wire_colors: Vec<egui::Color32> = self.graph.connections.iter()
            .map(|c| compute_wire_color(&self.graph.nodes, c.from_node, c.from_port))
            .collect();
        let drag_color = if let Some((nid, pidx, is_output)) = self.dragging_from {
            if is_output {
                compute_wire_color(&self.graph.nodes, nid, pidx)
            } else {
                match values.get(&(nid, pidx)) {
                    Some(PortValue::Float(_)) => color_float,
                    Some(PortValue::Text(_)) => color_text,
                    Some(PortValue::Image(_)) => color_image,
                    _ => color_none,
                }
            }
        } else {
            color_none
        };

        // ── Draw established connections ──────────────────────────────────
        for (idx, conn) in self.graph.connections.iter().enumerate() {
            let from = self.port_positions.get(&(conn.from_node, conn.from_port, false));
            let to = self.port_positions.get(&(conn.to_node, conn.to_port, true));
            if let (Some(&a), Some(&b)) = (from, to) {
                let is_selected = self.selected_connection == Some(idx);
                let is_hovered = pointer.map(|p| point_near_bezier(p, a, b, 10.0)).unwrap_or(false);
                if is_hovered { hovered_conn = Some(idx); }

                let base_color = wire_colors[idx];

                let (color, width) = if is_selected {
                    (egui::Color32::from_rgb(255, 200, 60), wire_thickness + 2.0)
                } else if is_hovered {
                    let arr = base_color.to_array();
                    (egui::Color32::from_rgb(
                        (arr[0] as u16 + 50).min(255) as u8,
                        (arr[1] as u16 + 50).min(255) as u8,
                        (arr[2] as u16 + 50).min(255) as u8,
                    ), wire_thickness + 1.0)
                } else {
                    (base_color, wire_thickness)
                };

                // Glow
                let rgba = color.to_array();
                let glow = egui::Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], 20);
                draw_bezier(&painter, a, b, glow, width + 4.0);
                // Main wire
                draw_bezier(&painter, a, b, color, width);

                // Endpoint dots at both ends
                painter.circle_filled(a, endpoint_radius, color);
                painter.circle_filled(b, endpoint_radius, color);

                // Wire label (centered on bezier midpoint)
                if !conn.label.is_empty() {
                    let mid_x = (a.x + b.x) / 2.0;
                    let mid_y = (a.y + b.y) / 2.0 - 12.0; // above the wire
                    painter.text(
                        egui::pos2(mid_x, mid_y),
                        egui::Align2::CENTER_BOTTOM,
                        &conn.label,
                        egui::FontId::proportional(11.0),
                        color,
                    );
                }
            }
        }

        // ── Click to select/open wire menu ────────────────────────────────
        if clicked {
            if let Some(idx) = hovered_conn {
                let near_port = pointer.map(|p| {
                    self.port_positions.values().any(|&pp| pp.distance(p) < PORT_INTERACT)
                }).unwrap_or(false);
                if !near_port {
                    self.selected_connection = Some(idx);
                    self.selected_nodes.clear();
                    // Open wire context menu
                    self.wire_menu_conn = Some(idx);
                    self.wire_menu_pos = pointer.unwrap_or(egui::Pos2::ZERO);
                }
            }
        }

        // ── Wire context menu (label + delete) ───────────────────────────
        if let Some(conn_idx) = self.wire_menu_conn {
            if conn_idx < self.graph.connections.len() {
                let orig_style = self.apply_inverse_zoom_style(ctx);
                let mut keep_open = true;
                let pos = self.wire_menu_pos;

                egui::Area::new(egui::Id::new("wire_context_menu"))
                    .fixed_pos(pos)
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.set_min_width(60.0 / self.canvas_zoom);

                            // Label text field
                            ui.label(egui::RichText::new("Wire Label").small().strong());
                            let label = &mut self.graph.connections[conn_idx].label;
                            let r = ui.text_edit_singleline(label);
                            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                keep_open = false;
                            }

                            ui.separator();

                            // Delete button
                            if ui.add(egui::Button::new(
                                egui::RichText::new(format!("{} Delete wire", crate::icons::TRASH))
                                    .color(egui::Color32::from_rgb(255, 100, 100))
                            ).frame(false)).clicked() {
                                self.push_undo();
                                self.graph.connections.remove(conn_idx);
                                self.selected_connection = None;
                                keep_open = false;
                            }
                        });
                    });

                // Close on click outside or Escape
                let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                let click_outside = ctx.input(|i| {
                    i.pointer.button_clicked(egui::PointerButton::Primary)
                        || i.pointer.button_clicked(egui::PointerButton::Secondary)
                }) && ctx.pointer_latest_pos()
                    .map(|p| {
                        let menu_rect = egui::Rect::from_min_size(pos, egui::vec2(150.0, 80.0));
                        !menu_rect.contains(p) && !hovered_conn.map(|h| h == conn_idx).unwrap_or(false)
                    })
                    .unwrap_or(false);

                if esc || click_outside || !keep_open {
                    self.wire_menu_conn = None;
                }
                ctx.set_style(orig_style);
            } else {
                self.wire_menu_conn = None;
            }
        }

        // ── Active drag wire (previews data type color) ───────────────────
        if let Some((nid, pidx, is_output)) = self.dragging_from {
            if let Some(&from) = self.port_positions.get(&(nid, pidx, !is_output)) {
                if let Some(ptr) = ctx.pointer_latest_pos() {
                    let rgba = drag_color.to_array();
                    let glow = egui::Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], 30);
                    if is_output {
                        draw_bezier(&painter, from, ptr, glow, wire_thickness + 4.0);
                        draw_bezier(&painter, from, ptr, drag_color, wire_thickness);
                        painter.circle_filled(from, endpoint_radius, drag_color);
                        painter.circle_filled(ptr, endpoint_radius - 2.0, drag_color);
                    } else {
                        draw_bezier(&painter, ptr, from, glow, wire_thickness + 4.0);
                        draw_bezier(&painter, ptr, from, drag_color, wire_thickness);
                        painter.circle_filled(from, endpoint_radius, drag_color);
                        painter.circle_filled(ptr, endpoint_radius - 2.0, drag_color);
                    }
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
}
