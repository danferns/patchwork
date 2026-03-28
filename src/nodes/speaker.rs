use crate::graph::*;
use crate::audio::AudioManager;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    active: &mut bool,
    volume: &mut f32,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    audio: &AudioManager,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    pending_disconnects: &mut Vec<(NodeId, usize)>,
) {
    let has_audio = connections.iter().any(|c| c.to_node == node_id && c.to_port == 0);
    let vol_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 1);

    // Read volume from port if wired
    if vol_wired {
        *volume = Graph::static_input_value(connections, values, node_id, 1).as_float().clamp(0.0, 1.0);
    }

    // ── Large speaker icon as main identity ───────────────────────────
    let icon = if *active && has_audio {
        crate::icons::SPEAKER_HIGH
    } else {
        crate::icons::SPEAKER_X
    };

    let icon_color = if *active && has_audio {
        egui::Color32::from_rgb(80, 220, 100)
    } else if *active {
        egui::Color32::from_rgb(160, 160, 160)
    } else {
        egui::Color32::from_rgb(80, 80, 90)
    };

    // ── Title ──────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon).size(28.0).color(icon_color));
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Speaker").size(14.0).strong().color(egui::Color32::from_rgb(80, 200, 80)));
            let status = if !has_audio {
                ("No input", egui::Color32::from_rgb(100, 100, 110))
            } else if *active {
                ("Playing", egui::Color32::from_rgb(80, 220, 100))
            } else {
                ("Muted", egui::Color32::from_rgb(200, 100, 80))
            };
            ui.label(egui::RichText::new(status.0).small().color(status.1));
        });
        // Toggle on the right
        let remaining = ui.available_width() - 30.0;
        if remaining > 0.0 { ui.add_space(remaining); }
        if ui.toggle_value(active, if *active { "On" } else { "Off" }).changed() {
            // handled in app.rs
        }
    });

    ui.add_space(4.0);

    // ── Audio input port ──────────────────────────────────────────────
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 0, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Audio);
        ui.label(egui::RichText::new("Audio").small());
    });

    ui.add_space(2.0);

    // ── Volume control with input port ────────────────────────────────
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 1, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Normalized);
        ui.label(egui::RichText::new("Vol").small());
        if vol_wired {
            ui.label(egui::RichText::new(format!("{:.0}%", *volume * 100.0)).small().color(egui::Color32::from_rgb(80, 170, 255)));
        } else {
            ui.add(egui::Slider::new(volume, 0.0..=1.0).show_value(false));
            ui.label(egui::RichText::new(format!("{:.0}%", *volume * 100.0)).small());
        }
    });

    ui.add_space(4.0);
    // Speaker volume is applied as a per-chain Gain effect by build_audio_chains(),
    // not as a global master_volume. This allows multiple speakers with independent volumes.
    let _ = audio;
}

/// Walk the connection graph backward from a Speaker node to find the full audio chain.
/// Returns: Vec of NodeId from source → effects (NOT including Speaker itself).
/// Stops at audio source nodes (Synth, AudioPlayer, AudioMixer) — does NOT walk through their
/// parameter inputs (Freq, Amp, etc.), only through Audio pass-through ports.
pub fn trace_audio_chain(
    speaker_id: NodeId,
    graph: &Graph,
) -> Vec<NodeId> {
    let mut chain = Vec::new();
    let mut current = speaker_id;

    // Walk backward through "Audio" input ports (port 0 for all audio nodes)
    loop {
        // Find what's connected to current node's port 0 (Audio input)
        let source = graph.connections.iter().find(|c| c.to_node == current && c.to_port == 0);
        match source {
            Some(conn) => {
                let from_id = conn.from_node;
                if chain.contains(&from_id) { break; } // Cycle detection
                chain.push(from_id);

                // Stop at audio sources — don't walk through their parameter inputs
                let is_source = graph.nodes.get(&from_id).map(|n| {
                    matches!(n.node_type, NodeType::Synth { .. } | NodeType::AudioPlayer { .. } | NodeType::AudioMixer { .. } | NodeType::AudioInput { .. })
                }).unwrap_or(false);
                if is_source { break; }

                current = from_id;
            }
            None => break, // No more sources
        }
    }

    chain.reverse(); // Source first, effects in order
    chain
}
