//! Background curves of a psychrometric chart, all computed with `psychrolib`.
//!
//! Every curve is a list of `(dry-bulb temperature in °C, humidity ratio in kg/kg)` points,
//! so the UI can project it onto either a psychrometric chart or a Mollier h-x diagram.

use super::air::{ATM_PRESSURE_PA, AirState};
use psychrolib::{Psychrolib, UnitSystem};

/// Dry-bulb temperature range shown by the chart, in °C.
pub const CHART_TEMP_RANGE_C: (i32, i32) = (-20, 50);
/// Upper end of the humidity-ratio axis in kg/kg.
pub const CHART_MAX_HUM_RATIO: f64 = 0.030;
/// Specific enthalpy of water vapor at 0 °C in kJ/kg, the slope of the Mollier h-x skew.
pub const LATENT_HEAT_KJ_KG: f64 = 2501.0;

/// Points below this humidity ratio were clamped by psychrolib and are not on the line.
const MIN_CHART_HUM_RATIO: f64 = 1e-6;

/// Samples per °C along a curve.
const STEPS_PER_C: u32 = 2;

fn temps() -> impl Iterator<Item = f64> {
    let (lo, hi) = CHART_TEMP_RANGE_C;
    let steps = (hi - lo).unsigned_abs() * STEPS_PER_C;
    (0..=steps).map(move |i| f64::from(lo) + f64::from(i) / f64::from(STEPS_PER_C))
}

/// Curve of constant relative humidity (percent) at absolute pressure `pressure_pa`.
pub fn rel_humidity_curve(rel_humidity: f64, pressure_pa: f64) -> Vec<(f64, f64)> {
    temps()
        .map(|t| (t, AirState::humidity_ratio_at(t, rel_humidity, pressure_pa)))
        .filter(|&(_, w)| w <= CHART_MAX_HUM_RATIO)
        .collect()
}

/// Line of constant dry-bulb temperature, from dry air up to saturation.
pub fn isotherm(temp_c: f64, pressure_pa: f64) -> Vec<(f64, f64)> {
    let saturated = AirState::humidity_ratio_at(temp_c, 100.0, pressure_pa);
    vec![(temp_c, 0.0), (temp_c, saturated.min(CHART_MAX_HUM_RATIO))]
}

/// Line of constant specific enthalpy (kJ/kg dry air), from dry air up to saturation.
pub fn enthalpy_line(enthalpy_kj: f64, pressure_pa: f64) -> Vec<(f64, f64)> {
    let psy = Psychrolib::new(UnitSystem::Si);
    temps()
        .map(|t| {
            let w = psy.hum_ratio_from_enthalpy_and_t_dry_bulb(enthalpy_kj * 1000.0, t);
            (t, w)
        })
        .filter(|&(t, w)| {
            let saturated = AirState::humidity_ratio_at(t, 100.0, pressure_pa);
            w > MIN_CHART_HUM_RATIO && w <= saturated && w <= CHART_MAX_HUM_RATIO
        })
        .collect()
}

/// Line of constant moist-air density (kg/m³ of moist air), from dry air up to saturation.
pub fn density_line(density: f64, pressure_pa: f64) -> Vec<(f64, f64)> {
    let psy = Psychrolib::new(UnitSystem::Si);
    // The specific volume is linear in the humidity ratio, so the density
    // (1 + w) / v(w) = density can be solved for w directly.
    temps()
        .filter_map(|t| {
            let dry = psy.moist_air_volume(t, 0.0, pressure_pa).ok()?;
            let wet = psy
                .moist_air_volume(t, CHART_MAX_HUM_RATIO, pressure_pa)
                .ok()?;
            let slope = (wet - dry) / CHART_MAX_HUM_RATIO;
            let w = (density * dry - 1.0) / (1.0 - density * slope);
            let saturated = AirState::humidity_ratio_at(t, 100.0, pressure_pa);
            (w >= 0.0 && w <= saturated && w <= CHART_MAX_HUM_RATIO).then_some((t, w))
        })
        .collect()
}

/// Specific enthalpy in kJ/kg of the point `(temp_c, hum_ratio)`.
pub fn enthalpy_at(temp_c: f64, hum_ratio: f64) -> f64 {
    AirState {
        temp_c,
        humidity_ratio: hum_ratio,
        flow_m3h: 0.0,
        pressure_pa: 0.0,
        atm_pressure_pa: ATM_PRESSURE_PA,
    }
    .enthalpy()
}

/// Standard pressure the chart background is drawn for.
pub const CHART_PRESSURE_PA: f64 = ATM_PRESSURE_PA;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_line_points_have_that_density() {
        let psy = Psychrolib::new(UnitSystem::Si);
        for (t, w) in density_line(1.15, CHART_PRESSURE_PA) {
            let density = psy
                .moist_air_density(t, w, CHART_PRESSURE_PA)
                .unwrap_or(f64::NAN);
            assert!((density - 1.15).abs() < 1e-6, "off at {t} °C");
        }
    }

    #[test]
    fn saturation_curve_rises_with_temperature() {
        let curve = rel_humidity_curve(100.0, CHART_PRESSURE_PA);
        assert!(curve.windows(2).all(|pair| pair[1].1 >= pair[0].1));
    }

    #[test]
    fn enthalpy_line_points_have_that_enthalpy() {
        for (t, w) in enthalpy_line(40.0, CHART_PRESSURE_PA) {
            assert!((enthalpy_at(t, w) - 40.0).abs() < 1e-6, "off at {t} °C");
        }
    }
}
