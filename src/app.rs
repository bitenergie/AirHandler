use crate::model::AirHandlerUnit;
use crate::ui::{self, Selection};
use std::future::Future;
use std::sync::{Arc, mpsc};

/// Root application: owns the unit being designed and wires the widgets together.
#[derive(Default, serde::Deserialize, serde::Serialize)]
#[serde(default)] // new fields fall back to defaults when loading older persisted state
pub struct AirHandlerApp {
    unit: AirHandlerUnit,
    chart: ui::ChartSettings,
    panels: PanelVisibility,
    #[serde(skip)]
    selection: Option<Selection>,
    #[serde(skip)]
    files: FileChannel,
    /// Plot area to crop from the screenshot that was requested for saving the chart.
    #[serde(skip)]
    pending_png: Option<egui::Rect>,
}

/// Which side and bottom panels are shown. Hiding some gives the diagram the whole screen,
/// which is what a narrow mobile display needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct PanelVisibility {
    library: bool,
    properties: bool,
    chart: bool,
}

impl Default for PanelVisibility {
    fn default() -> Self {
        Self {
            library: true,
            properties: true,
            chart: true,
        }
    }
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

/// Encodes `image` as an 8-bit RGBA PNG.
fn encode_png(image: &egui::ColorImage) -> Result<Vec<u8>, png::EncodingError> {
    let [width, height] = image.size;
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(
        &mut bytes,
        u32::try_from(width).unwrap_or(0),
        u32::try_from(height).unwrap_or(0),
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(image.as_raw())?;
    writer.finish()?;
    Ok(bytes)
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
        self.receive_screenshot(ui);
        self.top_panel(ui);

        if self.panels.library {
            egui::Panel::left("library")
                .resizable(false)
                .exact_size(160.0)
                .show(ui, ui::library);
        }

        if self.panels.properties {
            egui::Panel::right("properties")
                .default_size(280.0)
                .show(ui, |ui| {
                    ui::properties(ui, &mut self.unit, &mut self.selection);
                });
        }

        if self.panels.chart {
            egui::Panel::bottom("chart")
                .resizable(true)
                .default_size(380.0)
                .size_range(160.0..=900.0)
                .show(ui, |ui| {
                    if let Some(plot_rect) = ui::chart(ui, &mut self.unit, &mut self.chart) {
                        self.pending_png = Some(plot_rect);
                        ui.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                            egui::UserData::default(),
                        ));
                    }
                });
        }

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

    /// Picks up the screenshot requested by the chart's save button and saves the plot area
    /// of it as a PNG.
    fn receive_screenshot(&mut self, ui: &egui::Ui) {
        let Some(plot_rect) = self.pending_png else {
            return;
        };
        let screenshot = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(Arc::clone(image)),
                _ => None,
            })
        });
        let Some(screenshot) = screenshot else {
            return;
        };
        self.pending_png = None;
        let plot = screenshot.region(&plot_rect, Some(ui.pixels_per_point()));
        match encode_png(&plot) {
            Ok(png) => self.save_file(ui.ctx(), "air_handler_chart.png", "PNG", "png", png),
            Err(err) => self.files.error = Some(format!("Cannot encode the chart image: {err}")),
        }
    }

    fn save_json(&self, ctx: &egui::Context) {
        match serde_json::to_vec_pretty(&self.unit) {
            Ok(json) => self.save_file(ctx, "air_handler.json", "JSON", "json", json),
            Err(err) => {
                self.files
                    .sender
                    .send(FileEvent::Failed(err.to_string()))
                    .ok();
            }
        }
    }

    /// Asks where to save `bytes` and writes them there. On the web this downloads the file.
    fn save_file(
        &self,
        ctx: &egui::Context,
        file_name: &'static str,
        filter_name: &'static str,
        extension: &'static str,
        bytes: Vec<u8>,
    ) {
        let sender = self.files.sender.clone();
        let ctx = ctx.clone();
        spawn(async move {
            let file = rfd::AsyncFileDialog::new()
                .set_file_name(file_name)
                .add_filter(filter_name, &[extension])
                .save_file()
                .await;
            if let Some(file) = file
                && let Err(err) = file.write(&bytes).await
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
                ui.toggle_value(&mut self.panels.library, "Library");
                ui.toggle_value(&mut self.panels.properties, "Properties");
                ui.toggle_value(&mut self.panels.chart, "Chart");
                ui.add_space(16.0);
                egui::widgets::global_theme_preference_buttons(ui);
                if let Some(error) = &self.files.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_png() {
        let image = egui::ColorImage::filled([3, 2], egui::Color32::RED);
        let bytes = encode_png(&image).unwrap_or_default();
        assert_eq!(bytes.get(1..4), Some(&b"PNG"[..]), "missing PNG signature");
    }
}
