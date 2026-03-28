use crate::audio::AudioManager;
use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

/// Walk backward from a node's Audio input (port 0) through effect nodes
/// to find the actual audio source (Synth, AudioPlayer, AudioMixer).
pub fn trace_audio_source(node_id: NodeId, connections: &[Connection], nodes: &HashMap<NodeId, Node>) -> Option<NodeId> {
    let conn = connections.iter().find(|c| c.to_node == node_id && c.to_port == 0)?;
    let mut current = conn.from_node;
    for _ in 0..20 {
        let is_source = nodes.get(&current).map(|n| {
            matches!(n.node_type,
                NodeType::Synth { .. } | NodeType::AudioPlayer { .. } | NodeType::AudioMixer { .. }
            )
        }).unwrap_or(false);
        if is_source { return Some(current); }
        match connections.iter().find(|c| c.to_node == current && c.to_port == 0) {
            Some(c) => current = c.from_node,
            None => return None,
        }
    }
    None
}

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    _audio: &AudioManager,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    connections: &[Connection],
    pending_disconnects: &mut Vec<(NodeId, usize)>,
) {
    let _ = values;

    // Audio input port
    crate::nodes::inline_port_circle(
        ui, node_id, 0, true, connections,
        port_positions, dragging_from, pending_disconnects, PortKind::Audio,
    );

    // Read analysis stored by app/mod.rs (which does the source tracing)
    let (amp, peak, bass, mid, treble, source_name) = ui.ctx().data_mut(|d| {
        let vals: [f32; 5] = d.get_temp(egui::Id::new(("audio_analysis", node_id))).unwrap_or([0.0; 5]);
        let name: String = d.get_temp(egui::Id::new(("audio_analysis_source", node_id))).unwrap_or_default();
        (vals[0], vals[1], vals[2], vals[3], vals[4], name)
    });

    // Connection status
    if source_name.is_empty() {
        ui.label(egui::RichText::new("Master mix").small().color(egui::Color32::GRAY));
    } else {
        ui.label(egui::RichText::new(format!("Source: {}", source_name)).small().color(egui::Color32::from_rgb(80, 200, 120)));
    }

    // ── Level meters ──────────────────────────────────────────────────
    let bar_w = 110.0;
    let bar_h = 8.0;

    let mut meter = |ui: &mut egui::Ui, label: &str, value: f32, color: egui::Color32, port: usize| {
        ui.horizontal(|ui| {
            crate::nodes::inline_port_circle(
                ui, node_id, port, false, connections,
                port_positions, dragging_from, pending_disconnects, PortKind::Normalized,
            );
            let (rect, _) = ui.allocate_exact_size(egui::vec2(bar_w, bar_h), egui::Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(25, 25, 30));
            let fill_w = rect.width() * value.clamp(0.0, 1.0);
            if fill_w > 0.5 {
                let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, rect.height()));
                painter.rect_filled(fill_rect, 2.0, color);
            }
            ui.label(egui::RichText::new(format!("{} {:.0}%", label, value * 100.0)).small().monospace());
        });
    };

    meter(ui, "Amp", amp, egui::Color32::from_rgb(80, 200, 120), 0);
    meter(ui, "Peak", peak, egui::Color32::from_rgb(255, 200, 60), 1);
    meter(ui, "Bass", bass, egui::Color32::from_rgb(255, 80, 80), 2);
    meter(ui, "Mid", mid, egui::Color32::from_rgb(80, 160, 255), 3);
    meter(ui, "Treble", treble, egui::Color32::from_rgb(200, 120, 255), 4);

    if amp > 0.001 {
        ui.ctx().request_repaint();
    }
}
