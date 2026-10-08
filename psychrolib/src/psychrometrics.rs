//! The [`Psychrolib`] calculator and its result types.

use crate::error::{Error, Result};
use crate::units::{
    celsius_from_kelvin, fahrenheit_from_rankine, kelvin_from_celsius, rankine_from_fahrenheit,
    UnitSystem,
};

/// Universal gas constant for dry air (IP version) in ft∙lbf/lb(da)/R.
///
/// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1.
const R_DA_IP: f64 = 53.350;

/// Universal gas constant for dry air (SI version) in J/kg(da)/K.
///
/// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1.
const R_DA_SI: f64 = 287.042;

/// Maximum number of iterations before an iterative solver gives up.
const MAX_ITER_COUNT: u32 = 100;

/// Minimum acceptable humidity ratio used/returned by any function.
///
/// Any value above 0 but below `MIN_HUM_RATIO` is reset to this value.
pub const MIN_HUM_RATIO: f64 = 1e-7;

/// Freezing point of water in °F.
const FREEZING_POINT_WATER_IP: f64 = 32.0;

/// Freezing point of water in °C.
const FREEZING_POINT_WATER_SI: f64 = 0.0;

/// Triple point of water in °F.
const TRIPLE_POINT_WATER_IP: f64 = 32.018;

/// Triple point of water in °C.
const TRIPLE_POINT_WATER_SI: f64 = 0.01;

/// Returns `Ok(())` if `condition` holds, `Err(error)` otherwise.
///
/// Written so that comparisons involving NaN (which are all `false`) are
/// rejected, mirroring the original `ASSERT` semantics.
#[inline]
fn ensure(condition: bool, error: Error) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(error)
    }
}

/// Validates a humidity ratio and clamps it to [`MIN_HUM_RATIO`].
#[inline]
fn bounded_hum_ratio(hum_ratio: f64) -> Result<f64> {
    ensure(hum_ratio >= 0.0, Error::NegativeHumRatio)?;
    Ok(hum_ratio.max(MIN_HUM_RATIO))
}

/// Validates a relative humidity (`0 ..= 1`).
#[inline]
fn check_rel_hum(rel_hum: f64) -> Result<()> {
    ensure((0.0..=1.0).contains(&rel_hum), Error::RelHumOutOfRange)
}

/// A complete set of psychrometric properties of a moist-air state.
///
/// Returned by the `psychrometrics_from_*` methods of [`Psychrolib`]. Units
/// depend on the [`UnitSystem`] of the instance that produced it.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct MoistAirState {
    /// Dry-bulb temperature in °F (IP) or °C (SI).
    pub t_dry_bulb: f64,
    /// Wet-bulb temperature in °F (IP) or °C (SI).
    pub t_wet_bulb: f64,
    /// Dew-point temperature in °F (IP) or °C (SI).
    pub t_dew_point: f64,
    /// Relative humidity `[0-1]`.
    pub rel_hum: f64,
    /// Humidity ratio in lb(H₂O) lb(air)⁻¹ (IP) or kg(H₂O) kg(air)⁻¹ (SI).
    pub hum_ratio: f64,
    /// Partial pressure of water vapor in moist air in psi (IP) or Pa (SI).
    pub vap_pres: f64,
    /// Moist-air enthalpy in Btu lb⁻¹ (IP) or J kg⁻¹ (SI).
    pub moist_air_enthalpy: f64,
    /// Specific volume in ft³ lb⁻¹ (IP) or m³ kg⁻¹ (SI).
    pub moist_air_volume: f64,
    /// Degree of saturation (unitless).
    pub degree_of_saturation: f64,
}

/// Psychrometric calculator bound to a [`UnitSystem`].
///
/// All methods take and return values in the unit system chosen at
/// construction (see [`UnitSystem`] for the table of units). The type is
/// `Copy`, holds no global state, and is therefore trivially thread-safe: the
/// original C library's process-wide `SetUnitSystem` call is replaced by
/// constructing the value you need.
///
/// Functions that validate their inputs or solve iteratively return
/// [`Result`]; the others are infallible.
///
/// # Examples
///
/// ```
/// use psychrolib::{Psychrolib, UnitSystem};
///
/// let psy = Psychrolib::new(UnitSystem::Si);
/// // Dew point at 25 °C dry bulb and 80 % relative humidity.
/// let t_dew = psy.t_dew_point_from_rel_hum(25.0, 0.80)?;
/// assert!((t_dew - 21.3094).abs() < 1e-3);
/// # Ok::<(), psychrolib::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Psychrolib {
    units: UnitSystem,
}

impl Psychrolib {
    /// Creates a calculator that works in the given unit system.
    #[must_use]
    pub fn new(units: UnitSystem) -> Self {
        Self { units }
    }

    /// Returns the unit system in use.
    #[must_use]
    pub fn units(self) -> UnitSystem {
        self.units
    }

    /// Tolerance of temperature calculations (same physical value in IP and SI).
    fn tolerance(self) -> f64 {
        match self.units {
            UnitSystem::Ip => 0.001 * 9.0 / 5.0,
            UnitSystem::Si => 0.001,
        }
    }

    /// Domain of validity of the saturation-pressure equations, `(min, max)`.
    fn temperature_bounds(self) -> (f64, f64) {
        match self.units {
            UnitSystem::Ip => (-148.0, 392.0),
            UnitSystem::Si => (-100.0, 200.0),
        }
    }

    // ------------------------------------------------------------------
    // Conversions between dew point, wet bulb, and relative humidity
    // ------------------------------------------------------------------

    /// Returns the wet-bulb temperature given dry-bulb temperature, dew-point
    /// temperature, and pressure.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1.
    ///
    /// # Errors
    ///
    /// [`Error::DewPointAboveDryBulb`] if `t_dew_point > t_dry_bulb`, plus any
    /// error of the underlying calculations.
    pub fn t_wet_bulb_from_t_dew_point(
        self,
        t_dry_bulb: f64,
        t_dew_point: f64,
        pressure: f64,
    ) -> Result<f64> {
        ensure(t_dew_point <= t_dry_bulb, Error::DewPointAboveDryBulb)?;
        let hum_ratio = self.hum_ratio_from_t_dew_point(t_dew_point, pressure)?;
        self.t_wet_bulb_from_hum_ratio(t_dry_bulb, hum_ratio, pressure)
    }

    /// Returns the wet-bulb temperature given dry-bulb temperature, relative
    /// humidity, and pressure.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1.
    ///
    /// # Errors
    ///
    /// [`Error::RelHumOutOfRange`] if `rel_hum` is outside `[0, 1]`, plus any
    /// error of the underlying calculations.
    pub fn t_wet_bulb_from_rel_hum(
        self,
        t_dry_bulb: f64,
        rel_hum: f64,
        pressure: f64,
    ) -> Result<f64> {
        check_rel_hum(rel_hum)?;
        let hum_ratio = self.hum_ratio_from_rel_hum(t_dry_bulb, rel_hum, pressure)?;
        self.t_wet_bulb_from_hum_ratio(t_dry_bulb, hum_ratio, pressure)
    }

    /// Returns the relative humidity given dry-bulb and dew-point temperatures.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 22.
    ///
    /// # Errors
    ///
    /// [`Error::DewPointAboveDryBulb`] if `t_dew_point > t_dry_bulb`, or
    /// [`Error::TemperatureOutOfRange`] if either temperature is outside the
    /// validity range of the saturation-pressure equations.
    pub fn rel_hum_from_t_dew_point(self, t_dry_bulb: f64, t_dew_point: f64) -> Result<f64> {
        ensure(t_dew_point <= t_dry_bulb, Error::DewPointAboveDryBulb)?;
        let vap_pres = self.sat_vap_pres(t_dew_point)?;
        let sat_vap_pres = self.sat_vap_pres(t_dry_bulb)?;
        Ok(vap_pres / sat_vap_pres)
    }

    /// Returns the relative humidity given dry-bulb temperature, wet-bulb
    /// temperature, and pressure.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1.
    ///
    /// # Errors
    ///
    /// [`Error::WetBulbAboveDryBulb`] if `t_wet_bulb > t_dry_bulb`, plus any
    /// error of the underlying calculations.
    pub fn rel_hum_from_t_wet_bulb(
        self,
        t_dry_bulb: f64,
        t_wet_bulb: f64,
        pressure: f64,
    ) -> Result<f64> {
        ensure(t_wet_bulb <= t_dry_bulb, Error::WetBulbAboveDryBulb)?;
        let hum_ratio = self.hum_ratio_from_t_wet_bulb(t_dry_bulb, t_wet_bulb, pressure)?;
        self.rel_hum_from_hum_ratio(t_dry_bulb, hum_ratio, pressure)
    }

    /// Returns the dew-point temperature given dry-bulb temperature and
    /// relative humidity.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1.
    ///
    /// # Errors
    ///
    /// [`Error::RelHumOutOfRange`] if `rel_hum` is outside `[0, 1]`, plus any
    /// error of [`t_dew_point_from_vap_pres`](Self::t_dew_point_from_vap_pres).
    pub fn t_dew_point_from_rel_hum(self, t_dry_bulb: f64, rel_hum: f64) -> Result<f64> {
        check_rel_hum(rel_hum)?;
        let vap_pres = self.vap_pres_from_rel_hum(t_dry_bulb, rel_hum)?;
        self.t_dew_point_from_vap_pres(t_dry_bulb, vap_pres)
    }

    /// Returns the dew-point temperature given dry-bulb temperature, wet-bulb
    /// temperature, and pressure.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1.
    ///
    /// # Errors
    ///
    /// [`Error::WetBulbAboveDryBulb`] if `t_wet_bulb > t_dry_bulb`, plus any
    /// error of the underlying calculations.
    pub fn t_dew_point_from_t_wet_bulb(
        self,
        t_dry_bulb: f64,
        t_wet_bulb: f64,
        pressure: f64,
    ) -> Result<f64> {
        ensure(t_wet_bulb <= t_dry_bulb, Error::WetBulbAboveDryBulb)?;
        let hum_ratio = self.hum_ratio_from_t_wet_bulb(t_dry_bulb, t_wet_bulb, pressure)?;
        self.t_dew_point_from_hum_ratio(t_dry_bulb, hum_ratio, pressure)
    }

    // ------------------------------------------------------------------
    // Conversions between dew point, or relative humidity and vapor pressure
    // ------------------------------------------------------------------

    /// Returns the partial pressure of water vapor given dry-bulb temperature
    /// and relative humidity, in psi (IP) or Pa (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 12, 22.
    ///
    /// # Errors
    ///
    /// [`Error::RelHumOutOfRange`] if `rel_hum` is outside `[0, 1]`, or
    /// [`Error::TemperatureOutOfRange`].
    pub fn vap_pres_from_rel_hum(self, t_dry_bulb: f64, rel_hum: f64) -> Result<f64> {
        check_rel_hum(rel_hum)?;
        Ok(rel_hum * self.sat_vap_pres(t_dry_bulb)?)
    }

    /// Returns the relative humidity given dry-bulb temperature and vapor
    /// pressure (psi (IP) or Pa (SI)).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 12, 22.
    ///
    /// # Errors
    ///
    /// [`Error::NegativeVapPres`] if `vap_pres < 0`, or
    /// [`Error::TemperatureOutOfRange`].
    pub fn rel_hum_from_vap_pres(self, t_dry_bulb: f64, vap_pres: f64) -> Result<f64> {
        ensure(vap_pres >= 0.0, Error::NegativeVapPres)?;
        Ok(vap_pres / self.sat_vap_pres(t_dry_bulb)?)
    }

    /// Derivative of the natural log of the saturation vapor pressure with
    /// respect to dry-bulb temperature.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn. 5 & 6.
    fn d_ln_pws(self, t_dry_bulb: f64) -> f64 {
        match self.units {
            UnitSystem::Ip => {
                let t = rankine_from_fahrenheit(t_dry_bulb);
                if t_dry_bulb <= TRIPLE_POINT_WATER_IP {
                    1.0214165e4 / t.powi(2) - 5.3765794e-3
                        + 2.0 * 1.9202377e-7 * t
                        + 3.0 * 3.5575832e-10 * t.powi(2)
                        - 4.0 * 9.0344688e-14 * t.powi(3)
                        + 4.1635019 / t
                } else {
                    1.0440397e4 / t.powi(2) - 2.7022355e-2 + 2.0 * 1.2890360e-5 * t
                        - 3.0 * 2.4780681e-9 * t.powi(2)
                        + 6.5459673 / t
                }
            }
            UnitSystem::Si => {
                let t = kelvin_from_celsius(t_dry_bulb);
                if t_dry_bulb <= TRIPLE_POINT_WATER_SI {
                    5.6745359e3 / t.powi(2) - 9.677843e-3
                        + 2.0 * 6.2215701e-7 * t
                        + 3.0 * 2.0747825e-9 * t.powi(2)
                        - 4.0 * 9.484024e-13 * t.powi(3)
                        + 4.1635019 / t
                } else {
                    5.8002206e3 / t.powi(2) - 4.8640239e-2 + 2.0 * 4.1764768e-5 * t
                        - 3.0 * 1.4452093e-8 * t.powi(2)
                        + 6.5459673 / t
                }
            }
        }
    }

    /// Returns the dew-point temperature given dry-bulb temperature and vapor
    /// pressure (psi (IP) or Pa (SI)).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn. 5 and 6.
    ///
    /// The dew point is found by inverting the saturation-pressure equation
    /// (rather than using the much less accurate regressions of ASHRAE eqn. 37
    /// and 38) with Newton-Raphson on the logarithm of the vapor pressure,
    /// which is a very smooth function. Convergence usually takes 3 to 5
    /// iterations. `t_dry_bulb` serves as the initial guess and as an upper
    /// bound on the result.
    ///
    /// # Errors
    ///
    /// [`Error::VapPresOutOfRange`] if `vap_pres` is outside the range of
    /// saturation pressures covered by the equations,
    /// [`Error::TemperatureOutOfRange`] if `t_dry_bulb` is, or
    /// [`Error::NoConvergence`] if the iteration limit is exceeded.
    pub fn t_dew_point_from_vap_pres(self, t_dry_bulb: f64, vap_pres: f64) -> Result<f64> {
        let (t_min, t_max) = self.temperature_bounds();

        // Bounds outside which a solution cannot be found.
        ensure(
            vap_pres >= self.sat_vap_pres(t_min)? && vap_pres <= self.sat_vap_pres(t_max)?,
            Error::VapPresOutOfRange,
        )?;

        let ln_vp = vap_pres.ln();
        let tolerance = self.tolerance();
        let mut t_dew_point = t_dry_bulb;

        for _ in 0..MAX_ITER_COUNT {
            let t_iter = t_dew_point;
            let ln_vp_iter = self.sat_vap_pres(t_iter)?.ln();

            // New estimate, bounded by the domain of validity of eqn. 5 and 6.
            let d_ln_vp = self.d_ln_pws(t_iter);
            t_dew_point = (t_iter - (ln_vp_iter - ln_vp) / d_ln_vp).clamp(t_min, t_max);

            if (t_dew_point - t_iter).abs() <= tolerance {
                return Ok(t_dew_point.min(t_dry_bulb));
            }
        }
        Err(Error::NoConvergence {
            function: "t_dew_point_from_vap_pres",
        })
    }

    /// Returns the vapor pressure given the dew-point temperature, in psi (IP)
    /// or Pa (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn. 36.
    ///
    /// # Errors
    ///
    /// [`Error::TemperatureOutOfRange`].
    pub fn vap_pres_from_t_dew_point(self, t_dew_point: f64) -> Result<f64> {
        self.sat_vap_pres(t_dew_point)
    }

    // ------------------------------------------------------------------
    // Conversions from wet-bulb temperature, dew-point temperature, or
    // relative humidity to humidity ratio
    // ------------------------------------------------------------------

    /// Returns the wet-bulb temperature given dry-bulb temperature, humidity
    /// ratio, and pressure.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 33 and 35
    /// solved for `t*`, using bisection between the dew point and the dry bulb.
    ///
    /// # Errors
    ///
    /// [`Error::NegativeHumRatio`] if `hum_ratio < 0`,
    /// [`Error::NoConvergence`] if the iteration limit is exceeded, plus any
    /// error of the underlying calculations.
    pub fn t_wet_bulb_from_hum_ratio(
        self,
        t_dry_bulb: f64,
        hum_ratio: f64,
        pressure: f64,
    ) -> Result<f64> {
        let bounded = bounded_hum_ratio(hum_ratio)?;
        let t_dew_point = self.t_dew_point_from_hum_ratio(t_dry_bulb, bounded, pressure)?;

        // Initial bracket and guess.
        let mut t_sup = t_dry_bulb;
        let mut t_inf = t_dew_point;
        let mut t_wet_bulb = (t_inf + t_sup) / 2.0;

        let tolerance = self.tolerance();
        let mut iterations = 0;
        while t_sup - t_inf > tolerance {
            if iterations >= MAX_ITER_COUNT {
                return Err(Error::NoConvergence {
                    function: "t_wet_bulb_from_hum_ratio",
                });
            }
            iterations += 1;

            // Humidity ratio at the trial wet-bulb temperature.
            let w_star = self.hum_ratio_from_t_wet_bulb(t_dry_bulb, t_wet_bulb, pressure)?;

            // Narrow the bracket and make a new guess.
            if w_star > bounded {
                t_sup = t_wet_bulb;
            } else {
                t_inf = t_wet_bulb;
            }
            t_wet_bulb = (t_sup + t_inf) / 2.0;
        }
        Ok(t_wet_bulb)
    }

    /// Returns the humidity ratio given dry-bulb temperature, wet-bulb
    /// temperature, and pressure, in lb(H₂O) lb(air)⁻¹ (IP) or kg(H₂O) kg(air)⁻¹
    /// (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 33 and 35.
    ///
    /// # Errors
    ///
    /// [`Error::WetBulbAboveDryBulb`] if `t_wet_bulb > t_dry_bulb`, or
    /// [`Error::TemperatureOutOfRange`].
    pub fn hum_ratio_from_t_wet_bulb(
        self,
        t_dry_bulb: f64,
        t_wet_bulb: f64,
        pressure: f64,
    ) -> Result<f64> {
        ensure(t_wet_bulb <= t_dry_bulb, Error::WetBulbAboveDryBulb)?;

        let w_s_star = self.sat_hum_ratio(t_wet_bulb, pressure)?;
        let hum_ratio = match self.units {
            UnitSystem::Ip => {
                if t_wet_bulb >= FREEZING_POINT_WATER_IP {
                    ((1093.0 - 0.556 * t_wet_bulb) * w_s_star - 0.240 * (t_dry_bulb - t_wet_bulb))
                        / (1093.0 + 0.444 * t_dry_bulb - t_wet_bulb)
                } else {
                    ((1220.0 - 0.04 * t_wet_bulb) * w_s_star - 0.240 * (t_dry_bulb - t_wet_bulb))
                        / (1220.0 + 0.444 * t_dry_bulb - 0.48 * t_wet_bulb)
                }
            }
            UnitSystem::Si => {
                if t_wet_bulb >= FREEZING_POINT_WATER_SI {
                    ((2501.0 - 2.326 * t_wet_bulb) * w_s_star - 1.006 * (t_dry_bulb - t_wet_bulb))
                        / (2501.0 + 1.86 * t_dry_bulb - 4.186 * t_wet_bulb)
                } else {
                    ((2830.0 - 0.24 * t_wet_bulb) * w_s_star - 1.006 * (t_dry_bulb - t_wet_bulb))
                        / (2830.0 + 1.86 * t_dry_bulb - 2.1 * t_wet_bulb)
                }
            }
        };
        Ok(hum_ratio.max(MIN_HUM_RATIO))
    }

    /// Returns the humidity ratio given dry-bulb temperature, relative
    /// humidity, and pressure, in lb(H₂O) lb(air)⁻¹ (IP) or kg(H₂O) kg(air)⁻¹ (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1.
    ///
    /// # Errors
    ///
    /// [`Error::RelHumOutOfRange`] if `rel_hum` is outside `[0, 1]`, or
    /// [`Error::TemperatureOutOfRange`].
    pub fn hum_ratio_from_rel_hum(
        self,
        t_dry_bulb: f64,
        rel_hum: f64,
        pressure: f64,
    ) -> Result<f64> {
        check_rel_hum(rel_hum)?;
        let vap_pres = self.vap_pres_from_rel_hum(t_dry_bulb, rel_hum)?;
        self.hum_ratio_from_vap_pres(vap_pres, pressure)
    }

    /// Returns the relative humidity given dry-bulb temperature, humidity
    /// ratio, and pressure.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1.
    ///
    /// # Errors
    ///
    /// [`Error::NegativeHumRatio`] if `hum_ratio < 0`, or
    /// [`Error::TemperatureOutOfRange`].
    pub fn rel_hum_from_hum_ratio(
        self,
        t_dry_bulb: f64,
        hum_ratio: f64,
        pressure: f64,
    ) -> Result<f64> {
        ensure(hum_ratio >= 0.0, Error::NegativeHumRatio)?;
        let vap_pres = self.vap_pres_from_hum_ratio(hum_ratio, pressure)?;
        self.rel_hum_from_vap_pres(t_dry_bulb, vap_pres)
    }

    /// Returns the humidity ratio given dew-point temperature and pressure, in
    /// lb(H₂O) lb(air)⁻¹ (IP) or kg(H₂O) kg(air)⁻¹ (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1.
    ///
    /// # Errors
    ///
    /// [`Error::TemperatureOutOfRange`].
    pub fn hum_ratio_from_t_dew_point(self, t_dew_point: f64, pressure: f64) -> Result<f64> {
        let vap_pres = self.sat_vap_pres(t_dew_point)?;
        self.hum_ratio_from_vap_pres(vap_pres, pressure)
    }

    /// Returns the dew-point temperature given dry-bulb temperature, humidity
    /// ratio, and pressure.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1.
    ///
    /// # Errors
    ///
    /// [`Error::NegativeHumRatio`] if `hum_ratio < 0`, plus any error of
    /// [`t_dew_point_from_vap_pres`](Self::t_dew_point_from_vap_pres).
    pub fn t_dew_point_from_hum_ratio(
        self,
        t_dry_bulb: f64,
        hum_ratio: f64,
        pressure: f64,
    ) -> Result<f64> {
        ensure(hum_ratio >= 0.0, Error::NegativeHumRatio)?;
        let vap_pres = self.vap_pres_from_hum_ratio(hum_ratio, pressure)?;
        self.t_dew_point_from_vap_pres(t_dry_bulb, vap_pres)
    }

    // ------------------------------------------------------------------
    // Conversions between humidity ratio and vapor pressure
    // ------------------------------------------------------------------

    /// Returns the humidity ratio given water vapor pressure and atmospheric
    /// pressure, in lb(H₂O) lb(air)⁻¹ (IP) or kg(H₂O) kg(air)⁻¹ (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 20.
    ///
    /// # Errors
    ///
    /// [`Error::NegativeVapPres`] if `vap_pres < 0`.
    pub fn hum_ratio_from_vap_pres(self, vap_pres: f64, pressure: f64) -> Result<f64> {
        ensure(vap_pres >= 0.0, Error::NegativeVapPres)?;
        let hum_ratio = 0.621945 * vap_pres / (pressure - vap_pres);
        Ok(hum_ratio.max(MIN_HUM_RATIO))
    }

    /// Returns the vapor pressure given humidity ratio and pressure, in psi
    /// (IP) or Pa (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 20 solved
    /// for `pw`.
    ///
    /// # Errors
    ///
    /// [`Error::NegativeHumRatio`] if `hum_ratio < 0`.
    pub fn vap_pres_from_hum_ratio(self, hum_ratio: f64, pressure: f64) -> Result<f64> {
        let bounded = bounded_hum_ratio(hum_ratio)?;
        Ok(pressure * bounded / (0.621945 + bounded))
    }

    // ------------------------------------------------------------------
    // Conversions between humidity ratio and specific humidity
    // ------------------------------------------------------------------

    /// Returns the specific humidity from the humidity ratio (aka mixing
    /// ratio).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 9b.
    ///
    /// # Errors
    ///
    /// [`Error::NegativeHumRatio`] if `hum_ratio < 0`.
    pub fn specific_hum_from_hum_ratio(self, hum_ratio: f64) -> Result<f64> {
        let bounded = bounded_hum_ratio(hum_ratio)?;
        Ok(bounded / (1.0 + bounded))
    }

    /// Returns the humidity ratio (aka mixing ratio) from the specific
    /// humidity.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 9b (solved
    /// for humidity ratio).
    ///
    /// # Errors
    ///
    /// [`Error::SpecificHumOutOfRange`] if `specific_hum` is outside `[0, 1)`.
    pub fn hum_ratio_from_specific_hum(self, specific_hum: f64) -> Result<f64> {
        ensure(
            (0.0..1.0).contains(&specific_hum),
            Error::SpecificHumOutOfRange,
        )?;
        let hum_ratio = specific_hum / (1.0 - specific_hum);
        Ok(hum_ratio.max(MIN_HUM_RATIO))
    }

    // ------------------------------------------------------------------
    // Dry air calculations
    // ------------------------------------------------------------------

    /// Returns the dry-air enthalpy given dry-bulb temperature, in Btu lb⁻¹
    /// (IP) or J kg⁻¹ (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn. 28.
    #[must_use]
    pub fn dry_air_enthalpy(self, t_dry_bulb: f64) -> f64 {
        match self.units {
            UnitSystem::Ip => 0.240 * t_dry_bulb,
            UnitSystem::Si => 1006.0 * t_dry_bulb,
        }
    }

    /// Returns the dry-air density given dry-bulb temperature and pressure, in
    /// lb ft⁻³ (IP) or kg m⁻³ (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1, eqn 14 for the
    /// perfect-gas relationship and eqn 1 for the gas constant. The factor 144
    /// in IP converts psi (lb in⁻²) to lb ft⁻².
    #[must_use]
    pub fn dry_air_density(self, t_dry_bulb: f64, pressure: f64) -> f64 {
        match self.units {
            UnitSystem::Ip => (144.0 * pressure) / R_DA_IP / rankine_from_fahrenheit(t_dry_bulb),
            UnitSystem::Si => pressure / R_DA_SI / kelvin_from_celsius(t_dry_bulb),
        }
    }

    /// Returns the dry-air volume given dry-bulb temperature and pressure, in
    /// ft³ lb⁻¹ (IP) or m³ kg⁻¹ (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1, eqn 14 for the
    /// perfect-gas relationship and eqn 1 for the gas constant. The factor 144
    /// in IP converts psi (lb in⁻²) to lb ft⁻².
    #[must_use]
    pub fn dry_air_volume(self, t_dry_bulb: f64, pressure: f64) -> f64 {
        match self.units {
            UnitSystem::Ip => R_DA_IP * rankine_from_fahrenheit(t_dry_bulb) / (144.0 * pressure),
            UnitSystem::Si => R_DA_SI * kelvin_from_celsius(t_dry_bulb) / pressure,
        }
    }

    /// Returns the dry-bulb temperature from moist-air enthalpy (Btu lb⁻¹ (IP)
    /// or J kg⁻¹ (SI)) and humidity ratio.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 30, based on
    /// [`moist_air_enthalpy`](Self::moist_air_enthalpy) rearranged for
    /// temperature.
    ///
    /// # Errors
    ///
    /// [`Error::NegativeHumRatio`] if `hum_ratio < 0`.
    pub fn t_dry_bulb_from_enthalpy_and_hum_ratio(
        self,
        moist_air_enthalpy: f64,
        hum_ratio: f64,
    ) -> Result<f64> {
        let bounded = bounded_hum_ratio(hum_ratio)?;
        Ok(match self.units {
            UnitSystem::Ip => (moist_air_enthalpy - 1061.0 * bounded) / (0.240 + 0.444 * bounded),
            UnitSystem::Si => {
                (moist_air_enthalpy / 1000.0 - 2501.0 * bounded) / (1.006 + 1.86 * bounded)
            }
        })
    }

    /// Returns the humidity ratio from moist-air enthalpy (Btu lb⁻¹ (IP) or
    /// J kg⁻¹ (SI)) and dry-bulb temperature.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 30, based on
    /// [`moist_air_enthalpy`](Self::moist_air_enthalpy) rearranged for
    /// humidity ratio.
    #[must_use]
    pub fn hum_ratio_from_enthalpy_and_t_dry_bulb(
        self,
        moist_air_enthalpy: f64,
        t_dry_bulb: f64,
    ) -> f64 {
        let hum_ratio = match self.units {
            UnitSystem::Ip => {
                (moist_air_enthalpy - 0.240 * t_dry_bulb) / (1061.0 + 0.444 * t_dry_bulb)
            }
            UnitSystem::Si => {
                (moist_air_enthalpy / 1000.0 - 1.006 * t_dry_bulb) / (2501.0 + 1.86 * t_dry_bulb)
            }
        };
        hum_ratio.max(MIN_HUM_RATIO)
    }

    // ------------------------------------------------------------------
    // Saturated air calculations
    // ------------------------------------------------------------------

    /// Returns the saturation vapor pressure given dry-bulb temperature, in
    /// psi (IP) or Pa (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn. 5 & 6.
    ///
    /// The ASHRAE formulae are defined above and below the freezing point but
    /// have a small discontinuity there. Here they are switched at the triple
    /// point of water instead, which removes the discontinuity. This is
    /// essential for [`t_dew_point_from_vap_pres`](Self::t_dew_point_from_vap_pres),
    /// which inverts this function, to converge properly around freezing.
    ///
    /// # Errors
    ///
    /// [`Error::TemperatureOutOfRange`] if `t_dry_bulb` is outside
    /// `[-148, 392]` °F (IP) or `[-100, 200]` °C (SI).
    pub fn sat_vap_pres(self, t_dry_bulb: f64) -> Result<f64> {
        let (min, max) = self.temperature_bounds();
        ensure(
            (min..=max).contains(&t_dry_bulb),
            Error::TemperatureOutOfRange { min, max },
        )?;

        let ln_pws = match self.units {
            UnitSystem::Ip => {
                let t = rankine_from_fahrenheit(t_dry_bulb);
                if t_dry_bulb <= TRIPLE_POINT_WATER_IP {
                    -1.0214165e4 / t - 4.8932428 - 5.3765794e-3 * t
                        + 1.9202377e-7 * t * t
                        + 3.5575832e-10 * t.powi(3)
                        - 9.0344688e-14 * t.powi(4)
                        + 4.1635019 * t.ln()
                } else {
                    -1.0440397e4 / t - 1.1294650e1 - 2.7022355e-2 * t + 1.2890360e-5 * t * t
                        - 2.4780681e-9 * t.powi(3)
                        + 6.5459673 * t.ln()
                }
            }
            UnitSystem::Si => {
                let t = kelvin_from_celsius(t_dry_bulb);
                if t_dry_bulb <= TRIPLE_POINT_WATER_SI {
                    -5.6745359e3 / t + 6.3925247 - 9.677843e-3 * t
                        + 6.2215701e-7 * t * t
                        + 2.0747825e-9 * t.powi(3)
                        - 9.484024e-13 * t.powi(4)
                        + 4.1635019 * t.ln()
                } else {
                    -5.8002206e3 / t + 1.3914993 - 4.8640239e-2 * t + 4.1764768e-5 * t * t
                        - 1.4452093e-8 * t.powi(3)
                        + 6.5459673 * t.ln()
                }
            }
        };
        Ok(ln_pws.exp())
    }

    /// Returns the humidity ratio of saturated air given dry-bulb temperature
    /// and pressure, in lb(H₂O) lb(air)⁻¹ (IP) or kg(H₂O) kg(air)⁻¹ (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 36, solved
    /// for `W`.
    ///
    /// # Errors
    ///
    /// [`Error::TemperatureOutOfRange`].
    pub fn sat_hum_ratio(self, t_dry_bulb: f64, pressure: f64) -> Result<f64> {
        let sat_vap_pres = self.sat_vap_pres(t_dry_bulb)?;
        let sat_hum_ratio = 0.621945 * sat_vap_pres / (pressure - sat_vap_pres);
        Ok(sat_hum_ratio.max(MIN_HUM_RATIO))
    }

    /// Returns the saturated-air enthalpy given dry-bulb temperature and
    /// pressure, in Btu lb⁻¹ (IP) or J kg⁻¹ (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1.
    ///
    /// # Errors
    ///
    /// [`Error::TemperatureOutOfRange`].
    pub fn sat_air_enthalpy(self, t_dry_bulb: f64, pressure: f64) -> Result<f64> {
        let sat_hum_ratio = self.sat_hum_ratio(t_dry_bulb, pressure)?;
        self.moist_air_enthalpy(t_dry_bulb, sat_hum_ratio)
    }

    // ------------------------------------------------------------------
    // Moist air calculations
    // ------------------------------------------------------------------

    /// Returns the vapor pressure deficit given dry-bulb temperature, humidity
    /// ratio, and pressure, in psi (IP) or Pa (SI).
    ///
    /// Reference: Oke (1987) eqn. 2.13a.
    ///
    /// # Errors
    ///
    /// [`Error::NegativeHumRatio`] if `hum_ratio < 0`, or
    /// [`Error::TemperatureOutOfRange`].
    pub fn vapor_pressure_deficit(
        self,
        t_dry_bulb: f64,
        hum_ratio: f64,
        pressure: f64,
    ) -> Result<f64> {
        ensure(hum_ratio >= 0.0, Error::NegativeHumRatio)?;
        let rel_hum = self.rel_hum_from_hum_ratio(t_dry_bulb, hum_ratio, pressure)?;
        Ok(self.sat_vap_pres(t_dry_bulb)? * (1.0 - rel_hum))
    }

    /// Returns the degree of saturation: the humidity ratio of the air divided
    /// by the humidity ratio of saturated air at the same temperature and
    /// pressure.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2009) ch. 1 eqn. 12. The
    /// definition is absent from the 2017 Handbook.
    ///
    /// # Errors
    ///
    /// [`Error::NegativeHumRatio`] if `hum_ratio < 0`, or
    /// [`Error::TemperatureOutOfRange`].
    pub fn degree_of_saturation(
        self,
        t_dry_bulb: f64,
        hum_ratio: f64,
        pressure: f64,
    ) -> Result<f64> {
        let bounded = bounded_hum_ratio(hum_ratio)?;
        Ok(bounded / self.sat_hum_ratio(t_dry_bulb, pressure)?)
    }

    /// Returns the moist-air enthalpy given dry-bulb temperature and humidity
    /// ratio, in Btu lb⁻¹ (IP) or J kg⁻¹ (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn. 30.
    ///
    /// # Errors
    ///
    /// [`Error::NegativeHumRatio`] if `hum_ratio < 0`.
    pub fn moist_air_enthalpy(self, t_dry_bulb: f64, hum_ratio: f64) -> Result<f64> {
        let bounded = bounded_hum_ratio(hum_ratio)?;
        Ok(match self.units {
            UnitSystem::Ip => 0.240 * t_dry_bulb + bounded * (1061.0 + 0.444 * t_dry_bulb),
            UnitSystem::Si => {
                (1.006 * t_dry_bulb + bounded * (2501.0 + 1.86 * t_dry_bulb)) * 1000.0
            }
        })
    }

    /// Returns the moist-air specific volume given dry-bulb temperature,
    /// humidity ratio, and pressure, in ft³ lb⁻¹ (IP) or m³ kg⁻¹ (SI) of dry
    /// air.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn. 26. In IP
    /// units `R_DA_IP / 144` equals 0.370486, the coefficient appearing in
    /// the equation; the factor 144 converts psi (lb in⁻²) to lb ft⁻².
    ///
    /// # Errors
    ///
    /// [`Error::NegativeHumRatio`] if `hum_ratio < 0`.
    pub fn moist_air_volume(self, t_dry_bulb: f64, hum_ratio: f64, pressure: f64) -> Result<f64> {
        let bounded = bounded_hum_ratio(hum_ratio)?;
        Ok(match self.units {
            UnitSystem::Ip => {
                R_DA_IP * rankine_from_fahrenheit(t_dry_bulb) * (1.0 + 1.607858 * bounded)
                    / (144.0 * pressure)
            }
            UnitSystem::Si => {
                R_DA_SI * kelvin_from_celsius(t_dry_bulb) * (1.0 + 1.607858 * bounded) / pressure
            }
        })
    }

    /// Returns the dry-bulb temperature given moist-air specific volume,
    /// humidity ratio, and pressure.
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 26, based on
    /// [`moist_air_volume`](Self::moist_air_volume) rearranged for
    /// dry-bulb temperature.
    ///
    /// # Errors
    ///
    /// [`Error::NegativeHumRatio`] if `hum_ratio < 0`.
    pub fn t_dry_bulb_from_moist_air_volume_and_hum_ratio(
        self,
        moist_air_volume: f64,
        hum_ratio: f64,
        pressure: f64,
    ) -> Result<f64> {
        let bounded = bounded_hum_ratio(hum_ratio)?;
        Ok(match self.units {
            UnitSystem::Ip => fahrenheit_from_rankine(
                moist_air_volume * (144.0 * pressure) / (R_DA_IP * (1.0 + 1.607858 * bounded)),
            ),
            UnitSystem::Si => celsius_from_kelvin(
                moist_air_volume * pressure / (R_DA_SI * (1.0 + 1.607858 * bounded)),
            ),
        })
    }

    /// Returns the moist-air density given dry-bulb temperature, humidity
    /// ratio, and pressure, in lb ft⁻³ (IP) or kg m⁻³ (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn. 11.
    ///
    /// # Errors
    ///
    /// [`Error::NegativeHumRatio`] if `hum_ratio < 0`.
    pub fn moist_air_density(self, t_dry_bulb: f64, hum_ratio: f64, pressure: f64) -> Result<f64> {
        let bounded = bounded_hum_ratio(hum_ratio)?;
        Ok((1.0 + bounded) / self.moist_air_volume(t_dry_bulb, bounded, pressure)?)
    }

    // ------------------------------------------------------------------
    // Standard atmosphere
    // ------------------------------------------------------------------

    /// Returns the standard-atmosphere barometric pressure at the given
    /// altitude (ft (IP) or m (SI)), in psi (IP) or Pa (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 3.
    #[must_use]
    pub fn standard_atm_pressure(self, altitude: f64) -> f64 {
        match self.units {
            UnitSystem::Ip => 14.696 * (1.0 - 6.8754e-6 * altitude).powf(5.2559),
            UnitSystem::Si => 101_325.0 * (1.0 - 2.25577e-5 * altitude).powf(5.2559),
        }
    }

    /// Returns the standard-atmosphere dry-bulb temperature at the given
    /// altitude (ft (IP) or m (SI)), in °F (IP) or °C (SI).
    ///
    /// Reference: ASHRAE Handbook - Fundamentals (2017) ch. 1 eqn 4.
    #[must_use]
    pub fn standard_atm_temperature(self, altitude: f64) -> f64 {
        match self.units {
            UnitSystem::Ip => 59.0 - 0.00356620 * altitude,
            UnitSystem::Si => 15.0 - 0.0065 * altitude,
        }
    }

    /// Returns the sea-level pressure given station pressure, altitude above
    /// sea level (ft (IP) or m (SI)), and dry-bulb temperature.
    ///
    /// Reference: Hess SL, *Introduction to theoretical meteorology*, Holt
    /// Rinehart and Winston, NY 1959, ch. 6.5; Stull RB, *Meteorology for
    /// scientists and engineers*, 2nd edition, Brooks/Cole 2000, ch. 1.
    ///
    /// The standard procedure for the US is to use for `t_dry_bulb` the
    /// average of the current station temperature and the station temperature
    /// from 12 hours ago.
    #[must_use]
    pub fn sea_level_pressure(self, stn_pressure: f64, altitude: f64, t_dry_bulb: f64) -> f64 {
        // Scale height from the mean temperature of the air column, assuming a
        // constant lapse rate (3.6 °F/1000 ft in IP, 6.5 °C/km in SI).
        let scale_height = match self.units {
            UnitSystem::Ip => {
                let t_column = t_dry_bulb + 0.0036 * altitude / 2.0;
                53.351 * rankine_from_fahrenheit(t_column)
            }
            UnitSystem::Si => {
                let t_column = t_dry_bulb + 0.0065 * altitude / 2.0;
                287.055 * kelvin_from_celsius(t_column) / 9.807
            }
        };
        stn_pressure * (altitude / scale_height).exp()
    }

    /// Returns the station pressure from sea-level pressure.
    ///
    /// This is the inverse of [`sea_level_pressure`](Self::sea_level_pressure);
    /// see it for references.
    #[must_use]
    pub fn station_pressure(self, sea_level_pressure: f64, altitude: f64, t_dry_bulb: f64) -> f64 {
        sea_level_pressure / self.sea_level_pressure(1.0, altitude, t_dry_bulb)
    }

    // ------------------------------------------------------------------
    // Functions to compute all psychrometric values at once
    // ------------------------------------------------------------------

    /// Computes the full moist-air state from dry-bulb temperature, wet-bulb
    /// temperature, and pressure.
    ///
    /// # Errors
    ///
    /// [`Error::WetBulbAboveDryBulb`] if `t_wet_bulb > t_dry_bulb`, plus any
    /// error of the underlying calculations.
    pub fn psychrometrics_from_t_wet_bulb(
        self,
        t_dry_bulb: f64,
        t_wet_bulb: f64,
        pressure: f64,
    ) -> Result<MoistAirState> {
        ensure(t_wet_bulb <= t_dry_bulb, Error::WetBulbAboveDryBulb)?;

        let hum_ratio = self.hum_ratio_from_t_wet_bulb(t_dry_bulb, t_wet_bulb, pressure)?;
        let t_dew_point = self.t_dew_point_from_hum_ratio(t_dry_bulb, hum_ratio, pressure)?;
        self.state_from_hum_ratio(t_dry_bulb, hum_ratio, pressure, t_wet_bulb, t_dew_point)
    }

    /// Computes the full moist-air state from dry-bulb temperature, dew-point
    /// temperature, and pressure.
    ///
    /// # Errors
    ///
    /// [`Error::DewPointAboveDryBulb`] if `t_dew_point > t_dry_bulb`, plus any
    /// error of the underlying calculations.
    pub fn psychrometrics_from_t_dew_point(
        self,
        t_dry_bulb: f64,
        t_dew_point: f64,
        pressure: f64,
    ) -> Result<MoistAirState> {
        ensure(t_dew_point <= t_dry_bulb, Error::DewPointAboveDryBulb)?;

        let hum_ratio = self.hum_ratio_from_t_dew_point(t_dew_point, pressure)?;
        let t_wet_bulb = self.t_wet_bulb_from_hum_ratio(t_dry_bulb, hum_ratio, pressure)?;
        self.state_from_hum_ratio(t_dry_bulb, hum_ratio, pressure, t_wet_bulb, t_dew_point)
    }

    /// Computes the full moist-air state from dry-bulb temperature, relative
    /// humidity, and pressure.
    ///
    /// # Errors
    ///
    /// [`Error::RelHumOutOfRange`] if `rel_hum` is outside `[0, 1]`, plus any
    /// error of the underlying calculations.
    pub fn psychrometrics_from_rel_hum(
        self,
        t_dry_bulb: f64,
        rel_hum: f64,
        pressure: f64,
    ) -> Result<MoistAirState> {
        check_rel_hum(rel_hum)?;

        let hum_ratio = self.hum_ratio_from_rel_hum(t_dry_bulb, rel_hum, pressure)?;
        let t_wet_bulb = self.t_wet_bulb_from_hum_ratio(t_dry_bulb, hum_ratio, pressure)?;
        let t_dew_point = self.t_dew_point_from_hum_ratio(t_dry_bulb, hum_ratio, pressure)?;
        self.state_from_hum_ratio(t_dry_bulb, hum_ratio, pressure, t_wet_bulb, t_dew_point)
    }

    /// Assembles a [`MoistAirState`] once the humidity ratio and both derived
    /// temperatures are known. Relative humidity is always recomputed from the
    /// humidity ratio, as in the original library.
    fn state_from_hum_ratio(
        self,
        t_dry_bulb: f64,
        hum_ratio: f64,
        pressure: f64,
        t_wet_bulb: f64,
        t_dew_point: f64,
    ) -> Result<MoistAirState> {
        Ok(MoistAirState {
            t_dry_bulb,
            t_wet_bulb,
            t_dew_point,
            rel_hum: self.rel_hum_from_hum_ratio(t_dry_bulb, hum_ratio, pressure)?,
            hum_ratio,
            vap_pres: self.vap_pres_from_hum_ratio(hum_ratio, pressure)?,
            moist_air_enthalpy: self.moist_air_enthalpy(t_dry_bulb, hum_ratio)?,
            moist_air_volume: self.moist_air_volume(t_dry_bulb, hum_ratio, pressure)?,
            degree_of_saturation: self.degree_of_saturation(t_dry_bulb, hum_ratio, pressure)?,
        })
    }
}
