use eframe::egui;
use crate::graph::{NodeId, PortValue, Connection, Graph};
use crate::http::HttpAction;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub fn render(
    ui: &mut egui::Ui,
    url: &mut String,
    method: &mut String,
    headers: &mut String,
    response: &str,
    status: &str,
    auto_send: &mut bool,
    last_hash: &mut u64,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    is_pending: bool,
    actions: &mut Vec<HttpAction>,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
) {
    if method.is_empty() { *method = "POST".into(); }

    let url_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 0);
    let body_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 1);
    let hdr_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 2);

    // URL override from input
    let url_input = Graph::static_input_value(connections, values, node_id, 0);
    let effective_url = match &url_input {
        PortValue::Text(s) if !s.is_empty() => s.clone(),
        _ => url.clone(),
    };

    // Body from input
    let body_input = Graph::static_input_value(connections, values, node_id, 1);
    let body = match &body_input {
        PortValue::Text(s) => s.clone(),
        PortValue::Float(f) => format!("{}", f),
        _ => String::new(),
    };

    // Headers from input
    let hdr_input = Graph::static_input_value(connections, values, node_id, 2);
    let effective_headers = match &hdr_input {
        PortValue::Text(s) if !s.is_empty() => s.clone(),
        _ => headers.clone(),
    };

    // Port 0: URL
    ui.horizontal(|ui| {
        port_circle(ui, node_id, 0, url_wired, port_positions, dragging_from, connections);
        ui.label(egui::RichText::new("URL:").small());
        if url_wired {
            let short = if effective_url.len() > 30 { format!("{}...", &effective_url[..30]) } else { effective_url.clone() };
            ui.label(egui::RichText::new(short).small().monospace().color(egui::Color32::from_rgb(80, 170, 255)));
        }
    });
    if !url_wired {
        ui.add(egui::TextEdit::singleline(url).desired_width(ui.available_width()).hint_text("https://..."));
    }

    // Method + Auto
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(format!("method_{}", node_id))
            .selected_text(method.as_str())
            .width(60.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(method, "GET".into(), "GET");
                ui.selectable_value(method, "POST".into(), "POST");
                ui.selectable_value(method, "PUT".into(), "PUT");
                ui.selectable_value(method, "DELETE".into(), "DELETE");
            });
        ui.checkbox(auto_send, "Auto");
    });

    // Port 1: Body
    ui.horizontal(|ui| {
        port_circle(ui, node_id, 1, body_wired, port_positions, dragging_from, connections);
        ui.label(egui::RichText::new("Body:").small());
        if body_wired {
            let short = if body.len() > 25 { format!("{}...", &body[..25]) } else { body.clone() };
            ui.label(egui::RichText::new(short).small().monospace().color(egui::Color32::from_rgb(80, 170, 255)));
        } else {
            ui.label(egui::RichText::new("—").small().color(egui::Color32::GRAY));
        }
    });

    // Port 2: Headers (collapsible when not wired)
    ui.horizontal(|ui| {
        port_circle(ui, node_id, 2, hdr_wired, port_positions, dragging_from, connections);
        ui.label(egui::RichText::new("Headers:").small());
        if hdr_wired {
            ui.label(egui::RichText::new("connected").small().color(egui::Color32::from_rgb(80, 170, 255)));
        }
    });
    if !hdr_wired {
        ui.collapsing("Edit Headers", |ui| {
            ui.text_edit_multiline(headers);
        });
    }

    // Send button + status
    ui.horizontal(|ui| {
        let can_send = !effective_url.is_empty() && !is_pending;
        if ui.add_enabled(can_send, egui::Button::new(if is_pending { "Sending..." } else { "Send" })).clicked() {
            let parsed_headers = parse_headers(&effective_headers);
            actions.push(HttpAction::SendRequest {
                node_id,
                url: effective_url.clone(),
                method: method.clone(),
                headers: parsed_headers,
                body: body.clone(),
            });
        }
        let status_color = if status.starts_with('2') { egui::Color32::from_rgb(80, 200, 80) }
            else if status == "idle" || status.is_empty() { egui::Color32::GRAY }
            else if is_pending { egui::Color32::from_rgb(200, 200, 80) }
            else { egui::Color32::from_rgb(220, 80, 80) };
        ui.colored_label(status_color, if status.is_empty() { "idle" } else { status });
    });

    // Auto-send
    if *auto_send && !is_pending {
        let mut hasher = std::hash::DefaultHasher::new();
        effective_url.hash(&mut hasher);
        body.hash(&mut hasher);
        effective_headers.hash(&mut hasher);
        let new_hash = hasher.finish();
        if new_hash != *last_hash && *last_hash != 0 {
            let parsed_headers = parse_headers(&effective_headers);
            actions.push(HttpAction::SendRequest {
                node_id, url: effective_url, method: method.clone(),
                headers: parsed_headers, body,
            });
        }
        *last_hash = new_hash;
    }

    // Output ports: Response + Status
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Response:").small());
        let short = if response.len() > 30 { format!("{}...", &response[..30]) } else if response.is_empty() { "—".into() } else { response.to_string() };
        ui.label(egui::RichText::new(short).small().monospace());
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
        let col = if resp.hovered() || resp.dragged() { egui::Color32::YELLOW } else { egui::Color32::from_rgb(80, 170, 255) };
        ui.painter().circle_filled(rect.center(), 5.0, col);
        ui.painter().circle_stroke(rect.center(), 5.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
        port_positions.insert((node_id, 0, false), rect.center());
        if resp.drag_started() { *dragging_from = Some((node_id, 0, true)); }
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Status: {}", if status.is_empty() { "—" } else { status })).small());
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
        let col = if resp.hovered() || resp.dragged() { egui::Color32::YELLOW } else { egui::Color32::from_rgb(80, 170, 255) };
        ui.painter().circle_filled(rect.center(), 5.0, col);
        ui.painter().circle_stroke(rect.center(), 5.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
        port_positions.insert((node_id, 1, false), rect.center());
        if resp.drag_started() { *dragging_from = Some((node_id, 1, true)); }
    });

    // Response preview (collapsible)
    if !response.is_empty() {
        ui.collapsing("Response Body", |ui| {
            egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut response.to_string())
                    .code_editor().desired_width(ui.available_width()));
            });
        });
    }
}

fn port_circle(
    ui: &mut egui::Ui,
    node_id: NodeId, port: usize, is_wired: bool,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    connections: &[Connection],
) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
    let col = if response.hovered() || response.dragged() { egui::Color32::YELLOW }
        else if is_wired { egui::Color32::from_rgb(80, 170, 255) }
        else { egui::Color32::from_rgb(140, 140, 140) };
    ui.painter().circle_filled(rect.center(), 4.0, col);
    ui.painter().circle_stroke(rect.center(), 4.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
    port_positions.insert((node_id, port, true), rect.center());
    if response.drag_started() {
        if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == port) {
            *dragging_from = Some((existing.from_node, existing.from_port, true));
        } else {
            *dragging_from = Some((node_id, port, false));
        }
    }
}

fn parse_headers(raw: &str) -> Vec<(String, String)> {
    raw.lines()
        .filter_map(|line| line.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect()
}
