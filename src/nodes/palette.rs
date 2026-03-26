use eframe::egui;
use crate::graph::{NodeId, NodeType};
use crate::icons;
use super::catalog;

pub fn render(
    ui: &mut egui::Ui,
    search: &mut String,
    _node_id: NodeId,
) {
    // Search box
    ui.add(
        egui::TextEdit::singleline(search)
            .hint_text("Search nodes...")
            .desired_width(ui.available_width()),
    );

    ui.add_space(4.0);

    let query = search.to_lowercase();
    let cat = catalog();

    // Scrollable list with max height
    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
        let mut last_cat = "";
        let mut any_shown = false;

        for entry in &cat {
            if entry.category == "System" { continue; }

            if !query.is_empty()
                && !entry.label.to_lowercase().contains(&query)
                && !entry.category.to_lowercase().contains(&query)
            {
                continue;
            }

            // Category header with icon
            if entry.category != last_cat {
                if !last_cat.is_empty() {
                    ui.add_space(4.0);
                }
                let cat_icon = icons::category_icon(entry.category);
                ui.horizontal(|ui| {
                    ui.label(icons::icon_colored(cat_icon, 12.0, egui::Color32::GRAY));
                    ui.label(egui::RichText::new(entry.category).small().strong().color(egui::Color32::GRAY));
                });
                ui.add_space(2.0);
                last_cat = entry.category;
            }

            // Node card
            let color_hint = {
                let nt = (entry.factory)();
                let c = nt.color_hint();
                egui::Color32::from_rgb(c[0], c[1], c[2])
            };
            let n_inputs = { let nt = (entry.factory)(); nt.inputs().len() };
            let n_outputs = { let nt = (entry.factory)(); nt.outputs().len() };

            let clicked = render_node_card(ui, entry.label, color_hint, n_inputs, n_outputs);

            if clicked {
                let nt = (entry.factory)();
                ui.memory_mut(|mem| {
                    let mut v: Vec<NodeType> = mem.data.get_temp(egui::Id::new("palette_spawn")).unwrap_or_default();
                    v.push(nt);
                    mem.data.insert_temp(egui::Id::new("palette_spawn"), v);
                });
            }

            any_shown = true;
        }

        if !any_shown {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("No matches").color(egui::Color32::GRAY).italics());
        }
    }); // end ScrollArea
}

/// Render a mini node card. Returns true if clicked.
fn render_node_card(
    ui: &mut egui::Ui,
    label: &str,
    accent: egui::Color32,
    n_inputs: usize,
    n_outputs: usize,
) -> bool {
    let card_width = ui.available_width();
    let card_height = 28.0;
    let port_r = 3.0;
    let port_margin = 6.0;

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(card_width, card_height),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        // Card background
        let bg = if response.hovered() {
            egui::Color32::from_rgb(55, 55, 65)
        } else {
            egui::Color32::from_rgb(38, 38, 45)
        };
        painter.rect_filled(rect, 4.0, bg);

        // Accent bar on left
        let accent_rect = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(3.0, card_height),
        );
        painter.rect_filled(accent_rect, egui::Rounding { nw: 4, sw: 4, ne: 0, se: 0 }, accent);

        // Input port dots (left side)
        if n_inputs > 0 {
            let spacing = card_height / (n_inputs as f32 + 1.0);
            for i in 0..n_inputs {
                let y = rect.top() + spacing * (i as f32 + 1.0);
                painter.circle_filled(
                    egui::pos2(rect.left() + port_margin, y),
                    port_r,
                    egui::Color32::from_rgb(140, 140, 140),
                );
            }
        }

        // Output port dots (right side)
        if n_outputs > 0 {
            let spacing = card_height / (n_outputs as f32 + 1.0);
            for i in 0..n_outputs {
                let y = rect.top() + spacing * (i as f32 + 1.0);
                painter.circle_filled(
                    egui::pos2(rect.right() - port_margin, y),
                    port_r,
                    egui::Color32::from_rgb(80, 150, 255),
                );
            }
        }

        // Icon + label
        let node_icon = icons::node_icon(label);
        let icon_x = rect.left() + 14.0;
        let text_x = icon_x + 18.0;
        let cy = rect.center().y;

        painter.text(
            egui::pos2(icon_x, cy),
            egui::Align2::LEFT_CENTER,
            node_icon,
            egui::FontId::proportional(14.0),
            accent,
        );

        painter.text(
            egui::pos2(text_x, cy),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(200, 200, 200),
        );
    }

    response.clicked()
}
