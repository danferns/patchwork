use eframe::egui;

pub fn render(ui: &mut egui::Ui, value: &mut f32, min: &mut f32, max: &mut f32) {
    ui.add(egui::Slider::new(value, *min..=*max).text("Value"));
    ui.horizontal(|ui| {
        ui.label("Range:");
        ui.add(egui::DragValue::new(min).speed(0.1).prefix("min "));
        ui.add(egui::DragValue::new(max).speed(0.1).prefix("max "));
    });
}
