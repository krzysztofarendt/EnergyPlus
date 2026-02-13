//! Psychrometric calculations for EnergyPlus-rs.
//!
//! All functions use ASHRAE formulas ported from EnergyPlus Psychrometrics.cc.
//! Temperature inputs are in Celsius, pressure in Pascals, humidity ratio in kg/kg.
//!
//! Reference: ASHRAE Handbook of Fundamentals

// ep_units types used via raw f64 for direct C++ formula compatibility.

/// Minimum humidity ratio clamp to avoid division by zero.
const W_MIN: f64 = 1.0e-5;

/// Ratio of molecular mass of water to air (Mw/Ma = 18.015/28.966).
const RATIO_MW_MA: f64 = 0.62198;

/// Gas constant ratio (1 + Mw/Ma) = 1.6077687.
const GAS_CONSTANT_RATIO: f64 = 1.6077687;

/// Universal gas constant for dry air (J/(kg*K)).
const R_DA: f64 = 287.0;

/// Universal gas constant for water vapor (J/(kg*K)).
const R_W: f64 = 461.52;

// ===========================================================================
// Saturation pressure (Hyland-Wexler formulation)
// ===========================================================================

/// Coefficients for saturation pressure over ice (T < 0.01°C).
const C1: f64 = -5674.5359;
const C2: f64 = 6.3925247;
const C3: f64 = -0.9677843e-2;
const C4: f64 = 0.62215701e-6;
const C5: f64 = 0.20747825e-8;
const C6: f64 = -0.9484024e-12;
const C7: f64 = 4.1635019;

/// Coefficients for saturation pressure over liquid water (T >= 0.01°C).
const C8: f64 = -5800.2206;
const C9: f64 = 1.3914993;
const C10: f64 = -0.048640239;
const C11: f64 = 0.41764768e-4;
const C12: f64 = -0.14452093e-7;
const C13: f64 = 6.5459673;

/// Saturation pressure of water vapor as a function of temperature.
///
/// Uses ASHRAE Handbook of Fundamentals 2005, Chapter 6, Equations 5 & 6
/// (Hyland-Wexler formulation). Valid for -100°C to 200°C.
///
/// # Arguments
/// * `t_db` - Dry-bulb temperature in Celsius
///
/// # Returns
/// Saturation pressure in Pascals
pub fn saturation_pressure(t_db: f64) -> f64 {
    if t_db < -100.0 {
        // Below valid range - return extrapolated value
        0.001_405_102_123_874_164
    } else if t_db <= 0.01 {
        // Ice region
        let t_kel = t_db + 273.15;
        let ln_t = t_kel.ln();
        (C1 / t_kel + C2 + t_kel * (C3 + t_kel * (C4 + t_kel * (C5 + C6 * t_kel))) + C7 * ln_t).exp()
    } else if t_db <= 200.0 {
        // Liquid water region
        let t_kel = t_db + 273.15;
        let ln_t = t_kel.ln();
        (C8 / t_kel + C9 + t_kel * (C10 + t_kel * (C11 + C12 * t_kel)) + C13 * ln_t).exp()
    } else {
        // Above valid range - return extrapolated value
        1_555_073.745_636_215
    }
}

// ===========================================================================
// Primary psychrometric functions
// ===========================================================================

/// Air density from barometric pressure, dry-bulb temperature, and humidity ratio.
///
/// Uses ideal gas law: rho = Pb / (R_da * T * (1 + 1.6078 * W))
///
/// # Arguments
/// * `pb` - Barometric pressure (Pa)
/// * `t_db` - Dry-bulb temperature (°C)
/// * `w` - Humidity ratio (kg water / kg dry air)
pub fn rho_air(pb: f64, t_db: f64, w: f64) -> f64 {
    let w = w.max(W_MIN);
    pb / (R_DA * (t_db + 273.15) * (1.0 + GAS_CONSTANT_RATIO * w))
}

/// Enthalpy of moist air from dry-bulb temperature and humidity ratio.
///
/// H = 1004.84*T + W*(2500940 + 1858.95*T) [J/kg]
///
/// Reference: ASHRAE Handbook of Fundamentals 1972, P100, EQN 32
pub fn enthalpy(t_db: f64, w: f64) -> f64 {
    let w = w.max(W_MIN);
    1.004_84e3 * t_db + w * (2.500_94e6 + 1.858_95e3 * t_db)
}

/// Specific heat capacity of moist air from humidity ratio.
///
/// Cp = 1004.84 + W * 1858.95 [J/(kg·°C)]
pub fn cp_air(w: f64) -> f64 {
    let w = w.max(W_MIN);
    1.004_84e3 + w * 1.858_95e3
}

/// Dry-bulb temperature from enthalpy and humidity ratio.
///
/// Inverse of `enthalpy()`.
pub fn t_db_from_enthalpy_w(h: f64, w: f64) -> f64 {
    let w = w.max(W_MIN);
    (h - 2.500_94e6 * w) / (1.004_84e3 + 1.858_95e3 * w)
}

/// Humidity ratio from dry-bulb temperature and enthalpy.
///
/// Inverse of `enthalpy()` solved for W.
pub fn w_from_t_db_h(t_db: f64, h: f64) -> f64 {
    let w = (h - 1.004_84e3 * t_db) / (2.500_94e6 + 1.858_95e3 * t_db);
    w.max(W_MIN)
}

/// Humidity ratio from dew-point temperature and barometric pressure.
///
/// W = 0.62198 * Psat(Tdp) / (Pb - Psat(Tdp))
///
/// Reference: ASHRAE Handbook of Fundamentals 1972, P99, EQN 22
pub fn w_from_t_dp_pb(t_dp: f64, pb: f64) -> f64 {
    let p_sat = saturation_pressure(t_dp);
    let w = RATIO_MW_MA * p_sat / (pb - p_sat);
    w.max(W_MIN)
}

/// Humidity ratio from dry-bulb temperature, relative humidity, and barometric pressure.
///
/// Reference: ASHRAE Handbook of Fundamentals 1972, P99, EQN 22
pub fn w_from_t_db_rh_pb(t_db: f64, rh: f64, pb: f64) -> f64 {
    let p_dew = rh * saturation_pressure(t_db);
    let w = RATIO_MW_MA * p_dew / (pb - p_dew).max(1000.0);
    w.max(W_MIN)
}

/// Humidity ratio from dry-bulb, wet-bulb temperature, and barometric pressure.
///
/// Reference: ASHRAE Handbook of Fundamentals 1972, P99, EQ 22, 35
pub fn w_from_t_db_twb_pb(t_db: f64, twb: f64, pb: f64) -> f64 {
    let twb = twb.min(t_db);
    let p_sat_wb = saturation_pressure(twb);
    let w_star = RATIO_MW_MA * p_sat_wb / (pb - p_sat_wb);

    let w = if twb >= 0.0 {
        ((2501.0 - 2.326 * twb) * w_star - 1.006 * (t_db - twb)) / (2501.0 + 1.86 * t_db - 4.186 * twb)
    } else {
        ((2830.0 - 0.24 * twb) * w_star - 1.006 * (t_db - twb)) / (2830.0 + 1.86 * t_db - 2.1 * twb)
    };
    w.max(W_MIN)
}

/// Relative humidity from dry-bulb temperature, humidity ratio, and barometric pressure.
///
/// Reference: ASHRAE Handbook Fundamentals 1985, P6.12, EQN 10, 21, 23
pub fn rh_from_t_db_w_pb(t_db: f64, w: f64, pb: f64) -> f64 {
    let w = w.max(W_MIN);
    let pws = saturation_pressure(t_db);
    let u = w / (RATIO_MW_MA * pws / (pb - pws)); // degree of saturation
    let rh = u / (1.0 - (1.0 - u) * (pws / pb));
    rh.clamp(0.01, 1.0)
}

/// Specific volume from dry-bulb temperature, humidity ratio, and barometric pressure.
///
/// V = 159.473 * (1 + 1.6078*W) * (1.8*T + 492) / Pb [m³/kg]
///
/// Reference: ASHRAE Handbook of Fundamentals 1972, P99, EQN 28
pub fn specific_volume(t_db: f64, w: f64, pb: f64) -> f64 {
    let w = w.max(W_MIN);
    let v = 1.594_73e2 * (1.0 + 1.6078 * w) * (1.8 * t_db + 492.0) / pb;
    if v < 0.0 {
        0.83
    } else {
        v
    }
}

/// Dew-point temperature from humidity ratio and barometric pressure.
pub fn t_dp_from_w_pb(w: f64, pb: f64) -> f64 {
    let w = w.max(W_MIN);
    let p_dew = pb * w / (RATIO_MW_MA + w);
    t_sat_from_pressure(p_dew)
}

/// Dew-point temperature from dry-bulb, wet-bulb, and barometric pressure.
pub fn t_dp_from_t_db_twb_pb(t_db: f64, twb: f64, pb: f64) -> f64 {
    let w = w_from_t_db_twb_pb(t_db, twb, pb);
    let tdp = t_dp_from_w_pb(w, pb);
    tdp.min(twb)
}

/// Enthalpy from dry-bulb temperature, relative humidity, and barometric pressure.
pub fn enthalpy_from_t_db_rh_pb(t_db: f64, rh: f64, pb: f64) -> f64 {
    let w = w_from_t_db_rh_pb(t_db, rh, pb);
    enthalpy(t_db, w)
}

/// Vapor density from dry-bulb temperature and relative humidity.
///
/// rho_v = Psat(T) * RH / (R_w * T_K)
pub fn rho_vapor_from_t_db_rh(t_db: f64, rh: f64) -> f64 {
    let p_sat = saturation_pressure(t_db);
    p_sat * rh / (R_W * (t_db + 273.15))
}

/// Vapor density from dry-bulb temperature, humidity ratio, and barometric pressure.
pub fn rho_vapor_from_t_db_w_pb(t_db: f64, w: f64, pb: f64) -> f64 {
    let w = w.max(W_MIN);
    w * pb / (R_W * (t_db + 273.15) * (w + RATIO_MW_MA))
}

/// Relative humidity from dry-bulb temperature and vapor density.
pub fn rh_from_t_db_rho_vapor(t_db: f64, rho_vapor: f64) -> f64 {
    if rho_vapor <= 0.0 {
        return 0.0;
    }
    let rh = rho_vapor * R_W * (t_db + 273.15) / saturation_pressure(t_db);
    rh.clamp(0.01, 1.0)
}

/// Latent heat of vaporization of air.
///
/// Hfg = 2500940 + 1858.95*T - 4180*T [J/kg]
pub fn latent_heat_of_vaporization(_w: f64, t_db: f64) -> f64 {
    2_500_940.0 + 1_858.95 * t_db - 4180.0 * t_db
}

/// Enthalpy of water vapor as gas.
///
/// Hg = 2500940 + 1858.95*T [J/kg]
pub fn enthalpy_gas(_w: f64, t_db: f64) -> f64 {
    2_500_940.0 + 1_858.95 * t_db
}

/// Sensible enthalpy difference between two states at constant humidity ratio.
///
/// DeltaH = Cp(W) * (T2 - T1)
pub fn delta_h_sensible(t_db2: f64, t_db1: f64, w: f64) -> f64 {
    (1.004_84e3 + w.max(W_MIN) * 1.858_95e3) * (t_db2 - t_db1)
}

/// Sensible enthalpy difference between two states with different humidity ratios.
///
/// Uses minimum humidity ratio of the two states.
pub fn delta_h_sensible_2state(t_db2: f64, w2: f64, t_db1: f64, w1: f64) -> f64 {
    let w_min = w1.min(w2);
    delta_h_sensible(t_db2, t_db1, w_min)
}

/// Specific heat of chilled/hot water (constant 4180 J/(kg·K)).
pub fn cp_water(_t: f64) -> f64 {
    4180.0
}

/// Density of water as function of temperature.
///
/// rho = 1000.1207 + 8.3216e-4*T - 4.9300e-3*T² + 8.4792e-6*T³
pub fn rho_water(t_db: f64) -> f64 {
    1000.1207 + 8.321_587_4e-4 * t_db - 4.929_976e-3 * t_db * t_db + 8.479_186_3e-6 * t_db * t_db * t_db
}

// ===========================================================================
// Iterative functions
// ===========================================================================

/// Saturation temperature from pressure (Newton-Raphson iteration).
///
/// Reference: ASHRAE Handbook 1989 Fundamentals
pub fn t_sat_from_pressure(press: f64) -> f64 {
    if press <= 0.0017 {
        return -100.0;
    }
    if press >= 1_555_000.0 {
        return 200.0;
    }

    // Initial guess
    let mut t_sat = 100.0;
    if press < saturation_pressure(-40.0) {
        t_sat = -40.0;
    } else if press > saturation_pressure(150.0) {
        t_sat = 150.0;
    }

    // Newton-Raphson iteration
    let itmax = 50;
    for _ in 0..itmax {
        let p_calc = saturation_pressure(t_sat);
        let error = press - p_calc;

        if error.abs() < 0.1 {
            // Converged (0.1 Pa tolerance)
            break;
        }

        // Numerical derivative (central difference)
        let dt = 0.001;
        let p_hi = saturation_pressure(t_sat + dt);
        let p_lo = saturation_pressure(t_sat - dt);
        let dp_dt = (p_hi - p_lo) / (2.0 * dt);

        if dp_dt.abs() < 1.0e-20 {
            break;
        }

        t_sat += error / dp_dt;
        t_sat = t_sat.clamp(-100.0, 200.0);
    }

    t_sat
}

/// Wet-bulb temperature from dry-bulb, humidity ratio, and barometric pressure.
///
/// Uses iterative Newton-Raphson method.
pub fn t_wb_from_t_db_w_pb(t_db: f64, w: f64, pb: f64) -> f64 {
    let w = w.max(W_MIN);

    // WB cannot exceed DB
    if w >= w_from_t_db_twb_pb(t_db, t_db, pb) {
        return t_db;
    }

    // Initial guess: start at dew point
    let mut twb = t_dp_from_w_pb(w, pb);
    twb = twb.min(t_db);

    let itmax = 100;
    for _ in 0..itmax {
        let w_calc = w_from_t_db_twb_pb(t_db, twb, pb);
        let error = w - w_calc;

        if error.abs() < 1.0e-7 {
            break;
        }

        // Numerical derivative
        let dt = 0.001;
        let w_hi = w_from_t_db_twb_pb(t_db, twb + dt, pb);
        let dw_dt = (w_hi - w_calc) / dt;

        if dw_dt.abs() < 1.0e-20 {
            break;
        }

        twb += error / dw_dt;
        twb = twb.clamp(-100.0, t_db);
    }

    twb
}

/// Saturation temperature from enthalpy and barometric pressure.
///
/// Uses piecewise polynomial approximation with Newton-Raphson refinement.
pub fn t_sat_from_enthalpy_pb(h: f64, pb: f64) -> f64 {
    // Simple approach: find T where H(T, Wsat(T)) = H_target
    // This is equivalent to finding T on the saturation curve at enthalpy H

    // Get approximate range
    let hh = h + 17863.7;

    // Piecewise linear approximation for initial guess
    let mut t_sat = if hh < 0.0 {
        -20.0
    } else if hh < 100000.0 {
        hh / 2500.0
    } else {
        40.0
    };

    // Refine with Newton-Raphson
    for _ in 0..50 {
        let w_sat = w_from_t_db_rh_pb(t_sat, 1.0, pb);
        let h_calc = enthalpy(t_sat, w_sat);
        let error = h - h_calc;

        if error.abs() < 1.0 {
            break;
        }

        // Numerical derivative
        let dt = 0.01;
        let w_sat_hi = w_from_t_db_rh_pb(t_sat + dt, 1.0, pb);
        let h_hi = enthalpy(t_sat + dt, w_sat_hi);
        let dh_dt = (h_hi - h_calc) / dt;

        if dh_dt.abs() < 1.0e-10 {
            break;
        }

        t_sat += error / dh_dt;
        t_sat = t_sat.clamp(-100.0, 200.0);
    }

    t_sat
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const PB_STD: f64 = 101325.0;
    const TOL: f64 = 1.0e-3;

    #[test]
    fn saturation_pressure_known_values() {
        // At 100°C, saturation pressure should be ~101325 Pa (1 atm)
        let p100 = saturation_pressure(100.0);
        assert!((p100 - 101325.0).abs() / 101325.0 < 0.003, "p100={p100}");

        // At 0°C, saturation pressure should be ~611 Pa
        let p0 = saturation_pressure(0.0);
        assert!((p0 - 611.0).abs() < 2.0, "p0={p0}");

        // At 20°C, saturation pressure should be ~2338 Pa
        let p20 = saturation_pressure(20.0);
        assert!((p20 - 2338.0).abs() < 10.0, "p20={p20}");
    }

    #[test]
    fn saturation_pressure_range_limits() {
        let p_low = saturation_pressure(-150.0);
        assert!(p_low > 0.0);

        let p_high = saturation_pressure(250.0);
        assert!(p_high > 0.0);
    }

    #[test]
    fn enthalpy_at_standard_conditions() {
        // At 20°C, 50% RH, ~101325 Pa: W ≈ 0.00726
        let w = w_from_t_db_rh_pb(20.0, 0.50, PB_STD);
        let h = enthalpy(20.0, w);
        // Expected ~ 38500 J/kg
        assert!(h > 35000.0 && h < 42000.0, "h={h}");
    }

    #[test]
    fn enthalpy_roundtrip() {
        let t = 25.0;
        let w = 0.010;
        let h = enthalpy(t, w);
        let t_back = t_db_from_enthalpy_w(h, w);
        assert!((t - t_back).abs() < 1e-10, "t_back={t_back}");
    }

    #[test]
    fn humidity_ratio_roundtrip_rh() {
        let t = 30.0;
        let rh = 0.60;
        let w = w_from_t_db_rh_pb(t, rh, PB_STD);
        let rh_back = rh_from_t_db_w_pb(t, w, PB_STD);
        assert!((rh - rh_back).abs() < 0.01, "rh_back={rh_back}");
    }

    #[test]
    fn humidity_ratio_from_twb() {
        // At 30°C DB, 20°C WB, standard pressure
        let w = w_from_t_db_twb_pb(30.0, 20.0, PB_STD);
        assert!(w > 0.005 && w < 0.015, "w={w}");
    }

    #[test]
    fn wet_bulb_roundtrip() {
        let t = 30.0;
        let rh = 0.50;
        let w = w_from_t_db_rh_pb(t, rh, PB_STD);
        let twb = t_wb_from_t_db_w_pb(t, w, PB_STD);
        // WB should be between dewpoint and dry-bulb
        let tdp = t_dp_from_w_pb(w, PB_STD);
        assert!(twb >= tdp - TOL, "twb={twb} < tdp={tdp}");
        assert!(twb <= t + TOL, "twb={twb} > t={t}");
    }

    #[test]
    fn dew_point_from_w() {
        let w = 0.010;
        let tdp = t_dp_from_w_pb(w, PB_STD);
        // At W=0.010, Tdp should be around 14°C
        assert!(tdp > 10.0 && tdp < 18.0, "tdp={tdp}");
    }

    #[test]
    fn air_density_standard_conditions() {
        // At 20°C, ~1.2 kg/m³
        let rho = rho_air(PB_STD, 20.0, 0.008);
        assert!((rho - 1.2).abs() < 0.05, "rho={rho}");
    }

    #[test]
    fn specific_volume_standard() {
        let v = specific_volume(20.0, 0.008, PB_STD);
        // Should be around 0.84 m³/kg
        assert!(v > 0.80 && v < 0.90, "v={v}");
    }

    #[test]
    fn cp_air_dry() {
        let cp = cp_air(0.0);
        // Dry air Cp should be ~1004.84 J/(kg·K)
        assert!((cp - 1004.84).abs() < 1.0, "cp={cp}");
    }

    #[test]
    fn water_density() {
        let rho = rho_water(20.0);
        assert!((rho - 998.0).abs() < 5.0, "rho={rho}");

        let rho0 = rho_water(4.0);
        // Water is densest near 4°C
        assert!(rho0 > rho, "rho at 4°C should be > rho at 20°C");
    }

    #[test]
    fn t_sat_from_pressure_boiling() {
        let t = t_sat_from_pressure(101325.0);
        assert!((t - 100.0).abs() < 0.5, "t={t}");
    }

    #[test]
    fn t_sat_from_pressure_freezing() {
        let t = t_sat_from_pressure(611.0);
        assert!(t.abs() < 1.0, "t={t}");
    }

    #[test]
    fn delta_h_sensible_calculation() {
        let dh = delta_h_sensible(30.0, 20.0, 0.010);
        // ~10 * 1023.7 = ~10237 J/kg
        assert!(dh > 10000.0 && dh < 11000.0, "dh={dh}");
    }
}
