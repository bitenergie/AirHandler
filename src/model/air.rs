//! Moist-air state and the psychrometric formulas behind it.

/// Atmospheric pressure in hPa (sea level).
const PRESSURE_HPA: f64 = 1013.25;
/// Air density in kg/m³, used to convert volume flow to mass flow.
const AIR_DENSITY: f64 = 1.2;

/// Saturation vapour pressure in hPa (Magnus formula).
fn saturation_pressure(temp_c: f64) -> f64 {
    6.112 * (17.62 * temp_c / (243.12 + temp_c)).exp()
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
}

impl AirState {
    pub fn from_rel_humidity(temp_c: f64, rel_humidity: f64, flow_m3h: f64) -> Self {
        Self {
            temp_c,
            humidity_ratio: Self::humidity_ratio_at(temp_c, rel_humidity),
            flow_m3h,
        }
    }

    /// Humidity ratio of air at `temp_c` and `rel_humidity` (percent, capped at saturation).
    pub fn humidity_ratio_at(temp_c: f64, rel_humidity: f64) -> f64 {
        let vapour = (rel_humidity / 100.0).clamp(0.0, 1.0) * saturation_pressure(temp_c);
        0.622 * vapour / (PRESSURE_HPA - vapour)
    }

    /// Relative humidity in percent.
    pub fn rel_humidity(&self) -> f64 {
        let vapour = PRESSURE_HPA * self.humidity_ratio / (0.622 + self.humidity_ratio);
        100.0 * vapour / saturation_pressure(self.temp_c)
    }

    /// Specific enthalpy in kJ per kg of dry air.
    pub fn enthalpy(&self) -> f64 {
        1.006 * self.temp_c + self.humidity_ratio * (2501.0 + 1.86 * self.temp_c)
    }

    /// Dry-air mass flow in kg/s.
    pub fn mass_flow(&self) -> f64 {
        self.flow_m3h / 3600.0 * AIR_DENSITY
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
    fn standard_room_air_humidity_ratio() {
        // 20 °C / 50 % is about 7.3 g/kg.
        let state = AirState::from_rel_humidity(20.0, 50.0, 1000.0);
        assert!(
            (state.humidity_ratio - 0.0073).abs() < 0.0003,
            "unexpected ratio"
        );
    }
}
