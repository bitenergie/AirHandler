//! Moist-air state. The psychrometric formulas come from `psychrolib` (ASHRAE Fundamentals 2017).

use psychrolib::{Psychrolib, UnitSystem};

/// Standard atmospheric pressure in Pa.
pub const ATM_PRESSURE_PA: f64 = 101_325.0;
/// Temperature range (°C) for which `psychrolib` is valid. Inputs are clamped to it.
const TEMP_RANGE_C: (f64, f64) = (-100.0, 200.0);

fn psy() -> Psychrolib {
    Psychrolib::new(UnitSystem::Si)
}

fn valid_temp(temp_c: f64) -> f64 {
    temp_c.clamp(TEMP_RANGE_C.0, TEMP_RANGE_C.1)
}

/// Thermodynamic state of an air stream.
///
/// The humidity ratio is the source of truth; relative humidity is derived.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AirState {
    pub temp_c: f64,
    /// Water per kg of dry air, in kg/kg.
    pub humidity_ratio: f64,
    /// Volume flow in m³/h.
    pub flow_m3h: f64,
    /// Static pressure relative to the atmosphere, in Pa. Fans raise it, pressure drops lower it.
    #[serde(default)]
    pub pressure_pa: f64,
}

impl AirState {
    pub fn from_rel_humidity(temp_c: f64, rel_humidity: f64, flow_m3h: f64) -> Self {
        Self {
            temp_c,
            humidity_ratio: Self::humidity_ratio_at(temp_c, rel_humidity, ATM_PRESSURE_PA),
            flow_m3h,
            pressure_pa: 0.0,
        }
    }

    /// Humidity ratio of air at `temp_c`, `rel_humidity` (percent, capped at saturation) and
    /// absolute pressure `abs_pressure_pa`.
    pub fn humidity_ratio_at(temp_c: f64, rel_humidity: f64, abs_pressure_pa: f64) -> f64 {
        psy()
            .hum_ratio_from_rel_hum(
                valid_temp(temp_c),
                (rel_humidity / 100.0).clamp(0.0, 1.0),
                abs_pressure_pa,
            )
            .unwrap_or(0.0)
    }

    /// Absolute pressure in Pa.
    pub fn abs_pressure_pa(&self) -> f64 {
        ATM_PRESSURE_PA + self.pressure_pa
    }

    /// Humidity ratio of saturated air at this state's temperature and pressure.
    pub fn saturation_humidity_ratio(&self) -> f64 {
        Self::humidity_ratio_at(self.temp_c, 100.0, self.abs_pressure_pa())
    }

    /// Relative humidity in percent.
    pub fn rel_humidity(&self) -> f64 {
        psy()
            .rel_hum_from_hum_ratio(
                valid_temp(self.temp_c),
                self.humidity_ratio.max(0.0),
                self.abs_pressure_pa(),
            )
            .map_or(0.0, |rel_hum| 100.0 * rel_hum)
    }

    /// Dew point temperature in °C.
    pub fn dew_point_c(&self) -> f64 {
        psy()
            .t_dew_point_from_hum_ratio(
                valid_temp(self.temp_c),
                self.humidity_ratio.max(0.0),
                self.abs_pressure_pa(),
            )
            .unwrap_or(self.temp_c)
    }

    /// Specific enthalpy in kJ per kg of dry air.
    pub fn enthalpy(&self) -> f64 {
        psy()
            .moist_air_enthalpy(valid_temp(self.temp_c), self.humidity_ratio.max(0.0))
            .unwrap_or(0.0)
            / 1000.0
    }

    /// Specific volume in m³ per kg of dry air.
    fn specific_volume(&self) -> f64 {
        psy()
            .moist_air_volume(
                valid_temp(self.temp_c),
                self.humidity_ratio.max(0.0),
                self.abs_pressure_pa(),
            )
            .unwrap_or(0.83)
    }

    /// Dry-air mass flow in kg/s.
    pub fn mass_flow(&self) -> f64 {
        self.flow_m3h / 3600.0 / self.specific_volume()
    }

    /// The same state with the volume flow adjusted so that the dry-air mass flow is `mass_kg_s`.
    ///
    /// Heating, humidifying or changing the pressure changes the density, so the volume flow
    /// has to follow while the mass flow stays constant along the duct.
    #[must_use]
    pub fn with_mass_flow(self, mass_kg_s: f64) -> Self {
        Self {
            flow_m3h: mass_kg_s * 3600.0 * self.specific_volume(),
            ..self
        }
    }

    /// Heat capacity rate of the stream in kW/K.
    pub fn heat_capacity_rate(&self) -> f64 {
        self.mass_flow() * (1.006 + 1.86 * self.humidity_ratio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_humidity_roundtrip() {
        let state = AirState::from_rel_humidity(20.0, 50.0, 1000.0);
        assert!(
            (state.rel_humidity() - 50.0).abs() < 1e-9,
            "roundtrip drifted"
        );
    }

    #[test]
    fn matches_ashrae_reference_point() {
        // 25 °C / 80 % at sea level: dew point 21.31 °C, humidity ratio 15.96 g/kg.
        let state = AirState::from_rel_humidity(25.0, 80.0, 1000.0);
        assert!((state.humidity_ratio - 0.015962).abs() < 1e-5, "ratio");
        assert!((state.dew_point_c() - 21.3094).abs() < 1e-3, "dew point");
    }

    #[test]
    fn mass_flow_is_kept_by_density_change() {
        let cold = AirState::from_rel_humidity(-5.0, 80.0, 3000.0);
        let warm = AirState {
            temp_c: 21.0,
            ..cold
        }
        .with_mass_flow(cold.mass_flow());
        assert!(warm.flow_m3h > cold.flow_m3h, "warm air takes more volume");
        assert!(
            (warm.mass_flow() - cold.mass_flow()).abs() < 1e-12,
            "mass drifted"
        );
    }

    #[test]
    fn standard_room_air_humidity_ratio() {
        // 20 °C / 50 % is about 7.3 g/kg.
        let state = AirState::from_rel_humidity(20.0, 50.0, 1000.0);
        assert!(
            (state.humidity_ratio - 0.0073).abs() < 0.0003,
            "unexpected ratio"
        );
    }
}
