use eframe::egui;

/// Theme node: global controls for the app's visual style.
/// Only one Theme node's settings are applied (the first found).
pub fn render(
    ui: &mut egui::Ui,
    dark_mode: &mut bool,
    accent: &mut [u8; 3],
    font_size: &mut f32,
) {
    ui.horizontal(|ui| {
        ui.label("Mode:");
        if ui.selectable_label(*dark_mode, "Dark").clicked() {
            *dark_mode = true;
        }
        if ui.selectable_label(!*dark_mode, "Light").clicked() {
            *dark_mode = false;
        }
    });

    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Accent:");
        let mut color = egui::Color32::from_rgb(accent[0], accent[1], accent[2]);
        if ui.color_edit_button_srgba(&mut color).changed() {
            *accent = [color.r(), color.g(), color.b()];
        }
    });

    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Font size:");
        ui.add(egui::Slider::new(font_size, 10.0..=24.0).suffix("px"));
    });
}

/// Apply theme settings to the egui context. Called from app.rs each frame.
pub fn apply(ctx: &egui::Context, dark_mode: bool, accent: [u8; 3], font_size: f32) {
    let mut visuals = if dark_mode {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    let accent_color = egui::Color32::from_rgb(accent[0], accent[1], accent[2]);
    visuals.selection.bg_fill = accent_color;
    visuals.hyperlink_color = accent_color;
    visuals.widgets.hovered.bg_fill = accent_color.gamma_multiply(0.15);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    for (_text_style, font_id) in style.text_styles.iter_mut() {
        font_id.size = font_size;
    }
    ctx.set_style(style);
}
