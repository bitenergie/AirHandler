//! # psychrolib
//!
//! Thermodynamic properties of gas-vapor mixtures (moist air) and the standard
//! atmosphere, suitable for most engineering, physical and meteorological
//! applications.
//!
//! This is a Rust port of [PsychroLib](https://github.com/psychrometrics/psychrolib)
//! 2.5.0. Most functions implement formulae from the *2017 ASHRAE Handbook -
//! Fundamentals* in both International System (SI) and Imperial (IP) units;
//! see each function's documentation for its specific reference.
//!
//! ## Example
//!
//! ```
//! use psychrolib::{Psychrolib, UnitSystem};
//!
//! let psy = Psychrolib::new(UnitSystem::Si);
//!
//! // Dew point for a dry-bulb temperature of 25 °C and 80 % relative humidity.
//! let t_dew_point = psy.t_dew_point_from_rel_hum(25.0, 0.80)?;
//! println!("{t_dew_point:.4}"); // 21.3094
//!
//! // Everything at once, at standard sea-level pressure.
//! let state = psy.psychrometrics_from_rel_hum(25.0, 0.80, 101_325.0)?;
//! assert!((state.hum_ratio - 0.015962).abs() < 1e-6);
//! # Ok::<(), psychrolib::Error>(())
//! ```
//!
//! ## Differences from the C library
//!
//! * **No global state.** The unit system lives in a [`Psychrolib`] value
//!   instead of a process-wide `SetUnitSystem` call, so an "undefined unit
//!   system" is unrepresentable and the API is thread-safe.
//! * **Errors instead of `exit(1)`.** Invalid inputs and solver failures are
//!   returned as [`Error`] rather than aborting the process.
//! * **Return values instead of out-pointers.** The `CalcPsychrometrics*`
//!   functions return a [`MoistAirState`].
//! * **Rust naming.** `GetTDewPointFromRelHum` becomes
//!   [`Psychrolib::t_dew_point_from_rel_hum`], and so on.
//!
//! ## Note from the original authors
//!
//! Every effort has been made to ensure that the code is adequate, however no
//! representation is made with respect to its accuracy. Use at your own risk.

mod error;
mod psychrometrics;
mod units;

pub use error::{Error, Result};
pub use psychrometrics::{MoistAirState, Psychrolib, MIN_HUM_RATIO};
pub use units::{
    celsius_from_kelvin, fahrenheit_from_rankine, kelvin_from_celsius, rankine_from_fahrenheit,
    UnitSystem,
};
