use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    r: &mut u8,
    g: &mut u8,
    b: &mut u8,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
) {
    // Override from connected inputs first
    for i in 0..3 {
        let input = Graph::static_input_value(connections, values, node_id, i);
        if let PortValue::Float(f) = input {
            match i {
                0 => *r = (f as i32).clamp(0, 255) as u8,
                1 => *g = (f as i32).clamp(0, 255) as u8,
                2 => *b = (f as i32).clamp(0, 255) as u8,
                _ => {}
            }
        }
    }

    // Clickable color swatch + editable hex code
    let mut color = egui::Color32::from_rgb(*r, *g, *b);
    let hex_id = egui::Id::new(("color_hex", node_id));
    let mut hex_str = ui.ctx().data_mut(|d| {
        d.get_temp::<String>(hex_id).unwrap_or_else(|| format!("{:02X}{:02X}{:02X}", *r, *g, *b))
    });
    ui.horizontal(|ui| {
        ui.color_edit_button_srgba(&mut color);
        ui.label("#");
        let resp = ui.add(
            egui::TextEdit::singleline(&mut hex_str)
                .desired_width(52.0)
                .font(egui::TextStyle::Monospace)
                .char_limit(6),
        );
        if resp.changed() || resp.lost_focus() {
            // Parse hex input
            let clean: String = hex_str.chars().filter(|c| c.is_ascii_hexdigit()).take(6).collect();
            if clean.len() == 6 {
                if let Ok(val) = u32::from_str_radix(&clean, 16) {
                    color = egui::Color32::from_rgb(
                        ((val >> 16) & 0xFF) as u8,
                        ((val >> 8) & 0xFF) as u8,
                        (val & 0xFF) as u8,
                    );
                }
            }
        }
    });
    *r = color.r();
    *g = color.g();
    *b = color.b();
    // Sync hex display with current color (update when changed via picker or DragValues)
    hex_str = format!("{:02X}{:02X}{:02X}", *r, *g, *b);
    ui.ctx().data_mut(|d| d.insert_temp(hex_id, hex_str));

    ui.separator();

    // Channel rows: ● input | label + DragValue | ● output
    let channels = ["R", "G", "B"];

    for i in 0..3 {
        let label = channels[i];
        let is_input_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == i);

        ui.horizontal(|ui| {
            // Input port (left)
            {
                let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
                let col = if response.hovered() || response.dragged() {
                    egui::Color32::YELLOW
                } else if is_input_wired {
                    egui::Color32::from_rgb(80, 170, 255)
                } else {
                    egui::Color32::from_rgb(140, 140, 140)
                };
                ui.painter().circle_filled(rect.center(), 5.0, col);
                ui.painter().circle_stroke(rect.center(), 5.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
                port_positions.insert((node_id, i, true), rect.center());
                if response.drag_started() {
                    if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == i) {
                        *dragging_from = Some((existing.from_node, existing.from_port, true));
                    } else {
                        *dragging_from = Some((node_id, i, false));
                    }
                }
            }

            // DragValue
            let current_val = match i { 0 => *r, 1 => *g, 2 => *b, _ => 0 };

            if is_input_wired {
                ui.label(egui::RichText::new(format!("{} {}", label, current_val)).monospace());
            } else {
                let mut v = current_val as i32;
                ui.add(egui::DragValue::new(&mut v).range(0..=255).prefix(format!("{} ", label)));
                let new_val = v.clamp(0, 255) as u8;
                match i {
                    0 => *r = new_val,
                    1 => *g = new_val,
                    2 => *b = new_val,
                    _ => {}
                }
            }

            // Output port (right)
            {
                let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
                let col = if response.hovered() || response.dragged() {
                    egui::Color32::YELLOW
                } else {
                    egui::Color32::from_rgb(80, 170, 255)
                };
                ui.painter().circle_filled(rect.center(), 5.0, col);
                ui.painter().circle_stroke(rect.center(), 5.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
                port_positions.insert((node_id, i, false), rect.center());
                if response.drag_started() {
                    *dragging_from = Some((node_id, i, true));
                }
            }
        });
    }
}
