use eframe::egui;

pub fn render(
    ui: &mut egui::Ui,
    r: &mut u8,
    g: &mut u8,
    b: &mut u8,
) {
    let mut color = egui::Color32::from_rgb(*r, *g, *b);
    ui.color_edit_button_srgba(&mut color);
    *r = color.r();
    *g = color.g();
    *b = color.b();

    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(r).range(0..=255).prefix("R "));
        ui.add(egui::DragValue::new(g).range(0..=255).prefix("G "));
        ui.add(egui::DragValue::new(b).range(0..=255).prefix("B "));
    });

    // Preview swatch
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(60.0), 20.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(*r, *g, *b));
}
