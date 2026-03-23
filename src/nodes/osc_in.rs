use crate::graph::NodeId;
use crate::osc::OscAction;
use eframe::egui;

pub fn render(
    ui: &mut egui::Ui,
    port: &mut u16,
    address_filter: &mut String,
    arg_count: &mut usize,
    last_args: &mut Vec<f32>,
    log: &mut Vec<String>,
    listening: &mut bool,
    node_id: NodeId,
    is_listening: bool,
    osc_actions: &mut Vec<OscAction>,
) {
    // Port
    ui.horizontal(|ui| {
        ui.label("Port:");
        let mut port_str = port.to_string();
        if ui.add(egui::TextEdit::singleline(&mut port_str).desired_width(60.0)).changed() {
            if let Ok(p) = port_str.parse::<u16>() { *port = p; }
        }
    });

    // Address filter
    ui.horizontal(|ui| {
        ui.label("Filter:");
        ui.add(egui::TextEdit::singleline(address_filter).desired_width(100.0));
    });
    ui.label(egui::RichText::new("(empty = all)").small().color(egui::Color32::GRAY));

    // Outputs count with +/-
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Outputs").small().strong());
        if ui.small_button("+").clicked() {
            *arg_count += 1;
            last_args.push(0.0);
        }
        if *arg_count > 0 && ui.small_button("-").clicked() {
            *arg_count -= 1;
            last_args.pop();
        }
    });

    // Show current output values
    for i in 0..*arg_count {
        let val = last_args.get(i).copied().unwrap_or(0.0);
        ui.label(egui::RichText::new(format!("  [{}]: {:.3}", i, val)).small().color(egui::Color32::from_rgb(120, 200, 120)));
    }

    // Listen toggle
    ui.separator();
    if is_listening {
        ui.colored_label(egui::Color32::from_rgb(100, 255, 100), "Listening");
        if ui.button("Stop").clicked() {
            *listening = false;
            osc_actions.push(OscAction::StopListening { node_id });
        }
    } else {
        if ui.button("Listen").clicked() && *port > 0 {
            *listening = true;
            osc_actions.push(OscAction::StartListening { node_id, port: *port });
        }
    }

    // Message log
    if !log.is_empty() {
        ui.separator();
        ui.label(egui::RichText::new("Log").small().strong());
        egui::ScrollArea::vertical()
            .max_height(100.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for msg in log.iter().rev().take(50).collect::<Vec<_>>().into_iter().rev() {
                    ui.label(egui::RichText::new(msg).small().monospace().color(egui::Color32::from_rgb(180, 180, 180)));
                }
            });
    }
}
