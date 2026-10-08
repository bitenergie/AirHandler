use super::Selection;
use crate::model::{
    AirHandlerUnit, AirState, Component, ComponentId, DuctId, HeatRecovery, PlateArrangement,
};
use egui::{DragValue, Grid, Ui};

/// Shows and edits the properties of the selected component, duct or heat recovery.
pub fn properties(ui: &mut Ui, unit: &mut AirHandlerUnit, selection: &mut Option<Selection>) {
    ui.heading("Properties");
    ui.add_space(4.0);
    match *selection {
        None => {
            ui.weak("Select a component or a duct.");
        }
        Some(Selection::Duct(id)) => duct_properties(ui, unit, id),
        Some(Selection::Component(id)) => {
            if !component_properties(ui, unit, id) {
                *selection = None;
            }
        }
        Some(Selection::Recovery) => {
            if !recovery_properties(ui, unit) {
                *selection = None;
            }
        }
    }
}

fn duct_properties(ui: &mut Ui, unit: &mut AirHandlerUnit, id: DuctId) {
    ui.strong(format!("{} / {}", id.inlet_label(), id.outlet_label()));
    let duct = unit.duct_mut(id);
    Grid::new("duct_props").num_columns(2).show(ui, |ui| {
        ui.label("Inlet temperature");
        ui.add(
            DragValue::new(&mut duct.inlet_temp_c)
                .range(-40.0..=60.0)
                .suffix(" °C"),
        );
        ui.end_row();
        ui.label("Inlet humidity");
        ui.add(
            DragValue::new(&mut duct.inlet_rel_humidity)
                .range(0.0..=100.0)
                .suffix(" %"),
        );
        ui.end_row();
        ui.label("Flow");
        ui.add(
            DragValue::new(&mut duct.flow_m3h)
                .range(0.0..=100_000.0)
                .speed(10.0)
                .suffix(" m³/h"),
        );
        ui.end_row();
    });
}

/// Returns `false` if the component no longer exists or was just deleted.
fn component_properties(ui: &mut Ui, unit: &mut AirHandlerUnit, id: ComponentId) -> bool {
    let Some(component) = unit.component_mut(id) else {
        return false;
    };
    ui.strong(component.kind().label());
    Grid::new("component_props").num_columns(2).show(ui, |ui| {
        component_params(ui, component);
    });

    // Looked up after editing so the results reflect this frame's changes.
    if let Some(stage) = unit.stage(id) {
        ui.separator();
        Grid::new("component_results")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Outlet temperature");
                ui.label(format!("{:.1} °C", stage.out.temp_c));
                ui.end_row();
                ui.label("Outlet humidity");
                ui.label(format!("{:.0} %", stage.out.rel_humidity()));
                ui.end_row();
                ui.label("Dew point");
                ui.label(format!("{:.1} °C", stage.out.dew_point_c()));
                ui.end_row();
                ui.label("Static pressure");
                ui.label(format!("{:+.0} Pa", stage.out.pressure_pa));
                ui.end_row();
                ui.label("Power");
                ui.label(format!("{:.2} kW", stage.duty.power_kw));
                ui.end_row();
                if stage.duty.water_kg_h != 0.0 {
                    ui.label("Water (+ added / − condensed)");
                    ui.label(format!("{:+.1} kg/h", stage.duty.water_kg_h));
                    ui.end_row();
                }
            });
    }

    ui.separator();
    if ui.button("Remove").clicked() {
        unit.remove(id);
        return false;
    }
    true
}

fn component_params(ui: &mut Ui, component: &mut Component) {
    match component {
        Component::Heater { setpoint_c } | Component::Cooler { setpoint_c } => {
            ui.label("Setpoint");
            ui.add(DragValue::new(setpoint_c).range(-40.0..=60.0).suffix(" °C"));
            ui.end_row();
        }
        Component::Humidifier {
            setpoint_rh,
            adiabatic,
        } => {
            ui.label("Target humidity");
            ui.add(DragValue::new(setpoint_rh).range(0.0..=100.0).suffix(" %"));
            ui.end_row();
            ui.label("Adiabatic cooling");
            ui.checkbox(adiabatic, "evaporative");
            ui.end_row();
        }
        Component::Fan {
            pressure_pa,
            efficiency,
        } => {
            ui.label("Pressure rise");
            ui.add(
                DragValue::new(pressure_pa)
                    .range(0.0..=5000.0)
                    .speed(5.0)
                    .suffix(" Pa"),
            );
            ui.end_row();
            ui.label("Efficiency");
            ui.add(DragValue::new(efficiency).range(0.05..=1.0).speed(0.01));
            ui.end_row();
        }
        Component::PressureDrop { pressure_pa } => {
            ui.label("Pressure drop");
            ui.add(
                DragValue::new(pressure_pa)
                    .range(0.0..=2000.0)
                    .speed(2.0)
                    .suffix(" Pa"),
            );
            ui.end_row();
        }
    }
}

/// Returns `false` if the heat recovery no longer exists or was just removed.
fn recovery_properties(ui: &mut Ui, unit: &mut AirHandlerUnit) -> bool {
    let Some(recovery) = unit.recovery_mut() else {
        return false;
    };
    ui.strong(recovery.kind().label());
    Grid::new("recovery_props").num_columns(2).show(ui, |ui| {
        recovery_params(ui, recovery);
    });

    // Looked up after editing so the results reflect this frame's changes.
    if let Some(stage) = unit.simulate().recovery {
        ui.separator();
        let duty = stage.exchange.duty;
        Grid::new("recovery_results").num_columns(2).show(ui, |ui| {
            for (label, state) in [
                ("Supply in", stage.supply_in),
                ("Supply out", stage.exchange.supply_out),
                ("Extract in", stage.extract_in),
                ("Extract out", stage.exchange.extract_out),
            ] {
                ui.label(label);
                ui.label(state_text(&state));
                ui.end_row();
            }
            ui.label("Recovered heat");
            ui.label(format!("{:.2} kW", duty.heat_kw));
            ui.end_row();
            ui.label("Electrical power");
            ui.label(format!("{:.2} kW", duty.electrical_kw));
            ui.end_row();
            if duty.condensate_kg_h > 0.0 {
                ui.label("Condensate");
                ui.label(format!("{:.1} kg/h", duty.condensate_kg_h));
                ui.end_row();
            }
        });
    }

    ui.separator();
    if ui.button("Remove").clicked() {
        unit.remove_recovery();
        return false;
    }
    true
}

fn state_text(state: &AirState) -> String {
    format!("{:.1} °C · {:.0} %", state.temp_c, state.rel_humidity())
}

fn efficiency(ui: &mut Ui, label: &str, value: &mut f64) {
    ui.label(label);
    ui.add(DragValue::new(value).range(0.0..=1.0).speed(0.01));
    ui.end_row();
}

fn power(ui: &mut Ui, label: &str, value: &mut f64) {
    ui.label(label);
    ui.add(
        DragValue::new(value)
            .range(0.0..=50.0)
            .speed(0.01)
            .suffix(" kW"),
    );
    ui.end_row();
}

fn recovery_params(ui: &mut Ui, recovery: &mut HeatRecovery) {
    match recovery {
        HeatRecovery::RunAroundCoil {
            temp_efficiency,
            pump_power_kw,
        } => {
            efficiency(ui, "Temperature efficiency", temp_efficiency);
            power(ui, "Pump power", pump_power_kw);
        }
        HeatRecovery::RotaryWheel {
            temp_efficiency,
            humidity_efficiency,
            drive_power_kw,
        } => {
            efficiency(ui, "Temperature efficiency", temp_efficiency);
            efficiency(ui, "Humidity efficiency", humidity_efficiency);
            power(ui, "Drive power", drive_power_kw);
        }
        HeatRecovery::PlateExchanger {
            arrangement,
            temp_efficiency,
        } => {
            ui.label("Arrangement");
            ui.horizontal(|ui| {
                for option in PlateArrangement::ALL {
                    ui.selectable_value(arrangement, option, option.label());
                }
            });
            ui.end_row();
            efficiency(ui, "Temperature efficiency", temp_efficiency);
            ui.label("Max. for arrangement");
            ui.label(format!("{:.2}", arrangement.max_efficiency()));
            ui.end_row();
        }
    }
}
