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
) {
    if method.is_empty() { *method = "POST".into(); }

    // URL override from input
    let url_input = Graph::static_input_value(connections, values, node_id, 0);
    let effective_url = match &url_input {
        PortValue::Text(s) if !s.is_empty() => { s.clone() }
        _ => url.clone(),
    };

    // Body from input
    let body_input = Graph::static_input_value(connections, values, node_id, 1);
    let body = match &body_input {
        PortValue::Text(s) => s.clone(),
        PortValue::Float(f) => format!("{}", f),
        _ => String::new(),
    };

    // Headers from input (override)
    let hdr_input = Graph::static_input_value(connections, values, node_id, 2);
    let effective_headers = match &hdr_input {
        PortValue::Text(s) if !s.is_empty() => s.clone(),
        _ => headers.clone(),
    };

    // UI
    ui.horizontal(|ui| {
        ui.label("URL:");
        ui.text_edit_singleline(url);
    });
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

    ui.collapsing("Headers", |ui| {
        ui.text_edit_multiline(headers);
        ui.label("(key: value per line)");
    });

    // Send button + status
    ui.horizontal(|ui| {
        let can_send = !effective_url.is_empty() && !is_pending;
        if ui.add_enabled(can_send, egui::Button::new(if is_pending { "⏳ Sending..." } else { "▶ Send" })).clicked() {
            let parsed_headers = parse_headers(&effective_headers);
            actions.push(HttpAction::SendRequest {
                node_id,
                url: effective_url.clone(),
                method: method.clone(),
                headers: parsed_headers,
                body: body.clone(),
            });
        }
        let status_color = if status.starts_with('2') {
            egui::Color32::from_rgb(80, 200, 80)
        } else if status == "idle" || status.is_empty() {
            egui::Color32::GRAY
        } else if is_pending {
            egui::Color32::from_rgb(200, 200, 80)
        } else {
            egui::Color32::from_rgb(220, 80, 80)
        };
        ui.colored_label(status_color, if status.is_empty() { "idle" } else { status });
    });

    // Auto-send: detect input change
    if *auto_send && !is_pending {
        let mut hasher = std::hash::DefaultHasher::new();
        effective_url.hash(&mut hasher);
        body.hash(&mut hasher);
        effective_headers.hash(&mut hasher);
        let new_hash = hasher.finish();
        if new_hash != *last_hash && *last_hash != 0 {
            let parsed_headers = parse_headers(&effective_headers);
            actions.push(HttpAction::SendRequest {
                node_id,
                url: effective_url,
                method: method.clone(),
                headers: parsed_headers,
                body,
            });
        }
        *last_hash = new_hash;
    }

    // Response preview
    if !response.is_empty() {
        ui.separator();
        ui.label(format!("Response ({} chars)", response.len()));
        egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut response.to_string())
                .code_editor()
                .desired_width(f32::INFINITY)
                .interactive(false));
        });
    }
}

fn parse_headers(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() { return None; }
            let (key, val) = line.split_once(':')?;
            Some((key.trim().to_string(), val.trim().to_string()))
        })
        .collect()
}
