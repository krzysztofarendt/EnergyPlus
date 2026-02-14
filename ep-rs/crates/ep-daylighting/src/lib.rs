//! Daylighting calculations for EnergyPlus-rs.
//!
//! Implements the split-flux daylighting method with:
//! - CIE sky luminance distributions (clear, turbid, intermediate, overcast)
//! - Daylight factor pre-calculation for reference points
//! - Interior illuminance from daylight factors and exterior conditions
//! - Glare calculation (Cornell/BRS large-source formula)
//! - Lighting controls (continuous dimming, stepped, continuous-off)

pub mod controls;
pub mod glare;
pub mod sky;

/// A reference point for daylighting calculations.
#[derive(Debug, Clone)]
pub struct DaylightRefPoint {
    /// Position in zone coordinates (x, y, z) in meters.
    pub position: [f64; 3],
    /// Fraction of zone floor area controlled by this reference point (0-1).
    pub fraction_controlled: f64,
    /// Target illuminance setpoint (lux).
    pub illuminance_setpoint: f64,
}

impl DaylightRefPoint {
    pub fn new(x: f64, y: f64, z: f64, fraction: f64, setpoint: f64) -> Self {
        Self {
            position: [x, y, z],
            fraction_controlled: fraction,
            illuminance_setpoint: setpoint,
        }
    }
}

/// Pre-calculated daylight factors for one reference point and one window.
///
/// Stored per hour, per sky type (4), for bare and shaded window.
#[derive(Debug, Clone, Default)]
pub struct DaylightFactors {
    /// Illuminance factors for four sky types (clear, turbid, intermediate, overcast).
    /// Factor * exterior_horizontal_illuminance = interior_illuminance contribution.
    pub sky_factors: [f64; 4],
    /// Direct sun factor.
    pub sun_factor: f64,
    /// Sun disk factor (for glare).
    pub sun_disk_factor: f64,
}

/// CIE sky type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyType {
    Clear = 0,
    ClearTurbid = 1,
    Intermediate = 2,
    Overcast = 3,
}

/// Exterior horizontal illuminance components.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExteriorIlluminance {
    /// Diffuse horizontal illuminance from sky (lux).
    pub diffuse_horizontal: [f64; 4], // per sky type
    /// Direct normal illuminance from sun (lux).
    pub direct_normal: f64,
    /// Sun altitude angle (radians).
    pub sun_altitude: f64,
}

/// Calculate interior illuminance at a reference point from daylight factors.
///
/// Illuminance = sum over windows of (factor * exterior_illuminance)
pub fn interior_illuminance(
    factors: &[DaylightFactors],
    exterior: &ExteriorIlluminance,
    sky_type: SkyType,
    sky_weight: f64,
) -> f64 {
    let sky_idx = sky_type as usize;
    let mut total = 0.0;

    for df in factors {
        // Sky diffuse contribution
        total += df.sky_factors[sky_idx] * exterior.diffuse_horizontal[sky_idx];
        // Direct sun contribution
        total += df.sun_factor * exterior.direct_normal * sky_weight;
    }

    total.max(0.0)
}

/// Luminous efficacy: convert solar irradiance to illuminance.
///
/// Approximate luminous efficacy for different sky conditions.
/// Reference: Perez et al. (1990).
pub fn luminous_efficacy_sky(solar_altitude_rad: f64, sky_clearness: f64) -> f64 {
    let alt_deg = solar_altitude_rad.to_degrees().max(0.1);

    if sky_clearness < 1.065 {
        // Overcast: 130-150 lm/W
        136.6 - 20.0 * (alt_deg / 90.0)
    } else if sky_clearness < 2.8 {
        // Intermediate: 100-130 lm/W
        110.0 + 15.0 * (alt_deg / 90.0)
    } else {
        // Clear: 85-115 lm/W
        93.0 + 20.0 * (alt_deg / 90.0)
    }
}

/// Luminous efficacy for direct normal solar radiation.
///
/// Approximate based on solar altitude.
pub fn luminous_efficacy_direct(solar_altitude_rad: f64) -> f64 {
    let alt_deg = solar_altitude_rad.to_degrees().max(0.1);
    // Direct normal efficacy increases with altitude
    if alt_deg < 10.0 {
        60.0 + alt_deg * 2.5
    } else {
        85.0 + (alt_deg - 10.0) * 0.4
    }
}

/// Calculate exterior horizontal illuminance from solar radiation data.
pub fn exterior_illuminance_from_radiation(
    direct_normal_radiation: f64,
    diffuse_horizontal_radiation: f64,
    solar_altitude_rad: f64,
) -> (f64, f64) {
    if solar_altitude_rad <= 0.0 {
        return (0.0, 0.0);
    }

    let eff_direct = luminous_efficacy_direct(solar_altitude_rad);
    let direct_normal_illum = direct_normal_radiation * eff_direct;

    let clearness = if diffuse_horizontal_radiation > 0.0 {
        let sin_alt = solar_altitude_rad.sin();
        (direct_normal_radiation * sin_alt + diffuse_horizontal_radiation)
            / diffuse_horizontal_radiation
    } else {
        6.0 // assume clear
    };
    let eff_sky = luminous_efficacy_sky(solar_altitude_rad, clearness);
    let diffuse_horiz_illum = diffuse_horizontal_radiation * eff_sky;

    (direct_normal_illum, diffuse_horiz_illum)
}

/// Solid angle subtended by a window element at a reference point.
///
/// Approximate: omega ≈ A * cos(theta) / r^2
pub fn solid_angle(
    window_area: f64,
    distance: f64,
    cos_angle_of_incidence: f64,
) -> f64 {
    if distance < 0.01 {
        return 0.0;
    }
    (window_area * cos_angle_of_incidence.max(0.0) / (distance * distance)).max(0.0)
}

/// Window visible transmittance at a given incidence angle.
///
/// Simple angular model: T(theta) = T_normal * factor(cos_theta)
pub fn window_visible_transmittance(t_normal: f64, cos_incidence: f64) -> f64 {
    let cos_i = cos_incidence.clamp(0.0, 1.0);
    if cos_i < 0.001 {
        return 0.0;
    }
    // Polynomial angular factor (same as in ep-windows/optics)
    let x = cos_i;
    let factor = x * (0.0918 + x * (2.7709 + x * (-4.4284 + x * 2.5667)));
    t_normal * factor.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn daylight_ref_point() {
        let rp = DaylightRefPoint::new(5.0, 3.0, 0.8, 0.5, 500.0);
        assert!((rp.position[0] - 5.0).abs() < 1e-10);
        assert!((rp.illuminance_setpoint - 500.0).abs() < 1e-10);
    }

    #[test]
    fn interior_illuminance_calculation() {
        let factors = vec![
            DaylightFactors {
                sky_factors: [0.01, 0.008, 0.006, 0.004],
                sun_factor: 0.005,
                sun_disk_factor: 0.001,
            },
        ];
        let exterior = ExteriorIlluminance {
            diffuse_horizontal: [20000.0, 18000.0, 15000.0, 10000.0],
            direct_normal: 80000.0,
            sun_altitude: 0.8,
        };

        let illum = interior_illuminance(&factors, &exterior, SkyType::Clear, 1.0);
        // Sky: 0.01 * 20000 = 200 lux, Sun: 0.005 * 80000 = 400 lux
        assert!((illum - 600.0).abs() < 1.0, "illum={illum}");
    }

    #[test]
    fn luminous_efficacy_values() {
        // At 45 deg altitude, clear sky
        let eff = luminous_efficacy_sky(PI / 4.0, 5.0);
        assert!(eff > 90.0 && eff < 120.0, "eff={eff}");

        // Overcast sky
        let eff_oc = luminous_efficacy_sky(PI / 4.0, 1.0);
        assert!(eff_oc > 120.0, "eff_oc={eff_oc}");
    }

    #[test]
    fn exterior_illuminance_from_solar() {
        let (direct, diffuse) = exterior_illuminance_from_radiation(
            800.0,  // W/m2 direct normal
            200.0,  // W/m2 diffuse horizontal
            PI / 4.0, // 45 deg sun altitude
        );
        // Direct: 800 * ~100 = ~80000 lux
        assert!(direct > 50000.0 && direct < 120000.0, "direct={direct}");
        // Diffuse: 200 * ~100 = ~20000 lux
        assert!(diffuse > 10000.0 && diffuse < 40000.0, "diffuse={diffuse}");
    }

    #[test]
    fn solid_angle_basic() {
        // 1 m2 window at 5m distance, normal incidence
        let omega = solid_angle(1.0, 5.0, 1.0);
        assert!((omega - 0.04).abs() < 0.001, "omega={omega}");
    }

    #[test]
    fn window_transmittance_normal() {
        let t = window_visible_transmittance(0.8, 1.0);
        assert!((t - 0.8).abs() < 0.02, "t={t}");
    }

    #[test]
    fn window_transmittance_grazing() {
        let t = window_visible_transmittance(0.8, 0.0);
        assert!((t).abs() < 0.01);
    }

    #[test]
    fn luminous_efficacy_direct_low_altitude() {
        // Solar altitude ~5 degrees
        let alt = 5.0_f64.to_radians();
        let eff = luminous_efficacy_direct(alt);
        // At 5 deg: 60 + 5*2.5 = 72.5 lm/W
        assert!(eff > 60.0 && eff < 85.0, "eff={eff}");
        assert!((eff - 72.5).abs() < 0.5, "eff={eff}");
    }

    #[test]
    fn luminous_efficacy_direct_high_altitude() {
        // Solar altitude ~70 degrees
        let alt = 70.0_f64.to_radians();
        let eff = luminous_efficacy_direct(alt);
        // At 70 deg: 85 + (70-10)*0.4 = 85 + 24 = 109 lm/W
        assert!(eff > 100.0 && eff < 120.0, "eff={eff}");
        assert!((eff - 109.0).abs() < 1.0, "eff={eff}");
    }

    #[test]
    fn exterior_illuminance_night() {
        // When sun altitude <= 0, both direct and diffuse should be zero
        let (direct, diffuse) = exterior_illuminance_from_radiation(800.0, 200.0, -0.1);
        assert!((direct).abs() < 1e-10, "direct={direct}");
        assert!((diffuse).abs() < 1e-10, "diffuse={diffuse}");

        let (direct2, diffuse2) = exterior_illuminance_from_radiation(800.0, 200.0, 0.0);
        assert!((direct2).abs() < 1e-10, "direct2={direct2}");
        assert!((diffuse2).abs() < 1e-10, "diffuse2={diffuse2}");
    }

    #[test]
    fn exterior_illuminance_clear_vs_overcast() {
        // Different sky clearness values produce different efficacies
        let alt = PI / 4.0;
        // Clear sky efficacy (clearness >= 2.8)
        let eff_clear = luminous_efficacy_sky(alt, 5.0);
        // Overcast sky efficacy (clearness < 1.065)
        let eff_overcast = luminous_efficacy_sky(alt, 1.0);
        // They should differ meaningfully
        assert!((eff_clear - eff_overcast).abs() > 5.0,
            "clear={eff_clear}, overcast={eff_overcast}");
        // Overcast efficacy is typically higher than clear
        assert!(eff_overcast > eff_clear,
            "overcast={eff_overcast} should be > clear={eff_clear}");
    }

    #[test]
    fn interior_illuminance_no_sun() {
        // When direct_normal = 0, only sky contributes
        let factors = vec![
            DaylightFactors {
                sky_factors: [0.01, 0.008, 0.006, 0.004],
                sun_factor: 0.005,
                sun_disk_factor: 0.001,
            },
        ];
        let exterior = ExteriorIlluminance {
            diffuse_horizontal: [20000.0, 18000.0, 15000.0, 10000.0],
            direct_normal: 0.0,
            sun_altitude: 0.8,
        };
        let illum = interior_illuminance(&factors, &exterior, SkyType::Clear, 1.0);
        // Sky only: 0.01 * 20000 = 200 lux, sun: 0.005 * 0 = 0
        assert!((illum - 200.0).abs() < 1.0, "illum={illum}");
    }

    #[test]
    fn solid_angle_zero_distance() {
        // When distance < 0.01, solid angle should be 0
        let omega = solid_angle(1.0, 0.005, 1.0);
        assert!((omega).abs() < 1e-10, "omega={omega}");
        let omega2 = solid_angle(1.0, 0.0, 1.0);
        assert!((omega2).abs() < 1e-10, "omega2={omega2}");
    }

    #[test]
    fn solid_angle_oblique() {
        // cos_angle = 0.5 should give half of the normal incidence case
        let omega_normal = solid_angle(1.0, 5.0, 1.0);
        let omega_oblique = solid_angle(1.0, 5.0, 0.5);
        assert!((omega_oblique - omega_normal * 0.5).abs() < 1e-10,
            "normal={omega_normal}, oblique={omega_oblique}");
    }

    #[test]
    fn window_transmittance_mid_angle() {
        // cos_incidence = 0.7 should give a value between 0 and t_normal
        let t_normal = window_visible_transmittance(0.8, 1.0);
        let t_mid = window_visible_transmittance(0.8, 0.7);
        let t_grazing = window_visible_transmittance(0.8, 0.0);
        assert!(t_mid > t_grazing, "t_mid={t_mid} should be > t_grazing={t_grazing}");
        assert!(t_mid < t_normal, "t_mid={t_mid} should be < t_normal={t_normal}");
        assert!(t_mid > 0.0, "t_mid={t_mid} should be positive");
    }
}
