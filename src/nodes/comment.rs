use eframe::egui;

pub fn render(ui: &mut egui::Ui, text: &mut String) {
    ui.add(
        egui::TextEdit::multiline(text)
            .desired_width(f32::INFINITY)
            .desired_rows(3),
    );
}
