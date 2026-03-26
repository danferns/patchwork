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

    pub(super) fn render_connections(&mut self, ctx: &egui::Context) {
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
}
