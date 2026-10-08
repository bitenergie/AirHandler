//! Heat recovery between the supply and the extract airstream.
//!
//! All types share one steady-state model: the temperature efficiency is defined on the
//! supply side (as in EN 308), the exchanged heat is limited by the weaker stream, and the
//! extract stream condenses if it is cooled below its dew point. Types differ in which
//! parameters they have and whether they transfer moisture.

use super::air::AirState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
pub enum PlateArrangement {
    Counterflow,
    Crossflow,
}

impl PlateArrangement {
    pub const ALL: [Self; 2] = [Self::Counterflow, Self::Crossflow];

    pub fn label(self) -> &'static str {
        match self {
            Self::Counterflow => "Counterflow",
            Self::Crossflow => "Crossflow",
        }
    }

    /// Highest temperature efficiency the arrangement can physically reach.
    pub fn max_efficiency(self) -> f64 {
        match self {
            Self::Counterflow => 0.95,
            Self::Crossflow => 0.75,
        }
    }
}

/// Identifies a kind of heat recovery without its parameters. Used for the library and drag payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
pub enum HeatRecoveryKind {
    RunAroundCoil,
    RotaryWheel,
    PlateExchanger,
}

impl HeatRecoveryKind {
    pub const ALL: [Self; 3] = [Self::RunAroundCoil, Self::RotaryWheel, Self::PlateExchanger];

    pub fn label(self) -> &'static str {
        match self {
            Self::RunAroundCoil => "Run-around coil",
            Self::RotaryWheel => "Rotary wheel",
            Self::PlateExchanger => "Plate exchanger",
        }
    }

    /// A new heat recovery of this kind with typical default parameters.
    pub fn instantiate(self) -> HeatRecovery {
        match self {
            Self::RunAroundCoil => HeatRecovery::RunAroundCoil {
                temp_efficiency: 0.5,
                pump_power_kw: 0.3,
            },
            Self::RotaryWheel => HeatRecovery::RotaryWheel {
                temp_efficiency: 0.75,
                humidity_efficiency: 0.5,
                drive_power_kw: 0.1,
            },
            Self::PlateExchanger => HeatRecovery::PlateExchanger {
                arrangement: PlateArrangement::Counterflow,
                temp_efficiency: 0.8,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum HeatRecovery {
    /// Two coils joined by a pumped glycol loop. Sensible only, so the efficiency is moderate.
    RunAroundCoil {
        temp_efficiency: f64,
        pump_power_kw: f64,
    },
    /// Rotating storage matrix. A hygroscopic wheel also transfers moisture
    /// (`humidity_efficiency` is 0 for a non-hygroscopic one).
    RotaryWheel {
        temp_efficiency: f64,
        humidity_efficiency: f64,
        drive_power_kw: f64,
    },
    /// Static plates, sensible only. The arrangement caps the reachable efficiency.
    PlateExchanger {
        arrangement: PlateArrangement,
        temp_efficiency: f64,
    },
}

/// What the heat recovery moves and consumes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RecoveryDuty {
    /// Heat transferred into the supply stream (kW). Negative when it cools the supply.
    pub heat_kw: f64,
    /// Pump or drive power (kW).
    pub electrical_kw: f64,
    /// Water condensed out of the extract stream (kg/h).
    pub condensate_kg_h: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Exchange {
    pub supply_out: AirState,
    pub extract_out: AirState,
    pub duty: RecoveryDuty,
}

impl HeatRecovery {
    pub fn kind(&self) -> HeatRecoveryKind {
        match self {
            Self::RunAroundCoil { .. } => HeatRecoveryKind::RunAroundCoil,
            Self::RotaryWheel { .. } => HeatRecoveryKind::RotaryWheel,
            Self::PlateExchanger { .. } => HeatRecoveryKind::PlateExchanger,
        }
    }

    /// Temperature efficiency, moisture efficiency and electrical power.
    fn parameters(&self) -> (f64, f64, f64) {
        match *self {
            Self::RunAroundCoil {
                temp_efficiency,
                pump_power_kw,
            } => (temp_efficiency, 0.0, pump_power_kw),
            Self::RotaryWheel {
                temp_efficiency,
                humidity_efficiency,
                drive_power_kw,
            } => (temp_efficiency, humidity_efficiency, drive_power_kw),
            Self::PlateExchanger {
                arrangement,
                temp_efficiency,
            } => (temp_efficiency.min(arrangement.max_efficiency()), 0.0, 0.0),
        }
    }

    /// Exchanges heat (and moisture for wheels) between the two streams entering the recovery.
    ///
    /// Latent heat released by condensation is not credited to the supply stream, which
    /// makes the result slightly conservative.
    pub fn exchange(&self, supply: AirState, extract: AirState) -> Exchange {
        let (eps, eps_x, electrical_kw) = self.parameters();
        let supply_rate = supply.heat_capacity_rate().max(f64::EPSILON);
        let extract_rate = extract.heat_capacity_rate().max(f64::EPSILON);

        let delta_t = extract.temp_c - supply.temp_c;
        let wanted = eps * supply_rate * delta_t;
        let limit = supply_rate.min(extract_rate) * delta_t;
        let heat_kw = if delta_t >= 0.0 {
            wanted.min(limit)
        } else {
            wanted.max(limit)
        };

        let mut supply_out = supply;
        let mut extract_out = extract;
        supply_out.temp_c += heat_kw / supply_rate;
        extract_out.temp_c -= heat_kw / extract_rate;

        // Moisture transfer, conserving water between the streams.
        let moisture_kg_s =
            supply.mass_flow() * eps_x * (extract.humidity_ratio - supply.humidity_ratio);
        supply_out.humidity_ratio += moisture_kg_s / supply.mass_flow().max(f64::EPSILON);
        extract_out.humidity_ratio = (extract.humidity_ratio
            - moisture_kg_s / extract.mass_flow().max(f64::EPSILON))
        .max(0.0);

        supply_out.humidity_ratio = supply_out
            .humidity_ratio
            .min(supply_out.saturation_humidity_ratio());
        let extract_saturated = extract_out.saturation_humidity_ratio();
        let condensed = (extract_out.humidity_ratio - extract_saturated).max(0.0);
        extract_out.humidity_ratio -= condensed;

        Exchange {
            supply_out: supply_out.with_mass_flow(supply.mass_flow()),
            extract_out: extract_out.with_mass_flow(extract.mass_flow()),
            duty: RecoveryDuty {
                heat_kw,
                electrical_kw,
                condensate_kg_h: extract.mass_flow() * condensed * 3600.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn winter() -> (AirState, AirState) {
        (
            AirState::from_rel_humidity(-5.0, 80.0, 3000.0),
            AirState::from_rel_humidity(22.0, 40.0, 3000.0),
        )
    }

    #[test]
    fn plate_balanced_flow_hits_efficiency() {
        let (supply, extract) = winter();
        let plate = HeatRecovery::PlateExchanger {
            arrangement: PlateArrangement::Counterflow,
            temp_efficiency: 0.8,
        };
        let out = plate.exchange(supply, extract);
        assert!(
            (out.supply_out.temp_c - (-5.0 + 0.8 * 27.0)).abs() < 1e-6,
            "supply temperature"
        );
        assert!(out.duty.heat_kw > 0.0, "heat must flow into the supply");
        assert_eq!(
            out.supply_out.humidity_ratio, supply.humidity_ratio,
            "plates are sensible only"
        );
    }

    #[test]
    fn crossflow_caps_efficiency() {
        let (supply, extract) = winter();
        let plate = HeatRecovery::PlateExchanger {
            arrangement: PlateArrangement::Crossflow,
            temp_efficiency: 0.95,
        };
        let out = plate.exchange(supply, extract);
        assert!(
            (out.supply_out.temp_c - (-5.0 + 0.75 * 27.0)).abs() < 1e-6,
            "cap not applied"
        );
    }

    #[test]
    fn weaker_stream_limits_the_transfer() {
        let (supply, mut extract) = winter();
        extract.flow_m3h = 1000.0;
        let plate = HeatRecovery::PlateExchanger {
            arrangement: PlateArrangement::Counterflow,
            temp_efficiency: 0.95,
        };
        let out = plate.exchange(supply, extract);
        assert!(
            out.extract_out.temp_c >= supply.temp_c - 1e-9,
            "extract cooled below the supply inlet"
        );
    }

    #[test]
    fn cold_extract_outlet_condenses() {
        let (supply, extract) = winter();
        let out = HeatRecoveryKind::PlateExchanger
            .instantiate()
            .exchange(supply, extract);
        assert!(out.duty.condensate_kg_h > 0.0, "expected condensate");
        assert!(
            out.extract_out.rel_humidity() <= 100.0 + 1e-6,
            "above saturation"
        );
    }

    #[test]
    fn hygroscopic_wheel_moves_moisture_to_supply() {
        let (supply, extract) = winter();
        let out = HeatRecoveryKind::RotaryWheel
            .instantiate()
            .exchange(supply, extract);
        assert!(
            out.supply_out.humidity_ratio > supply.humidity_ratio,
            "supply not humidified"
        );
        assert!(out.duty.electrical_kw > 0.0, "drive power missing");
    }
}
