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
) {
    let (waveform, frequency, amplitude, active) = match node_type {
        NodeType::Synth { waveform, frequency, amplitude, active } => (waveform, frequency, amplitude, active),
        _ => return,
    };

    // Read from input ports if connected (Freq=0, Amp=1, Gate=2)
    let freq_in = Graph::static_input_value(connections, values, node_id, 0);
    let amp_in = Graph::static_input_value(connections, values, node_id, 1);
    let gate_in = Graph::static_input_value(connections, values, node_id, 2);

    if let PortValue::Float(f) = freq_in { if f > 0.0 { *frequency = f; } }
    if let PortValue::Float(a) = amp_in { *amplitude = a.clamp(0.0, 1.0); }
    if let PortValue::Float(g) = gate_in { *active = g > 0.5; }

    // Waveform selector
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

    // Waveform visualization
    let viz_h = 30.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(200.0), viz_h), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(15, 15, 25));
    let n = 80;
    let points: Vec<egui::Pos2> = (0..=n).map(|i| {
        let t = i as f32 / n as f32;
        let y = waveform.sample(t) * *amplitude;
        egui::pos2(
            rect.left() + t * rect.width(),
            rect.center().y - y * viz_h * 0.4,
        )
    }).collect();
    let color = if *active {
        egui::Color32::from_rgb(80, 200, 120)
    } else {
        egui::Color32::from_rgb(80, 80, 80)
    };
    for w in points.windows(2) {
        painter.line_segment([w[0], w[1]], egui::Stroke::new(1.5, color));
    }

    // Frequency
    ui.horizontal(|ui| {
        ui.label("Freq:");
        ui.add(egui::DragValue::new(frequency).speed(1.0).range(20.0..=20000.0).suffix(" Hz"));
    });

    // Amplitude
    ui.horizontal(|ui| {
        ui.label("Amp:");
        ui.add(egui::Slider::new(amplitude, 0.0..=1.0));
    });

    // Active toggle
    ui.horizontal(|ui| {
        ui.checkbox(active, "Active");
    });

    // Update audio manager with current params
    audio.set_synth(node_id, SynthParams {
        waveform: *waveform,
        frequency: *frequency,
        amplitude: *amplitude,
        phase: 0.0, // phase is tracked by audio thread
        active: *active,
    });
}
