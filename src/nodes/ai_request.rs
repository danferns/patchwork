use eframe::egui;
use crate::graph::{NodeId, PortValue, Connection, Graph};
use crate::http::HttpAction;
use std::collections::HashMap;

/// Parse the config JSON to extract provider, model, api_key, max_tokens, temperature, custom_url
fn parse_config(json_str: &str) -> Option<AiConfig> {
    let v: serde_json::Value = serde_json::from_str(json_str).ok()?;
    Some(AiConfig {
        provider: v.get("provider").and_then(|v| v.as_str()).unwrap_or("anthropic").to_string(),
        model: v.get("model").and_then(|v| v.as_str()).unwrap_or("claude-sonnet-4-20250514").to_string(),
        api_key: v.get("api_key").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        max_tokens: v.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(4096) as u32,
        temperature: v.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.7) as f32,
        custom_url: v.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
    })
}

struct AiConfig {
    provider: String,
    model: String,
    api_key: String,
    max_tokens: u32,
    temperature: f32,
    custom_url: String,
}

pub fn render(
    ui: &mut egui::Ui,
    provider: &mut String,
    model: &mut String,
    response: &str,
    status: &str,
    max_tokens: &mut u32,
    temperature: &mut f32,
    api_key_name: &mut String,
    custom_url: &mut String,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    is_pending: bool,
    actions: &mut Vec<HttpAction>,
    _api_keys: &HashMap<String, String>,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
) {
    // Inline input ports
    let cfg_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 0);
    let sys_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 1);
    let prm_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 2);

    for (port, label, wired) in [(0, "Config", cfg_wired), (1, "System", sys_wired), (2, "Prompt", prm_wired)] {
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
            ui.label(egui::RichText::new(format!("{}:", label)).small());
            if wired {
                ui.label(egui::RichText::new("connected").small().color(egui::Color32::from_rgb(80, 170, 255)));
            } else {
                ui.label(egui::RichText::new("—").small().color(egui::Color32::GRAY));
            }
        });
    }
    ui.separator();

    // Input 0 = Config JSON, Input 1 = System, Input 2 = Prompt
    let config_input = Graph::static_input_value(connections, values, node_id, 0);
    let system_input = Graph::static_input_value(connections, values, node_id, 1);
    let prompt_input = Graph::static_input_value(connections, values, node_id, 2);

    let config_text = match &config_input {
        PortValue::Text(s) => s.clone(),
        _ => String::new(),
    };
    let system_prompt = match &system_input {
        PortValue::Text(s) => s.clone(),
        _ => String::new(),
    };
    let user_prompt = match &prompt_input {
        PortValue::Text(s) => s.clone(),
        _ => String::new(),
    };

    // Parse config from input or use node's stored values as fallback
    let config = if !config_text.is_empty() {
        parse_config(&config_text)
    } else {
        None
    };

    let eff_provider = config.as_ref().map(|c| c.provider.clone()).unwrap_or(provider.clone());
    let eff_model = config.as_ref().map(|c| c.model.clone()).unwrap_or(model.clone());
    let eff_key = config.as_ref().map(|c| c.api_key.clone()).unwrap_or_default();
    let eff_max_tokens = config.as_ref().map(|c| c.max_tokens).unwrap_or(*max_tokens);
    let eff_temp = config.as_ref().map(|c| c.temperature).unwrap_or(*temperature);
    let eff_custom_url = config.as_ref().map(|c| c.custom_url.clone()).unwrap_or(custom_url.clone());

    // Show parsed config summary
    if config.is_some() {
        ui.label(format!("Provider: {}", eff_provider));
        ui.label(format!("Model: {}", eff_model));
        if !eff_key.is_empty() {
            let masked = format!("{}...{}", &eff_key[..eff_key.len().min(8)], &eff_key[eff_key.len().saturating_sub(4)..]);
            ui.label(format!("Key: {}", masked));
        } else {
            ui.colored_label(egui::Color32::from_rgb(200, 80, 80), "Key: (missing in config)");
        }
        ui.label(format!("Temp: {:.1}", eff_temp));
    } else {
        // Fallback: manual UI when no config connected
        ui.colored_label(egui::Color32::from_rgb(180, 180, 80), "No config connected — using manual:");
        ui.horizontal(|ui| {
            ui.label("Provider:");
            egui::ComboBox::from_id_salt(format!("prov_{}", node_id))
                .selected_text(provider.as_str())
                .width(100.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(provider, "anthropic".into(), "Anthropic");
                    ui.selectable_value(provider, "openai".into(), "OpenAI");
                    ui.selectable_value(provider, "custom".into(), "Custom URL");
                });
        });
        ui.horizontal(|ui| {
            ui.label("Model:");
            ui.text_edit_singleline(model);
        });
        if provider == "custom" {
            ui.horizontal(|ui| {
                ui.label("URL:");
                ui.text_edit_singleline(custom_url);
            });
        }
        ui.horizontal(|ui| {
            ui.label("Key name:");
            ui.text_edit_singleline(api_key_name);
        });
        ui.horizontal(|ui| {
            ui.label("Temp:");
            ui.add(egui::Slider::new(temperature, 0.0..=2.0).step_by(0.1));
        });
    }

    // Send button
    ui.separator();
    ui.horizontal(|ui| {
        let can_send = !user_prompt.is_empty() && !eff_key.is_empty() && !is_pending;
        if ui.add_enabled(can_send, egui::Button::new(
            if is_pending { "⏳ Thinking..." } else { "▶ Send" }
        )).clicked() {
            let (url, headers, body) = build_request(
                &eff_provider, &eff_model, &eff_custom_url, &eff_key,
                &system_prompt, &user_prompt, eff_max_tokens, eff_temp,
            );
            actions.push(HttpAction::SendRequest {
                node_id, url, method: "POST".into(), headers, body,
            });
        }

        if user_prompt.is_empty() {
            ui.colored_label(egui::Color32::GRAY, "(connect Prompt)");
        } else if eff_key.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(200, 80, 80), "(no API key)");
        } else {
            let status_color = if status.contains("done") {
                egui::Color32::from_rgb(80, 200, 80)
            } else if status.contains("error") {
                egui::Color32::from_rgb(220, 80, 80)
            } else if is_pending {
                egui::Color32::from_rgb(200, 200, 80)
            } else {
                egui::Color32::GRAY
            };
            ui.colored_label(status_color, if status.is_empty() { "ready" } else { status });
        }
    });

    // Config JSON template hint
    if config.is_none() && config_text.is_empty() {
        ui.separator();
        ui.collapsing("Config JSON format", |ui| {
            ui.code(r#"{
  "provider": "anthropic",
  "model": "claude-sonnet-4-20250514",
  "api_key": "sk-ant-...",
  "temperature": 0.7
}"#);
            ui.label("Connect via Text Editor or File node");
        });
    }

    // Output ports
    ui.separator();
    for (port, label) in [(0, "Response"), (1, "Status")] {
        ui.horizontal(|ui| {
            let val_str = match port {
                0 => if response.is_empty() { "—".to_string() } else { format!("{} chars", response.len()) },
                1 => if status.is_empty() { "—".to_string() } else { status.to_string() },
                _ => "—".to_string(),
            };
            ui.label(egui::RichText::new(format!("{}: {}", label, val_str)).small());
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
            let col = if resp.hovered() || resp.dragged() { egui::Color32::YELLOW } else { egui::Color32::from_rgb(80, 170, 255) };
            ui.painter().circle_filled(rect.center(), 5.0, col);
            ui.painter().circle_stroke(rect.center(), 5.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
            port_positions.insert((node_id, port, false), rect.center());
            if resp.drag_started() { *dragging_from = Some((node_id, port, true)); }
        });
    }

    // Response preview (collapsible)
    if !response.is_empty() {
        ui.collapsing("Response Body", |ui| {
            egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut response.to_string())
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .interactive(false));
            });
        });
    }
}

fn build_request(
    provider: &str,
    model: &str,
    custom_url: &str,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: u32,
    temperature: f32,
) -> (String, Vec<(String, String)>, String) {
    match provider {
        "anthropic" => {
            let url = "https://api.anthropic.com/v1/messages".into();
            let headers = vec![
                ("x-api-key".into(), api_key.into()),
                ("anthropic-version".into(), "2023-06-01".into()),
                ("content-type".into(), "application/json".into()),
            ];
            let messages = vec![
                serde_json::json!({"role": "user", "content": user_prompt})
            ];
            let mut body = serde_json::json!({
                "model": model,
                "max_tokens": max_tokens,
                "temperature": temperature,
                "messages": messages,
            });
            if !system_prompt.is_empty() {
                body["system"] = serde_json::json!(system_prompt);
            }
            (url, headers, body.to_string())
        }
        "openai" => {
            let url = "https://api.openai.com/v1/chat/completions".into();
            let headers = vec![
                ("Authorization".into(), format!("Bearer {}", api_key)),
                ("Content-Type".into(), "application/json".into()),
            ];
            let mut messages = vec![];
            if !system_prompt.is_empty() {
                messages.push(serde_json::json!({"role": "system", "content": system_prompt}));
            }
            messages.push(serde_json::json!({"role": "user", "content": user_prompt}));
            let body = serde_json::json!({
                "model": model,
                "messages": messages,
            });
            (url, headers, body.to_string())
        }
        _ => {
            let url = custom_url.to_string();
            let headers = vec![
                ("Authorization".into(), format!("Bearer {}", api_key)),
                ("Content-Type".into(), "application/json".into()),
            ];
            let mut messages = vec![];
            if !system_prompt.is_empty() {
                messages.push(serde_json::json!({"role": "system", "content": system_prompt}));
            }
            messages.push(serde_json::json!({"role": "user", "content": user_prompt}));
            let body = serde_json::json!({
                "model": model,
                "messages": messages,
            });
            (url, headers, body.to_string())
        }
    }
}

/// Extract the text content from a provider's response JSON
pub fn extract_ai_response(provider: &str, body: &str) -> String {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
        return body.to_string();
    };
    match provider {
        "anthropic" => {
            json.get("content")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or(body)
                .to_string()
        }
        "openai" | _ => {
            json.get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|t| t.as_str())
                .unwrap_or(body)
                .to_string()
        }
    }
}
