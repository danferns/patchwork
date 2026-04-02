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
    let has_l = connections.iter().any(|c| c.to_node == node_id && c.to_port == 0);
    let has_r = connections.iter().any(|c| c.to_node == node_id && c.to_port == 1);
    let has_audio = has_l || has_r;
    let vol_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 2);

    // Read volume from port if wired
    if vol_wired {
        *volume = Graph::static_input_value(connections, values, node_id, 2).as_float().clamp(0.0, 1.0);
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
            let mode = if has_l && has_r { "Stereo" } else if has_l { "Mono" } else { "No input" };
            let status = if !has_audio {
                (mode, egui::Color32::from_rgb(100, 100, 110))
            } else if *active {
                (mode, egui::Color32::from_rgb(80, 220, 100))
            } else {
                ("Muted", egui::Color32::from_rgb(200, 100, 80))
            };
            ui.label(egui::RichText::new(status.0).small().color(status.1));
        });
        let remaining = ui.available_width() - 30.0;
        if remaining > 0.0 { ui.add_space(remaining); }
        ui.toggle_value(active, if *active { "On" } else { "Off" });
    });

    ui.add_space(4.0);

    // ── Audio input ports ────────────────────────────────────────────
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 0, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Audio);
        ui.label(egui::RichText::new("L / Mono").small());
    });
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 1, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Audio);
        ui.label(egui::RichText::new("R").small().color(if has_r { egui::Color32::from_rgb(80, 200, 120) } else { egui::Color32::GRAY }));
    });

    ui.add_space(2.0);

    // ── Volume control ───────────────────────────────────────────────
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 2, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Normalized);
        ui.label(egui::RichText::new("Vol").small());
        if vol_wired {
            ui.label(egui::RichText::new(format!("{:.0}%", *volume * 100.0)).small().color(egui::Color32::from_rgb(80, 170, 255)));
        } else {
            ui.add(egui::Slider::new(volume, 0.0..=1.0).show_value(false));
            ui.label(egui::RichText::new(format!("{:.0}%", *volume * 100.0)).small());
        }
    });

    ui.add_space(4.0);

    // Update engine params
    audio.engine_write_param(node_id, 0, *volume);
    audio.engine_write_param(node_id, 1, if *active { 1.0 } else { 0.0 });
}
