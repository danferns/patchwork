use eframe::egui;

pub fn render(ui: &mut egui::Ui, path: &mut String, content: &mut String) {
    ui.horizontal(|ui| {
        if ui.button("Open...").clicked() {
            if let Some(fp) = rfd::FileDialog::new()
                .add_filter("Text", &["json", "csv", "txt", "toml", "yaml", "wgsl", "rs", "py"])
                .pick_file()
            {
                *path = fp.display().to_string();
                *content = std::fs::read_to_string(&fp).unwrap_or_else(|e| format!("Error: {e}"));
            }
        }
        if !path.is_empty() && ui.button("Save").clicked() {
            let _ = std::fs::write(path.as_str(), content.as_str());
        }
    });
    if !path.is_empty() {
        ui.label(egui::RichText::new(path.as_str()).small().color(egui::Color32::GRAY));
    }
    egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
        ui.add(
            egui::TextEdit::multiline(content)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .desired_rows(10),
        );
    });
}
