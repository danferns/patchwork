use crate::audio::{AudioEffect, AudioManager};
use crate::graph::*;
use crate::icons;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    node_type: &mut NodeType,
    _values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    audio: &AudioManager,
) {
    let effects = match node_type {
        NodeType::AudioFx { effects } => effects,
        _ => return,
    };

    // Find source node by looking at what's connected to our input port 0
    let target_id: u64 = connections.iter()
        .find(|c| c.to_node == node_id && c.to_port == 0)
        .map(|c| c.from_node)
        .unwrap_or(0);

    ui.label(egui::RichText::new("Effects Chain").small().strong());

    // Add effect button
    ui.horizontal(|ui| {
        ui.label("Add:");
        if ui.small_button("Gain").clicked() {
            effects.push(AudioEffect::Gain { level: crate::audio::SmoothedParam::new(1.0, 5.0) });
        }
        if ui.small_button("LPF").clicked() {
            effects.push(AudioEffect::LowPass { cutoff: crate::audio::SmoothedParam::new(1000.0, 10.0), state: 0.0 });
        }
        if ui.small_button("HPF").clicked() {
            effects.push(AudioEffect::HighPass { cutoff: crate::audio::SmoothedParam::new(200.0, 10.0), state: 0.0 });
        }
    });
    ui.horizontal(|ui| {
        if ui.small_button("Delay").clicked() {
            effects.push(AudioEffect::Delay { time_ms: 250.0, feedback: crate::audio::SmoothedParam::new(0.4, 10.0), buffer: Vec::new(), write_pos: 0, max_delay_samples: 0 });
        }
        if ui.small_button("Dist").clicked() {
            effects.push(AudioEffect::Distortion { drive: crate::audio::SmoothedParam::new(2.0, 10.0) });
        }
    });

    if effects.is_empty() {
        ui.colored_label(egui::Color32::GRAY, "No effects — add above");
        // Clear effects for the target source
        if target_id != 0 {
            audio.set_effects(target_id, vec![]);
        }
        return;
    }

    ui.separator();

    // Render each effect with controls
    let mut remove_idx = None;
    let mut swap: Option<(usize, usize)> = None;
    let num_effects = effects.len();

    for i in 0..num_effects {
        let effect = &mut effects[i];
        ui.horizontal(|ui| {
            if i > 0 && icons::icon_button(ui, icons::CARET_UP, "Move up") {
                swap = Some((i, i - 1));
            }
            if i < num_effects - 1 && icons::icon_button(ui, icons::CARET_DOWN, "Move down") {
                swap = Some((i, i + 1));
            }
            ui.label(egui::RichText::new(format!("{}.", i + 1)).small().strong());
            ui.label(egui::RichText::new(effect.name()).strong());
            if icons::icon_button(ui, icons::TRASH, "Remove effect") {
                remove_idx = Some(i);
            }
        });

        match effect {
            AudioEffect::Gain { level } => {
                ui.horizontal(|ui| {
                    ui.label("Level:");
                    ui.add(egui::Slider::new(&mut level.target, 0.0..=2.0));
                });
            }
            AudioEffect::LowPass { cutoff, .. } => {
                ui.horizontal(|ui| {
                    ui.label("Cutoff:");
                    ui.add(egui::Slider::new(&mut cutoff.target, 20.0..=20000.0).logarithmic(true).suffix(" Hz"));
                });
            }
            AudioEffect::HighPass { cutoff, .. } => {
                ui.horizontal(|ui| {
                    ui.label("Cutoff:");
                    ui.add(egui::Slider::new(&mut cutoff.target, 20.0..=20000.0).logarithmic(true).suffix(" Hz"));
                });
            }
            AudioEffect::Delay { time_ms, feedback, .. } => {
                ui.horizontal(|ui| {
                    ui.label("Time:");
                    ui.add(egui::Slider::new(time_ms, 10.0..=2000.0).suffix(" ms"));
                });
                ui.horizontal(|ui| {
                    ui.label("Feedback:");
                    ui.add(egui::Slider::new(&mut feedback.target, 0.0..=0.95));
                });
            }
            AudioEffect::Distortion { drive } => {
                ui.horizontal(|ui| {
                    ui.label("Drive:");
                    ui.add(egui::Slider::new(&mut drive.target, 1.0..=20.0));
                });
            }
            AudioEffect::Reverb { room_size, damping, mix, .. } => {
                ui.horizontal(|ui| {
                    ui.label("Room:");
                    ui.add(egui::Slider::new(&mut room_size.target, 0.0..=1.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Damp:");
                    ui.add(egui::Slider::new(&mut damping.target, 0.0..=1.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Mix:");
                    ui.add(egui::Slider::new(&mut mix.target, 0.0..=1.0));
                });
            }
            AudioEffect::ParametricEq { bands, .. } => {
                ui.label(format!("EQ: {} bands", bands.len()));
            }
        }

        if i < num_effects - 1 {
            ui.separator();
        }
    }

    // Apply modifications
    if let Some(idx) = remove_idx {
        effects.remove(idx);
    }
    if let Some((a, b)) = swap {
        effects.swap(a, b);
    }

    // Update audio manager with effects keyed to the SOURCE node (synth), not the FX node
    // This way the audio callback applies effects to the correct source's samples
    if target_id != 0 {
        audio.set_effects(target_id, effects.clone());
    }
}
