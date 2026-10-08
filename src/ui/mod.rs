//! egui widgets. They read and edit the [`crate::model`] and never own it.

mod chart;
mod diagram;
mod library;
mod properties;

pub use chart::{ChartSettings, chart};
pub use diagram::diagram;
pub use library::library;
pub use properties::properties;

use crate::model::{ComponentId, ComponentKind, DuctId};
use egui::Color32;

/// What the properties widget is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Component(ComponentId),
    Duct(DuctId),
    /// The unit's heat recovery.
    Recovery,
}

fn kind_color(kind: ComponentKind) -> Color32 {
    match kind {
        ComponentKind::Heater => Color32::from_rgb(0xd9, 0x5f, 0x4b),
        ComponentKind::Cooler => Color32::from_rgb(0x4b, 0x8f, 0xd9),
        ComponentKind::Humidifier => Color32::from_rgb(0x3f, 0xb5, 0xa5),
        ComponentKind::Fan => Color32::from_rgb(0x9a, 0x9a, 0xa5),
        ComponentKind::PressureDrop => Color32::from_rgb(0xc9, 0xa2, 0x3c),
    }
}

const RECOVERY_COLOR: Color32 = Color32::from_rgb(0xb0, 0x7a, 0xd9);
