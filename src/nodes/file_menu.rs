use eframe::egui;

pub struct FileMenuAction {
    pub new_project: bool,
    pub load_project: bool,
    pub save_project: bool,
}

pub fn render(ui: &mut egui::Ui) -> FileMenuAction {
    let mut action = FileMenuAction {
        new_project: false,
        load_project: false,
        save_project: false,
    };

    ui.horizontal(|ui| {
        if ui.button("New").clicked() {
            action.new_project = true;
        }
        if ui.button("Open").clicked() {
            action.load_project = true;
        }
        if ui.button("Save").clicked() {
            action.save_project = true;
        }
    });

    action
}
