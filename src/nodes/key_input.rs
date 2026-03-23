use eframe::egui;

/// Maps a user-typed key name to an egui::Key
pub fn parse_key(name: &str) -> Option<egui::Key> {
    match name.to_lowercase().trim() {
        "a" => Some(egui::Key::A), "b" => Some(egui::Key::B), "c" => Some(egui::Key::C),
        "d" => Some(egui::Key::D), "e" => Some(egui::Key::E), "f" => Some(egui::Key::F),
        "g" => Some(egui::Key::G), "h" => Some(egui::Key::H), "i" => Some(egui::Key::I),
        "j" => Some(egui::Key::J), "k" => Some(egui::Key::K), "l" => Some(egui::Key::L),
        "m" => Some(egui::Key::M), "n" => Some(egui::Key::N), "o" => Some(egui::Key::O),
        "p" => Some(egui::Key::P), "q" => Some(egui::Key::Q), "r" => Some(egui::Key::R),
        "s" => Some(egui::Key::S), "t" => Some(egui::Key::T), "u" => Some(egui::Key::U),
        "v" => Some(egui::Key::V), "w" => Some(egui::Key::W), "x" => Some(egui::Key::X),
        "y" => Some(egui::Key::Y), "z" => Some(egui::Key::Z),
        "0" => Some(egui::Key::Num0), "1" => Some(egui::Key::Num1), "2" => Some(egui::Key::Num2),
        "3" => Some(egui::Key::Num3), "4" => Some(egui::Key::Num4), "5" => Some(egui::Key::Num5),
        "6" => Some(egui::Key::Num6), "7" => Some(egui::Key::Num7), "8" => Some(egui::Key::Num8),
        "9" => Some(egui::Key::Num9),
        "space" => Some(egui::Key::Space),
        "enter" | "return" => Some(egui::Key::Enter),
        "tab" => Some(egui::Key::Tab),
        "up" | "arrowup" => Some(egui::Key::ArrowUp),
        "down" | "arrowdown" => Some(egui::Key::ArrowDown),
        "left" | "arrowleft" => Some(egui::Key::ArrowLeft),
        "right" | "arrowright" => Some(egui::Key::ArrowRight),
        _ => None,
    }
}

pub fn render(
    ui: &mut egui::Ui,
    key_name: &mut String,
    pressed: &mut bool,
    toggle_mode: &mut bool,
    toggled_on: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label("Key:");
        ui.add(egui::TextEdit::singleline(key_name).desired_width(50.0).hint_text("e.g. A"));
    });

    let valid = parse_key(key_name).is_some();
    if !valid && !key_name.is_empty() {
        ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "Unknown key");
    }

    ui.checkbox(toggle_mode, "Toggle mode");
    if *toggle_mode {
        ui.label(egui::RichText::new("Press key to toggle on/off").small().color(egui::Color32::GRAY));
    }

    ui.separator();

    // Status display
    let (status_text, status_color) = if *pressed {
        ("PRESSED", egui::Color32::from_rgb(100, 255, 100))
    } else {
        ("—", egui::Color32::from_rgb(100, 100, 100))
    };
    ui.horizontal(|ui| {
        ui.label("State:");
        ui.colored_label(status_color, status_text);
    });

    if *toggle_mode {
        let toggle_color = if *toggled_on {
            egui::Color32::from_rgb(100, 200, 255)
        } else {
            egui::Color32::from_rgb(80, 80, 80)
        };
        ui.horizontal(|ui| {
            ui.label("Toggle:");
            ui.colored_label(toggle_color, if *toggled_on { "ON" } else { "OFF" });
        });
    }
}
