//! Unit systems and temperature-scale conversions.

use std::fmt;

/// Offset between degrees Fahrenheit and degrees Rankine (exact).
///
/// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 39.
const ZERO_FAHRENHEIT_AS_RANKINE: f64 = 459.67;

/// Offset between degrees Celsius and Kelvin (exact).
///
/// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 39.
const ZERO_CELSIUS_AS_KELVIN: f64 = 273.15;

/// System of units used by a [`Psychrolib`](crate::Psychrolib) instance.
///
/// | Quantity             | [`Si`](Self::Si)       | [`Ip`](Self::Ip)       |
/// |----------------------|------------------------|------------------------|
/// | Temperature          | °C                     | °F                     |
/// | Pressure             | Pa                     | psi                    |
/// | Humidity ratio       | kg(H₂O) kg(air)⁻¹        | lb(H₂O) lb(air)⁻¹        |
/// | Enthalpy             | J kg⁻¹                 | Btu lb⁻¹               |
/// | Specific volume      | m³ kg⁻¹                | ft³ lb⁻¹               |
/// | Density              | kg m⁻³                 | lb ft⁻³                |
/// | Altitude             | m                      | ft                     |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnitSystem {
    /// International System of Units.
    Si,
    /// Imperial (inch-pound) units.
    Ip,
}

impl fmt::Display for UnitSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Si => "SI",
            Self::Ip => "IP",
        })
    }
}

/// Converts a temperature from degrees Fahrenheit (°F) to degrees Rankine (°R).
///
/// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 section 3.
#[must_use]
pub fn rankine_from_fahrenheit(t_f: f64) -> f64 {
    t_f + ZERO_FAHRENHEIT_AS_RANKINE
}

/// Converts a temperature from degrees Rankine (°R) to degrees Fahrenheit (°F).
///
/// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 section 3.
#[must_use]
pub fn fahrenheit_from_rankine(t_r: f64) -> f64 {
    t_r - ZERO_FAHRENHEIT_AS_RANKINE
}

/// Converts a temperature from degrees Celsius (°C) to Kelvin (K).
///
/// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 section 3.
#[must_use]
pub fn kelvin_from_celsius(t_c: f64) -> f64 {
    t_c + ZERO_CELSIUS_AS_KELVIN
}

/// Converts a temperature from Kelvin (K) to degrees Celsius (°C).
///
/// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 section 3.
#[must_use]
pub fn celsius_from_kelvin(t_k: f64) -> f64 {
    t_k - ZERO_CELSIUS_AS_KELVIN
}
