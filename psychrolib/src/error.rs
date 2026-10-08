//! Error type returned by fallible psychrometric calculations.

use std::fmt;

/// Convenient alias for results produced by this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Reasons a psychrometric calculation can fail.
///
/// These replace the process-aborting `ASSERT` macro of the original C
/// library: invalid physical inputs are reported to the caller instead.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// The dew-point temperature is above the dry-bulb temperature.
    DewPointAboveDryBulb,
    /// The wet-bulb temperature is above the dry-bulb temperature.
    WetBulbAboveDryBulb,
    /// The relative humidity is outside the range `[0, 1]`.
    RelHumOutOfRange,
    /// The specific humidity is outside the range `[0, 1)`.
    SpecificHumOutOfRange,
    /// The humidity ratio is negative.
    NegativeHumRatio,
    /// The partial pressure of water vapor is negative.
    NegativeVapPres,
    /// The partial pressure of water vapor is outside the range for which the
    /// saturation-pressure equations are valid.
    VapPresOutOfRange,
    /// The dry-bulb temperature is outside the validity range of the
    /// saturation-pressure equations (`min..=max`, in the active unit system).
    TemperatureOutOfRange {
        /// Lowest valid temperature.
        min: f64,
        /// Highest valid temperature.
        max: f64,
    },
    /// An iterative solver did not converge within the iteration limit.
    NoConvergence {
        /// Name of the function whose solver failed.
        function: &'static str,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DewPointAboveDryBulb => {
                f.write_str("dew point temperature is above dry bulb temperature")
            }
            Self::WetBulbAboveDryBulb => {
                f.write_str("wet bulb temperature is above dry bulb temperature")
            }
            Self::RelHumOutOfRange => f.write_str("relative humidity is outside range [0, 1]"),
            Self::SpecificHumOutOfRange => f.write_str("specific humidity is outside range [0, 1)"),
            Self::NegativeHumRatio => f.write_str("humidity ratio is negative"),
            Self::NegativeVapPres => {
                f.write_str("partial pressure of water vapor in moist air is negative")
            }
            Self::VapPresOutOfRange => f.write_str(
                "partial pressure of water vapor is outside range of validity of equations",
            ),
            Self::TemperatureOutOfRange { min, max } => {
                write!(f, "dry bulb temperature is outside range [{min}, {max}]")
            }
            Self::NoConvergence { function } => {
                write!(f, "convergence not reached in {function}")
            }
        }
    }
}

impl std::error::Error for Error {}
