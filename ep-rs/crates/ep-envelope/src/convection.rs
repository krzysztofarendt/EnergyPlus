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

// ─── Walton Interior Correlations ────────────────────────────────────

/// Walton unstable tilted surface correlation.
///
/// For tilted surfaces with buoyancy-enhanced (unstable) convection.
/// h = 9.482 * |ΔT|^(1/3) / (7.283 - |cos(tilt)|)
///
/// Reference: Walton, G.N. 1983.
pub fn walton_unstable_tilted(delta_t: f64, cos_tilt: f64) -> f64 {
    let dt_abs = delta_t.abs();
    if dt_abs < 1e-10 {
        return 0.1;
    }
    let denom = 7.283 - cos_tilt.abs();
    if denom > 0.001 {
        9.482 * dt_abs.powf(1.0 / 3.0) / denom
    } else {
        1.31 * dt_abs.powf(1.0 / 3.0)
    }
}

/// Walton stable tilted surface correlation.
///
/// For tilted surfaces with buoyancy-suppressed (stable) convection.
/// h = 1.810 * |ΔT|^(1/3) / (1.382 + |cos(tilt)|)
///
/// Reference: Walton, G.N. 1983.
pub fn walton_stable_tilted(delta_t: f64, cos_tilt: f64) -> f64 {
    let dt_abs = delta_t.abs();
    if dt_abs < 1e-10 {
        return 0.1;
    }
    1.810 * dt_abs.powf(1.0 / 3.0) / (1.382 + cos_tilt.abs())
}

/// Ceiling diffuser interior convection coefficient (Fisher/Pedersen).
///
/// h = C * (ACH)^a * |ΔT|^b
/// where ACH = air changes per hour, C/a/b depend on surface type.
///
/// Reference: Fisher, D.E. and C.O. Pedersen. 1997.
pub fn ceiling_diffuser_convection(
    delta_t: f64,
    cos_tilt: f64,
    ach: f64, // air changes per hour
) -> f64 {
    let dt_abs = delta_t.abs().max(0.001);
    let ach_val = ach.max(0.0);

    if cos_tilt.abs() < 0.3827 {
        // Wall: h = 1.208 * ACH^0.467 + 1.31 * |ΔT|^(1/3)
        let h_forced = if ach_val > 0.0 {
            1.208 * ach_val.powf(0.467)
        } else {
            0.0
        };
        let h_natural = 1.31 * dt_abs.powf(1.0 / 3.0);
        (h_forced * h_forced + h_natural * h_natural).sqrt()
    } else if cos_tilt > 0.0 {
        // Floor (upward-facing): h = 3.873 + 0.082 * ACH^0.98
        3.873 + 0.082 * ach_val.powf(0.98)
    } else {
        // Ceiling (downward-facing): h = 0.49 + 0.327 * ACH^1.0
        0.49 + 0.327 * ach_val
    }
}

// ─── Enhanced Model Selection ────────────────────────────────────────

/// Interior convection model type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InteriorConvectionModel {
    /// Simple ASHRAE fixed values.
    Simple,
    /// TARP natural convection correlations.
    #[default]
    Tarp,
    /// Ceiling diffuser model (Fisher/Pedersen).
    CeilingDiffuser,
}

/// Exterior convection model type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExteriorConvectionModel {
    /// Simple ASHRAE combined coefficients.
    Simple,
    /// DOE-2 model with TARP natural + forced.
    #[default]
    Doe2,
    /// MoWiTT wind-direction-dependent model.
    MoWiTT,
}

/// Select interior convection coefficient based on model.
pub fn interior_convection(
    model: InteriorConvectionModel,
    delta_t: f64,
    cos_tilt: f64,
    surface_height: f64,
    ach: f64,
) -> f64 {
    match model {
        InteriorConvectionModel::Simple => ashrae_interior_convection(cos_tilt),
        InteriorConvectionModel::Tarp => tarp_interior_convection(delta_t, cos_tilt, surface_height),
        InteriorConvectionModel::CeilingDiffuser => ceiling_diffuser_convection(delta_t, cos_tilt, ach),
    }
}

/// Select exterior convection coefficient based on model.
pub fn exterior_convection(
    model: ExteriorConvectionModel,
    delta_t: f64,
    cos_tilt: f64,
    wind_speed: f64,
    wind_direction_rad: f64,
    surface_azimuth_rad: f64,
    roughness: SurfaceRoughness,
) -> f64 {
    match model {
        ExteriorConvectionModel::Simple => ashrae_exterior_convection(wind_speed, roughness),
        ExteriorConvectionModel::Doe2 => doe2_exterior_convection(delta_t, cos_tilt, wind_speed, roughness),
        ExteriorConvectionModel::MoWiTT => {
            mowitt_exterior_convection(delta_t, cos_tilt, wind_speed, wind_direction_rad, surface_azimuth_rad)
        }
    }
}

// ─── MoWiTT Exterior Convection ─────────────────────────────────────

/// MoWiTT wind-direction-dependent exterior convection model.
///
/// h = sqrt(h_natural² + (a * V^b)²)
/// where a, b depend on windward/leeward orientation.
///
/// Reference: Yazdanian, M. and J.H. Klems. 1994.
pub fn mowitt_exterior_convection(
    delta_t: f64,
    cos_tilt: f64,
    wind_speed: f64,
    wind_direction_rad: f64,
    surface_azimuth_rad: f64,
) -> f64 {
    let h_natural = tarp_interior_convection(delta_t, cos_tilt, 1.0);

    // Determine if windward or leeward
    let angle_diff = (wind_direction_rad - surface_azimuth_rad).abs();
    let is_windward = angle_diff < std::f64::consts::FRAC_PI_2
        || angle_diff > 3.0 * std::f64::consts::FRAC_PI_2;

    // MoWiTT coefficients (from EnergyPlus)
    let (a, b) = if is_windward {
        (3.26, 0.89) // windward
    } else {
        (3.55, 0.617) // leeward
    };

    let h_forced = a * wind_speed.powf(b);
    (h_natural * h_natural + h_forced * h_forced).sqrt().max(0.1)
}

/// Simplified combined exterior convection coefficient.
///
/// Returns standard ASHRAE values for exposed or sheltered conditions.
/// Exposed: 17.8 W/(m²·K), Sheltered: 8.3 W/(m²·K).
pub fn simplified_combined_exterior(is_sheltered: bool) -> f64 {
    if is_sheltered {
        8.3
    } else {
        17.8
    }
}

// ─── Terrain Parameters ─────────────────────────────────────────────

/// Terrain category for wind profile calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerrainCategory {
    /// Open terrain (flat, open country; airports).
    Ocean,
    /// Flat open country.
    Flat,
    /// Rough/open with scattered obstructions.
    #[default]
    Country,
    /// Suburban terrain.
    Suburbs,
    /// Urban/city center.
    City,
}

impl TerrainCategory {
    /// Power-law exponent for terrain.
    pub fn exponent(self) -> f64 {
        match self {
            Self::Ocean => 0.10,
            Self::Flat => 0.14,
            Self::Country => 0.22,
            Self::Suburbs => 0.22,
            Self::City => 0.33,
        }
    }

    /// Boundary layer thickness (m).
    pub fn boundary_layer_thickness(self) -> f64 {
        match self {
            Self::Ocean => 210.0,
            Self::Flat => 270.0,
            Self::Country => 370.0,
            Self::Suburbs => 370.0,
            Self::City => 460.0,
        }
    }

    /// Get local wind speed for this terrain.
    pub fn local_wind(&self, met_wind_speed: f64, surface_height: f64) -> f64 {
        local_wind_speed(
            met_wind_speed,
            surface_height,
            self.exponent(),
            self.boundary_layer_thickness(),
        )
    }
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

    // ─── Walton correlations ─────────────────────────────────────────

    #[test]
    fn walton_unstable_vertical() {
        let h = walton_unstable_tilted(5.0, 0.0);
        // For vertical (cos_tilt=0): h = 9.482 * 5^(1/3) / (7.283 - 0)
        let expected = 9.482 * 5.0_f64.powf(1.0 / 3.0) / 7.283;
        assert!((h - expected).abs() < 1e-10, "h={h}, expected={expected}");
    }

    #[test]
    fn walton_stable_horizontal() {
        let h = walton_stable_tilted(5.0, 1.0);
        let expected = 1.810 * 5.0_f64.powf(1.0 / 3.0) / (1.382 + 1.0);
        assert!((h - expected).abs() < 1e-10, "h={h}, expected={expected}");
        assert!(h < 2.0, "stable h={h} should be low");
    }

    #[test]
    fn walton_unstable_gt_stable() {
        let h_unstable = walton_unstable_tilted(5.0, 0.5);
        let h_stable = walton_stable_tilted(5.0, 0.5);
        assert!(
            h_unstable > h_stable,
            "unstable h={h_unstable} should be > stable h={h_stable}"
        );
    }

    // ─── Ceiling diffuser ────────────────────────────────────────────

    #[test]
    fn ceiling_diffuser_wall() {
        let h = ceiling_diffuser_convection(5.0, 0.0, 6.0);
        // Wall: sqrt((1.208 * 6^0.467)^2 + (1.31 * 5^(1/3))^2)
        assert!(h > 2.0 && h < 5.0, "wall h={h}");
    }

    #[test]
    fn ceiling_diffuser_floor() {
        let h = ceiling_diffuser_convection(5.0, 1.0, 6.0);
        // Floor: 3.873 + 0.082 * 6^0.98
        assert!(h > 3.5 && h < 5.0, "floor h={h}");
    }

    #[test]
    fn ceiling_diffuser_ceiling() {
        let h = ceiling_diffuser_convection(5.0, -1.0, 6.0);
        // Ceiling: 0.49 + 0.327 * 6
        let expected = 0.49 + 0.327 * 6.0;
        assert!((h - expected).abs() < 1e-10, "ceiling h={h}, expected={expected}");
    }

    #[test]
    fn ceiling_diffuser_zero_ach() {
        // With no air changes, should still have natural convection component
        let h = ceiling_diffuser_convection(5.0, 0.0, 0.0);
        assert!(h > 0.0, "h={h} should be > 0 even with zero ACH");
    }

    // ─── MoWiTT exterior ─────────────────────────────────────────────

    #[test]
    fn mowitt_windward() {
        let h = mowitt_exterior_convection(5.0, 0.0, 5.0, 0.0, 0.0);
        // Windward: sqrt(h_nat² + (3.26 * 5^0.89)²)
        assert!(h > 10.0, "windward h={h} should be significant");
    }

    #[test]
    fn mowitt_leeward() {
        let h = mowitt_exterior_convection(5.0, 0.0, 5.0, std::f64::consts::PI, 0.0);
        // Leeward: lower forced convection
        assert!(h > 5.0, "leeward h={h} should be positive");
    }

    #[test]
    fn mowitt_windward_gt_leeward() {
        let h_ww = mowitt_exterior_convection(5.0, 0.0, 5.0, 0.0, 0.0);
        let h_lw = mowitt_exterior_convection(5.0, 0.0, 5.0, std::f64::consts::PI, 0.0);
        assert!(
            h_ww > h_lw,
            "windward h={h_ww} should be > leeward h={h_lw}"
        );
    }

    #[test]
    fn mowitt_zero_wind() {
        let h = mowitt_exterior_convection(5.0, 0.0, 0.0, 0.0, 0.0);
        let h_nat = tarp_interior_convection(5.0, 0.0, 1.0);
        // With zero wind, should equal natural convection
        assert!(
            (h - h_nat).abs() < 0.1,
            "h={h} should ≈ h_nat={h_nat} at zero wind"
        );
    }

    // ─── Simplified combined ─────────────────────────────────────────

    #[test]
    fn simplified_combined_values() {
        assert!((simplified_combined_exterior(false) - 17.8).abs() < 1e-10);
        assert!((simplified_combined_exterior(true) - 8.3).abs() < 1e-10);
    }

    // ─── Enhanced model selection ────────────────────────────────────

    #[test]
    fn interior_model_selection() {
        let h_simple = interior_convection(InteriorConvectionModel::Simple, 5.0, 0.0, 3.0, 0.0);
        let h_tarp = interior_convection(InteriorConvectionModel::Tarp, 5.0, 0.0, 3.0, 0.0);
        let h_cd = interior_convection(InteriorConvectionModel::CeilingDiffuser, 5.0, 0.0, 3.0, 6.0);

        assert!((h_simple - 3.076).abs() < 1e-10);
        assert!(h_tarp > 0.0 && h_tarp < 10.0);
        assert!(h_cd > 0.0 && h_cd < 10.0);
    }

    #[test]
    fn exterior_model_selection() {
        let h_simple = exterior_convection(
            ExteriorConvectionModel::Simple, 5.0, 0.0, 5.0, 0.0, 0.0, SurfaceRoughness::MediumRough,
        );
        let h_doe2 = exterior_convection(
            ExteriorConvectionModel::Doe2, 5.0, 0.0, 5.0, 0.0, 0.0, SurfaceRoughness::MediumRough,
        );
        let h_mowitt = exterior_convection(
            ExteriorConvectionModel::MoWiTT, 5.0, 0.0, 5.0, 0.0, 0.0, SurfaceRoughness::MediumRough,
        );

        assert!(h_simple > 0.0);
        assert!(h_doe2 > 0.0);
        assert!(h_mowitt > 0.0);
    }

    // ─── Terrain ─────────────────────────────────────────────────────

    #[test]
    fn terrain_local_wind() {
        let v_city = TerrainCategory::City.local_wind(5.0, 15.0);
        let v_flat = TerrainCategory::Flat.local_wind(5.0, 15.0);
        assert!(
            v_flat > v_city,
            "flat v={v_flat} should be > city v={v_city}"
        );
    }

    #[test]
    fn terrain_parameters() {
        assert!(TerrainCategory::City.exponent() > TerrainCategory::Flat.exponent());
        assert!(TerrainCategory::City.boundary_layer_thickness() > TerrainCategory::Flat.boundary_layer_thickness());
    }
}
