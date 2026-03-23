use eframe::egui;

pub fn render(
    ui: &mut egui::Ui,
    elapsed: &mut f32,
    speed: &mut f32,
    running: &mut bool,
) {
    ui.horizontal(|ui| {
        if ui.button(if *running { "⏸" } else { "▶" }).clicked() {
            *running = !*running;
        }
        if ui.button("Reset").clicked() {
            *elapsed = 0.0;
        }
    });

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Speed").small());
        ui.add(egui::Slider::new(speed, 0.0..=10.0).step_by(0.1));
    });

    ui.label(format!("{:.2}s", elapsed));
}
