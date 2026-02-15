//! Interior daylight reflections using the split-flux method.
//!
//! Computes first-bounce and higher-order interior reflections of daylight
//! from floor, walls, and ceiling to daylighting reference points.
//! Based on the split-flux method used in EnergyPlus daylighting.

/// Surface reflectances for interior reflection calculations.
#[derive(Debug, Clone, Copy)]
pub struct ZoneReflectances {
    /// Area-weighted average visible reflectance of floor surfaces.
    pub floor: f64,
    /// Area-weighted average visible reflectance of wall surfaces.
    pub wall: f64,
    /// Area-weighted average visible reflectance of ceiling surfaces.
    pub ceiling: f64,
    /// Total floor area (m2).
    pub floor_area: f64,
    /// Total wall area (m2).
    pub wall_area: f64,
    /// Total ceiling area (m2).
    pub ceiling_area: f64,
}

impl Default for ZoneReflectances {
    fn default() -> Self {
        Self {
            floor: 0.2,
            wall: 0.5,
            ceiling: 0.7,
            floor_area: 10.0,
            wall_area: 40.0,
            ceiling_area: 10.0,
        }
    }
}

impl ZoneReflectances {
    /// Total interior surface area.
    pub fn total_area(&self) -> f64 {
        self.floor_area + self.wall_area + self.ceiling_area
    }

    /// Area-weighted average reflectance.
    pub fn average_reflectance(&self) -> f64 {
        let total = self.total_area();
        if total <= 0.0 {
            return 0.0;
        }
        (self.floor * self.floor_area
            + self.wall * self.wall_area
            + self.ceiling * self.ceiling_area)
            / total
    }
}

/// First-bounce reflected illuminance at a reference point.
///
/// Uses the split-flux method: transmitted daylight first hits interior
/// surfaces, and the reflected flux is split into upward (floor→ceiling)
/// and downward (ceiling→floor) components.
///
/// # Arguments
/// * `transmitted_flux` - Total luminous flux through window (lumens)
/// * `zone` - Zone surface reflectances and areas
/// * `window_head_height` - Height of window head above floor (m)
/// * `zone_height` - Zone ceiling height (m)
/// * `ref_height` - Reference point height above floor (m)
///
/// # Returns
/// Reflected illuminance at the reference point (lux)
pub fn first_bounce_illuminance(
    transmitted_flux: f64,
    zone: &ZoneReflectances,
    window_head_height: f64,
    zone_height: f64,
    ref_height: f64,
) -> f64 {
    if transmitted_flux <= 0.0 || zone.total_area() <= 0.0 {
        return 0.0;
    }

    // Split factor: fraction of first-hit flux going to upper hemisphere (ceiling)
    // vs lower hemisphere (floor). Based on window height.
    let split_up = if zone_height > 0.0 {
        (window_head_height / zone_height).clamp(0.0, 1.0)
    } else {
        0.5
    };
    let split_down = 1.0 - split_up;

    // First-bounce flux to floor and ceiling
    let flux_to_floor = transmitted_flux * split_down;
    let flux_to_ceiling = transmitted_flux * split_up;

    // First-bounce reflected flux
    let reflected_from_floor = flux_to_floor * zone.floor;
    let reflected_from_ceiling = flux_to_ceiling * zone.ceiling;
    let first_bounce = reflected_from_floor + reflected_from_ceiling;

    // Average reflectance for inter-reflection series
    let rho_avg = zone.average_reflectance();

    // Inter-reflection multiplier: 1 / (1 - rho_avg)
    // accounts for multiple bounces within the room
    let multiplier = if rho_avg < 0.99 {
        1.0 / (1.0 - rho_avg)
    } else {
        100.0 // cap for very reflective rooms
    };

    // Total reflected flux including inter-reflections
    let total_reflected = first_bounce * multiplier;

    // Convert flux to illuminance at the reference point
    // Approximation: uniform reflected illuminance over all interior surfaces
    // weighted by reference point position (higher points get more ceiling reflection)
    let ref_fraction = if zone_height > 0.0 {
        ref_height / zone_height
    } else {
        0.5
    };

    // Mix of floor-reflected (reaching ref from below) and ceiling-reflected
    let ref_illuminance = total_reflected / zone.total_area();

    // Position correction: reference points near floor get more floor reflection,
    // near ceiling get more ceiling reflection
    let pos_correction = 0.8 + 0.4 * (0.5 - ref_fraction).abs();

    (ref_illuminance * pos_correction).max(0.0)
}

/// Compute the internally reflected component (IRC) of daylight factor.
///
/// IRC = (τ_vis × A_win) / (A_total × (1 - ρ_avg)) × (ρ_fw × C_fw + ρ_cw × C_cw)
///
/// where:
/// - τ_vis: visible transmittance of window
/// - A_win: window area
/// - A_total: total interior surface area
/// - ρ_avg: average interior reflectance
/// - ρ_fw, ρ_cw: floor-wall and ceiling-wall reflectances
/// - C_fw, C_cw: configuration factors (depend on window position)
pub fn internally_reflected_component(
    window_visible_transmittance: f64,
    window_area: f64,
    zone: &ZoneReflectances,
) -> f64 {
    let total_area = zone.total_area();
    if total_area <= 0.0 || window_area <= 0.0 {
        return 0.0;
    }

    let rho_avg = zone.average_reflectance();
    let denom = total_area * (1.0 - rho_avg).max(0.01);

    // Configuration factors (simplified for diffuse initial distribution)
    // C_fw ≈ 0.5 (equal split between floor and walls)
    // C_cw ≈ 0.5 (equal split between ceiling and walls)
    let rho_fw = 0.5 * zone.floor + 0.5 * zone.wall;
    let rho_cw = 0.5 * zone.ceiling + 0.5 * zone.wall;

    let irc = window_visible_transmittance * window_area * (rho_fw + rho_cw) / denom;

    irc.max(0.0)
}

/// Tregenza sky patch definition for hemisphere discretization.
///
/// The Tregenza scheme divides the sky hemisphere into 145 patches
/// at 7 altitude bands plus zenith, providing a standard basis for
/// daylight coefficient calculations.
#[derive(Debug, Clone, Copy)]
pub struct SkyPatch {
    /// Altitude of patch center (radians).
    pub altitude: f64,
    /// Azimuth of patch center (radians).
    pub azimuth: f64,
    /// Solid angle subtended by this patch (steradians).
    pub solid_angle: f64,
}

/// Generate the 145 Tregenza sky patches.
///
/// Returns patches organized in 7 altitude bands (6°, 18°, 30°, 42°, 54°, 66°, 78°)
/// plus 1 zenith patch. The number of patches per band decreases with altitude
/// to maintain approximately equal solid angles.
pub fn tregenza_patches() -> Vec<SkyPatch> {
    use std::f64::consts::PI;

    // Tregenza subdivision: (center_altitude_deg, number_of_patches)
    let bands: [(f64, usize); 8] = [
        (6.0, 30),   // band 1: 30 patches
        (18.0, 30),  // band 2: 30 patches
        (30.0, 24),  // band 3: 24 patches
        (42.0, 24),  // band 4: 24 patches
        (54.0, 18),  // band 5: 18 patches
        (66.0, 12),  // band 6: 12 patches
        (78.0, 6),   // band 7: 6 patches
        (90.0, 1),   // zenith: 1 patch
    ];

    let mut patches = Vec::with_capacity(145);

    for &(alt_deg, n_patches) in &bands {
        let alt_rad = alt_deg * PI / 180.0;
        let d_az = 2.0 * PI / n_patches as f64;

        // Solid angle per patch: approximate band area / n_patches
        let half_band = 6.0 * PI / 180.0; // ±6° per band
        let cos_low = (alt_rad - half_band).max(0.0).cos();
        let cos_high = (alt_rad + half_band).min(PI / 2.0).cos();
        let band_area = 2.0 * PI * (cos_high - cos_low).abs();
        let solid_angle = if n_patches > 0 {
            band_area / n_patches as f64
        } else {
            0.0
        };

        for i in 0..n_patches {
            let azi_rad = (i as f64 + 0.5) * d_az;
            patches.push(SkyPatch {
                altitude: alt_rad,
                azimuth: azi_rad,
                solid_angle,
            });
        }
    }

    patches
}

/// Compute daylight factor using Tregenza sky patches.
///
/// Integrates the sky component of daylight factor by summing contributions
/// from all visible sky patches through a window to a reference point.
///
/// # Arguments
/// * `patches` - Tregenza sky patches
/// * `window_transmittance` - Diffuse visible transmittance of window
/// * `window_solid_angle` - Solid angle of window as seen from reference point (sr)
/// * `window_altitude` - Window center altitude as seen from reference point (radians)
/// * `sky_type` - Sky luminance distribution
/// * `sun_altitude` - Sun altitude for sky model
/// * `sun_azimuth` - Sun azimuth for sky model
/// * `zone` - Zone reflectances for IRC
/// * `window_area` - Window area (m2)
///
/// # Returns
/// Daylight factor (ratio of interior to exterior illuminance)
pub fn daylight_factor_from_patches(
    patches: &[SkyPatch],
    window_transmittance: f64,
    window_solid_angle: f64,
    sky_type: crate::SkyType,
    sun_altitude: f64,
    sun_azimuth: f64,
    zone: &ZoneReflectances,
    window_area: f64,
) -> f64 {
    use crate::sky;

    if patches.is_empty() || window_solid_angle <= 0.0 {
        return 0.0;
    }

    // 1. Compute exterior horizontal illuminance from sky patches
    let mut ext_horiz = 0.0;
    for patch in patches {
        let lum = sky::sky_luminance(
            sky_type,
            patch.altitude,
            patch.azimuth,
            sun_altitude,
            sun_azimuth,
        );
        ext_horiz += lum * patch.altitude.sin() * patch.altitude.cos() * patch.solid_angle;
    }

    if ext_horiz <= 0.0 {
        return 0.0;
    }

    // 2. Sky component: fraction of sky visible through window × transmittance
    let sky_component = window_transmittance * window_solid_angle / (2.0 * std::f64::consts::PI);

    // 3. Internally reflected component
    let irc = internally_reflected_component(window_transmittance, window_area, zone);

    // Total daylight factor
    (sky_component + irc).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tregenza_has_145_patches() {
        let patches = tregenza_patches();
        assert_eq!(patches.len(), 145, "count={}", patches.len());
    }

    #[test]
    fn tregenza_solid_angles_sum() {
        let patches = tregenza_patches();
        let total: f64 = patches.iter().map(|p| p.solid_angle).sum();
        // Total should approximate hemisphere solid angle: 2π ≈ 6.28
        assert!(
            (total - 2.0 * std::f64::consts::PI).abs() < 1.0,
            "total={total}, expected≈{}",
            2.0 * std::f64::consts::PI
        );
    }

    #[test]
    fn tregenza_altitudes_reasonable() {
        let patches = tregenza_patches();
        for p in &patches {
            assert!(p.altitude > 0.0 && p.altitude <= std::f64::consts::PI / 2.0 + 0.01,
                "alt={}", p.altitude.to_degrees());
            assert!(p.solid_angle > 0.0, "omega={}", p.solid_angle);
        }
    }

    #[test]
    fn first_bounce_positive_for_flux() {
        let zone = ZoneReflectances::default();
        let illum = first_bounce_illuminance(10000.0, &zone, 2.5, 3.0, 0.8);
        assert!(illum > 0.0, "illum={illum}");
    }

    #[test]
    fn first_bounce_zero_for_no_flux() {
        let zone = ZoneReflectances::default();
        let illum = first_bounce_illuminance(0.0, &zone, 2.5, 3.0, 0.8);
        assert!(illum.abs() < 1e-10);
    }

    #[test]
    fn first_bounce_increases_with_reflectance() {
        let zone_low = ZoneReflectances {
            floor: 0.1, wall: 0.2, ceiling: 0.3,
            ..Default::default()
        };
        let zone_high = ZoneReflectances {
            floor: 0.5, wall: 0.7, ceiling: 0.8,
            ..Default::default()
        };
        let illum_low = first_bounce_illuminance(10000.0, &zone_low, 2.5, 3.0, 0.8);
        let illum_high = first_bounce_illuminance(10000.0, &zone_high, 2.5, 3.0, 0.8);
        assert!(
            illum_high > illum_low,
            "high={illum_high}, low={illum_low}"
        );
    }

    #[test]
    fn irc_positive_for_window() {
        let zone = ZoneReflectances::default();
        let irc = internally_reflected_component(0.7, 2.0, &zone);
        assert!(irc > 0.0, "irc={irc}");
    }

    #[test]
    fn irc_increases_with_window_area() {
        let zone = ZoneReflectances::default();
        let irc_small = internally_reflected_component(0.7, 1.0, &zone);
        let irc_large = internally_reflected_component(0.7, 4.0, &zone);
        assert!(
            irc_large > irc_small,
            "large={irc_large}, small={irc_small}"
        );
    }

    #[test]
    fn average_reflectance() {
        let zone = ZoneReflectances {
            floor: 0.2, wall: 0.5, ceiling: 0.7,
            floor_area: 10.0, wall_area: 40.0, ceiling_area: 10.0,
        };
        let rho = zone.average_reflectance();
        // (0.2*10 + 0.5*40 + 0.7*10) / 60 = (2+20+7)/60 = 29/60 ≈ 0.483
        assert!((rho - 0.483).abs() < 0.01, "rho={rho}");
    }

    #[test]
    fn daylight_factor_positive() {
        let patches = tregenza_patches();
        let zone = ZoneReflectances::default();
        let df = daylight_factor_from_patches(
            &patches,
            0.7,  // window transmittance
            0.04, // solid angle of window
            crate::SkyType::Overcast,
            std::f64::consts::PI / 4.0,
            0.0,
            &zone,
            2.0,
        );
        assert!(df > 0.0, "df={df}");
    }

    #[test]
    fn daylight_factor_larger_window_higher() {
        let patches = tregenza_patches();
        let zone = ZoneReflectances::default();
        let df_small = daylight_factor_from_patches(
            &patches, 0.7, 0.02, crate::SkyType::Overcast,
            std::f64::consts::PI / 4.0, 0.0, &zone, 1.0,
        );
        let df_large = daylight_factor_from_patches(
            &patches, 0.7, 0.08, crate::SkyType::Overcast,
            std::f64::consts::PI / 4.0, 0.0, &zone, 4.0,
        );
        assert!(
            df_large > df_small,
            "large={df_large}, small={df_small}"
        );
    }
}
