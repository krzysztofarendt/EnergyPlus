//! Convection coefficient models for interior and exterior surfaces.
//!
//! Interior: TARP (Walton) natural convection correlations.
//! Exterior: DOE-2 combined (natural + forced) and ASHRAE simple models.

use ep_materials::SurfaceRoughness;

/// Stefan-Boltzmann constant (W/(m2-K4)).
pub const STEFAN_BOLTZMANN: f64 = 5.670374419e-8;

// ─── Interior Convection (TARP) ──────────────────────────────────────

/// TARP interior natural convection coefficient.
///
/// Uses Walton's correlations based on surface tilt and temperature difference.
/// Reference: Walton, G.N. 1983. Thermal Analysis Research Program Reference Manual.
///
/// Returns h_c in W/(m2-K).
pub fn tarp_interior_convection(
    delta_t: f64,       // T_surface - T_air (positive = surface warmer)
    cos_tilt: f64,      // cosine of surface tilt (1=horiz up, 0=vert, -1=horiz down)
    _surface_height: f64, // characteristic height (m)
) -> f64 {
    let dt_abs = delta_t.abs();
    if dt_abs < 1e-10 {
        return 0.1; // Minimum convection
    }

    // Classify the convection regime
    let is_heated_floor_or_cooled_ceiling = delta_t * cos_tilt > 0.0;
    let is_cooled_floor_or_heated_ceiling = delta_t * cos_tilt < 0.0;

    if cos_tilt.abs() < 0.3827 {
        // Near-vertical surface: h = 1.31 * |dT|^(1/3)
        1.31 * dt_abs.powf(1.0 / 3.0)
    } else if is_heated_floor_or_cooled_ceiling {
        // Unstable (buoyancy-enhanced): h = 9.482 * |dT|^(1/3) / (7.283 - |cos(tilt)|)
        let denom = 7.283 - cos_tilt.abs();
        if denom > 0.001 {
            9.482 * dt_abs.powf(1.0 / 3.0) / denom
        } else {
            1.31 * dt_abs.powf(1.0 / 3.0)
        }
    } else if is_cooled_floor_or_heated_ceiling {
        // Stable (buoyancy-suppressed): h = 1.810 * |dT|^(1/3) / (1.382 + |cos(tilt)|)
        1.810 * dt_abs.powf(1.0 / 3.0) / (1.382 + cos_tilt.abs())
    } else {
        // Fallback
        1.31 * dt_abs.powf(1.0 / 3.0)
    }
}

/// ASHRAE simple interior convection coefficient.
///
/// Fixed values based on surface orientation (ASHRAE Handbook of Fundamentals).
/// Returns h_c in W/(m2-K).
pub fn ashrae_interior_convection(cos_tilt: f64) -> f64 {
    if cos_tilt.abs() < 0.3827 {
        // Vertical: 3.076 W/(m2-K)
        3.076
    } else if cos_tilt > 0.0 {
        // Upward facing (floor/ceiling depending on heat flow): 4.043
        4.043
    } else {
        // Downward facing: 0.948
        0.948
    }
}

// ─── Exterior Convection (DOE-2 / ASHRAE Simple) ────────────────────

/// DOE-2 exterior convection model.
///
/// Combines natural convection (Walton) with forced convection (Sparrow et al).
/// h_total = h_natural + h_forced
///
/// Returns h_c in W/(m2-K).
pub fn doe2_exterior_convection(
    delta_t: f64,       // T_surface - T_air
    cos_tilt: f64,      // cosine of surface tilt
    wind_speed: f64,    // local wind speed at surface (m/s)
    roughness: SurfaceRoughness,
) -> f64 {
    // Natural convection (same correlations as TARP interior)
    let h_natural = tarp_interior_convection(delta_t, cos_tilt, 1.0);

    // Forced convection: h_f = 2.537 * R_f * sqrt(P * V / A)
    // Simplified for typical surfaces: h_f = R_f * (a + b * V)
    let rf = roughness.doe2_roughness_multiplier();

    // MoWiTT correlation coefficients (windward)
    let h_forced = if wind_speed > 0.0 {
        rf * 2.537 * (wind_speed * 1.0_f64).sqrt() // Simplified: P/A ~ 1.0
    } else {
        0.0
    };

    h_natural + h_forced
}

/// ASHRAE simple exterior convection model.
///
/// h = D + E * V + F * V^2
/// where D, E, F depend on surface roughness.
///
/// Returns h_c in W/(m2-K).
pub fn ashrae_exterior_convection(
    wind_speed: f64,
    roughness: SurfaceRoughness,
) -> f64 {
    let (d, e, f) = roughness.ashrae_ext_conv_coeffs();
    (d + e * wind_speed + f * wind_speed * wind_speed).max(0.1)
}

/// Calculate local wind speed at surface height using power-law profile.
///
/// V_local = V_met * (delta_met / z_met)^p_met * (z / delta)^p
///
/// For urban terrain: p=0.33, delta=460m
/// For suburban terrain: p=0.22, delta=370m
/// For open terrain: p=0.14, delta=270m
pub fn local_wind_speed(
    met_wind_speed: f64,  // weather station wind speed (m/s)
    surface_height: f64,  // height of surface centroid (m)
    terrain_exponent: f64, // power law exponent for site
    terrain_thickness: f64, // boundary layer thickness for site (m)
) -> f64 {
    // Meteorological station assumed: z_met = 10m, open terrain p=0.14, delta=270m
    let z_met = 10.0;
    let p_met = 0.14;
    let delta_met = 270.0;

    let z = surface_height.max(z_met); // Don't go below met height

    let met_factor = (delta_met / z_met).powf(p_met);
    let site_factor = (z / terrain_thickness).powf(terrain_exponent);

    met_wind_speed * site_factor / met_factor
}

// ─── Exterior Longwave Radiation ─────────────────────────────────────

/// Linearized exterior longwave radiation coefficient.
///
/// h_rad = eps * sigma * (T_surf^2 + T_env^2) * (T_surf + T_env)
///
/// Returns h_rad in W/(m2-K).
pub fn exterior_lw_radiation_coeff(
    emissivity: f64,
    t_surface_k: f64,
    t_environment_k: f64,
) -> f64 {
    let ts2 = t_surface_k * t_surface_k;
    let te2 = t_environment_k * t_environment_k;
    emissivity * STEFAN_BOLTZMANN * (ts2 + te2) * (t_surface_k + t_environment_k)
}

/// Interior longwave radiation coefficient (linearized).
///
/// For interior surfaces, the environment temperature is the mean radiant temperature.
pub fn interior_lw_radiation_coeff(
    emissivity: f64,
    t_surface_k: f64,
    t_mrt_k: f64,
) -> f64 {
    exterior_lw_radiation_coeff(emissivity, t_surface_k, t_mrt_k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tarp_vertical_wall() {
        // Vertical wall, 5K temperature difference
        let h = tarp_interior_convection(5.0, 0.0, 3.0);
        // h = 1.31 * 5^(1/3) ≈ 2.24
        assert!(h > 2.0 && h < 2.5, "h={h}");
    }

    #[test]
    fn tarp_heated_floor() {
        // Horizontal floor (cos_tilt=1), surface warmer than air (unstable)
        let h = tarp_interior_convection(5.0, 1.0, 3.0);
        // Unstable: higher convection
        assert!(h > 2.0, "h={h}");
    }

    #[test]
    fn tarp_cooled_floor() {
        // Horizontal floor (cos_tilt=1), surface cooler than air (stable)
        let h = tarp_interior_convection(-5.0, 1.0, 3.0);
        // Stable: lower convection
        assert!(h > 0.0 && h < 2.0, "h={h}");
    }

    #[test]
    fn tarp_zero_dt() {
        let h = tarp_interior_convection(0.0, 0.0, 3.0);
        assert!((h - 0.1).abs() < 1e-10);
    }

    #[test]
    fn ashrae_interior_vertical() {
        let h = ashrae_interior_convection(0.0);
        assert!((h - 3.076).abs() < 1e-10);
    }

    #[test]
    fn ashrae_exterior_smooth() {
        let h = ashrae_exterior_convection(5.0, SurfaceRoughness::VerySmooth);
        let (d, e, f) = SurfaceRoughness::VerySmooth.ashrae_ext_conv_coeffs();
        let expected = d + e * 5.0 + f * 25.0;
        assert!((h - expected).abs() < 1e-10);
    }

    #[test]
    fn doe2_exterior_with_wind() {
        let h = doe2_exterior_convection(5.0, 0.0, 3.0, SurfaceRoughness::MediumRough);
        // Should be larger than natural convection alone
        let h_nat = tarp_interior_convection(5.0, 0.0, 1.0);
        assert!(h > h_nat, "h={h}, h_nat={h_nat}");
    }

    #[test]
    fn local_wind_speed_calculation() {
        // Urban terrain: exponent=0.33, thickness=460
        let v = local_wind_speed(5.0, 15.0, 0.33, 460.0);
        assert!(v > 0.0 && v < 5.0, "v={v}");
    }

    #[test]
    fn lw_radiation_coeff() {
        let h = exterior_lw_radiation_coeff(0.9, 293.15, 253.15);
        // Should be around 4-5 W/(m2-K) for typical conditions
        assert!(h > 3.0 && h < 7.0, "h={h}");
    }
}
