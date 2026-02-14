//! Window thermal calculations.
//!
//! Computes U-value, center-of-glass thermal resistance, and
//! gas gap conductance for multi-pane glazing systems.

use ep_materials::gas::gap_conductance;
use ep_materials::{GasType, GlassMaterial};

/// Stefan-Boltzmann constant (W/(m2-K4)).
const SIGMA: f64 = 5.670374419e-8;

/// Standard interior film coefficient for windows (W/(m2-K)).
pub const H_INSIDE_WINTER: f64 = 8.29;

/// Standard exterior film coefficient for windows (W/(m2-K)).
pub const H_OUTSIDE_WINTER: f64 = 23.0;

/// Thermal properties of a window gap.
#[derive(Debug, Clone)]
pub struct GapThermal {
    pub gas_type: GasType,
    pub width: f64,         // gap width (m)
    pub conductance: f64,   // gap conductance (W/(m2-K))
    pub h_convective: f64,  // convective component
    pub h_radiative: f64,   // radiative component
}

/// Calculate the thermal conductance of a gas gap between two glazing surfaces.
///
/// Includes both convective and radiative components.
///
/// # Arguments
/// * `gas_type` - Type of fill gas
/// * `gap_width` - Gap width (m)
/// * `t_left` - Temperature of left (outer) surface (K)
/// * `t_right` - Temperature of right (inner) surface (K)
/// * `emissivity_left` - IR emissivity of left surface
/// * `emissivity_right` - IR emissivity of right surface
pub fn gap_thermal_conductance(
    gas_type: GasType,
    gap_width: f64,
    t_left: f64,
    t_right: f64,
    emissivity_left: f64,
    emissivity_right: f64,
) -> GapThermal {
    let pressure = 101325.0; // Standard pressure

    // Gas conduction/convection
    let (h_cond, pr, gr) = gap_conductance(gas_type, gap_width, t_left, t_right, pressure);

    // Nusselt number for natural convection in a vertical gap
    let ra = gr * pr; // Rayleigh number
    let nu = nusselt_vertical_gap(ra, gap_width, 1.0); // Assume 1m height

    let h_conv = h_cond * nu;

    // Radiative conductance between parallel surfaces
    let h_rad = radiative_conductance(emissivity_left, emissivity_right, t_left, t_right);

    let total = h_conv + h_rad;

    GapThermal {
        gas_type,
        width: gap_width,
        conductance: total,
        h_convective: h_conv,
        h_radiative: h_rad,
    }
}

/// Nusselt number for natural convection in a vertical enclosed gap.
///
/// Uses the ISO 15099 / Wright correlation.
fn nusselt_vertical_gap(rayleigh: f64, gap_width: f64, height: f64) -> f64 {
    if rayleigh < 0.0 || gap_width <= 0.0 || height <= 0.0 {
        return 1.0;
    }

    let aspect = height / gap_width;

    if rayleigh < 1708.0 {
        // Conduction regime
        1.0
    } else if aspect > 40.0 {
        // Tall cavity: Nu = 0.42 * Ra^0.25 * Pr^0.012 * (H/L)^-0.3
        // Simplified for typical window gas Pr ≈ 0.7
        0.42 * rayleigh.powf(0.25) * aspect.powf(-0.3)
    } else {
        // General: Nu = max(1, 0.0673838 * Ra^(1/3))  for Ra > 5e4
        // Or: Nu = 1 + 1.44 * (1 - 1708/Ra) for 1708 < Ra < 5e4
        if rayleigh < 5.0e4 {
            (1.0 + 1.44 * (1.0 - 1708.0 / rayleigh)).max(1.0)
        } else {
            0.0673838 * rayleigh.powf(1.0 / 3.0)
        }
    }
}

/// Radiative conductance between two parallel surfaces.
///
/// h_rad = sigma * (T1^2 + T2^2) * (T1 + T2) / (1/e1 + 1/e2 - 1)
fn radiative_conductance(
    emissivity_1: f64,
    emissivity_2: f64,
    t1_k: f64,
    t2_k: f64,
) -> f64 {
    if emissivity_1 <= 0.0 || emissivity_2 <= 0.0 {
        return 0.0;
    }

    let effective_emissivity = 1.0 / (1.0 / emissivity_1 + 1.0 / emissivity_2 - 1.0);
    let t1_2 = t1_k * t1_k;
    let t2_2 = t2_k * t2_k;

    effective_emissivity * SIGMA * (t1_2 + t2_2) * (t1_k + t2_k)
}

/// Calculate the center-of-glass U-value for a window.
///
/// Uses an iterative approach to find the temperature distribution
/// through the glazing system layers.
///
/// # Arguments
/// * `glass_layers` - Glass material properties (outside to inside)
/// * `gap_gases` - Gas type for each gap (between layers)
/// * `gap_widths` - Width of each gap (m)
/// * `t_outside` - Outside air temperature (K)
/// * `t_inside` - Inside air temperature (K)
/// * `h_outside` - Exterior film coefficient (W/(m2-K))
/// * `h_inside` - Interior film coefficient (W/(m2-K))
pub fn center_of_glass_u_value(
    glass_layers: &[GlassMaterial],
    gap_gases: &[GasType],
    gap_widths: &[f64],
    t_outside: f64,
    t_inside: f64,
    h_outside: f64,
    h_inside: f64,
) -> f64 {
    let n_layers = glass_layers.len();
    if n_layers == 0 {
        return 0.0;
    }

    // Number of surfaces = 2 * n_layers (outside and inside face of each pane)
    let n_surfaces = 2 * n_layers;

    // Initial temperature guess: linear distribution
    let mut t_surf = vec![0.0; n_surfaces];
    for i in 0..n_surfaces {
        let frac = (i as f64 + 0.5) / n_surfaces as f64;
        t_surf[i] = t_outside + frac * (t_inside - t_outside);
    }

    // Iterate to convergence
    for _iter in 0..50 {
        let mut r_total = 1.0 / h_outside + 1.0 / h_inside;

        // Glass layer resistances
        for glass in glass_layers.iter() {
            if glass.conductivity > 0.0 {
                r_total += glass.thickness.value() / glass.conductivity;
            }
        }

        // Gap resistances
        for i in 0..gap_gases.len().min(gap_widths.len()) {
            let surf_left = 2 * i + 1;  // inside face of outer pane
            let surf_right = 2 * (i + 1); // outside face of inner pane

            let t_left = t_surf.get(surf_left).copied().unwrap_or(t_outside);
            let t_right = t_surf.get(surf_right).copied().unwrap_or(t_inside);

            let e_left = glass_layers[i].emissivity_back;
            let e_right = glass_layers.get(i + 1)
                .map(|g| g.emissivity_front)
                .unwrap_or(0.84);

            let gap = gap_thermal_conductance(
                gap_gases[i],
                gap_widths[i],
                t_left.max(200.0),
                t_right.max(200.0),
                e_left,
                e_right,
            );

            if gap.conductance > 0.0 {
                r_total += 1.0 / gap.conductance;
            }
        }

        let u = if r_total > 0.0 { 1.0 / r_total } else { 0.0 };
        let q = u * (t_inside - t_outside);

        // Update surface temperatures
        let mut t_current = t_outside + q / h_outside;
        for i in 0..n_layers {
            t_surf[2 * i] = t_current;
            if glass_layers[i].conductivity > 0.0 {
                t_current += q * glass_layers[i].thickness.value() / glass_layers[i].conductivity;
            }
            t_surf[2 * i + 1] = t_current;

            // Gap
            if i < gap_gases.len().min(gap_widths.len()) {
                let e_left = glass_layers[i].emissivity_back;
                let e_right = glass_layers.get(i + 1)
                    .map(|g| g.emissivity_front)
                    .unwrap_or(0.84);

                let gap = gap_thermal_conductance(
                    gap_gases[i],
                    gap_widths[i],
                    t_surf[2 * i + 1].max(200.0),
                    (t_current + q / 10.0).max(200.0),
                    e_left,
                    e_right,
                );
                if gap.conductance > 0.0 {
                    t_current += q / gap.conductance;
                }
            }
        }
    }

    // Final U-value calculation
    let mut r_total = 1.0 / h_outside + 1.0 / h_inside;

    for glass in glass_layers {
        if glass.conductivity > 0.0 {
            r_total += glass.thickness.value() / glass.conductivity;
        }
    }

    for i in 0..gap_gases.len().min(gap_widths.len()) {
        let t_left = t_surf.get(2 * i + 1).copied().unwrap_or(t_outside);
        let t_right = t_surf.get(2 * (i + 1)).copied().unwrap_or(t_inside);

        let e_left = glass_layers[i].emissivity_back;
        let e_right = glass_layers.get(i + 1)
            .map(|g| g.emissivity_front)
            .unwrap_or(0.84);

        let gap = gap_thermal_conductance(
            gap_gases[i],
            gap_widths[i],
            t_left.max(200.0),
            t_right.max(200.0),
            e_left,
            e_right,
        );

        if gap.conductance > 0.0 {
            r_total += 1.0 / gap.conductance;
        }
    }

    if r_total > 0.0 { 1.0 / r_total } else { 0.0 }
}

/// Calculate U-value for a simple window from NFRC conditions.
///
/// NFRC winter conditions: T_outside = -18C (255.15K), T_inside = 21C (294.15K)
pub fn nfrc_u_value(
    glass_layers: &[GlassMaterial],
    gap_gases: &[GasType],
    gap_widths: &[f64],
) -> f64 {
    center_of_glass_u_value(
        glass_layers,
        gap_gases,
        gap_widths,
        255.15,
        294.15,
        H_OUTSIDE_WINTER,
        H_INSIDE_WINTER,
    )
}

// ─── Full Window Heat Balance Solver ─────────────────────────────────

/// Exterior conditions for window heat balance.
#[derive(Debug, Clone)]
pub struct ExteriorConditions {
    /// Outside air temperature (K).
    pub t_air: f64,
    /// Sky effective temperature (K) for longwave radiation.
    pub t_sky: f64,
    /// Exterior convection coefficient (W/(m²·K)).
    pub h_conv: f64,
}

/// Interior conditions for window heat balance.
#[derive(Debug, Clone)]
pub struct InteriorConditions {
    /// Zone air temperature (K).
    pub t_air: f64,
    /// Zone mean radiant temperature (K).
    pub t_mrt: f64,
    /// Interior convection coefficient (W/(m²·K)).
    pub h_conv: f64,
    /// Dew point temperature (K) for condensation check.
    pub t_dew_point: f64,
}

/// Result of window heat balance calculation.
#[derive(Debug, Clone)]
pub struct WindowThermalResult {
    /// Temperature of each glass face (K), outside to inside (length = 2*N_layers).
    pub glass_temps: Vec<f64>,
    /// Mean temperature of each gap (K).
    pub gap_temps: Vec<f64>,
    /// Heat flow into zone (W/m²), positive = heat gain to zone.
    pub heat_flow_in: f64,
    /// Solar absorbed per glass layer (W/m²).
    pub solar_absorbed: Vec<f64>,
    /// Center-of-glass U-value (W/(m²·K)).
    pub u_value: f64,
    /// True if inside glass face temperature is below dew point.
    pub condensation: bool,
    /// Number of iterations used.
    pub iterations: usize,
    /// Whether solution converged.
    pub converged: bool,
}

/// Frame and divider thermal properties.
#[derive(Debug, Clone)]
pub struct FrameDivider {
    /// Frame U-value (W/(m²·K)).
    pub frame_u_value: f64,
    /// Frame area fraction of total window area.
    pub frame_area_fraction: f64,
    /// Divider U-value (W/(m²·K)).
    pub divider_u_value: f64,
    /// Divider area fraction of total window area.
    pub divider_area_fraction: f64,
}

impl FrameDivider {
    /// Calculate frame/divider heat flow (W/m² of total window area).
    ///
    /// Positive result means heat flows from outside to inside (heating load).
    pub fn heat_flow(&self, t_outside: f64, t_inside: f64) -> f64 {
        let dt = t_outside - t_inside;
        let q_frame = self.frame_u_value * self.frame_area_fraction * dt;
        let q_divider = self.divider_u_value * self.divider_area_fraction * dt;
        q_frame + q_divider
    }

    /// Glass area fraction (remaining after frame and divider).
    pub fn glass_area_fraction(&self) -> f64 {
        (1.0 - self.frame_area_fraction - self.divider_area_fraction).max(0.0)
    }
}

/// Solve the window heat balance iteratively.
///
/// For N glass layers, solves for 2N surface temperatures (outside and inside face
/// of each pane). Gap conductances are updated each iteration since they depend on
/// temperature.
///
/// The tridiagonal system for each face's heat balance is:
/// - Face 1 (exterior): h_ext_total·θ₁ + k₁/d₁·(θ₁ - θ₂) = h_ext_sources + S₁
/// - Interior faces: conductance from left + glass conductance = conductance to right
/// - Face 2N (interior): k_N/d_N·(θ_{2N-1} - θ_{2N}) + S_{2N} = h_int_total·(θ_{2N} - T_zone)
pub fn solve_window_heat_balance(
    glass_layers: &[GlassMaterial],
    gap_gases: &[GasType],
    gap_widths: &[f64],
    exterior: &ExteriorConditions,
    interior: &InteriorConditions,
    solar_absorbed_per_layer: &[f64],
    max_iterations: usize,
    tolerance: f64,
) -> WindowThermalResult {
    let n_layers = glass_layers.len();
    if n_layers == 0 {
        return WindowThermalResult {
            glass_temps: vec![],
            gap_temps: vec![],
            heat_flow_in: 0.0,
            solar_absorbed: vec![],
            u_value: 0.0,
            condensation: false,
            iterations: 0,
            converged: true,
        };
    }

    let n_faces = 2 * n_layers;
    let n_gaps = n_layers.saturating_sub(1).min(gap_gases.len()).min(gap_widths.len());

    // Initialize face temperatures: linear interpolation between exterior and interior
    let mut theta = vec![0.0; n_faces];
    for i in 0..n_faces {
        let frac = (i as f64 + 0.5) / n_faces as f64;
        theta[i] = exterior.t_air + frac * (interior.t_air - exterior.t_air);
    }

    // Solar absorbed per face: split each layer evenly between its two faces
    let mut s_face = vec![0.0; n_faces];
    for (i, &q_sol) in solar_absorbed_per_layer.iter().enumerate().take(n_layers) {
        s_face[2 * i] += q_sol / 2.0;
        s_face[2 * i + 1] += q_sol / 2.0;
    }

    // Glass conductances k/d (constant, not temperature-dependent)
    let k_glass: Vec<f64> = glass_layers
        .iter()
        .map(|g| {
            let d = g.thickness.value();
            if d > 0.0 && g.conductivity > 0.0 {
                g.conductivity / d
            } else {
                1000.0
            }
        })
        .collect();

    let mut converged = false;
    let mut iterations = 0;

    for iter in 0..max_iterations {
        iterations = iter + 1;
        let old_theta = theta.clone();

        // Compute gap conductances at current temperatures
        let mut h_gap = vec![0.0; n_gaps];
        for i in 0..n_gaps {
            let t_left = theta[2 * i + 1].max(200.0);
            let t_right = theta[2 * (i + 1)].max(200.0);
            let e_left = glass_layers[i].emissivity_back;
            let e_right = glass_layers
                .get(i + 1)
                .map(|g| g.emissivity_front)
                .unwrap_or(0.84);
            let gap = gap_thermal_conductance(
                gap_gases[i],
                gap_widths[i],
                t_left,
                t_right,
                e_left,
                e_right,
            );
            h_gap[i] = gap.conductance;
        }

        // Exterior radiation coefficient (linearized)
        let t1 = theta[0].max(200.0);
        let e_ext = glass_layers[0].emissivity_front;
        let h_rad_ext =
            e_ext * SIGMA * (t1 * t1 + exterior.t_sky * exterior.t_sky) * (t1 + exterior.t_sky);
        let h_ext_total = exterior.h_conv + h_rad_ext;

        // Interior radiation coefficient (linearized)
        let t_last = theta[n_faces - 1].max(200.0);
        let e_int = glass_layers[n_layers - 1].emissivity_back;
        let h_rad_int = e_int
            * SIGMA
            * (t_last * t_last + interior.t_mrt * interior.t_mrt)
            * (t_last + interior.t_mrt);
        let h_int_total = interior.h_conv + h_rad_int;

        // Build tridiagonal system: a[i]*θ[i-1] + b[i]*θ[i] + c[i]*θ[i+1] = d[i]
        let mut a_coef = vec![0.0; n_faces];
        let mut b_coef = vec![0.0; n_faces];
        let mut c_coef = vec![0.0; n_faces];
        let mut d_coef = vec![0.0; n_faces];

        for face in 0..n_faces {
            let layer = face / 2;
            let is_outer_face = face % 2 == 0;

            if face == 0 {
                // Outermost face: exterior coupling + glass conduction to face 1
                b_coef[0] = h_ext_total + k_glass[0];
                c_coef[0] = -k_glass[0];
                d_coef[0] = exterior.h_conv * exterior.t_air
                    + h_rad_ext * exterior.t_sky
                    + s_face[0];
            } else if face == n_faces - 1 {
                // Innermost face: glass conduction from face 2N-2 + interior coupling
                a_coef[face] = -k_glass[layer];
                b_coef[face] = k_glass[layer] + h_int_total;
                d_coef[face] = interior.h_conv * interior.t_air
                    + h_rad_int * interior.t_mrt
                    + s_face[face];
            } else if is_outer_face {
                // Outer face of interior pane: gap from left + glass to right
                let gap_idx = layer - 1;
                let h_g = h_gap.get(gap_idx).copied().unwrap_or(5.0);
                a_coef[face] = -h_g;
                b_coef[face] = h_g + k_glass[layer];
                c_coef[face] = -k_glass[layer];
                d_coef[face] = s_face[face];
            } else {
                // Inner face of a pane: glass from left + gap to right
                let gap_idx = layer;
                let h_g = h_gap.get(gap_idx).copied().unwrap_or(5.0);
                a_coef[face] = -k_glass[layer];
                b_coef[face] = k_glass[layer] + h_g;
                c_coef[face] = -h_g;
                d_coef[face] = s_face[face];
            }
        }

        // Thomas algorithm (tridiagonal solve)
        let mut cp = vec![0.0; n_faces];
        let mut dp = vec![0.0; n_faces];
        if b_coef[0].abs() < 1e-20 {
            break;
        }
        cp[0] = c_coef[0] / b_coef[0];
        dp[0] = d_coef[0] / b_coef[0];
        for i in 1..n_faces {
            let m = b_coef[i] - a_coef[i] * cp[i - 1];
            if m.abs() < 1e-20 {
                break;
            }
            cp[i] = c_coef[i] / m;
            dp[i] = (d_coef[i] - a_coef[i] * dp[i - 1]) / m;
        }
        theta[n_faces - 1] = dp[n_faces - 1];
        for i in (0..n_faces - 1).rev() {
            theta[i] = dp[i] - cp[i] * theta[i + 1];
        }

        // Check convergence
        let max_change = theta
            .iter()
            .zip(old_theta.iter())
            .map(|(new, old)| (new - old).abs())
            .fold(0.0_f64, f64::max);

        if max_change < tolerance {
            converged = true;
            break;
        }
    }

    // Compute heat flow into zone from inside face
    let t_inside_face = theta[n_faces - 1].max(200.0);
    let e_int = glass_layers[n_layers - 1].emissivity_back;
    let h_rad_int = e_int
        * SIGMA
        * (t_inside_face * t_inside_face + interior.t_mrt * interior.t_mrt)
        * (t_inside_face + interior.t_mrt);
    let heat_flow_in = interior.h_conv * (t_inside_face - interior.t_air)
        + h_rad_int * (t_inside_face - interior.t_mrt);

    // Gap mean temperatures
    let gap_temps: Vec<f64> = (0..n_gaps)
        .map(|i| {
            let t_left = theta.get(2 * i + 1).copied().unwrap_or(0.0);
            let t_right = theta.get(2 * (i + 1)).copied().unwrap_or(0.0);
            0.5 * (t_left + t_right)
        })
        .collect();

    // U-value from air-to-air temperature difference
    let dt_air = (interior.t_air - exterior.t_air).abs();
    let u_value = if dt_air > 0.1 {
        heat_flow_in.abs() / dt_air
    } else {
        0.0
    };

    let condensation = t_inside_face < interior.t_dew_point;

    WindowThermalResult {
        glass_temps: theta,
        gap_temps,
        heat_flow_in,
        solar_absorbed: solar_absorbed_per_layer.to_vec(),
        u_value,
        condensation,
        iterations,
        converged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ep_materials::GlassMaterial;
    use ep_units::Length;

    fn clear_3mm() -> GlassMaterial {
        GlassMaterial {
            name: "Clear3mm".into(),
            thickness: Length::new(0.003),
            conductivity: 0.9,
            emissivity_front: 0.84,
            emissivity_back: 0.84,
            ..Default::default()
        }
    }

    fn clear_6mm() -> GlassMaterial {
        GlassMaterial {
            name: "Clear6mm".into(),
            thickness: Length::new(0.006),
            conductivity: 0.9,
            emissivity_front: 0.84,
            emissivity_back: 0.84,
            ..Default::default()
        }
    }

    #[test]
    fn single_pane_u_value() {
        let glass = clear_6mm();
        let u = nfrc_u_value(&[glass], &[], &[]);
        // Single clear pane: U ≈ 5.8 W/(m2-K)
        assert!(u > 4.5 && u < 7.0, "U={u}");
    }

    #[test]
    fn double_pane_air_u_value() {
        let glass = clear_3mm();
        let u = nfrc_u_value(
            &[glass.clone(), glass],
            &[GasType::Air],
            &[0.012],
        );
        // Double clear with 12mm air: U ≈ 2.7-3.2 W/(m2-K)
        assert!(u > 2.0 && u < 4.0, "U={u}");
    }

    #[test]
    fn double_pane_argon_u_value() {
        let glass = clear_3mm();
        let u = nfrc_u_value(
            &[glass.clone(), glass],
            &[GasType::Argon],
            &[0.012],
        );
        // Argon should give lower U than air
        let u_air = nfrc_u_value(
            &[clear_3mm(), clear_3mm()],
            &[GasType::Air],
            &[0.012],
        );
        assert!(u < u_air, "U_argon={u} should be < U_air={u_air}");
    }

    #[test]
    fn gap_radiative_conductance() {
        let h = radiative_conductance(0.84, 0.84, 280.0, 270.0);
        // Should be around 4-5 W/(m2-K)
        assert!(h > 3.0 && h < 6.0, "h_rad={h}");
    }

    #[test]
    fn nusselt_conduction_regime() {
        // Ra < 1708: pure conduction
        let nu = nusselt_vertical_gap(1000.0, 0.012, 1.0);
        assert!((nu - 1.0).abs() < 1e-10);
    }

    #[test]
    fn nusselt_convection_regime() {
        // With a small aspect ratio (shorter gap), convection kicks in
        let nu = nusselt_vertical_gap(50000.0, 0.1, 0.1);
        assert!(nu > 1.0, "Nu={nu}");
    }

    // ─── Window Heat Balance Solver Tests ─────────────────────────────

    fn winter_exterior() -> ExteriorConditions {
        ExteriorConditions {
            t_air: 255.15,  // -18°C (NFRC winter)
            t_sky: 243.15,  // -30°C
            h_conv: 23.0,   // standard exterior
        }
    }

    fn winter_interior() -> InteriorConditions {
        InteriorConditions {
            t_air: 294.15,  // 21°C (NFRC winter)
            t_mrt: 294.15,  // same as air
            h_conv: 8.29,   // standard interior
            t_dew_point: 283.15, // 10°C
        }
    }

    #[test]
    fn window_hb_single_pane_converges() {
        let glass = clear_6mm();
        let result = solve_window_heat_balance(
            &[glass],
            &[],
            &[],
            &winter_exterior(),
            &winter_interior(),
            &[0.0],
            50,
            0.01,
        );

        assert!(result.converged, "Single pane should converge");
        assert_eq!(result.glass_temps.len(), 2);
        // Outside face should be close to exterior temp, inside face closer to interior
        assert!(result.glass_temps[0] < result.glass_temps[1],
            "Outside face {} should be colder than inside face {}",
            result.glass_temps[0], result.glass_temps[1]);
    }

    #[test]
    fn window_hb_single_pane_temps_reasonable() {
        let glass = clear_6mm();
        let result = solve_window_heat_balance(
            &[glass],
            &[],
            &[],
            &winter_exterior(),
            &winter_interior(),
            &[0.0],
            50,
            0.01,
        );

        assert!(result.converged);
        // All temperatures should be between exterior and interior air temps
        for &t in &result.glass_temps {
            assert!(t > 240.0 && t < 300.0,
                "Glass temp {t} K should be between exterior and interior");
        }
    }

    #[test]
    fn window_hb_single_pane_heat_loss() {
        let glass = clear_6mm();
        let result = solve_window_heat_balance(
            &[glass],
            &[],
            &[],
            &winter_exterior(),
            &winter_interior(),
            &[0.0],
            50,
            0.01,
        );

        assert!(result.converged);
        // In winter (cold outside, warm inside), inside face is colder than zone
        // → heat flows from zone through window → heat_flow_in should be negative
        // (the inside face is absorbing heat from the zone, but from the zone's
        // perspective it's losing heat to the window)
        assert!(result.heat_flow_in < 0.0,
            "Winter single pane should have negative heat_flow_in (heat loss): {}",
            result.heat_flow_in);
    }

    #[test]
    fn window_hb_double_pane_converges() {
        let glass = clear_3mm();
        let result = solve_window_heat_balance(
            &[glass.clone(), glass],
            &[GasType::Air],
            &[0.012],
            &winter_exterior(),
            &winter_interior(),
            &[0.0, 0.0],
            50,
            0.01,
        );

        assert!(result.converged, "Double pane should converge");
        assert_eq!(result.glass_temps.len(), 4);
        assert_eq!(result.gap_temps.len(), 1);

        // Temperatures should monotonically increase from outside to inside
        for i in 1..result.glass_temps.len() {
            assert!(result.glass_temps[i] >= result.glass_temps[i - 1] - 0.1,
                "Temps should increase outside→inside: face {} = {}, face {} = {}",
                i - 1, result.glass_temps[i - 1], i, result.glass_temps[i]);
        }
    }

    #[test]
    fn window_hb_double_pane_less_heat_loss() {
        let glass_single = clear_6mm();
        let glass_double = clear_3mm();

        let ext = winter_exterior();
        let int = winter_interior();

        let result_single = solve_window_heat_balance(
            &[glass_single], &[], &[],
            &ext, &int, &[0.0], 50, 0.01,
        );
        let result_double = solve_window_heat_balance(
            &[glass_double.clone(), glass_double],
            &[GasType::Air], &[0.012],
            &ext, &int, &[0.0, 0.0], 50, 0.01,
        );

        assert!(result_single.converged);
        assert!(result_double.converged);
        // Double pane should have less heat loss (smaller magnitude negative)
        assert!(result_double.heat_flow_in.abs() < result_single.heat_flow_in.abs(),
            "Double pane loss {} should be less than single pane loss {}",
            result_double.heat_flow_in.abs(), result_single.heat_flow_in.abs());
    }

    #[test]
    fn window_hb_argon_better_than_air() {
        let glass = clear_3mm();
        let ext = winter_exterior();
        let int = winter_interior();

        let result_air = solve_window_heat_balance(
            &[glass.clone(), glass.clone()],
            &[GasType::Air], &[0.012],
            &ext, &int, &[0.0, 0.0], 50, 0.01,
        );
        let result_argon = solve_window_heat_balance(
            &[glass.clone(), glass],
            &[GasType::Argon], &[0.012],
            &ext, &int, &[0.0, 0.0], 50, 0.01,
        );

        assert!(result_air.converged);
        assert!(result_argon.converged);
        // Argon should reduce heat loss
        assert!(result_argon.heat_flow_in.abs() < result_air.heat_flow_in.abs(),
            "Argon loss {} should be less than air loss {}",
            result_argon.heat_flow_in.abs(), result_air.heat_flow_in.abs());
    }

    #[test]
    fn window_hb_solar_warms_inside() {
        let glass = clear_6mm();
        let ext = winter_exterior();
        let int = winter_interior();

        let result_no_solar = solve_window_heat_balance(
            &[glass.clone()], &[], &[],
            &ext, &int, &[0.0], 50, 0.01,
        );
        let result_solar = solve_window_heat_balance(
            &[glass], &[], &[],
            &ext, &int, &[200.0], 50, 0.01,  // 200 W/m² absorbed
        );

        assert!(result_no_solar.converged);
        assert!(result_solar.converged);
        // Solar absorption should warm the glass → higher inside face temp
        let t_inside_no_solar = result_no_solar.glass_temps.last().unwrap();
        let t_inside_solar = result_solar.glass_temps.last().unwrap();
        assert!(t_inside_solar > t_inside_no_solar,
            "Solar should warm inside face: {} vs {}",
            t_inside_solar, t_inside_no_solar);
    }

    #[test]
    fn window_hb_condensation_detection() {
        let glass = clear_6mm();
        let ext = ExteriorConditions {
            t_air: 243.15,  // -30°C (very cold)
            t_sky: 233.15,  // -40°C
            h_conv: 30.0,   // windy
        };
        let int_high_dew = InteriorConditions {
            t_air: 294.15,
            t_mrt: 294.15,
            h_conv: 3.0,    // low interior convection
            t_dew_point: 290.0,  // high dew point (17°C) - humid room
        };

        let result = solve_window_heat_balance(
            &[glass], &[], &[],
            &ext, &int_high_dew, &[0.0], 50, 0.01,
        );

        assert!(result.converged);
        // With very cold outside and high indoor humidity, condensation should occur
        let t_inside = *result.glass_temps.last().unwrap();
        if t_inside < 290.0 {
            assert!(result.condensation,
                "Should detect condensation: inside face {} K < dew point 290 K",
                t_inside);
        }
    }

    #[test]
    fn window_hb_no_condensation_normal() {
        let glass = clear_6mm();
        let result = solve_window_heat_balance(
            &[glass], &[], &[],
            &winter_exterior(),
            &InteriorConditions {
                t_dew_point: 250.0,  // very low dew point
                ..winter_interior()
            },
            &[0.0], 50, 0.01,
        );

        assert!(result.converged);
        assert!(!result.condensation,
            "Should not condense with low dew point");
    }

    #[test]
    fn window_hb_empty_layers() {
        let result = solve_window_heat_balance(
            &[], &[], &[],
            &winter_exterior(),
            &winter_interior(),
            &[],
            50, 0.01,
        );
        assert!(result.converged);
        assert_eq!(result.glass_temps.len(), 0);
        assert_eq!(result.heat_flow_in, 0.0);
    }

    #[test]
    fn window_hb_u_value_reasonable() {
        let glass = clear_6mm();
        let result = solve_window_heat_balance(
            &[glass], &[], &[],
            &winter_exterior(),
            &winter_interior(),
            &[0.0], 50, 0.01,
        );

        assert!(result.converged);
        // Single clear pane U should be around 5-7 W/(m²·K)
        assert!(result.u_value > 3.0 && result.u_value < 10.0,
            "U-value {} should be reasonable for single clear pane",
            result.u_value);
    }

    // ─── Frame/Divider Tests ─────────────────────────────────────────

    #[test]
    fn frame_divider_heat_flow() {
        let fd = FrameDivider {
            frame_u_value: 3.0,
            frame_area_fraction: 0.15,
            divider_u_value: 5.0,
            divider_area_fraction: 0.05,
        };

        // Winter: cold outside (255K), warm inside (294K)
        let q = fd.heat_flow(255.15, 294.15);
        // Outside is colder → heat flows from inside to outside → negative
        assert!(q < 0.0, "Frame heat flow should be negative in winter: {q}");

        // Magnitude check
        let dt = 255.15 - 294.15;
        let expected = 3.0 * 0.15 * dt + 5.0 * 0.05 * dt;
        assert!((q - expected).abs() < 1e-6);
    }

    #[test]
    fn frame_divider_glass_area_fraction() {
        let fd = FrameDivider {
            frame_u_value: 3.0,
            frame_area_fraction: 0.15,
            divider_u_value: 5.0,
            divider_area_fraction: 0.05,
        };
        assert!((fd.glass_area_fraction() - 0.80).abs() < 1e-10);
    }
}
