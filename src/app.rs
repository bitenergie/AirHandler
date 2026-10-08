use crate::model::AirHandlerUnit;
use crate::ui::{self, Selection};

/// Root application: owns the unit being designed and wires the widgets together.
#[derive(Default, serde::Deserialize, serde::Serialize)]
#[serde(default)] // new fields fall back to defaults when loading older persisted state
pub struct AirHandlerApp {
    unit: AirHandlerUnit,
    #[serde(skip)]
    selection: Option<Selection>,
}

impl AirHandlerApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.storage
            .and_then(|storage| eframe::get_value(storage, eframe::APP_KEY))
            .unwrap_or_default()
    }
}

impl eframe::App for AirHandlerApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        top_panel(ui);

        egui::Panel::left("library")
            .resizable(false)
            .exact_size(160.0)
            .show(ui, ui::library);

        egui::Panel::right("properties")
            .default_size(280.0)
            .show(ui, |ui| {
                ui::properties(ui, &mut self.unit, &mut self.selection);
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui::diagram(ui, &mut self.unit, &mut self.selection);
        });
    }
}

fn top_panel(ui: &mut egui::Ui) {
    egui::Panel::top("top_panel").show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            // No File->Quit on web pages.
            if !cfg!(target_arch = "wasm32") {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        ui.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.add_space(16.0);
            }
            egui::widgets::global_theme_preference_buttons(ui);
        });
    });
}
