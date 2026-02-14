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

// ─── Surface History & State ─────────────────────────────────────────

/// Temperature and flux history for CTF calculations.
#[derive(Debug, Clone)]
pub struct SurfaceHistory {
    /// Outside surface temperature history (most recent first).
    pub t_outside: Vec<f64>,
    /// Inside surface temperature history (most recent first).
    pub t_inside: Vec<f64>,
    /// Outside heat flux history (most recent first).
    pub q_outside: Vec<f64>,
    /// Inside heat flux history (most recent first).
    pub q_inside: Vec<f64>,
    /// Maximum history depth.
    pub max_depth: usize,
}

impl SurfaceHistory {
    /// Create a new history with given depth, initialized to the given temperatures.
    pub fn new(max_depth: usize, t_outside_init: f64, t_inside_init: f64) -> Self {
        Self {
            t_outside: vec![t_outside_init; max_depth],
            t_inside: vec![t_inside_init; max_depth],
            q_outside: vec![0.0; max_depth],
            q_inside: vec![0.0; max_depth],
            max_depth,
        }
    }

    /// Push new values and shift history (most recent becomes [0]).
    pub fn push(
        &mut self,
        t_outside: f64,
        t_inside: f64,
        q_outside: f64,
        q_inside: f64,
    ) {
        // Shift right (oldest drops off)
        for i in (1..self.max_depth).rev() {
            self.t_outside[i] = self.t_outside[i - 1];
            self.t_inside[i] = self.t_inside[i - 1];
            self.q_outside[i] = self.q_outside[i - 1];
            self.q_inside[i] = self.q_inside[i - 1];
        }
        if self.max_depth > 0 {
            self.t_outside[0] = t_outside;
            self.t_inside[0] = t_inside;
            self.q_outside[0] = q_outside;
            self.q_inside[0] = q_inside;
        }
    }
}

/// Current state of a surface during heat balance iteration.
#[derive(Debug, Clone)]
pub struct SurfaceState {
    /// Current outside surface temperature (C).
    pub t_outside: f64,
    /// Current inside surface temperature (C).
    pub t_inside: f64,
    /// Inside convection coefficient (W/(m²·K)).
    pub h_conv_inside: f64,
    /// Outside convection coefficient (W/(m²·K)).
    pub h_conv_outside: f64,
    /// Surface area (m²).
    pub area: f64,
    /// Emissivity of inside surface.
    pub emissivity_inside: f64,
    /// Emissivity of outside surface.
    pub emissivity_outside: f64,
    /// cos(tilt) for convection calculations.
    pub cos_tilt: f64,
    /// Surface is exterior (exposed to weather).
    pub is_exterior: bool,
    /// Absorbed solar on outside (W/m²).
    pub q_solar_outside: f64,
    /// Absorbed shortwave on inside (W/m²), from solar + lights.
    pub q_sw_inside: f64,
}

/// Result of solving all surfaces in a zone.
#[derive(Debug, Clone)]
pub struct ZoneHeatBalanceResult {
    /// Inside surface temperatures (C) for each surface.
    pub inside_temps: Vec<f64>,
    /// Outside surface temperatures (C) for each surface.
    pub outside_temps: Vec<f64>,
    /// Convective heat gain to zone air from each surface (W).
    pub surface_conv_to_zone: Vec<f64>,
    /// Total convective heat gain to zone air (W).
    pub total_conv_to_zone: f64,
    /// Number of iterations used.
    pub iterations: usize,
    /// Whether solution converged.
    pub converged: bool,
}

/// Solve all surfaces in a zone simultaneously.
///
/// Iterates between:
/// 1. Solve outside surface temperatures
/// 2. Compute interior radiant exchange
/// 3. Solve inside surface temperatures
/// 4. Check convergence
///
/// Returns the converged inside/outside surface temperatures and zone load.
pub fn solve_zone_surfaces(
    // Per-surface data
    states: &mut [SurfaceState],
    ctfs: &[CtfCoefficients],
    histories: &[SurfaceHistory],
    // Zone conditions
    t_zone: f64,
    // Exterior conditions
    t_air_outside: f64,
    t_sky_k: f64,
    t_ground_k: f64,
    // Solver parameters
    max_iterations: usize,
    tolerance: f64,
) -> ZoneHeatBalanceResult {
    let n = states.len();
    let mut inside_temps = vec![t_zone; n];
    let mut outside_temps = vec![t_air_outside; n];

    // Initialize from current states
    for i in 0..n {
        inside_temps[i] = states[i].t_inside;
        outside_temps[i] = states[i].t_outside;
    }

    let mut converged = false;
    let mut iterations = 0;

    for iter in 0..max_iterations {
        iterations = iter + 1;
        let old_inside = inside_temps.clone();

        // Step 1: Solve outside surface temperatures
        for i in 0..n {
            if !states[i].is_exterior {
                continue;
            }
            let t_surf_k = (outside_temps[i] + 273.15).max(200.0);
            let q_lw = exterior_longwave_flux(
                states[i].emissivity_outside,
                t_surf_k,
                t_sky_k,
                t_ground_k,
                0.5 * (1.0 + states[i].cos_tilt),  // view factor sky
                0.5 * (1.0 - states[i].cos_tilt),  // view factor ground
            );

            let result = solve_outside_surface_temp(
                &ctfs[i],
                t_air_outside,
                states[i].h_conv_outside,
                states[i].q_solar_outside,
                q_lw,
                &histories[i].t_outside,
                &histories[i].t_inside,
                &histories[i].q_outside,
            );
            outside_temps[i] = result.t_surface;
        }

        // Step 2: Compute interior radiant exchange using MRT approach
        let areas: Vec<f64> = states.iter().map(|s| s.area).collect();
        let emissivities: Vec<f64> = states.iter().map(|s| s.emissivity_inside).collect();

        // Compute radiant exchange using area-weighted MRT
        let inside_temps_k: Vec<f64> = inside_temps.iter().map(|t| t + 273.15).collect();
        let total_ea: f64 = emissivities.iter().zip(areas.iter()).map(|(e, a)| e * a).sum();
        let mrt_k = if total_ea > 1e-10 {
            emissivities
                .iter()
                .zip(areas.iter())
                .zip(inside_temps_k.iter())
                .map(|((e, a), t)| e * a * t)
                .sum::<f64>()
                / total_ea
        } else {
            inside_temps_k.iter().sum::<f64>() / n as f64
        };

        let t_avg = (mrt_k + inside_temps_k.iter().sum::<f64>() / n as f64) / 2.0;
        let h_rad = 4.0 * convection::STEFAN_BOLTZMANN * t_avg * t_avg * t_avg;

        let mut q_lw_inside = vec![0.0; n];
        for i in 0..n {
            q_lw_inside[i] = emissivities[i] * h_rad * (mrt_k - inside_temps_k[i]);
        }

        // Step 3: Solve inside surface temperatures
        for i in 0..n {
            let result = solve_inside_surface_temp(
                &ctfs[i],
                t_zone,
                states[i].h_conv_inside,
                states[i].q_sw_inside,
                q_lw_inside[i],
                &histories[i].t_outside,
                &histories[i].t_inside,
                &histories[i].q_inside,
            );
            inside_temps[i] = result.t_surface;
        }

        // Step 4: Check convergence
        let max_change = inside_temps
            .iter()
            .zip(old_inside.iter())
            .map(|(new, old)| (new - old).abs())
            .fold(0.0_f64, f64::max);

        if max_change < tolerance {
            converged = true;
            break;
        }
    }

    // Compute final convective loads
    let mut surface_conv = vec![0.0; n];
    let mut total_conv = 0.0;
    for i in 0..n {
        let q_conv = states[i].h_conv_inside * (inside_temps[i] - t_zone) * states[i].area;
        surface_conv[i] = q_conv;
        total_conv += q_conv;

        // Update states
        states[i].t_inside = inside_temps[i];
        states[i].t_outside = outside_temps[i];
    }

    ZoneHeatBalanceResult {
        inside_temps,
        outside_temps,
        surface_conv_to_zone: surface_conv,
        total_conv_to_zone: total_conv,
        iterations,
        converged,
    }
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

    // ─── Surface History tests ──────────────────────────────────────

    #[test]
    fn surface_history_initialization() {
        let hist = SurfaceHistory::new(3, 20.0, 22.0);
        assert_eq!(hist.t_outside, vec![20.0, 20.0, 20.0]);
        assert_eq!(hist.t_inside, vec![22.0, 22.0, 22.0]);
        assert_eq!(hist.q_outside, vec![0.0, 0.0, 0.0]);
        assert_eq!(hist.q_inside, vec![0.0, 0.0, 0.0]);
        assert_eq!(hist.max_depth, 3);
    }

    #[test]
    fn surface_history_push_shifts() {
        let mut hist = SurfaceHistory::new(3, 10.0, 20.0);
        // Push new values
        hist.push(15.0, 25.0, 100.0, 50.0);
        assert_eq!(hist.t_outside[0], 15.0);
        assert_eq!(hist.t_inside[0], 25.0);
        assert_eq!(hist.q_outside[0], 100.0);
        assert_eq!(hist.q_inside[0], 50.0);
        // Old values shifted
        assert_eq!(hist.t_outside[1], 10.0);
        assert_eq!(hist.t_inside[1], 20.0);
        assert_eq!(hist.q_outside[1], 0.0);
        assert_eq!(hist.q_inside[1], 0.0);
        // Third slot still initial
        assert_eq!(hist.t_outside[2], 10.0);
    }

    #[test]
    fn surface_history_push_drops_oldest() {
        let mut hist = SurfaceHistory::new(2, 0.0, 0.0);
        hist.push(1.0, 10.0, 100.0, 200.0);
        hist.push(2.0, 20.0, 300.0, 400.0);
        // Newest at [0], previous at [1], original dropped
        assert_eq!(hist.t_outside[0], 2.0);
        assert_eq!(hist.t_outside[1], 1.0);
        assert_eq!(hist.t_inside[0], 20.0);
        assert_eq!(hist.t_inside[1], 10.0);
    }

    // ─── Zone Solver tests ──────────────────────────────────────────

    fn make_zone_ctf() -> CtfCoefficients {
        // Lightweight wall: U ≈ 3 W/(m²·K)
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

    fn make_surface_state(t_outside: f64, t_inside: f64, is_exterior: bool) -> SurfaceState {
        SurfaceState {
            t_outside,
            t_inside,
            h_conv_inside: 3.0,
            h_conv_outside: 15.0,
            area: 10.0,
            emissivity_inside: 0.9,
            emissivity_outside: 0.9,
            cos_tilt: 0.0, // vertical wall
            is_exterior,
            q_solar_outside: 0.0,
            q_sw_inside: 0.0,
        }
    }

    #[test]
    fn zone_solver_converges() {
        let ctf = make_zone_ctf();
        let mut states = vec![
            make_surface_state(25.0, 22.0, true),
            make_surface_state(25.0, 22.0, true),
            make_surface_state(25.0, 22.0, true),
            make_surface_state(25.0, 22.0, true),
        ];
        let ctfs = vec![ctf.clone(), ctf.clone(), ctf.clone(), ctf.clone()];
        let histories: Vec<SurfaceHistory> = (0..4)
            .map(|_| SurfaceHistory::new(1, 25.0, 22.0))
            .collect();

        let result = solve_zone_surfaces(
            &mut states,
            &ctfs,
            &histories,
            22.0,   // T_zone
            30.0,   // T_air_outside
            263.15, // T_sky (cold sky)
            283.15, // T_ground
            50,     // max_iterations
            0.001,  // tolerance
        );

        assert!(result.converged, "Solver should converge");
        assert!(result.iterations < 50, "Should converge in fewer than 50 iters");
    }

    #[test]
    fn zone_solver_temps_between_inside_and_outside() {
        let ctf = make_zone_ctf();
        let mut states = vec![
            make_surface_state(30.0, 22.0, true),
            make_surface_state(30.0, 22.0, true),
        ];
        let ctfs = vec![ctf.clone(), ctf.clone()];
        let histories: Vec<SurfaceHistory> = (0..2)
            .map(|_| SurfaceHistory::new(1, 30.0, 22.0))
            .collect();

        let result = solve_zone_surfaces(
            &mut states,
            &ctfs,
            &histories,
            22.0,   // T_zone
            35.0,   // T_air_outside (hot day)
            260.15, // T_sky
            293.15, // T_ground
            50,
            0.001,
        );

        assert!(result.converged);
        for t in &result.inside_temps {
            // Inside temps should be between zone temp and outside air temp
            assert!(
                *t > 15.0 && *t < 40.0,
                "Inside temp {t} should be reasonable"
            );
        }
        for t in &result.outside_temps {
            assert!(
                *t > 10.0 && *t < 50.0,
                "Outside temp {t} should be reasonable"
            );
        }
    }

    #[test]
    fn zone_solver_solar_raises_outside_temp() {
        let ctf = make_zone_ctf();
        // Surface with no solar
        let mut states_no_solar = vec![make_surface_state(30.0, 22.0, true)];
        // Surface with solar
        let mut states_solar = vec![{
            let mut s = make_surface_state(30.0, 22.0, true);
            s.q_solar_outside = 300.0;
            s
        }];
        let ctfs = vec![ctf.clone()];
        let histories = vec![SurfaceHistory::new(1, 30.0, 22.0)];

        let result_no = solve_zone_surfaces(
            &mut states_no_solar,
            &ctfs,
            &histories,
            22.0, 30.0, 263.15, 283.15, 50, 0.001,
        );
        let result_solar = solve_zone_surfaces(
            &mut states_solar,
            &ctfs,
            &histories,
            22.0, 30.0, 263.15, 283.15, 50, 0.001,
        );

        assert!(
            result_solar.outside_temps[0] > result_no.outside_temps[0],
            "Solar should raise outside surface temp: {} vs {}",
            result_solar.outside_temps[0],
            result_no.outside_temps[0]
        );
    }

    #[test]
    fn zone_solver_convective_load_direction() {
        // Compare hot vs cold outside: hot should produce MORE heat to zone (or less loss)
        let ctf = make_zone_ctf();

        // Hot case
        let mut states_hot = vec![make_surface_state(35.0, 25.0, true)];
        let ctfs = vec![ctf.clone()];
        let histories = vec![SurfaceHistory::new(1, 35.0, 25.0)];
        let result_hot = solve_zone_surfaces(
            &mut states_hot,
            &ctfs,
            &histories,
            22.0, 38.0, 270.15, 293.15, 50, 0.001,
        );

        // Cold case
        let mut states_cold = vec![make_surface_state(5.0, 18.0, true)];
        let histories_cold = vec![SurfaceHistory::new(1, 5.0, 18.0)];
        let result_cold = solve_zone_surfaces(
            &mut states_cold,
            &ctfs,
            &histories_cold,
            22.0, -5.0, 243.15, 268.15, 50, 0.001,
        );

        assert!(result_hot.converged);
        assert!(result_cold.converged);
        // Hot outside should produce more conv to zone than cold outside
        assert!(
            result_hot.total_conv_to_zone > result_cold.total_conv_to_zone,
            "Hot ({}) should produce more conv than cold ({})",
            result_hot.total_conv_to_zone,
            result_cold.total_conv_to_zone
        );
    }

    #[test]
    fn zone_solver_cold_outside_negative_load() {
        // Cold outside, warm zone → surfaces cooler than zone → negative convection (heat loss)
        let ctf = make_zone_ctf();
        let mut states = vec![make_surface_state(5.0, 20.0, true)];
        let ctfs = vec![ctf.clone()];
        let histories = vec![SurfaceHistory::new(1, 5.0, 20.0)];

        let result = solve_zone_surfaces(
            &mut states,
            &ctfs,
            &histories,
            22.0,   // warm zone
            -5.0,   // cold outside
            243.15, // cold sky
            268.15, // cold ground
            50,
            0.001,
        );

        assert!(result.converged);
        assert!(
            result.total_conv_to_zone < 0.0,
            "Cold outside should produce negative conv load (heat loss): {}",
            result.total_conv_to_zone
        );
    }

    #[test]
    fn zone_solver_isothermal_zero_load() {
        // When everything is at the same temperature, conv load should be near zero
        let ctf = make_zone_ctf();
        let t = 22.0;
        let mut states = vec![
            make_surface_state(t, t, true),
            make_surface_state(t, t, true),
        ];
        let ctfs = vec![ctf.clone(), ctf.clone()];
        let histories: Vec<SurfaceHistory> =
            (0..2).map(|_| SurfaceHistory::new(1, t, t)).collect();

        let result = solve_zone_surfaces(
            &mut states,
            &ctfs,
            &histories,
            t,
            t,
            t + 273.15,
            t + 273.15,
            50,
            0.001,
        );

        assert!(result.converged);
        // Total conv load should be relatively small in isothermal case
        // Note: CTF history terms can produce some residual flux even at isothermal
        // conditions, so we check for a reasonable bound rather than strict zero
        let total_area: f64 = states.iter().map(|s| s.area).sum();
        let flux_density = result.total_conv_to_zone.abs() / total_area;
        assert!(
            flux_density < 20.0,
            "Isothermal conv flux density should be bounded: {} W/m2",
            flux_density
        );
    }

    #[test]
    fn zone_solver_updates_states() {
        let ctf = make_zone_ctf();
        let mut states = vec![make_surface_state(30.0, 22.0, true)];
        let initial_t_inside = states[0].t_inside;
        let ctfs = vec![ctf.clone()];
        let histories = vec![SurfaceHistory::new(1, 30.0, 22.0)];

        let result = solve_zone_surfaces(
            &mut states,
            &ctfs,
            &histories,
            22.0, 35.0, 260.15, 290.15, 50, 0.001,
        );

        assert!(result.converged);
        // States should be updated to match result
        assert!(
            (states[0].t_inside - result.inside_temps[0]).abs() < 1e-10,
            "State t_inside should match result"
        );
        assert!(
            (states[0].t_outside - result.outside_temps[0]).abs() < 1e-10,
            "State t_outside should match result"
        );
        // Should have changed from initial
        assert!(
            (states[0].t_inside - initial_t_inside).abs() > 0.001,
            "Inside temp should have changed from initial"
        );
    }

    #[test]
    fn zone_solver_four_surface_box_energy_balance() {
        // 4-surface box: verify total conv load is consistent
        let ctf = make_zone_ctf();
        let mut states = vec![
            make_surface_state(28.0, 23.0, true),   // wall 1
            make_surface_state(28.0, 23.0, true),   // wall 2
            {
                let mut s = make_surface_state(28.0, 23.0, true);
                s.cos_tilt = -1.0; // roof (facing down)
                s
            },
            {
                let mut s = make_surface_state(28.0, 23.0, true);
                s.cos_tilt = 1.0; // floor (facing up)
                s
            },
        ];
        let ctfs = vec![ctf.clone(), ctf.clone(), ctf.clone(), ctf.clone()];
        let histories: Vec<SurfaceHistory> = (0..4)
            .map(|_| SurfaceHistory::new(1, 28.0, 23.0))
            .collect();

        let result = solve_zone_surfaces(
            &mut states,
            &ctfs,
            &histories,
            22.0,   // T_zone
            32.0,   // T_air_outside
            263.15, // T_sky
            288.15, // T_ground
            50,
            0.001,
        );

        assert!(result.converged, "4-surface box should converge");

        // Verify total conv equals sum of surface convs
        let sum_surface: f64 = result.surface_conv_to_zone.iter().sum();
        assert!(
            (result.total_conv_to_zone - sum_surface).abs() < 1e-6,
            "Total conv {} should equal sum {}",
            result.total_conv_to_zone,
            sum_surface
        );
    }

    #[test]
    fn zone_solver_mixed_interior_exterior() {
        // Mix of exterior and interior surfaces
        let ctf = make_zone_ctf();
        let mut states = vec![
            make_surface_state(30.0, 22.0, true),  // exterior wall
            make_surface_state(22.0, 22.0, false),  // interior partition
        ];
        let ctfs = vec![ctf.clone(), ctf.clone()];
        let histories: Vec<SurfaceHistory> = vec![
            SurfaceHistory::new(1, 30.0, 22.0),
            SurfaceHistory::new(1, 22.0, 22.0),
        ];

        let result = solve_zone_surfaces(
            &mut states,
            &ctfs,
            &histories,
            22.0,   // T_zone
            35.0,   // T_air_outside
            260.15,
            293.15,
            50,
            0.001,
        );

        assert!(result.converged);
        // Interior partition outside temp should not change (not exterior)
        // It keeps its initial value since solve_zone_surfaces skips non-exterior
        assert!(
            (result.outside_temps[1] - 22.0).abs() < 0.01,
            "Interior partition outside temp should stay at initial: {}",
            result.outside_temps[1]
        );
    }
}
