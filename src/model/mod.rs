//! Domain model and calculations. Knows nothing about egui.

mod air;
mod component;
mod recovery;
mod unit;

pub use air::AirState;
pub use component::{Component, ComponentKind, Duty};
pub use recovery::{Exchange, HeatRecovery, HeatRecoveryKind, PlateArrangement, RecoveryDuty};
pub use unit::{
    AirHandlerUnit, ComponentId, Duct, DuctId, DuctResult, Placed, RecoveryStage, Simulation, Stage,
};
