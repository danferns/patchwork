use crate::audio::AudioManager;
use crate::graph::*;
use eframe::egui;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    node_type: &mut NodeType,
    audio: &mut AudioManager,
) {
    let (selected_output, selected_input, master_volume) = match node_type {
        NodeType::AudioDevice { selected_output, selected_input, master_volume } =>
            (selected_output, selected_input, master_volume),
        _ => return,
    };

    let is_running = audio.is_running();

    // Output device (use cached list — refreshed every ~60 frames by app.rs)
    ui.label(egui::RichText::new("Output").small().strong());
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(egui::Id::new(("audio_out", node_id)))
            .selected_text(if selected_output.is_empty() { "Default" } else { selected_output.as_str() })
            .width(140.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(selected_output.is_empty(), "Default").clicked() {
                    selected_output.clear();
                }
                for d in &audio.cached_output_devices {
                    if ui.selectable_label(selected_output == d, d).clicked() {
                        *selected_output = d.clone();
                    }
                }
            });
    });

    // Start/Stop
    ui.horizontal(|ui| {
        if is_running {
            ui.colored_label(egui::Color32::from_rgb(80, 200, 80), "● Running");
            if ui.button("Stop").clicked() {
                audio.stop_output();
            }
        } else {
            ui.colored_label(egui::Color32::from_rgb(150, 150, 150), "○ Stopped");
            if ui.button("▶ Start").clicked() {
                let dev = if selected_output.is_empty() { None } else { Some(selected_output.as_str()) };
                match audio.start_output(dev) {
                    Ok(()) => {}
                    Err(e) => {
                        ui.colored_label(egui::Color32::RED, format!("Error: {}", e));
                    }
                }
            }
        }
    });

    ui.separator();

    // Master volume
    ui.horizontal(|ui| {
        ui.label("Master:");
        if ui.add(egui::Slider::new(master_volume, 0.0..=1.0)).changed() {
            if let Ok(mut s) = audio.state.lock() {
                s.master_volume = *master_volume;
            }
        }
    });

    // Sample rate display
    if let Ok(s) = audio.state.lock() {
        ui.label(egui::RichText::new(format!("{}Hz", s.sample_rate as u32)).small().color(egui::Color32::GRAY));
    }

    ui.separator();

    // Input device
    ui.label(egui::RichText::new("Input").small().strong());
    egui::ComboBox::from_id_salt(egui::Id::new(("audio_in", node_id)))
        .selected_text(if selected_input.is_empty() { "None" } else { selected_input.as_str() })
        .width(140.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(selected_input.is_empty(), "None").clicked() {
                selected_input.clear();
            }
            for d in &audio.cached_input_devices {
                if ui.selectable_label(selected_input == d, d).clicked() {
                    *selected_input = d.clone();
                }
            }
        });
}
