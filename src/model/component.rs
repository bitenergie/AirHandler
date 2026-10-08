//! The air-treatment components that can be placed in a duct.

use super::air::AirState;

/// Identifies a kind of component without its parameters. Used for the library and drag payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
pub enum ComponentKind {
    Heater,
    Cooler,
    Humidifier,
    Fan,
}

impl ComponentKind {
    pub const ALL: [Self; 4] = [Self::Heater, Self::Cooler, Self::Humidifier, Self::Fan];

    pub fn label(self) -> &'static str {
        match self {
            Self::Heater => "Heater",
            Self::Cooler => "Cooler",
            Self::Humidifier => "Humidifier",
            Self::Fan => "Fan",
        }
    }

    /// A new component of this kind with sensible default parameters.
    pub fn instantiate(self) -> Component {
        match self {
            Self::Heater => Component::Heater { setpoint_c: 21.0 },
            Self::Cooler => Component::Cooler { setpoint_c: 14.0 },
            Self::Humidifier => Component::Humidifier { setpoint_rh: 45.0 },
            Self::Fan => Component::Fan {
                pressure_pa: 600.0,
                efficiency: 0.65,
            },
        }
    }
}

/// A component together with its parameters.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum Component {
    /// Heats the air to `setpoint_c` if it is colder.
    Heater { setpoint_c: f64 },
    /// Cools the air to `setpoint_c` if it is warmer, condensing water below the dew point.
    Cooler { setpoint_c: f64 },
    /// Adds steam until `setpoint_rh` (percent) is reached.
    Humidifier { setpoint_rh: f64 },
    /// Raises the pressure by `pressure_pa`; the electrical power ends up as heat in the air.
    Fan { pressure_pa: f64, efficiency: f64 },
}

/// What a component consumes or produces while treating the air.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Duty {
    /// Thermal power for heaters and coolers, electrical power for fans (kW).
    pub power_kw: f64,
    /// Water added (positive) or condensed (negative), in kg/h.
    pub water_kg_h: f64,
}

impl Component {
    pub fn kind(&self) -> ComponentKind {
        match self {
            Self::Heater { .. } => ComponentKind::Heater,
            Self::Cooler { .. } => ComponentKind::Cooler,
            Self::Humidifier { .. } => ComponentKind::Humidifier,
            Self::Fan { .. } => ComponentKind::Fan,
        }
    }

    /// Computes the outlet state and the duty for the given inlet state.
    pub fn apply(&self, inlet: AirState) -> (AirState, Duty) {
        let mass = inlet.mass_flow();
        let mut out = inlet;
        let mut duty = Duty::default();
        match *self {
            Self::Heater { setpoint_c } => {
                out.temp_c = inlet.temp_c.max(setpoint_c);
                duty.power_kw = mass * (out.enthalpy() - inlet.enthalpy());
            }
            Self::Cooler { setpoint_c } => {
                out.temp_c = inlet.temp_c.min(setpoint_c);
                let saturated = AirState::humidity_ratio_at(out.temp_c, 100.0);
                out.humidity_ratio = inlet.humidity_ratio.min(saturated);
                duty.power_kw = mass * (inlet.enthalpy() - out.enthalpy());
                duty.water_kg_h = mass * (out.humidity_ratio - inlet.humidity_ratio) * 3600.0;
            }
            Self::Humidifier { setpoint_rh } => {
                let target = AirState::humidity_ratio_at(inlet.temp_c, setpoint_rh);
                out.humidity_ratio = inlet.humidity_ratio.max(target);
                duty.water_kg_h = mass * (out.humidity_ratio - inlet.humidity_ratio) * 3600.0;
            }
            Self::Fan {
                pressure_pa,
                efficiency,
            } => {
                let electrical_kw =
                    inlet.flow_m3h / 3600.0 * pressure_pa / efficiency.max(0.01) / 1000.0;
                out.temp_c += electrical_kw / inlet.heat_capacity_rate().max(f64::EPSILON);
                duty.power_kw = electrical_kw;
            }
        }
        (out, duty)
    }

    /// One-line summary of `duty` for display on the component.
    pub fn duty_summary(&self, duty: Duty) -> String {
        match self {
            Self::Heater { .. } | Self::Cooler { .. } | Self::Fan { .. } => {
                format!("{:.1} kW", duty.power_kw)
            }
            Self::Humidifier { .. } => format!("{:.1} kg/h", duty.water_kg_h),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn winter_air() -> AirState {
        AirState::from_rel_humidity(-5.0, 80.0, 3000.0)
    }

    #[test]
    fn heater_reaches_setpoint_at_constant_humidity_ratio() {
        let (out, duty) = ComponentKind::Heater.instantiate().apply(winter_air());
        assert!((out.temp_c - 21.0).abs() < 1e-9, "setpoint not reached");
        assert_eq!(out.humidity_ratio, winter_air().humidity_ratio);
        assert!(duty.power_kw > 0.0, "heater must consume power");
    }

    #[test]
    fn heater_does_not_cool() {
        let warm = AirState::from_rel_humidity(30.0, 40.0, 1000.0);
        let (out, duty) = ComponentKind::Heater.instantiate().apply(warm);
        assert_eq!(out, warm);
        assert_eq!(duty, Duty::default());
    }

    #[test]
    fn cooler_condenses_below_dew_point() {
        let humid = AirState::from_rel_humidity(30.0, 70.0, 3000.0);
        let (out, duty) = ComponentKind::Cooler.instantiate().apply(humid);
        assert!(duty.water_kg_h < 0.0, "expected condensate");
        assert!(
            (out.rel_humidity() - 100.0).abs() < 1e-6,
            "should leave saturated"
        );
    }

    #[test]
    fn humidifier_reaches_target() {
        let dry = AirState::from_rel_humidity(21.0, 10.0, 3000.0);
        let (out, duty) = ComponentKind::Humidifier.instantiate().apply(dry);
        assert!(
            (out.rel_humidity() - 45.0).abs() < 1e-6,
            "target not reached"
        );
        assert!(duty.water_kg_h > 0.0, "humidifier must add water");
    }
}
