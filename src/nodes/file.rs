use eframe::egui;

/// File node: opens any file, outputs its text content.
/// No editing — just a source. Connect to TextEditor or WgslViewer.
pub fn render(ui: &mut egui::Ui, path: &mut String, content: &mut String) {
    ui.horizontal(|ui| {
        if ui.button("Open...").clicked() {
            if let Some(fp) = rfd::FileDialog::new()
                .add_filter("All supported", &["wgsl", "json", "csv", "txt", "toml", "yaml", "rs", "py", "glsl", "hlsl"])
                .add_filter("All files", &["*"])
                .pick_file()
            {
                *path = fp.display().to_string();
                *content = std::fs::read_to_string(&fp)
                    .unwrap_or_else(|e| format!("Error: {e}"));
            }
        }
        if !path.is_empty() && ui.button("Reload").clicked() {
            *content = std::fs::read_to_string(path.as_str())
                .unwrap_or_else(|e| format!("Error: {e}"));
        }
    });

    if !path.is_empty() {
        // Show just the filename, not full path
        let name = std::path::Path::new(path.as_str())
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        ui.label(egui::RichText::new(name).strong());
        ui.label(
            egui::RichText::new(format!("{} chars", content.len()))
                .small()
                .color(egui::Color32::GRAY),
        );
    } else {
        ui.label(
            egui::RichText::new("No file loaded\nDrag & drop or click Open")
                .small()
                .color(egui::Color32::GRAY),
        );
    }
}
