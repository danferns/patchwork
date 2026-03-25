use eframe::egui;
use crate::graph::{NodeId, NodeType};
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

    // Scrollable list with max height so the palette stays compact
    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
    let mut last_cat = "";
    let mut any_shown = false;

    for entry in &cat {
        // Skip system nodes (File Menu, Zoom, Palette, MCP — auto-created)
        if entry.category == "System" { continue; }

        if !query.is_empty()
            && !entry.label.to_lowercase().contains(&query)
            && !entry.category.to_lowercase().contains(&query)
        {
            continue;
        }

        // Category header
        if entry.category != last_cat {
            if !last_cat.is_empty() {
                ui.add_space(2.0);
            }
            ui.label(egui::RichText::new(entry.category).small().strong().color(egui::Color32::GRAY));
            last_cat = entry.category;
        }

        // Node button — full width, compact
        let btn = ui.add_sized(
            [ui.available_width(), 22.0],
            egui::Button::new(egui::RichText::new(entry.label).size(12.0))
                .frame(true),
        );

        if btn.clicked() {
            // Store the node type to spawn via egui temp data (Vec has Default)
            let nt = (entry.factory)();
            ui.memory_mut(|mem| {
                let mut v: Vec<NodeType> = mem.data.get_temp(egui::Id::new("palette_spawn")).unwrap_or_default();
                v.push(nt);
                mem.data.insert_temp(egui::Id::new("palette_spawn"), v);
            });
        }

        btn.on_hover_text(format!("Click to add {}", entry.label));
        any_shown = true;
    }

    if !any_shown {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("No matches").color(egui::Color32::GRAY).italics());
    }
    }); // end ScrollArea
}
