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

    // ─── New convection tests ────────────────────────────────────────

    #[test]
    fn ashrae_interior_upward() {
        // cos_tilt=0.5 → upward-facing (> 0.3827), returns 4.043
        let h = ashrae_interior_convection(0.5);
        assert!((h - 4.043).abs() < 1e-10, "h={h}");
        assert!(h > 3.0, "upward h should be reasonable");
    }

    #[test]
    fn ashrae_interior_downward() {
        // cos_tilt=-0.5 → downward-facing (< -0.3827), returns 0.948
        let h = ashrae_interior_convection(-0.5);
        assert!((h - 0.948).abs() < 1e-10, "h={h}");
        // Downward h should be lower than upward h
        let h_up = ashrae_interior_convection(0.5);
        assert!(h < h_up, "downward h={h} should be < upward h={h_up}");
    }

    #[test]
    fn tarp_large_delta_t() {
        // 50K temperature difference on vertical wall: h = 1.31 * 50^(1/3)
        let h = tarp_interior_convection(50.0, 0.0, 3.0);
        let expected = 1.31 * 50.0_f64.powf(1.0 / 3.0);
        assert!((h - expected).abs() < 1e-10, "h={h}, expected={expected}");
        // h should scale with dt^(1/3): compare to 5K case
        let h_small = tarp_interior_convection(5.0, 0.0, 3.0);
        let ratio = h / h_small;
        let expected_ratio = (50.0_f64 / 5.0).powf(1.0 / 3.0);
        assert!(
            (ratio - expected_ratio).abs() < 1e-6,
            "ratio={ratio}, expected_ratio={expected_ratio}"
        );
    }

    #[test]
    fn tarp_heated_ceiling() {
        // Heated ceiling: surface warmer (+5K), cos_tilt=-1 (face-down)
        // delta_t=5, cos_tilt=-1 → delta_t * cos_tilt = -5 < 0 → stable (cooled_floor_or_heated_ceiling)
        let h = tarp_interior_convection(5.0, -1.0, 3.0);
        // Stable regime: h = 1.810 * |dT|^(1/3) / (1.382 + |cos_tilt|)
        let expected = 1.810 * 5.0_f64.powf(1.0 / 3.0) / (1.382 + 1.0);
        assert!((h - expected).abs() < 1e-10, "h={h}, expected={expected}");
        // Stable → lower convection compared to vertical
        let h_vert = tarp_interior_convection(5.0, 0.0, 3.0);
        assert!(h < h_vert, "heated ceiling h={h} should be < vertical h={h_vert}");
    }

    #[test]
    fn tarp_cooled_ceiling() {
        // Cooled ceiling: surface cooler (-5K), cos_tilt=-1 (face-down)
        // delta_t=-5, cos_tilt=-1 → delta_t * cos_tilt = 5 > 0 → unstable (heated_floor_or_cooled_ceiling)
        let h = tarp_interior_convection(-5.0, -1.0, 3.0);
        // Unstable regime: h = 9.482 * |dT|^(1/3) / (7.283 - |cos_tilt|)
        let expected = 9.482 * 5.0_f64.powf(1.0 / 3.0) / (7.283 - 1.0);
        assert!((h - expected).abs() < 1e-10, "h={h}, expected={expected}");
        // Unstable → higher convection compared to stable heated ceiling
        let h_stable = tarp_interior_convection(5.0, -1.0, 3.0);
        assert!(
            h > h_stable,
            "cooled ceiling h={h} should be > heated ceiling h={h_stable}"
        );
    }

    #[test]
    fn doe2_exterior_zero_wind() {
        // With zero wind speed, forced convection is zero → result equals natural only
        let h = doe2_exterior_convection(5.0, 0.0, 0.0, SurfaceRoughness::MediumRough);
        let h_natural = tarp_interior_convection(5.0, 0.0, 1.0);
        assert!(
            (h - h_natural).abs() < 1e-10,
            "h={h} should equal h_natural={h_natural} at zero wind"
        );
    }

    #[test]
    fn doe2_exterior_rough_vs_smooth() {
        // Rough roughness should give higher h than VerySmooth at same wind
        let h_rough = doe2_exterior_convection(5.0, 0.0, 5.0, SurfaceRoughness::Rough);
        let h_smooth = doe2_exterior_convection(5.0, 0.0, 5.0, SurfaceRoughness::VerySmooth);
        assert!(
            h_rough > h_smooth,
            "Rough h={h_rough} should be > VerySmooth h={h_smooth}"
        );
        // Verify the difference comes from roughness multiplier
        let rf_rough = SurfaceRoughness::Rough.doe2_roughness_multiplier();
        let rf_smooth = SurfaceRoughness::VerySmooth.doe2_roughness_multiplier();
        assert!(rf_rough > rf_smooth);
    }

    #[test]
    fn ashrae_exterior_zero_wind() {
        // At zero wind: h = D + E*0 + F*0 = D, clamped to at least 0.1
        let h = ashrae_exterior_convection(0.0, SurfaceRoughness::MediumRough);
        let (d, _e, _f) = SurfaceRoughness::MediumRough.ashrae_ext_conv_coeffs();
        assert!((h - d).abs() < 1e-10, "h={h}, D={d}");
        assert!(h >= 0.1, "h={h} should be >= 0.1");
    }

    #[test]
    fn local_wind_speed_open_terrain() {
        // Open terrain: exponent=0.14, thickness=270 (same as met station)
        let v_open = local_wind_speed(5.0, 15.0, 0.14, 270.0);
        // Urban terrain: exponent=0.33, thickness=460
        let v_urban = local_wind_speed(5.0, 15.0, 0.33, 460.0);
        // Open terrain should have higher local wind speed than urban
        assert!(
            v_open > v_urban,
            "open v={v_open} should be > urban v={v_urban}"
        );
        // For open terrain at met height (10m), with same parameters as met station,
        // but since z is clamped to max(surface_height, z_met)=max(15,10)=15,
        // v should be somewhat close to met wind speed
        assert!(v_open > 0.0 && v_open < 10.0, "v_open={v_open}");
    }

    #[test]
    fn local_wind_speed_below_met_height() {
        // Surface height below met height (10m) → clamped to met height
        let v_low = local_wind_speed(5.0, 3.0, 0.22, 370.0);
        let v_at_met = local_wind_speed(5.0, 10.0, 0.22, 370.0);
        // Both should give same result since 3m is clamped to 10m
        assert!(
            (v_low - v_at_met).abs() < 1e-10,
            "v_low={v_low} should equal v_at_met={v_at_met} (clamped)"
        );
    }
}
