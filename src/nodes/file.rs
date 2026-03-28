use eframe::egui;

/// File node: opens any text file, outputs its content.
/// Compact design: file icon + filename, editable path, click to open.
pub fn render(ui: &mut egui::Ui, path: &mut String, content: &mut String) {
    // File icon + filename display
    if !path.is_empty() {
        let name = std::path::Path::new(path.as_str())
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        let ext = std::path::Path::new(path.as_str())
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();

        // Icon based on file type
        let icon = match ext.as_str() {
            "json" | "toml" | "yaml" | "yml" => crate::icons::CODE,
            "csv" | "tsv" => crate::icons::FILE_TEXT,
            "rs" | "py" | "js" | "ts" | "c" | "cpp" | "h" => crate::icons::CODE,
            "wgsl" | "glsl" | "hlsl" => crate::icons::DIAMOND_FOUR,
            _ => crate::icons::FILE_TEXT,
        };

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(icon).size(16.0));
            ui.label(egui::RichText::new(&name).strong());
            ui.label(egui::RichText::new(format!("{}c", content.len())).small().color(egui::Color32::GRAY));
        });
    } else {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(crate::icons::FILE_TEXT).size(16.0).color(egui::Color32::GRAY));
            ui.label(egui::RichText::new("No file").color(egui::Color32::GRAY));
        });
    }

    // Editable path field — type/paste a path and it auto-loads
    let old_path = path.clone();
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(path)
            .desired_width(ui.available_width() - 50.0)
            .hint_text("path or drag file...")
            .font(egui::TextStyle::Small));
        if ui.small_button(crate::icons::FOLDER_OPEN).on_hover_text("Open file").clicked() {
            if let Some(fp) = rfd::FileDialog::new()
                .add_filter("All supported", &["wgsl", "json", "csv", "txt", "toml", "yaml", "rs", "py", "glsl", "hlsl", "md", "xml", "html", "css", "js"])
                .add_filter("All files", &["*"])
                .pick_file()
            {
                *path = fp.display().to_string();
                *content = std::fs::read_to_string(&fp)
                    .unwrap_or_else(|e| format!("Error: {e}"));
            }
        }
    });

    // Auto-reload if path changed (typed, pasted, or set via MCP)
    if *path != old_path && !path.is_empty() {
        *content = std::fs::read_to_string(path.as_str())
            .unwrap_or_else(|e| format!("Error: {e}"));
    }

    // Reload button (only if file loaded)
    if !path.is_empty() {
        if ui.small_button("Reload").clicked() {
            *content = std::fs::read_to_string(path.as_str())
                .unwrap_or_else(|e| format!("Error: {e}"));
        }
    }
}
