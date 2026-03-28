use crate::audio::AudioManager;
use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

const WAVEFORM_HEIGHT: f32 = 45.0;
const WHEEL_SIZE: f32 = 36.0;

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
    let (file_path, volume, looping, duration_secs) = match node_type {
        NodeType::AudioPlayer { file_path, volume, looping, duration_secs } => (file_path, volume, looping, duration_secs),
        _ => return,
    };

    let is_playing = audio.file_playing.get(&node_id).copied().unwrap_or(false);
    let is_paused = audio.is_file_paused(node_id);

    // Auto-detect duration when file is loaded but duration unknown
    if !file_path.is_empty() && *duration_secs <= 0.0 {
        let cached = audio.get_file_duration(node_id);
        if cached > 0.0 {
            *duration_secs = cached;
        } else {
            // Use Symphonia to probe file for duration (fast metadata read)
            if let Some(dur) = crate::audio::probe_file_duration(file_path) {
                *duration_secs = dur;
            }
        }
    }

    let duration = *duration_secs;

    // Keep looping flag in sync with decode thread
    audio.set_file_looping(node_id, *looping);

    // Detect end of playback
    if is_playing && !is_paused && audio.is_file_finished(node_id) {
        if *looping && !file_path.is_empty() {
            // Looping is handled by the decode thread — just restart
            audio.stop_file(node_id);
            let _ = audio.play_file(node_id, file_path);
        } else {
            audio.stop_file(node_id);
        }
    }

    // Re-read state after potential auto-stop
    let is_playing = audio.file_playing.get(&node_id).copied().unwrap_or(false);
    let is_paused = audio.is_file_paused(node_id);

    // Sample-accurate playback position from FilePlayerBuffer atomic counter
    let playback_pos = if is_playing || is_paused {
        audio.get_playback_position(node_id)
    } else {
        0.0
    };

    // Read from input ports if connected
    // Port 0: Play (>0.5 = play, <=0.5 = pause)
    let play_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == 0);
    if play_wired {
        let play_val = Graph::static_input_value(connections, values, node_id, 0).as_float();
        if play_val > 0.5 && !is_playing && !file_path.is_empty() {
            let _ = audio.play_file(node_id, file_path);
        } else if play_val <= 0.5 && is_playing {
            audio.pause_file(node_id);
        }
    }
    // Port 1: Volume
    if connections.iter().any(|c| c.to_node == node_id && c.to_port == 1) {
        *volume = Graph::static_input_value(connections, values, node_id, 1).as_float().clamp(0.0, 1.0);
        audio.set_file_volume(node_id, *volume);
    }

    // ── Input ports ──────────────────────────────────────────────
    ui.horizontal(|ui| {
        crate::nodes::inline_port_circle(ui, node_id, 0, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Trigger);
        ui.label(egui::RichText::new("Play").small());
        ui.add_space(8.0);
        crate::nodes::inline_port_circle(ui, node_id, 1, true, connections, port_positions, dragging_from, pending_disconnects, PortKind::Normalized);
        ui.label(egui::RichText::new("Vol").small());
    });

    // ── Empty state ──────────────────────────────────────────────
    if file_path.is_empty() {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 50.0), egui::Sense::click());
        let painter = ui.painter();
        painter.rect_filled(rect, 6.0, egui::Color32::from_rgb(35, 35, 40));
        painter.text(rect.center(), egui::Align2::CENTER_CENTER,
            format!("{} Drop audio file", crate::icons::MUSIC_NOTE),
            egui::FontId::proportional(12.0), egui::Color32::from_rgb(100, 100, 110));
        if resp.clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Audio", &["wav", "mp3", "ogg", "flac", "aac", "m4a"])
                .pick_file() {
                *file_path = path.to_string_lossy().to_string();
            }
        }
        // Output port
        crate::nodes::audio_port_row(ui, "Audio", node_id, 0, false, port_positions, dragging_from, connections, pending_disconnects, PortKind::Audio);
        return;
    }

    // ── Waveform ─────────────────────────────────────────────────
    ui.set_max_width(220.0);
    let w = 220.0_f32.min(ui.available_width().max(100.0));
    let (wf_rect, wf_response) = ui.allocate_exact_size(egui::vec2(w, WAVEFORM_HEIGHT), egui::Sense::click_and_drag());
    let painter = ui.painter();
    painter.rect_filled(wf_rect, 4.0, egui::Color32::from_rgb(25, 25, 30));

    // Click/drag on waveform = seek
    if (wf_response.clicked() || wf_response.dragged()) && duration > 0.0 {
        if let Some(pos) = wf_response.interact_pointer_pos() {
            let seek_t = ((pos.x - wf_rect.left()) / wf_rect.width()).clamp(0.0, 1.0);
            let new_pos = seek_t as f64 * duration;
            if is_playing || is_paused {
                let _ = audio.seek_file(node_id, file_path, new_pos);
            }
        }
    }

    // Waveform bars
    {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        file_path.hash(&mut h);
        let seed = h.finish();
        let num_bars = 50;
        let bar_w = wf_rect.width() / num_bars as f32;
        let center_y = wf_rect.center().y;
        let progress = if duration > 0.0 { (playback_pos as f32 / duration as f32).clamp(0.0, 1.0) } else { 0.0 };

        for i in 0..num_bars {
            let t = i as f64 / num_bars as f64;
            let v = ((seed as f64 * 0.0001 + t * 13.7).sin() * 0.5 + 0.5) as f32
                * ((seed as f64 * 0.0003 + t * 7.3).sin() * 0.3 + 0.5) as f32;
            let half_h = v.clamp(0.05, 0.9) * (WAVEFORM_HEIGHT * 0.4);
            let x = wf_rect.left() + i as f32 * bar_w;
            let bar_progress = i as f32 / num_bars as f32;
            let wf_color = if bar_progress <= progress {
                egui::Color32::from_rgb(80, 200, 120)
            } else {
                egui::Color32::from_rgb(50, 60, 65)
            };
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(x + bar_w * 0.5, center_y), egui::vec2(bar_w * 0.5, half_h * 2.0)),
                1.0, wf_color);
        }

        // Playhead
        {
            let px = wf_rect.left() + progress * wf_rect.width();
            let ph_color = egui::Color32::from_rgb(255, 60, 60);
            painter.line_segment(
                [egui::pos2(px, wf_rect.top()), egui::pos2(px, wf_rect.bottom())],
                egui::Stroke::new(2.5, ph_color));
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(px - 4.0, wf_rect.top()),
                    egui::pos2(px + 4.0, wf_rect.top()),
                    egui::pos2(px, wf_rect.top() + 6.0),
                ],
                ph_color,
                egui::Stroke::NONE,
            ));
        }
    }

    // ── Time display ─────────────────────────────────────────────
    ui.horizontal(|ui| {
        let current_secs = playback_pos as f32;
        let total_secs = duration as f32;
        let fmt_time = |s: f32| -> String {
            let m = (s / 60.0).floor() as i32;
            let sec = s % 60.0;
            format!("{}:{:05.2}", m, sec)
        };
        ui.label(egui::RichText::new(fmt_time(current_secs)).small().monospace().color(egui::Color32::from_rgb(255, 60, 60)));
        ui.label(egui::RichText::new("/").small().color(egui::Color32::from_rgb(80, 80, 85)));
        ui.label(egui::RichText::new(fmt_time(total_secs)).small().monospace().color(egui::Color32::from_rgb(120, 120, 130)));
    });

    // ── Filename ─────────────────────────────────────────────────
    let name = std::path::Path::new(file_path.as_str())
        .file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| file_path.clone());
    ui.label(egui::RichText::new(&name).small().monospace().color(egui::Color32::from_rgb(160, 160, 170)));

    // ── Transport + wheel ────────────────────────────────────────
    ui.horizontal(|ui| {
        // DJ wheel
        let (wheel_rect, _) = ui.allocate_exact_size(egui::vec2(WHEEL_SIZE, WHEEL_SIZE), egui::Sense::hover());
        let center = wheel_rect.center();
        let radius = WHEEL_SIZE * 0.44;
        let wp = ui.painter().clone();
        wp.circle_filled(center, radius, egui::Color32::from_rgb(30, 30, 35));
        wp.circle_stroke(center, radius, egui::Stroke::new(1.5, egui::Color32::from_rgb(55, 55, 65)));
        wp.circle_filled(center, radius * 0.18, egui::Color32::from_rgb(50, 50, 55));
        let angle = playback_pos as f32 * 2.0;
        let notch = egui::pos2(center.x + angle.cos() * radius * 0.7, center.y + angle.sin() * radius * 0.7);
        wp.line_segment([center, notch], egui::Stroke::new(1.5, egui::Color32::from_rgb(180, 180, 190)));
        for i in 1..3 {
            wp.circle_stroke(center, radius * (0.35 + i as f32 * 0.18), egui::Stroke::new(0.5, egui::Color32::from_rgb(42, 42, 48)));
        }

        // Buttons
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let icon = if is_playing { crate::icons::PAUSE } else { crate::icons::PLAY };
                let accent = ui.ctx().data_mut(|d| d.get_temp::<[u8; 3]>(egui::Id::new("theme_accent")))
                    .unwrap_or([80, 160, 255]);
                let btn = if !is_playing {
                    egui::Button::new(egui::RichText::new(icon).size(14.0).color(egui::Color32::WHITE))
                        .fill(egui::Color32::from_rgb(accent[0], accent[1], accent[2]))
                        .min_size(egui::vec2(28.0, 22.0))
                } else {
                    egui::Button::new(egui::RichText::new(icon).size(14.0))
                        .min_size(egui::vec2(28.0, 22.0))
                };
                if ui.add(btn).clicked() {
                    if is_playing {
                        audio.pause_file(node_id);
                    } else {
                        let _ = audio.play_file(node_id, file_path);
                        let dur = audio.get_file_duration(node_id);
                        if dur > 0.0 { *duration_secs = dur; }
                    }
                }
                // Stop
                if ui.add(egui::Button::new(egui::RichText::new(crate::icons::STOP).size(14.0)).min_size(egui::vec2(26.0, 22.0))).clicked() {
                    audio.stop_file(node_id);
                }
                // Loop
                let lc = if *looping { egui::Color32::from_rgb(80, 170, 255) } else { egui::Color32::GRAY };
                if ui.add(egui::Button::new(egui::RichText::new("↻").size(12.0).color(lc)).min_size(egui::vec2(22.0, 22.0))).clicked() {
                    *looping = !*looping;
                    audio.set_file_looping(node_id, *looping);
                }
                // Open
                if ui.add(egui::Button::new(egui::RichText::new(crate::icons::FOLDER_OPEN).size(12.0)).min_size(egui::vec2(22.0, 22.0))).clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("Audio", &["wav", "mp3", "ogg", "flac"]).pick_file() {
                        audio.stop_file(node_id);
                        *file_path = path.to_string_lossy().to_string();
                        *duration_secs = 0.0; // will be re-probed next frame
                        audio.file_durations.remove(&node_id);
                    }
                }
            });
            // Volume
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(crate::icons::SPEAKER_HIGH).size(10.0));
                if ui.add(egui::Slider::new(volume, 0.0..=1.0).show_value(false)).changed() {
                    audio.set_file_volume(node_id, *volume);
                }
                ui.label(egui::RichText::new(format!("{:.0}%", *volume * 100.0)).small().color(egui::Color32::GRAY));
            });
        });
    });

    // ── Output port ──────────────────────────────────────────────
    crate::nodes::audio_port_row(ui, "Audio", node_id, 0, false, port_positions, dragging_from, connections, pending_disconnects, PortKind::Audio);

    // Output: progress (for downstream nodes that want normalized position)
    // Port 1 output = progress 0..1
    ui.horizontal(|ui| {
        let progress = if duration > 0.0 { (playback_pos / duration).clamp(0.0, 1.0) as f32 } else { 0.0 };
        ui.label(egui::RichText::new(format!("Progress: {:.1}%", progress * 100.0)).small().color(egui::Color32::GRAY));
        crate::nodes::inline_port_circle(ui, node_id, 1, false, connections, port_positions, dragging_from, pending_disconnects, PortKind::Normalized);
    });

    if is_playing { ui.ctx().request_repaint(); }
}
