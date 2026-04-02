use crate::audio::AudioManager;
use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    node_type: &mut NodeType,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    audio: &mut AudioManager,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    pending_disconnects: &mut Vec<(NodeId, usize)>,
) {
    let (selected_device, gain, active) = match node_type {
        NodeType::AudioInput { selected_device, gain, active } =>
            (selected_device, gain, active),
        _ => return,
    };

    // ── Mic icon + status ─────────────────────────────────────────────
    let status_text = if *active { "Listening" } else { "Stopped" };
    let status_color = if *active {
        egui::Color32::from_rgb(255, 80, 80)
    } else {
        egui::Color32::from_rgb(130, 130, 130)
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("\u{1F3A4}").size(18.0)); // 🎤
        ui.colored_label(status_color, egui::RichText::new(status_text).strong());
        if *active {
            ui.colored_label(egui::Color32::from_rgb(255, 60, 60), "●");
        }
    });

    // ── Device selector ───────────────────────────────────────────────
    let devices = &audio.cached_input_devices;
    ui.horizontal(|ui| {
        ui.label("Device:");
        let display = if selected_device.is_empty() {
            "Default"
        } else {
            selected_device.as_str()
        };
        egui::ComboBox::from_id_salt(egui::Id::new(("audio_input_dev", node_id)))
            .selected_text(if display.len() > 22 { &display[..22] } else { display })
            .width(140.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(selected_device.is_empty(), "Default").clicked() {
                    *selected_device = String::new();
                }
                for dev in devices {
                    if ui.selectable_label(*selected_device == *dev, dev).clicked() {
                        *selected_device = dev.clone();
                    }
                }
            });
    });

    // Auto-start input if active but stream doesn't exist
    // (happens after project load or DSP restart)
    if *active && !audio.input_buffers.contains_key(&node_id) {
        let dev = if selected_device.is_empty() { None } else { Some(selected_device.as_str()) };
        if let Err(e) = audio.start_input(node_id, dev) {
            crate::system_log::warn(format!("Mic auto-start failed: {}", e));
            *active = false;
        }
    }

    // ── Start / Stop ──────────────────────────────────────────────────
    ui.horizontal(|ui| {
        if *active {
            if ui.button("\u{23F9} Stop").clicked() {
                *active = false;
                audio.stop_input(node_id);
            }
        } else {
            if ui.button("\u{25B6} Start").clicked() {
                let dev = if selected_device.is_empty() { None } else { Some(selected_device.as_str()) };
                match audio.start_input(node_id, dev) {
                    Ok(()) => *active = true,
                    Err(e) => crate::system_log::error(format!("Mic start failed: {}", e)),
                }
            }
        }
    });

    // ── Gain slider ───────────────────────────────────────────────────
    // Read from wired port 0 (Gain) if connected
    let gain_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 0);
    crate::nodes::inline_port_circle(
        ui, node_id, 0, true, connections,
        port_positions, dragging_from, pending_disconnects, PortKind::Normalized,
    );
    if gain_wired {
        let v = Graph::static_input_value(connections, values, node_id, 0).as_float();
        *gain = v.clamp(0.0, 2.0);
        ui.horizontal(|ui| {
            ui.label("Gain:");
            ui.label(format!("{:.0}%", *gain * 100.0));
        });
    } else {
        ui.horizontal(|ui| {
            ui.label("Gain:");
            ui.add(egui::Slider::new(gain, 0.0..=2.0).show_value(true));
        });
    }

    // ── Output port: Audio ────────────────────────────────────────────
    ui.separator();
    crate::nodes::audio_port_row(ui, "Audio", node_id, 0, false, port_positions, dragging_from, connections, pending_disconnects, PortKind::Audio);

    // Write gain to engine (lock-free atomic)
    if *active {
        audio.engine_write_param(node_id, 0, *gain);
    }
}
