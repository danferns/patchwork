use eframe::egui;

pub fn render(ui: &mut egui::Ui, x: f32, y: f32) {
    ui.label(format!("X: {:.1}", x));
    ui.label(format!("Y: {:.1}", y));
    ui.label(egui::RichText::new("(tracks pointer position)").small().color(egui::Color32::GRAY));
}
