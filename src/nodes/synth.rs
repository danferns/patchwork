use crate::audio::{Waveform, AudioManager, SynthParams};
use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    node_type: &mut NodeType,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    audio: &AudioManager,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    pending_disconnects: &mut Vec<(NodeId, usize)>,
) {
    let (waveform, frequency, amplitude, active) = match node_type {
        NodeType::Synth { waveform, frequency, amplitude, active } => (waveform, frequency, amplitude, active),
        _ => return,
    };

    // ── Inline input ports: Freq(0), Amp(1), Gate(2) ─────────────────
    let labels = ["Freq", "Amp", "Gate"];
    let mut freq_val = *frequency;
    let mut amp_val = *amplitude;
    let mut gate_val = if *active { 1.0f32 } else { 0.0 };
    let mut vals = [&mut freq_val, &mut amp_val, &mut gate_val];

    for i in 0..3 {
        let is_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == i);
        let connected_val = Graph::static_input_value(connections, values, node_id, i);

        ui.horizontal(|ui| {
            // Port circle
            let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
            let col = if response.hovered() || response.dragged() {
                egui::Color32::YELLOW
            } else if is_wired {
                egui::Color32::from_rgb(80, 170, 255)
            } else {
                egui::Color32::from_rgb(170, 170, 170)
            };
            ui.painter().circle_filled(rect.center(), 5.0, col);
            ui.painter().circle_stroke(rect.center(), 5.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
            port_positions.insert((node_id, i, true), rect.center());

            if response.drag_started() {
                if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == i) {
                    *dragging_from = Some((existing.from_node, existing.from_port, true));
                    pending_disconnects.push((node_id, i));
                } else {
                    *dragging_from = Some((node_id, i, false));
                }
            }

            ui.label(egui::RichText::new(labels[i]).small());

            if is_wired {
                let v = connected_val.as_float();
                *vals[i] = v;
                match i {
                    0 => ui.label(egui::RichText::new(format!("{:.0} Hz", v)).strong().monospace()),
                    1 => ui.label(egui::RichText::new(format!("{:.2}", v)).strong().monospace()),
                    2 => ui.label(egui::RichText::new(if v > 0.5 { "ON" } else { "off" }).strong().monospace()),
                    _ => ui.label(""),
                };
                ui.label(egui::RichText::new("⟵").small().color(egui::Color32::from_rgb(80, 170, 255)));
            } else {
                match i {
                    0 => { ui.add(egui::DragValue::new(vals[i]).speed(1.0).range(20.0..=20000.0).suffix(" Hz")); }
                    1 => { ui.add(egui::Slider::new(vals[i], 0.0..=1.0).show_value(true)); }
                    2 => {
                        let mut on = *vals[i] > 0.5;
                        ui.checkbox(&mut on, "");
                        *vals[i] = if on { 1.0 } else { 0.0 };
                    }
                    _ => {}
                }
            }
        });
    }

    // Write back
    *frequency = freq_val;
    *amplitude = amp_val;
    *active = gate_val > 0.5;

    // Override from wired inputs
    let freq_in = Graph::static_input_value(connections, values, node_id, 0);
    let amp_in = Graph::static_input_value(connections, values, node_id, 1);
    let gate_in = Graph::static_input_value(connections, values, node_id, 2);
    if let PortValue::Float(f) = freq_in { if f > 0.0 { *frequency = f; } }
    if let PortValue::Float(a) = amp_in { *amplitude = a.clamp(0.0, 1.0); }
    if let PortValue::Float(g) = gate_in { *active = g > 0.5; }

    ui.separator();

    // ── Waveform selector ────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label("Wave:");
        egui::ComboBox::from_id_salt(egui::Id::new(("synth_wave", node_id)))
            .selected_text(waveform.name())
            .width(80.0)
            .show_ui(ui, |ui| {
                for w in Waveform::all() {
                    if ui.selectable_label(*waveform == *w, w.name()).clicked() {
                        *waveform = *w;
                    }
                }
            });
    });

    // ── Waveform visualization ───────────────────────────────────────
    let viz_h = 30.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(200.0), viz_h), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(15, 15, 25));
    let n = 80;
    let points: Vec<egui::Pos2> = (0..=n).map(|i| {
        let t = i as f32 / n as f32;
        let y = waveform.sample(t) * *amplitude;
        egui::pos2(rect.left() + t * rect.width(), rect.center().y - y * viz_h * 0.4)
    }).collect();
    let color = if *active {
        egui::Color32::from_rgb(80, 200, 120)
    } else {
        egui::Color32::from_rgb(80, 80, 80)
    };
    for w in points.windows(2) {
        painter.line_segment([w[0], w[1]], egui::Stroke::new(1.5, color));
    }

    // ── Update audio ─────────────────────────────────────────────────
    audio.set_synth(node_id, SynthParams {
        waveform: *waveform,
        frequency: *frequency,
        amplitude: *amplitude,
        phase: 0.0,
        active: *active,
    });
}
