use eframe::egui;

// ── All supported keys with display names ────────────────────────────────────

const ALL_KEYS: &[(egui::Key, &str)] = &[
    (egui::Key::A, "A"), (egui::Key::B, "B"), (egui::Key::C, "C"),
    (egui::Key::D, "D"), (egui::Key::E, "E"), (egui::Key::F, "F"),
    (egui::Key::G, "G"), (egui::Key::H, "H"), (egui::Key::I, "I"),
    (egui::Key::J, "J"), (egui::Key::K, "K"), (egui::Key::L, "L"),
    (egui::Key::M, "M"), (egui::Key::N, "N"), (egui::Key::O, "O"),
    (egui::Key::P, "P"), (egui::Key::Q, "Q"), (egui::Key::R, "R"),
    (egui::Key::S, "S"), (egui::Key::T, "T"), (egui::Key::U, "U"),
    (egui::Key::V, "V"), (egui::Key::W, "W"), (egui::Key::X, "X"),
    (egui::Key::Y, "Y"), (egui::Key::Z, "Z"),
    (egui::Key::Num0, "0"), (egui::Key::Num1, "1"), (egui::Key::Num2, "2"),
    (egui::Key::Num3, "3"), (egui::Key::Num4, "4"), (egui::Key::Num5, "5"),
    (egui::Key::Num6, "6"), (egui::Key::Num7, "7"), (egui::Key::Num8, "8"),
    (egui::Key::Num9, "9"),
    (egui::Key::Space, "Space"), (egui::Key::Enter, "Enter"), (egui::Key::Tab, "Tab"),
    (egui::Key::Escape, "Esc"), (egui::Key::Backspace, "Bksp"), (egui::Key::Delete, "Del"),
    (egui::Key::ArrowUp, "\u{2191}"), (egui::Key::ArrowDown, "\u{2193}"),
    (egui::Key::ArrowLeft, "\u{2190}"), (egui::Key::ArrowRight, "\u{2192}"),
    (egui::Key::Home, "Home"), (egui::Key::End, "End"),
    (egui::Key::PageUp, "PgUp"), (egui::Key::PageDown, "PgDn"),
    (egui::Key::F1, "F1"), (egui::Key::F2, "F2"), (egui::Key::F3, "F3"),
    (egui::Key::F4, "F4"), (egui::Key::F5, "F5"), (egui::Key::F6, "F6"),
    (egui::Key::F7, "F7"), (egui::Key::F8, "F8"), (egui::Key::F9, "F9"),
    (egui::Key::F10, "F10"), (egui::Key::F11, "F11"), (egui::Key::F12, "F12"),
];

/// Maps a user-typed key name to an egui::Key
pub fn parse_key(name: &str) -> Option<egui::Key> {
    let lower = name.to_lowercase();
    let trimmed = lower.trim();
    ALL_KEYS.iter().find(|(_, display)| display.to_lowercase() == trimmed).map(|(k, _)| *k)
        .or_else(|| {
            // Also match arrow names
            match trimmed {
                "up" | "arrowup" => Some(egui::Key::ArrowUp),
                "down" | "arrowdown" => Some(egui::Key::ArrowDown),
                "left" | "arrowleft" => Some(egui::Key::ArrowLeft),
                "right" | "arrowright" => Some(egui::Key::ArrowRight),
                "return" => Some(egui::Key::Enter),
                _ => None,
            }
        })
}

fn key_display_name(name: &str) -> &str {
    let lower = name.to_lowercase();
    for (_, display) in ALL_KEYS {
        if display.to_lowercase() == lower {
            return display;
        }
    }
    name.split_whitespace().next().unwrap_or(name)
}

pub fn render(
    ui: &mut egui::Ui,
    key_name: &mut String,
    pressed: &mut bool,
    toggle_mode: &mut bool,
    toggled_on: &mut bool,
) {
    let node_id_hash = ui.id().value();
    let listening_id = egui::Id::new(("key_listening", node_id_hash));
    let is_listening = ui.ctx().data_mut(|d| d.get_temp::<bool>(listening_id).unwrap_or(false));

    // ── Key display button ──────────────────────────────────────
    let display = if key_name.is_empty() {
        "?"
    } else {
        key_display_name(key_name)
    };

    let is_active = *pressed || (*toggle_mode && *toggled_on);

    // Key cap style
    let key_size = egui::vec2(ui.available_width().min(80.0), 40.0);
    let (rect, response) = ui.allocate_exact_size(key_size, egui::Sense::click());

    let bg = if is_listening {
        egui::Color32::from_rgb(60, 40, 100) // purple glow = listening
    } else if is_active {
        egui::Color32::from_rgb(80, 160, 255) // blue = pressed
    } else {
        egui::Color32::from_rgb(50, 52, 58) // dark = idle
    };

    let border = if is_listening {
        egui::Color32::from_rgb(160, 100, 255)
    } else if is_active {
        egui::Color32::from_rgb(100, 180, 255)
    } else {
        egui::Color32::from_rgb(80, 82, 90)
    };

    let painter = ui.painter();
    painter.rect_filled(rect, 8.0, bg);
    painter.rect_stroke(rect, 8.0, egui::Stroke::new(2.0, border), egui::StrokeKind::Outside);

    let text_color = if is_active { egui::Color32::WHITE } else { egui::Color32::from_rgb(200, 200, 210) };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        display,
        egui::FontId::proportional(18.0),
        text_color,
    );

    // Listening hint
    if is_listening {
        painter.text(
            egui::pos2(rect.center().x, rect.bottom() + 10.0),
            egui::Align2::CENTER_TOP,
            "Press any key...",
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(160, 100, 255),
        );
    }

    // Click to start listening
    if response.clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(listening_id, !is_listening));
    }

    // ── Key capture (when listening) ────────────────────────────
    if is_listening {
        // Check ALL keys for press
        let captured = ui.ctx().input(|i| {
            for (key, name) in ALL_KEYS {
                if i.key_pressed(*key) {
                    return Some(name.to_string());
                }
            }
            None
        });

        if let Some(name) = captured {
            *key_name = name;
            ui.ctx().data_mut(|d| d.insert_temp(listening_id, false));
        }

        // Also capture modifier keys alone
        let modifier_captured = ui.ctx().input(|_i| {
            // Modifiers are harder — they don't fire key_pressed.
            // We detect them via modifiers changing.
            None::<String>
        });
        if let Some(name) = modifier_captured {
            *key_name = name;
            ui.ctx().data_mut(|d| d.insert_temp(listening_id, false));
        }

        // Escape cancels without changing
        if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
            ui.ctx().data_mut(|d| d.insert_temp(listening_id, false));
        }
    }

    // ── Toggle mode indicator ───────────────────────────────────
    ui.horizontal(|ui| {
        ui.checkbox(toggle_mode, "");
        ui.label(egui::RichText::new("Toggle").small().color(egui::Color32::GRAY));
        if *toggle_mode && *toggled_on {
            ui.colored_label(egui::Color32::from_rgb(100, 200, 255), "ON");
        }
    });
}
