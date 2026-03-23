use eframe::egui;

pub fn render(
    ui: &mut egui::Ui,
    messages: &mut Vec<String>,
) {
    ui.horizontal(|ui| {
        if ui.button("Clear").clicked() {
            messages.clear();
        }
        ui.label(format!("{} messages", messages.len()));
    });

    ui.separator();

    // Scrollable message area
    egui::ScrollArea::vertical()
        .max_height(150.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for msg in messages.iter() {
                // Color-code based on message type
                let color = if msg.contains("error") || msg.contains("Error") {
                    egui::Color32::from_rgb(255, 100, 100)
                } else if msg.contains("warning") || msg.contains("Warning") {
                    egui::Color32::from_rgb(255, 200, 100)
                } else if msg.contains("✓") || msg.contains("success") {
                    egui::Color32::from_rgb(100, 255, 100)
                } else {
                    egui::Color32::from_rgb(200, 200, 200)
                };

                ui.label(egui::RichText::new(msg).color(color).monospace());
            }
        });
}
