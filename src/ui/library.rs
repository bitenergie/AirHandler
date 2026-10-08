use super::{RECOVERY_COLOR, kind_color};
use crate::model::{ComponentKind, HeatRecoveryKind};
use egui::{Frame, Id, RichText, Ui};

/// Palette of components. Each entry is a drag source carrying its [`ComponentKind`] or
/// [`HeatRecoveryKind`]; the diagram accepts the drop.
/// The ids carry a section tag: the kind enums hash by discriminant only, so
/// `Heater` and `RunAroundCoil` would otherwise share an id and drag together.
pub fn library(ui: &mut Ui) {
    ui.heading("Library");
    ui.add_space(4.0);
    for kind in ComponentKind::ALL {
        ui.dnd_drag_source(Id::new(("library", "component", kind)), kind, |ui| {
            Frame::group(ui.style())
                .stroke(egui::Stroke::new(2.0, kind_color(kind)))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(kind.label()).strong());
                });
        });
    }
    ui.add_space(8.0);
    ui.strong("Heat recovery");
    for kind in HeatRecoveryKind::ALL {
        ui.dnd_drag_source(Id::new(("library", "recovery", kind)), kind, |ui| {
            Frame::group(ui.style())
                .stroke(egui::Stroke::new(2.0, RECOVERY_COLOR))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(kind.label()).strong());
                });
        });
    }
    ui.add_space(8.0);
    ui.weak("Drag a component onto a duct. Heat recovery spans both ducts.");
}
