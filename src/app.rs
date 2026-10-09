use crate::model::AirHandlerUnit;
use crate::ui::{self, Selection};
use std::future::Future;
use std::sync::mpsc;

/// Root application: owns the unit being designed and wires the widgets together.
#[derive(Default, serde::Deserialize, serde::Serialize)]
#[serde(default)] // new fields fall back to defaults when loading older persisted state
pub struct AirHandlerApp {
    unit: AirHandlerUnit,
    chart: ui::ChartSettings,
    #[serde(skip)]
    selection: Option<Selection>,
    #[serde(skip)]
    files: FileChannel,
}

/// Result of an asynchronous file dialog, sent back to the UI thread.
enum FileEvent {
    Loaded(AirHandlerUnit),
    Failed(String),
}

/// Receives the outcome of file dialogs, which run off the UI thread (a browser has no blocking
/// dialogs).
struct FileChannel {
    sender: mpsc::Sender<FileEvent>,
    receiver: mpsc::Receiver<FileEvent>,
    /// Last error to show in the menu bar.
    error: Option<String>,
}

impl Default for FileChannel {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            error: None,
        }
    }
}

/// Runs `future` to completion without blocking the UI.
#[cfg(target_arch = "wasm32")]
fn spawn(future: impl Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(future);
}

/// Runs `future` to completion without blocking the UI.
#[cfg(not(target_arch = "wasm32"))]
fn spawn(future: impl Future<Output = ()> + Send + 'static) {
    std::thread::spawn(move || pollster::block_on(future));
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
        self.receive_files();
        self.top_panel(ui);

        egui::Panel::left("library")
            .resizable(false)
            .exact_size(160.0)
            .show(ui, ui::library);

        egui::Panel::right("properties")
            .default_size(280.0)
            .show(ui, |ui| {
                ui::properties(ui, &mut self.unit, &mut self.selection);
            });

        egui::Panel::bottom("chart")
            .resizable(true)
            .default_size(380.0)
            .size_range(160.0..=900.0)
            .show(ui, |ui| {
                ui::chart(ui, &self.unit, &mut self.chart);
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui::diagram(ui, &mut self.unit, &mut self.selection);
        });
    }
}

impl AirHandlerApp {
    fn receive_files(&mut self) {
        while let Ok(event) = self.files.receiver.try_recv() {
            match event {
                FileEvent::Loaded(unit) => {
                    self.unit = unit;
                    self.selection = None;
                    self.files.error = None;
                }
                FileEvent::Failed(message) => self.files.error = Some(message),
            }
        }
    }

    /// Replaces the unit with the default design.
    fn reset(&mut self) {
        self.unit = AirHandlerUnit::default();
        self.selection = None;
        self.files.error = None;
    }

    fn save_json(&self, ctx: &egui::Context) {
        let json = match serde_json::to_vec_pretty(&self.unit) {
            Ok(json) => json,
            Err(err) => {
                self.files
                    .sender
                    .send(FileEvent::Failed(err.to_string()))
                    .ok();
                return;
            }
        };
        let sender = self.files.sender.clone();
        let ctx = ctx.clone();
        spawn(async move {
            let file = rfd::AsyncFileDialog::new()
                .set_file_name("air_handler.json")
                .add_filter("JSON", &["json"])
                .save_file()
                .await;
            if let Some(file) = file
                && let Err(err) = file.write(&json).await
            {
                sender.send(FileEvent::Failed(err.to_string())).ok();
            }
            ctx.request_repaint();
        });
    }

    fn load_json(&self, ctx: &egui::Context) {
        let sender = self.files.sender.clone();
        let ctx = ctx.clone();
        spawn(async move {
            let file = rfd::AsyncFileDialog::new()
                .add_filter("JSON", &["json"])
                .pick_file()
                .await;
            if let Some(file) = file {
                let bytes = file.read().await;
                let event = match serde_json::from_slice(&bytes) {
                    Ok(unit) => FileEvent::Loaded(unit),
                    Err(err) => {
                        FileEvent::Failed(format!("Cannot load {}: {err}", file.file_name()))
                    }
                };
                sender.send(event).ok();
            }
            ctx.request_repaint();
        });
    }

    fn top_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_panel").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Save as JSON…").clicked() {
                        self.save_json(ui.ctx());
                    }
                    if ui.button("Load JSON…").clicked() {
                        self.load_json(ui.ctx());
                    }
                    if ui.button("Reset").clicked() {
                        self.reset();
                    }
                    // No File->Quit on web pages.
                    if !cfg!(target_arch = "wasm32") && ui.button("Quit").clicked() {
                        ui.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.add_space(16.0);
                egui::widgets::global_theme_preference_buttons(ui);
                if let Some(error) = &self.files.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });
        });
    }
}
