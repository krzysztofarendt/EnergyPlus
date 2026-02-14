//! Surface heat balance calculations.
//!
//! Implements the outside and inside surface heat balance equations:
//!
//! Outside: q_solar + q_lw_sky + q_lw_ground + q_conv - q_conduction = 0
//! Inside:  q_sw_gains + q_lw_internal + q_lw_surfaces + q_conv - q_conduction = 0

use crate::convection;
use ep_materials::CtfCoefficients;

/// Outside surface heat balance result.
#[derive(Debug, Clone, Default)]
pub struct OutsideHeatBalance {
    /// Absorbed solar radiation (W/m2).
    pub q_solar: f64,
    /// Net longwave radiation to sky (W/m2), typically negative (surface loses heat).
    pub q_lw_sky: f64,
    /// Net longwave radiation to ground (W/m2).
    pub q_lw_ground: f64,
    /// Convective heat flux (W/m2), positive = from air to surface.
    pub q_conv: f64,
    /// Conduction into the wall (W/m2), positive = into wall.
    pub q_conduction: f64,
    /// Resulting outside surface temperature (C).
    pub t_surface: f64,
}

/// Inside surface heat balance result.
#[derive(Debug, Clone, Default)]
pub struct InsideHeatBalance {
    /// Absorbed shortwave from internal gains (W/m2).
    pub q_sw_gains: f64,
    /// Longwave radiation exchange with other surfaces (W/m2).
    pub q_lw_surfaces: f64,
    /// Convective heat flux (W/m2), positive = from air to surface.
    pub q_conv: f64,
    /// Conduction through wall at inside face (W/m2).
    pub q_conduction: f64,
    /// Resulting inside surface temperature (C).
    pub t_surface: f64,
}

/// Solve outside surface temperature for a CTF surface.
///
/// The outside surface heat balance is:
/// q_solar_abs + q_lw + h_conv * (T_air - T_surf) = CTF_outside_flux
///
/// Rearranging for T_surf:
/// T_surf = (q_solar_abs + q_lw + h_conv * T_air + CTF_hist) / (h_conv + CTF_self)
pub fn solve_outside_surface_temp(
    ctf: &CtfCoefficients,
    // Environmental conditions
    t_air_outside: f64,    // Outside dry-bulb temperature (C)
    h_conv: f64,           // Exterior convection coefficient (W/(m2-K))
    // Radiation
    q_solar_absorbed: f64, // Absorbed solar (beam+diffuse) (W/m2)
    q_lw_radiation: f64,   // Net longwave exchange (W/m2), negative=heat loss
    // CTF history
    t_outside_history: &[f64], // Previous outside surface temps [t-1, t-2, ...]
    t_inside_history: &[f64],  // Previous inside surface temps [t-1, t-2, ...]
    q_outside_history: &[f64], // Previous outside heat flux [t-1, t-2, ...]
) -> OutsideHeatBalance {
    // CTF: q_cond = X[0]*T_outside + sum(X[j]*T_out_hist + Y[j]*T_in_hist + Phi[j]*q_hist)
    // The current-time contribution is X[0]*T_outside + Y[0]*T_inside_current
    // History terms:
    let mut hist_terms = 0.0;
    for j in 1..=ctf.num_terms {
        if j - 1 < t_outside_history.len() {
            hist_terms += ctf.outside[j] * t_outside_history[j - 1];
        }
        if j - 1 < t_inside_history.len() {
            hist_terms += ctf.cross[j] * t_inside_history[j - 1];
        }
        if j - 1 < q_outside_history.len() {
            hist_terms += ctf.flux[j] * q_outside_history[j - 1];
        }
    }

    // Heat balance: q_solar + q_lw + h_conv*(T_air - T_surf) = X[0]*T_surf + Y[0]*T_inside + hist
    // We need T_inside_current for Y[0] term - use last known value
    let t_inside_current = if !t_inside_history.is_empty() {
        t_inside_history[0]
    } else {
        t_air_outside // Approximation
    };

    let cross_term = ctf.cross[0] * t_inside_current;

    // Solve for T_surf:
    // q_solar + q_lw + h_conv*T_air - h_conv*T_surf = X[0]*T_surf + cross_term + hist
    // q_solar + q_lw + h_conv*T_air - cross_term - hist = (h_conv + X[0])*T_surf
    let denom = h_conv + ctf.outside[0];
    let t_surface = if denom.abs() > 1e-10 {
        (q_solar_absorbed + q_lw_radiation + h_conv * t_air_outside - cross_term - hist_terms) / denom
    } else {
        t_air_outside
    };

    let q_conv = h_conv * (t_air_outside - t_surface);
    let q_conduction = ctf.outside[0] * t_surface + cross_term + hist_terms;

    OutsideHeatBalance {
        q_solar: q_solar_absorbed,
        q_lw_sky: q_lw_radiation,
        q_lw_ground: 0.0,
        q_conv,
        q_conduction,
        t_surface,
    }
}

/// Solve inside surface temperature for a CTF surface.
///
/// The inside surface heat balance is:
/// q_sw_gains + q_lw_surfaces + h_conv * (T_zone - T_surf) = CTF_inside_flux
pub fn solve_inside_surface_temp(
    ctf: &CtfCoefficients,
    // Zone conditions
    t_zone: f64,           // Zone air temperature (C)
    h_conv: f64,           // Interior convection coefficient (W/(m2-K))
    // Radiation
    q_sw_gains: f64,       // Absorbed shortwave from lights/solar (W/m2)
    q_lw_surfaces: f64,    // Net longwave exchange with other surfaces (W/m2)
    // CTF history
    t_outside_history: &[f64],
    t_inside_history: &[f64],
    q_inside_history: &[f64],
) -> InsideHeatBalance {
    // History terms for inside flux
    let mut hist_terms = 0.0;
    for j in 1..=ctf.num_terms {
        if j - 1 < t_outside_history.len() {
            hist_terms += ctf.cross[j] * t_outside_history[j - 1];
        }
        if j - 1 < t_inside_history.len() {
            hist_terms += ctf.inside[j] * t_inside_history[j - 1];
        }
        if j - 1 < q_inside_history.len() {
            hist_terms += ctf.flux[j] * q_inside_history[j - 1];
        }
    }

    // Current outside surface temp (use history)
    let t_outside_current = if !t_outside_history.is_empty() {
        t_outside_history[0]
    } else {
        t_zone
    };

    let cross_term = ctf.cross[0] * t_outside_current;

    // Solve for T_surf:
    // q_sw + q_lw + h_conv*T_zone - h_conv*T_surf = Z[0]*T_surf + cross_term + hist
    let denom = h_conv + ctf.inside[0];
    let t_surface = if denom.abs() > 1e-10 {
        (q_sw_gains + q_lw_surfaces + h_conv * t_zone - cross_term - hist_terms) / denom
    } else {
        t_zone
    };

    let q_conv = h_conv * (t_zone - t_surface);
    let q_conduction = ctf.inside[0] * t_surface + cross_term + hist_terms;

    InsideHeatBalance {
        q_sw_gains,
        q_lw_surfaces,
        q_conv,
        q_conduction,
        t_surface,
    }
}

/// Calculate net longwave radiation from an exterior surface.
///
/// q_lw = eps * sigma * F_sky * (T_sky^4 - T_surf^4)
///      + eps * sigma * F_ground * (T_ground^4 - T_surf^4)
pub fn exterior_longwave_flux(
    emissivity: f64,
    t_surface_k: f64,
    t_sky_k: f64,
    t_ground_k: f64,
    view_factor_sky: f64,
    view_factor_ground: f64,
) -> f64 {
    let sigma = convection::STEFAN_BOLTZMANN;
    let ts4 = t_surface_k.powi(4);
    let q_sky = emissivity * sigma * view_factor_sky * (t_sky_k.powi(4) - ts4);
    let q_ground = emissivity * sigma * view_factor_ground * (t_ground_k.powi(4) - ts4);
    q_sky + q_ground
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_ctf() -> CtfCoefficients {
        // Simple single-term CTF representing a lightweight wall
        // U ≈ 3.0 W/(m2-K)
        CtfCoefficients {
            outside: vec![5.0, -2.0],
            cross: vec![-3.0, 1.5],
            inside: vec![5.0, -2.0],
            flux: vec![0.0, 0.3],
            num_terms: 1,
            num_histories: 1,
            time_step: 3600.0,
        }
    }

    #[test]
    fn outside_heat_balance_steady_state() {
        let ctf = simple_ctf();

        // Steady state: no solar, no LW, T_air = 35C
        let result = solve_outside_surface_temp(
            &ctf,
            35.0,  // T_air
            20.0,  // h_conv
            0.0,   // q_solar
            0.0,   // q_lw
            &[35.0], // T_outside history
            &[22.0], // T_inside history
            &[0.0],  // q history
        );

        // Temperature should be between outside air and inside
        assert!(
            result.t_surface > 20.0 && result.t_surface < 40.0,
            "T_surf={}",
            result.t_surface
        );
    }

    #[test]
    fn inside_heat_balance_steady_state() {
        let ctf = simple_ctf();

        let result = solve_inside_surface_temp(
            &ctf,
            22.0,  // T_zone
            3.0,   // h_conv
            0.0,   // q_sw
            0.0,   // q_lw
            &[35.0],
            &[22.0],
            &[0.0],
        );

        // Inside surface temp should be near zone temp
        assert!(
            result.t_surface > 18.0 && result.t_surface < 30.0,
            "T_surf={}",
            result.t_surface
        );
    }

    #[test]
    fn outside_with_solar() {
        let ctf = simple_ctf();

        let result = solve_outside_surface_temp(
            &ctf,
            30.0,   // T_air
            15.0,   // h_conv
            200.0,  // q_solar (strong sun)
            -50.0,  // q_lw (radiative cooling)
            &[35.0],
            &[22.0],
            &[0.0],
        );

        // Solar should raise surface temp above air temp
        assert!(
            result.t_surface > 30.0,
            "T_surf={} should be > 30 with solar",
            result.t_surface
        );
    }

    #[test]
    fn exterior_longwave() {
        let q = exterior_longwave_flux(
            0.9,    // emissivity
            293.15, // surface = 20C
            253.15, // sky = -20C
            283.15, // ground = 10C
            0.5,    // view factor sky
            0.5,    // view factor ground
        );
        // Net LW should be negative (surface warmer than avg surroundings)
        assert!(q < 0.0, "q_lw={q}");
    }

    // ─── New heat balance tests ──────────────────────────────────────

    #[test]
    fn outside_no_solar_no_lw() {
        // With zero solar and LW, the surface should equilibrate near air temperature
        let ctf = simple_ctf();
        let t_air = 20.0;
        let result = solve_outside_surface_temp(
            &ctf,
            t_air,  // T_air
            10.0,   // h_conv
            0.0,    // q_solar = 0
            0.0,    // q_lw = 0
            &[t_air],  // T_outside history = air temp (steady)
            &[t_air],  // T_inside history = same (isothermal)
            &[0.0],    // q_history
        );

        // Surface should be reasonably close to air temperature when isothermal
        // With CTF history terms, some deviation is expected
        assert!(
            (result.t_surface - t_air).abs() < 5.0,
            "T_surf={} should be close to T_air={} with no solar/LW",
            result.t_surface,
            t_air
        );
    }

    #[test]
    fn inside_with_sw_gains() {
        // 100 W/m2 shortwave gains → surface should be warmer than zone air
        let ctf = simple_ctf();
        let t_zone = 22.0;
        let result = solve_inside_surface_temp(
            &ctf,
            t_zone, // T_zone
            3.0,    // h_conv
            100.0,  // q_sw = 100 W/m2 (strong shortwave)
            0.0,    // q_lw = 0
            &[t_zone],
            &[t_zone],
            &[0.0],
        );

        assert!(
            result.t_surface > t_zone,
            "T_surf={} should be > T_zone={} with 100 W/m2 SW gains",
            result.t_surface,
            t_zone
        );
    }

    #[test]
    fn exterior_lw_surface_warmer() {
        // Warm surface (30C = 303.15K), cool sky (-10C = 263.15K), cool ground (5C = 278.15K)
        // Surface warmer than surroundings → should lose heat (q_lw < 0)
        let q = exterior_longwave_flux(
            0.9,    // emissivity
            303.15, // surface = 30C
            263.15, // sky = -10C
            278.15, // ground = 5C
            0.5,    // view factor sky
            0.5,    // view factor ground
        );
        assert!(
            q < 0.0,
            "Warm surface should lose heat via LW: q_lw={q}"
        );
    }

    #[test]
    fn exterior_lw_surface_cooler() {
        // Cool surface (0C = 273.15K), warm sky (20C = 293.15K), warm ground (25C = 298.15K)
        // Surface cooler than surroundings → should gain heat (q_lw > 0)
        let q = exterior_longwave_flux(
            0.9,    // emissivity
            273.15, // surface = 0C
            293.15, // sky = 20C
            298.15, // ground = 25C
            0.5,    // view factor sky
            0.5,    // view factor ground
        );
        assert!(
            q > 0.0,
            "Cool surface should gain heat via LW: q_lw={q}"
        );
    }
}
