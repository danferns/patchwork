use crate::graph::*;
use crate::midi::MidiAction;
use eframe::egui;
use std::collections::HashMap;

/// MIDI Out node: select a device, choose Note/CC mode, send MIDI.
/// Inputs: Channel, Note/CC#, Velocity/Value (all float, clamped to MIDI range).
pub fn render(
    ui: &mut egui::Ui,
    port_name: &mut String,
    mode: &mut MidiMode,
    channel: &mut u8,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    available_ports: &[String],
    is_connected: bool,
    midi_actions: &mut Vec<MidiAction>,
) {
    // ── Device selector ─────────────────────────────────────────────
    ui.horizontal(|ui| {
        let status = if is_connected { "\u{25cf}" } else { "\u{25cb}" };
        let status_color = if is_connected {
            egui::Color32::from_rgb(80, 220, 80)
        } else {
            egui::Color32::GRAY
        };
        ui.label(egui::RichText::new(status).color(status_color));

        egui::ComboBox::from_id_salt(("midi_out_port", node_id))
            .selected_text(if port_name.is_empty() {
                "Select device..."
            } else {
                port_name.as_str()
            })
            .width(140.0)
            .show_ui(ui, |ui| {
                for p in available_ports {
                    if ui
                        .selectable_value(port_name, p.clone(), p.as_str())
                        .clicked()
                    {
                        midi_actions.push(MidiAction::ConnectOutput {
                            node_id,
                            port_name: p.clone(),
                        });
                    }
                }
            });
    });

    ui.horizontal(|ui| {
        if !port_name.is_empty() && !is_connected {
            if ui.button("Connect").clicked() {
                midi_actions.push(MidiAction::ConnectOutput {
                    node_id,
                    port_name: port_name.clone(),
                });
            }
        }
        if is_connected && ui.button("Disconnect").clicked() {
            midi_actions.push(MidiAction::DisconnectOutput { node_id });
        }
    });

    ui.separator();

    // ── Mode toggle ─────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label("Mode:");
        if ui
            .selectable_label(*mode == MidiMode::Note, "Note")
            .clicked()
        {
            *mode = MidiMode::Note;
        }
        if ui.selectable_label(*mode == MidiMode::CC, "CC").clicked() {
            *mode = MidiMode::CC;
        }
    });

    // ── Channel ─────────────────────────────────────────────────────
    let in_ch = Graph::static_input_value(connections, values, node_id, 0);
    let ch = if let PortValue::Float(v) = in_ch {
        v.clamp(0.0, 15.0) as u8
    } else {
        *channel
    };
    ui.horizontal(|ui| {
        ui.label("Ch:");
        ui.add(egui::DragValue::new(channel).range(0..=15));
        if matches!(in_ch, PortValue::Float(_)) {
            ui.label(
                egui::RichText::new(format!("(in: {})", ch))
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }
    });

    // ── Data 1 & 2 ─────────────────────────────────────────────────
    let in_d1 = Graph::static_input_value(connections, values, node_id, 1);
    let in_d2 = Graph::static_input_value(connections, values, node_id, 2);

    let d1 = if let PortValue::Float(v) = in_d1 {
        v.clamp(0.0, 127.0) as u8
    } else {
        0
    };
    let d2 = if let PortValue::Float(v) = in_d2 {
        v.clamp(0.0, 127.0) as u8
    } else {
        0
    };

    match mode {
        MidiMode::Note => {
            ui.label(format!("Note: {}  Vel: {}", d1, d2));
        }
        MidiMode::CC => {
            ui.label(format!("CC#: {}  Value: {}", d1, d2));
        }
    }

    // ── Build and send MIDI message ─────────────────────────────────
    if is_connected {
        let status_byte = match mode {
            MidiMode::Note => 0x90 | (ch & 0x0F),
            MidiMode::CC => 0xB0 | (ch & 0x0F),
        };
        let msg = [status_byte, d1 & 0x7F, d2 & 0x7F];
        midi_actions.push(MidiAction::Send {
            node_id,
            message: msg,
        });
    }
}
