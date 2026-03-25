use crate::audio::AudioManager;
use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    node_type: &mut NodeType,
    _values: &HashMap<(NodeId, usize), PortValue>,
    _connections: &[Connection],
    audio: &mut AudioManager,
) {
    let (file_path, volume, looping) = match node_type {
        NodeType::AudioPlayer { file_path, volume, looping } => (file_path, volume, looping),
        _ => return,
    };

    let is_playing = audio.file_playing.get(&node_id).copied().unwrap_or(false);

    // File selector
    ui.horizontal(|ui| {
        if ui.button("Open...").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Audio", &["wav", "mp3", "ogg", "flac"])
                .pick_file()
            {
                *file_path = path.to_string_lossy().to_string();
            }
        }
        if !file_path.is_empty() {
            let name = std::path::Path::new(file_path.as_str())
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| file_path.clone());
            ui.label(egui::RichText::new(name).small().monospace());
        }
    });

    if file_path.is_empty() {
        ui.colored_label(egui::Color32::GRAY, "No file loaded");
        return;
    }

    // Transport controls
    ui.horizontal(|ui| {
        if is_playing {
            if ui.button("⏸ Pause").clicked() {
                audio.toggle_file(node_id);
            }
            if ui.button("⏹ Stop").clicked() {
                audio.stop_file(node_id);
            }
            ui.colored_label(egui::Color32::from_rgb(80, 200, 80), "▶ Playing");
        } else {
            if ui.button("▶ Play").clicked() {
                if let Err(e) = audio.play_file(node_id, file_path) {
                    ui.colored_label(egui::Color32::RED, format!("Error: {}", e));
                }
            }
        }
    });

    // Volume
    ui.horizontal(|ui| {
        ui.label("Vol:");
        if ui.add(egui::Slider::new(volume, 0.0..=1.0)).changed() {
            if let Some(sink) = &audio.rodio_sink {
                sink.set_volume(*volume);
            }
        }
    });

    // Loop toggle
    ui.checkbox(looping, "Loop");
}
