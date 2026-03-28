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
    let (waveform, frequency, amplitude, active, fm_depth) = match node_type {
        NodeType::Synth { waveform, frequency, amplitude, active, fm_depth } => (waveform, frequency, amplitude, active, fm_depth),
        _ => return,
    };

    // ── Waveform selector (top) ───────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Wave:").small());
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

    // Read inputs early for visualization
    let freq_in = Graph::static_input_value(connections, values, node_id, 0);
    let amp_in = Graph::static_input_value(connections, values, node_id, 1);
    let gate_in = Graph::static_input_value(connections, values, node_id, 2);

    let display_freq = if let PortValue::Float(f) = freq_in { if f > 0.0 { f } else { *frequency } } else { *frequency };
    let display_amp = if let PortValue::Float(a) = amp_in { a.clamp(0.0, 1.0) } else { *amplitude };
    let display_active = if let PortValue::Float(g) = gate_in { g > 0.5 } else { *active };

    // ── Waveform visualization (frequency-responsive) ─────────────────
    let viz_w = ui.available_width().min(200.0);
    let viz_h = 40.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(viz_w, viz_h), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, egui::Color32::from_rgb(15, 15, 25));
    painter.rect_stroke(rect, 3.0, egui::Stroke::new(0.5, egui::Color32::from_rgb(40, 40, 55)), egui::StrokeKind::Outside);

    // Zero line
    painter.line_segment(
        [egui::pos2(rect.left(), rect.center().y), egui::pos2(rect.right(), rect.center().y)],
        egui::Stroke::new(0.5, egui::Color32::from_rgb(40, 40, 55)),
    );

    // Number of visible cycles scales with frequency:
    // 100 Hz → ~2 cycles, 440 Hz → ~4, 2000 Hz → ~8, 10000 Hz → ~12
    let cycles = (display_freq / 100.0).clamp(1.0, 16.0).round().max(1.0);
    let n = 120;
    let points: Vec<egui::Pos2> = (0..=n).map(|i| {
        let t = i as f32 / n as f32;
        let phase = (t * cycles) % 1.0;
        let y = waveform.sample(phase) * display_amp;
        egui::pos2(rect.left() + t * rect.width(), rect.center().y - y * viz_h * 0.4)
    }).collect();

    let wave_col = if display_active {
        egui::Color32::from_rgb(80, 220, 140)
    } else {
        egui::Color32::from_rgb(60, 60, 70)
    };
    for w in points.windows(2) {
        painter.line_segment([w[0], w[1]], egui::Stroke::new(1.5, wave_col));
    }

    // Frequency label overlay (top-right of viz)
    painter.text(
        egui::pos2(rect.right() - 3.0, rect.top() + 2.0),
        egui::Align2::RIGHT_TOP,
        format!("{:.0} Hz", display_freq),
        egui::FontId::new(9.0, egui::FontFamily::Proportional),
        egui::Color32::from_rgba_unmultiplied(180, 180, 200, 160),
    );

    ui.separator();

    // ── Auto-detect FM: Freq input connected to another Synth's Audio output ──
    let fm_source_node: Option<NodeId> = connections.iter()
        .find(|c| c.to_node == node_id && c.to_port == 0 && c.from_port == 0)
        .and_then(|c| values.get(&(c.from_node, 0)).map(|_| c.from_node));
    let is_fm = fm_source_node.is_some();

    // FM Weight from port 3 (0–1, defaults to 1.0)
    let fm_wt_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 3);
    let mut fm_weight = if fm_wt_wired {
        Graph::static_input_value(connections, values, node_id, 3).as_float().clamp(0.0, 1.0)
    } else {
        *fm_depth // reuse fm_depth field as weight when not wired (store 0–1)
    };
    // Clamp stored fm_depth to valid weight range if used as weight
    if !fm_wt_wired && !is_fm {
        fm_weight = 1.0; // default weight when no FM
    }

    // ── Inline input ports ────────────────────────────────────────────
    // Port 0: Freq / FM Depth (dual-purpose)
    let freq_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 0);
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 0, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Number);
        if is_fm {
            ui.label(egui::RichText::new("⚡ Depth").small().color(egui::Color32::from_rgb(255, 200, 80)));
            ui.add(egui::DragValue::new(frequency).speed(1.0).range(0.0..=10000.0).suffix(" Hz"));
        } else if freq_wired {
            let v = freq_in.as_float();
            ui.label(egui::RichText::new("Freq").small());
            ui.label(egui::RichText::new(format!("{:.0} Hz", v)).strong().monospace());
        } else {
            ui.label(egui::RichText::new("Freq").small());
            ui.add(egui::DragValue::new(frequency).speed(1.0).range(20.0..=20000.0).suffix(" Hz"));
        }
    });

    // Port 1: Amp
    let amp_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 1);
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 1, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Normalized);
        ui.label(egui::RichText::new("Amp").small());
        if amp_wired {
            ui.label(egui::RichText::new(format!("{:.2}", amp_in.as_float())).strong().monospace());
        } else {
            ui.add(egui::Slider::new(amplitude, 0.0..=1.0).show_value(true));
        }
    });

    // Port 2: Gate
    let gate_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 2);
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 2, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Gate);
        ui.label(egui::RichText::new("Gate").small());
        if gate_wired {
            let v = gate_in.as_float();
            ui.label(egui::RichText::new(if v > 0.5 { "ON" } else { "off" }).strong().monospace());
        } else {
            let mut on = *active;
            ui.checkbox(&mut on, "");
            *active = on;
        }
    });

    // Port 3: FM Weight (only shown when FM is active)
    if is_fm {
        ui.horizontal(|ui| {
            crate::nodes::inline_port_circle(ui, node_id, 3, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Normalized);
            ui.label(egui::RichText::new("FM Wt").small().color(egui::Color32::from_rgb(255, 200, 80)));
            if fm_wt_wired {
                ui.label(egui::RichText::new(format!("{:.2}", fm_weight)).small().color(egui::Color32::from_rgb(80, 170, 255)));
            } else {
                ui.add(egui::Slider::new(fm_depth, 0.0..=1.0).show_value(true));
                fm_weight = *fm_depth;
            }
        });
    } else {
        // Still register port position even when hidden so connections don't break
        // (but don't render it)
    }

    // Override from wired inputs (skip Freq override when FM is active)
    if !is_fm {
        if let PortValue::Float(f) = freq_in { if f > 0.0 { *frequency = f; } }
    }
    if let PortValue::Float(a) = amp_in { *amplitude = a.clamp(0.0, 1.0); }
    if let PortValue::Float(g) = gate_in { *active = g > 0.5; }

    // ── Output port: Audio (port 0, output) — right-aligned ──────────
    ui.separator();
    crate::nodes::audio_port_row(ui, "Audio", node_id, 0, false, port_positions, dragging_from, connections, pending_disconnects, PortKind::Audio);

    // ── Update audio ─────────────────────────────────────────────────
    audio.set_synth(node_id, SynthParams {
        waveform: *waveform,
        frequency: *frequency,
        amplitude: *amplitude,
        phase: 0.0,
        active: *active,
        fm_source: if is_fm { fm_source_node } else { None },
        // FM depth = frequency value * weight (0–1)
        fm_depth: if is_fm { *frequency * fm_weight } else { 0.0 },
    });
}
