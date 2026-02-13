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
}
