use eframe::egui;
use crate::graph::{NodeId, PortValue, Connection, Graph};
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    template: &mut String,
    arg_count: &mut usize,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
) {
    // Input ports: port 0 = Template (optional), ports 1..=arg_count = args
    let tmpl_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 0);

    // Template input port
    ui.horizontal(|ui| {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
        let col = if resp.hovered() || resp.dragged() { egui::Color32::YELLOW }
            else if tmpl_wired { egui::Color32::from_rgb(80, 170, 255) }
            else { egui::Color32::from_rgb(140, 140, 140) };
        ui.painter().circle_filled(rect.center(), 4.0, col);
        ui.painter().circle_stroke(rect.center(), 4.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
        port_positions.insert((node_id, 0, true), rect.center());
        if resp.drag_started() {
            if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == 0) {
                *dragging_from = Some((existing.from_node, existing.from_port, true));
            } else {
                *dragging_from = Some((node_id, 0, false));
            }
        }
        ui.label(egui::RichText::new("Template:").small());
        if tmpl_wired {
            ui.label(egui::RichText::new("connected").small().color(egui::Color32::from_rgb(80, 170, 255)));
        }
    });

    // Get effective template
    let effective_template = if tmpl_wired {
        match Graph::static_input_value(connections, values, node_id, 0) {
            PortValue::Text(s) => s,
            _ => template.clone(),
        }
    } else {
        template.clone()
    };

    // Template editor (only when not wired)
    if !tmpl_wired {
        let resp = ui.add(
            egui::TextEdit::multiline(template)
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace)
                .hint_text("Hello {0}, you are {1}!")
        );

        // Show tooltip when editing
        if resp.has_focus() {
            egui::show_tooltip_at_pointer(ui.ctx(), ui.layer_id(), egui::Id::new(("fmt_help", node_id)), |ui| {
                ui.label(egui::RichText::new("Format Placeholders").strong());
                ui.separator();
                ui.label("{0} → first input");
                ui.label("{1} → second input");
                ui.label("{2} → third input ...");
                ui.separator();
                ui.label(egui::RichText::new("Examples:").small().strong());
                ui.label(egui::RichText::new("\"Sensor: {0}°C at {1}%\"").small().code());
                ui.label(egui::RichText::new("\"/osc/{0}/value\"").small().code());
                ui.label(egui::RichText::new("{\"x\": {0}, \"y\": {1}}").small().code());
            });
        }
    } else {
        // Show the wired template text (read-only)
        ui.label(egui::RichText::new(&effective_template).small().code());
    }

    ui.separator();

    // Arg count control
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Args:").small());
        if ui.small_button("−").clicked() && *arg_count > 0 {
            *arg_count -= 1;
        }
        ui.label(egui::RichText::new(format!("{}", arg_count)).strong());
        if ui.small_button("+").clicked() && *arg_count < 10 {
            *arg_count += 1;
        }
    });

    // Arg input ports
    for i in 0..*arg_count {
        let port = i + 1; // port 0 is template
        let wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == port);
        ui.horizontal(|ui| {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
            let col = if resp.hovered() || resp.dragged() { egui::Color32::YELLOW }
                else if wired { egui::Color32::from_rgb(80, 170, 255) }
                else { egui::Color32::from_rgb(140, 140, 140) };
            ui.painter().circle_filled(rect.center(), 4.0, col);
            ui.painter().circle_stroke(rect.center(), 4.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
            port_positions.insert((node_id, port, true), rect.center());
            if resp.drag_started() {
                if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == port) {
                    *dragging_from = Some((existing.from_node, existing.from_port, true));
                } else {
                    *dragging_from = Some((node_id, port, false));
                }
            }
            ui.label(egui::RichText::new(format!("{{{}}}", i)).small().code());
            if wired {
                let val = Graph::static_input_value(connections, values, node_id, port);
                let val_str = match &val {
                    PortValue::Float(f) => format!("{:.3}", f),
                    PortValue::Text(s) => {
                        if s.len() > 20 { format!("\"{}...\"", &s[..20]) } else { format!("\"{}\"", s) }
                    }
                    _ => "—".into(),
                };
                ui.label(egui::RichText::new(val_str).small().color(egui::Color32::from_rgb(80, 170, 255)));
            } else {
                ui.label(egui::RichText::new("—").small().color(egui::Color32::GRAY));
            }
        });
    }

    ui.separator();

    // Build formatted string
    let mut result = effective_template.clone();
    for i in 0..*arg_count {
        let port = i + 1;
        let val = Graph::static_input_value(connections, values, node_id, port);
        let replacement = match &val {
            PortValue::Float(f) => {
                // Clean float formatting: no trailing zeros
                let s = format!("{:.6}", f);
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            }
            PortValue::Text(s) => s.clone(),
            _ => String::new(),
        };
        let placeholder = format!("{{{}}}", i);
        result = result.replace(&placeholder, &replacement);
    }

    // Preview
    ui.label(egui::RichText::new("Output:").small().strong());
    egui::ScrollArea::vertical().max_height(60.0).show(ui, |ui| {
        ui.add(egui::TextEdit::multiline(&mut result.as_str())
            .desired_width(f32::INFINITY)
            .font(egui::TextStyle::Monospace)
            .interactive(false));
    });

    // Output port
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Text: {} chars", result.len())).small());
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
        let col = if resp.hovered() || resp.dragged() { egui::Color32::YELLOW } else { egui::Color32::from_rgb(80, 170, 255) };
        ui.painter().circle_filled(rect.center(), 5.0, col);
        ui.painter().circle_stroke(rect.center(), 5.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
        port_positions.insert((node_id, 0, false), rect.center());
        if resp.drag_started() { *dragging_from = Some((node_id, 0, true)); }
    });
}
