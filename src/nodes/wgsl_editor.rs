use eframe::egui;

pub const DEFAULT_WGSL: &str = "\
@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4f {
    var pos = array<vec2f, 3>(
        vec2f( 0.0,  0.5),
        vec2f(-0.5, -0.5),
        vec2f( 0.5, -0.5),
    );
    return vec4f(pos[idx], 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4f {
    return vec4f(1.0, 0.4, 0.1, 1.0);
}
";

pub fn render(ui: &mut egui::Ui, code: &mut String, path: &mut Option<String>) {
    ui.horizontal(|ui| {
        if ui.button("Open...").clicked() {
            if let Some(fp) = rfd::FileDialog::new()
                .add_filter("WGSL", &["wgsl"])
                .add_filter("All", &["*"])
                .pick_file()
            {
                *path = Some(fp.display().to_string());
                *code = std::fs::read_to_string(&fp).unwrap_or_else(|e| format!("// Error: {e}"));
            }
        }
        if ui.button("Save").clicked() {
            if let Some(p) = path.as_ref() {
                let _ = std::fs::write(p, code.as_str());
            } else if let Some(fp) = rfd::FileDialog::new()
                .add_filter("WGSL", &["wgsl"])
                .save_file()
            {
                *path = Some(fp.display().to_string());
                let _ = std::fs::write(&fp, code.as_str());
            }
        }
    });
    if let Some(p) = path.as_ref() {
        ui.label(egui::RichText::new(p.as_str()).small().color(egui::Color32::GRAY));
    }
    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
        ui.add(
            egui::TextEdit::multiline(code)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .desired_rows(15),
        );
    });
}
