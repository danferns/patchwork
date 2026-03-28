use eframe::egui;
use crate::graph::{NodeId, PortValue, Connection, Graph, PortKind};
use crate::http::HttpAction;
use std::collections::HashMap;

/// Model presets per provider
fn models_for_provider(provider: &str) -> Vec<(&'static str, &'static str)> {
    match provider {
        "anthropic" => vec![
            ("claude-sonnet-4-20250514", "Claude Sonnet 4"),
            ("claude-haiku-35-20241022", "Claude Haiku 3.5"),
            ("claude-opus-4-20250514", "Claude Opus 4"),
        ],
        "openai" => vec![
            ("gpt-4o", "GPT-4o"),
            ("gpt-4o-mini", "GPT-4o Mini"),
            ("gpt-4-turbo", "GPT-4 Turbo"),
            ("o1-mini", "o1 Mini"),
        ],
        "google" => vec![
            ("gemini-2.0-flash", "Gemini 2.0 Flash"),
            ("gemini-2.0-pro", "Gemini 2.0 Pro"),
            ("gemini-1.5-flash", "Gemini 1.5 Flash"),
        ],
        _ => vec![],
    }
}

fn provider_label(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "Anthropic",
        "openai" => "OpenAI",
        "google" => "Google",
        _ => "Unknown",
    }
}

const RESPONSE_TYPES: &[&str] = &["Text", "JSON", "Code", "WGSL", "HTML", "Image"];

pub fn render(
    ui: &mut egui::Ui,
    provider: &mut String,
    model: &mut String,
    system_prompt: &mut String,
    user_prompt: &mut String,
    response: &str,
    status: &str,
    max_tokens: &mut u32,
    temperature: &mut f32,
    api_key: &mut String,
    response_type: &mut u8,
    last_trigger: &mut f32,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    is_pending: bool,
    actions: &mut Vec<HttpAction>,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    pending_disconnects: &mut Vec<(NodeId, usize)>,
) {
    let system_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 0);
    let prompt_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 1);
    let trigger_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 2);

    // ── Input ports ──
    ui.horizontal(|ui| {
        super::inline_port_circle(ui, node_id, 0, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Text);
        ui.label(egui::RichText::new("System").small());
        if system_wired {
            ui.label(egui::RichText::new("●").small().color(egui::Color32::from_rgb(80, 200, 80)));
        }
    });
    ui.horizontal(|ui| {
        super::inline_port_circle(ui, node_id, 1, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Text);
        ui.label(egui::RichText::new("Prompt").small());
        if prompt_wired {
            ui.label(egui::RichText::new("●").small().color(egui::Color32::from_rgb(80, 200, 80)));
        }
    });
    ui.horizontal(|ui| {
        super::inline_port_circle(ui, node_id, 2, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Trigger);
        ui.label(egui::RichText::new("Send").small());
        if trigger_wired {
            ui.label(egui::RichText::new("▸").small().color(egui::Color32::from_rgb(220, 160, 40)));
        }
    });

    ui.separator();

    // ── Provider dropdown ──
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Provider").small());
        let prev_provider = provider.clone();
        egui::ComboBox::from_id_salt(format!("ai_prov_{}", node_id))
            .selected_text(provider_label(provider))
            .width(120.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(provider, "anthropic".into(), "Anthropic");
                ui.selectable_value(provider, "openai".into(), "OpenAI");
                ui.selectable_value(provider, "google".into(), "Google");
            });
        // Reset model when provider changes
        if *provider != prev_provider {
            if let Some((id, _)) = models_for_provider(provider).first() {
                *model = id.to_string();
            }
        }
    });

    // ── Model dropdown ──
    let models = models_for_provider(provider);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Model").small());
        let display_name = models.iter()
            .find(|(id, _)| *id == model.as_str())
            .map(|(_, name)| *name)
            .unwrap_or(model.as_str());
        egui::ComboBox::from_id_salt(format!("ai_model_{}", node_id))
            .selected_text(display_name)
            .width(140.0)
            .show_ui(ui, |ui| {
                for (id, name) in &models {
                    ui.selectable_value(model, id.to_string(), *name);
                }
            });
    });

    // ── API Key (password field) ──
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("API Key").small());
        ui.add(
            egui::TextEdit::singleline(api_key)
                .password(true)
                .desired_width(140.0)
                .hint_text("sk-...")
        );
    });

    ui.separator();

    // ── System Prompt (textarea, unless wired) ──
    if !system_wired {
        ui.label(egui::RichText::new("System Prompt").small().color(egui::Color32::from_rgb(140, 140, 155)));
        egui::ScrollArea::vertical().max_height(60.0).show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(system_prompt)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text("You are a helpful assistant...")
                    .font(egui::TextStyle::Small)
            );
        });
    }

    // ── User Prompt (textarea, unless wired) ──
    if !prompt_wired {
        ui.label(egui::RichText::new("Prompt").small().color(egui::Color32::from_rgb(140, 140, 155)));
        ui.add(
            egui::TextEdit::multiline(user_prompt)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text("Ask something...")
                .font(egui::TextStyle::Small)
        );
    }

    ui.separator();

    // ── Temperature + Response Type ──
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Temp").small());
        ui.add(egui::Slider::new(temperature, 0.0..=2.0).step_by(0.1).show_value(true));
    });

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Return").small());
        let rt = *response_type as usize;
        let rt_label = RESPONSE_TYPES.get(rt).unwrap_or(&"Text");
        egui::ComboBox::from_id_salt(format!("ai_rt_{}", node_id))
            .selected_text(*rt_label)
            .width(80.0)
            .show_ui(ui, |ui| {
                for (i, name) in RESPONSE_TYPES.iter().enumerate() {
                    if ui.selectable_label(rt == i, *name).clicked() {
                        *response_type = i as u8;
                    }
                }
            });
    });

    // ── Resolve effective prompts (wired overrides inline) ──
    let eff_system = if system_wired {
        match Graph::static_input_value(connections, values, node_id, 0) {
            PortValue::Text(s) => s,
            _ => system_prompt.clone(),
        }
    } else {
        system_prompt.clone()
    };
    let eff_prompt = if prompt_wired {
        match Graph::static_input_value(connections, values, node_id, 1) {
            PortValue::Text(s) => s,
            _ => user_prompt.clone(),
        }
    } else {
        user_prompt.clone()
    };

    // ── Send button ──
    ui.horizontal(|ui| {
        let can_send = !eff_prompt.is_empty() && !api_key.is_empty() && !is_pending;
        let btn_text = if is_pending { "\u{23f3} Thinking..." } else { "\u{25b6} Send" };
        if ui.add_enabled(can_send, egui::Button::new(btn_text)).clicked() {
            let (url, headers, body) = build_request(
                provider, model, "", api_key,
                &eff_system, &eff_prompt, *max_tokens, *temperature, *response_type,
            );
            actions.push(HttpAction::SendRequest {
                node_id, url, method: "POST".into(), headers, body,
            });
        }

        // Status indicator
        if eff_prompt.is_empty() {
            ui.label(egui::RichText::new("(type a prompt)").small().color(egui::Color32::GRAY));
        } else if api_key.is_empty() {
            ui.label(egui::RichText::new("(need API key)").small().color(egui::Color32::from_rgb(200, 80, 80)));
        } else {
            let (color, text) = if status.contains("done") {
                (egui::Color32::from_rgb(80, 200, 80), "done")
            } else if status.contains("error") {
                (egui::Color32::from_rgb(220, 80, 80), status)
            } else if is_pending {
                (egui::Color32::from_rgb(200, 200, 80), "thinking...")
            } else {
                (egui::Color32::GRAY, "ready")
            };
            ui.label(egui::RichText::new(text).small().color(color));
        }
    });

    // ── Trigger port auto-send (rising edge) ──
    if trigger_wired {
        let trigger_val = match Graph::static_input_value(connections, values, node_id, 2) {
            PortValue::Float(v) => v,
            _ => 0.0,
        };
        if trigger_val > 0.5 && *last_trigger <= 0.5 && !eff_prompt.is_empty() && !api_key.is_empty() && !is_pending {
            let (url, headers, body) = build_request(
                provider, model, "", api_key,
                &eff_system, &eff_prompt, *max_tokens, *temperature, *response_type,
            );
            actions.push(HttpAction::SendRequest {
                node_id, url, method: "POST".into(), headers, body,
            });
        }
        *last_trigger = trigger_val as f32;
    }

    // ── MCP auto-trigger ──
    if status == "mcp_trigger" && !eff_prompt.is_empty() && !api_key.is_empty() && !is_pending {
        let (url, headers, body) = build_request(
            provider, model, "", api_key,
            &eff_system, &eff_prompt, *max_tokens, *temperature, *response_type,
        );
        actions.push(HttpAction::SendRequest {
            node_id, url, method: "POST".into(), headers, body,
        });
        ui.ctx().data_mut(|d| d.insert_temp(
            egui::Id::new(("mcp_ai_triggered", node_id)),
            true,
        ));
    }

    // ── Output ports ──
    ui.separator();
    {
        let val_str = if response.is_empty() { "\u{2014}".to_string() } else { format!("{}ch", response.len()) };
        super::output_port_row(ui, "Response", &val_str, node_id, 0, port_positions, dragging_from, connections, pending_disconnects, PortKind::Text);
    }
    {
        let val_str = if status.is_empty() { "\u{2014}".to_string() } else { status.to_string() };
        super::output_port_row(ui, "Status", &val_str, node_id, 1, port_positions, dragging_from, connections, pending_disconnects, PortKind::Text);
    }

    // ── Response preview ──
    if !response.is_empty() {
        ui.collapsing(format!("Response ({} chars)", response.len()), |ui| {
            egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                let rt = *response_type;
                if rt == 2 || rt == 3 || rt == 4 {
                    // Code/WGSL/HTML — show as code
                    ui.add(egui::TextEdit::multiline(&mut response.to_string())
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .interactive(false));
                } else {
                    // Text/JSON — show as wrapped text
                    ui.add(egui::TextEdit::multiline(&mut response.to_string())
                        .desired_width(f32::INFINITY)
                        .interactive(false));
                }
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
    response_type: u8,
) -> (String, Vec<(String, String)>, String) {
    let is_image = response_type == 5;

    match provider {
        "anthropic" => {
            let url = "https://api.anthropic.com/v1/messages".into();
            let headers = vec![
                ("x-api-key".into(), api_key.into()),
                ("anthropic-version".into(), "2023-06-01".into()),
                ("content-type".into(), "application/json".into()),
            ];
            let mut body = serde_json::json!({
                "model": model,
                "max_tokens": max_tokens,
                "temperature": temperature,
                "messages": [{"role": "user", "content": user_prompt}],
            });
            if !system_prompt.is_empty() {
                body["system"] = serde_json::json!(system_prompt);
            }
            (url, headers, body.to_string())
        }
        "google" => {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                model, api_key
            );
            let headers = vec![
                ("Content-Type".into(), "application/json".into()),
            ];
            let mut body = serde_json::json!({
                "contents": [{"role": "user", "parts": [{"text": user_prompt}]}],
                "generationConfig": {
                    "maxOutputTokens": max_tokens,
                    "temperature": temperature,
                }
            });
            if !system_prompt.is_empty() {
                body["systemInstruction"] = serde_json::json!({
                    "parts": [{"text": system_prompt}]
                });
            }
            (url, headers, body.to_string())
        }
        "openai" | _ => {
            if is_image {
                // DALL-E image generation endpoint
                let url = if !custom_url.is_empty() {
                    custom_url.to_string()
                } else {
                    "https://api.openai.com/v1/images/generations".into()
                };
                let headers = vec![
                    ("Authorization".into(), format!("Bearer {}", api_key)),
                    ("Content-Type".into(), "application/json".into()),
                ];
                let body = serde_json::json!({
                    "model": "dall-e-3",
                    "prompt": user_prompt,
                    "n": 1,
                    "size": "1024x1024",
                    "response_format": "url",
                });
                (url, headers, body.to_string())
            } else {
                let url = if !custom_url.is_empty() {
                    custom_url.to_string()
                } else {
                    "https://api.openai.com/v1/chat/completions".into()
                };
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
                    "max_tokens": max_tokens,
                    "temperature": temperature,
                });
                (url, headers, body.to_string())
            }
        }
    }
}

/// Strip markdown code fences from AI responses.
/// Handles ```lang\n...\n``` and ```\n...\n```
fn strip_code_fences(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("```") {
        // Find the end of the first line (the opening fence + optional language tag)
        let after_opening = if let Some(newline_pos) = trimmed.find('\n') {
            &trimmed[newline_pos + 1..]
        } else {
            return trimmed.to_string();
        };
        // Strip trailing ``` if present
        let result = if after_opening.trim_end().ends_with("```") {
            let end = after_opening.trim_end();
            &end[..end.len() - 3]
        } else {
            after_opening
        };
        result.trim().to_string()
    } else {
        text.to_string()
    }
}

/// Extract the text content (or image URL) from a provider's response JSON
pub fn extract_ai_response(provider: &str, body: &str) -> String {
    // Check if this is a DALL-E image response
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(url) = json.get("data")
            .and_then(|d| d.get(0))
            .and_then(|d| d.get("url"))
            .and_then(|u| u.as_str())
        {
            return url.to_string();
        }
    }
    let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
        return strip_code_fences(body);
    };
    let raw = match provider {
        "anthropic" => {
            json.get("content")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or(body)
                .to_string()
        }
        "google" => {
            json.get("candidates")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("content"))
                .and_then(|c| c.get("parts"))
                .and_then(|p| p.get(0))
                .and_then(|p| p.get("text"))
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
    };
    strip_code_fences(&raw)
}
