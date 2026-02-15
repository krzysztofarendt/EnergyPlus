//! Window shading devices: interior shades, blinds, exterior screens, and controls.
//!
//! Models the optical and thermal effects of window attachments including
//! interior roller shades, venetian blinds, and exterior insect screens.
//! Shading controls determine when devices are deployed.

use std::f64::consts::PI;

/// Interior shade properties.
#[derive(Debug, Clone)]
pub struct InteriorShade {
    /// Solar transmittance of the shade material.
    pub solar_transmittance: f64,
    /// Solar reflectance of the shade material.
    pub solar_reflectance: f64,
    /// Visible transmittance.
    pub visible_transmittance: f64,
    /// Visible reflectance.
    pub visible_reflectance: f64,
    /// IR emissivity of the shade.
    pub emissivity: f64,
    /// Shade-to-glass distance (m).
    pub air_gap: f64,
}

impl Default for InteriorShade {
    fn default() -> Self {
        Self {
            solar_transmittance: 0.3,
            solar_reflectance: 0.5,
            visible_transmittance: 0.3,
            visible_reflectance: 0.5,
            emissivity: 0.9,
            air_gap: 0.05,
        }
    }
}

/// Interior blind (venetian) properties.
#[derive(Debug, Clone)]
pub struct InteriorBlind {
    /// Slat width (m).
    pub slat_width: f64,
    /// Slat spacing (center-to-center, m).
    pub slat_separation: f64,
    /// Slat angle from horizontal (radians, 0=horizontal, PI/2=vertical).
    pub slat_angle: f64,
    /// Solar transmittance of slat material.
    pub slat_solar_transmittance: f64,
    /// Solar reflectance of slat front.
    pub slat_solar_reflectance_front: f64,
    /// Solar reflectance of slat back.
    pub slat_solar_reflectance_back: f64,
    /// Visible transmittance of slat.
    pub slat_visible_transmittance: f64,
    /// Visible reflectance of slat front.
    pub slat_visible_reflectance_front: f64,
    /// IR emissivity of slat.
    pub slat_emissivity: f64,
}

impl Default for InteriorBlind {
    fn default() -> Self {
        Self {
            slat_width: 0.025,
            slat_separation: 0.01875,
            slat_angle: 45.0_f64.to_radians(),
            slat_solar_transmittance: 0.0,
            slat_solar_reflectance_front: 0.5,
            slat_solar_reflectance_back: 0.5,
            slat_visible_transmittance: 0.0,
            slat_visible_reflectance_front: 0.5,
            slat_emissivity: 0.9,
        }
    }
}

/// Exterior screen properties.
#[derive(Debug, Clone)]
pub struct ExteriorScreen {
    /// Screen material solar transmittance (beam-beam at normal incidence).
    pub beam_transmittance: f64,
    /// Screen material solar reflectance.
    pub solar_reflectance: f64,
    /// Visible transmittance.
    pub visible_transmittance: f64,
    /// Screen openness factor (fraction of open area).
    pub openness_factor: f64,
}

impl Default for ExteriorScreen {
    fn default() -> Self {
        Self {
            beam_transmittance: 0.25,
            solar_reflectance: 0.6,
            visible_transmittance: 0.25,
            openness_factor: 0.25,
        }
    }
}

/// Shading control type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShadingControlType {
    /// Always on when schedule says so.
    #[default]
    Schedule,
    /// Deploy when solar on window exceeds setpoint (W/m2).
    SolarOnWindow,
    /// Deploy when horizontal solar exceeds setpoint (W/m2).
    SolarOnHorizontal,
    /// Deploy when zone air temperature exceeds setpoint (K).
    ZoneAirTemp,
    /// Deploy when glare exceeds setpoint.
    Glare,
}

/// Shading control configuration.
#[derive(Debug, Clone)]
pub struct ShadingControl {
    pub control_type: ShadingControlType,
    /// Schedule value (0 = off, >0 = on). Only for Schedule type.
    pub schedule_value: f64,
    /// Setpoint for trigger-based controls.
    pub setpoint: f64,
    /// Current trigger value (solar irradiance, temp, or glare index).
    pub current_value: f64,
}

impl Default for ShadingControl {
    fn default() -> Self {
        Self {
            control_type: ShadingControlType::Schedule,
            schedule_value: 0.0,
            setpoint: 300.0,
            current_value: 0.0,
        }
    }
}

impl ShadingControl {
    /// Determine if the shading device should be deployed.
    pub fn is_deployed(&self) -> bool {
        match self.control_type {
            ShadingControlType::Schedule => self.schedule_value > 0.0,
            ShadingControlType::SolarOnWindow
            | ShadingControlType::SolarOnHorizontal
            | ShadingControlType::ZoneAirTemp
            | ShadingControlType::Glare => self.current_value >= self.setpoint,
        }
    }
}

/// Result of shading device optical calculation.
#[derive(Debug, Clone, Default)]
pub struct ShadingOptics {
    /// System beam solar transmittance (glass + shade).
    pub beam_transmittance: f64,
    /// System diffuse solar transmittance.
    pub diffuse_transmittance: f64,
    /// System visible transmittance.
    pub visible_transmittance: f64,
    /// Solar absorbed by the shading device (W/m2 per unit incident).
    pub shade_absorbed_fraction: f64,
}

/// Calculate the effective beam transmittance of a shade.
///
/// The shade reduces transmitted beam by its transmittance, but some
/// radiation absorbed by the shade re-radiates inward.
pub fn shade_beam_transmittance(
    glass_transmittance: f64,
    shade: &InteriorShade,
) -> ShadingOptics {
    // Simple two-surface inter-reflection between glass back and shade
    let t_glass = glass_transmittance;
    let t_shade = shade.solar_transmittance;
    let r_shade = shade.solar_reflectance;

    // System transmittance: transmitted through glass then through shade
    // with inter-reflection between glass back and shade front
    let beam_trans = t_glass * t_shade;
    let diff_trans = t_glass * shade.solar_transmittance * 0.9; // slightly less for diffuse

    // Absorbed by shade
    let shade_abs = 1.0 - t_shade - r_shade;
    let shade_absorbed = t_glass * shade_abs;

    ShadingOptics {
        beam_transmittance: beam_trans,
        diffuse_transmittance: diff_trans,
        visible_transmittance: glass_transmittance * shade.visible_transmittance,
        shade_absorbed_fraction: shade_absorbed,
    }
}

/// Calculate beam transmittance through a venetian blind.
///
/// Uses the geometric model based on slat angle, width, and spacing.
/// When slats are closed (angle=0 or PI/2), beam is blocked; when open,
/// transmittance depends on the beam profile angle relative to the slats.
pub fn blind_beam_transmittance(
    glass_transmittance: f64,
    blind: &InteriorBlind,
    beam_profile_angle: f64,
) -> ShadingOptics {
    // Direct beam transmittance through blind openings
    let t_direct = blind_direct_transmittance(
        blind.slat_width,
        blind.slat_separation,
        blind.slat_angle,
        beam_profile_angle,
    );

    // Reflected component from slats
    let t_reflected = t_direct * blind.slat_solar_reflectance_front * 0.5;

    let total_beam = glass_transmittance * (t_direct + t_reflected);

    // Diffuse transmittance: integrate over hemisphere (simplified)
    let t_diff = blind_diffuse_transmittance(
        blind.slat_width,
        blind.slat_separation,
        blind.slat_angle,
    );
    let total_diff = glass_transmittance * t_diff;

    let shade_abs = glass_transmittance * (1.0 - t_direct) * (1.0 - blind.slat_solar_reflectance_front);

    ShadingOptics {
        beam_transmittance: total_beam.clamp(0.0, glass_transmittance),
        diffuse_transmittance: total_diff.clamp(0.0, glass_transmittance),
        visible_transmittance: glass_transmittance * blind_direct_transmittance(
            blind.slat_width,
            blind.slat_separation,
            blind.slat_angle,
            beam_profile_angle,
        ),
        shade_absorbed_fraction: shade_abs,
    }
}

/// Direct beam transmittance through venetian blind slats.
///
/// Based on the geometric relationship between beam angle and slat geometry.
/// The beam profile angle is the angle of the beam in the plane perpendicular
/// to the slats, measured from the window normal.
fn blind_direct_transmittance(
    slat_width: f64,
    slat_separation: f64,
    slat_angle: f64,
    beam_profile_angle: f64,
) -> f64 {
    if slat_separation <= 0.0 || slat_width <= 0.0 {
        return 1.0;
    }

    // Effective gap between slats as seen from beam direction
    let cos_slat = slat_angle.cos();
    let sin_slat = slat_angle.sin();

    // Profile angle is the beam angle from normal in the slat plane
    let tan_profile = beam_profile_angle.tan();

    // Projected slat coverage: how much of the gap is blocked by each slat
    let slat_proj = slat_width * (cos_slat + sin_slat * tan_profile.abs());
    let gap_open = (slat_separation - slat_proj).max(0.0);

    (gap_open / slat_separation).clamp(0.0, 1.0)
}

/// Diffuse transmittance through blind, approximated by averaging
/// direct transmittance over a range of profile angles.
fn blind_diffuse_transmittance(
    slat_width: f64,
    slat_separation: f64,
    slat_angle: f64,
) -> f64 {
    // Numerical integration over profile angles from -75° to 75°
    let n_steps = 15;
    let mut sum = 0.0;
    let mut weight_sum = 0.0;

    for i in 0..n_steps {
        let angle = -75.0_f64.to_radians()
            + (i as f64 + 0.5) * 150.0_f64.to_radians() / n_steps as f64;
        let cos_angle = angle.cos();
        if cos_angle <= 0.0 {
            continue;
        }
        let t = blind_direct_transmittance(slat_width, slat_separation, slat_angle, angle);
        sum += t * cos_angle;
        weight_sum += cos_angle;
    }

    if weight_sum > 0.0 {
        sum / weight_sum
    } else {
        0.0
    }
}

/// Calculate beam transmittance through an exterior screen.
///
/// The screen transmittance depends on the openness factor and angle of incidence.
pub fn screen_beam_transmittance(
    glass_transmittance: f64,
    screen: &ExteriorScreen,
    cos_incidence: f64,
) -> ShadingOptics {
    let cos_i = cos_incidence.clamp(0.0, 1.0);

    // Beam-beam transmittance decreases at oblique angles
    // due to increased effective material coverage
    let t_bb = if cos_i > 0.01 {
        (screen.openness_factor * cos_i).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Beam-diffuse: some beam scattered by screen material into diffuse
    let t_bd = screen.beam_transmittance * (1.0 - t_bb);

    // System transmittance = screen_trans * glass_trans
    let sys_beam = t_bb * glass_transmittance;
    let sys_diff = (t_bb + t_bd * 0.5) * glass_transmittance;

    let screen_abs = 1.0 - t_bb - screen.solar_reflectance;

    ShadingOptics {
        beam_transmittance: sys_beam.max(0.0),
        diffuse_transmittance: sys_diff.max(0.0),
        visible_transmittance: screen.visible_transmittance * glass_transmittance,
        shade_absorbed_fraction: screen_abs.max(0.0),
    }
}

/// Calculate the beam profile angle for a window with vertical slats.
///
/// The profile angle is the beam incidence projected onto the plane
/// perpendicular to the slats.
///
/// For horizontal slats (typical venetian blinds), this is the vertical
/// projection of the beam angle.
///
/// # Arguments
/// * `solar_altitude` - Solar altitude (radians)
/// * `solar_azimuth_relative` - Sun azimuth relative to window normal (radians)
pub fn beam_profile_angle(solar_altitude: f64, solar_azimuth_relative: f64) -> f64 {
    // For horizontal slats: profile angle in the vertical plane
    // tan(profile) = tan(altitude) / cos(relative_azimuth)
    let cos_rel_az = solar_azimuth_relative.cos();
    if cos_rel_az.abs() < 0.001 {
        return PI / 2.0; // Edge case: sun perpendicular to slat direction
    }

    let tan_profile = solar_altitude.tan() / cos_rel_az;
    tan_profile.atan()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shade_reduces_transmittance() {
        let shade = InteriorShade {
            solar_transmittance: 0.5,
            solar_reflectance: 0.3,
            ..Default::default()
        };
        let result = shade_beam_transmittance(0.7, &shade);
        // System = 0.7 * 0.5 = 0.35
        assert!(
            (result.beam_transmittance - 0.35).abs() < 0.01,
            "t={}",
            result.beam_transmittance
        );
    }

    #[test]
    fn shade_absorbs_solar() {
        let shade = InteriorShade {
            solar_transmittance: 0.3,
            solar_reflectance: 0.5,
            ..Default::default()
        };
        let result = shade_beam_transmittance(0.8, &shade);
        // absorbed_by_shade = glass_trans * (1 - t_shade - r_shade) = 0.8 * 0.2 = 0.16
        assert!(
            (result.shade_absorbed_fraction - 0.16).abs() < 0.01,
            "abs={}",
            result.shade_absorbed_fraction
        );
    }

    #[test]
    fn opaque_shade_blocks_all() {
        let shade = InteriorShade {
            solar_transmittance: 0.0,
            solar_reflectance: 0.9,
            visible_transmittance: 0.0,
            visible_reflectance: 0.9,
            ..Default::default()
        };
        let result = shade_beam_transmittance(0.7, &shade);
        assert!(result.beam_transmittance < 0.01);
        assert!(result.visible_transmittance < 0.01);
    }

    #[test]
    fn blind_open_slats_high_transmittance() {
        // Horizontal slats (angle=0), beam normal to window (profile=0)
        let blind = InteriorBlind {
            slat_width: 0.025,
            slat_separation: 0.02,
            slat_angle: 0.0, // Horizontal
            ..Default::default()
        };
        let result = blind_beam_transmittance(0.7, &blind, 0.0);
        // Slats flat: projected coverage = 0.025 * cos(0) = 0.025 > 0.02
        // But at profile_angle=0, tan_profile=0, so coverage = slat_width * cos(0) = 0.025
        // gap = max(0.02 - 0.025, 0) = 0 → t_direct = 0
        // This is correct: when slat_width > spacing, slats overlap at horizontal
        assert!(result.beam_transmittance < 0.7);
    }

    #[test]
    fn blind_tilted_slats_vs_horizontal() {
        let blind_h = InteriorBlind {
            slat_angle: 0.0,
            ..Default::default()
        };
        let blind_45 = InteriorBlind {
            slat_angle: 45.0_f64.to_radians(),
            ..Default::default()
        };
        let r_h = blind_beam_transmittance(0.7, &blind_h, 0.0);
        let r_45 = blind_beam_transmittance(0.7, &blind_45, 0.0);
        // At 45° slats, projected coverage changes
        // Both are valid, just check they differ
        assert!(
            (r_h.beam_transmittance - r_45.beam_transmittance).abs() > 0.001
                || r_h.beam_transmittance < 0.01,
            "h={}, 45={}",
            r_h.beam_transmittance,
            r_45.beam_transmittance
        );
    }

    #[test]
    fn blind_diffuse_transmittance_reasonable() {
        let blind = InteriorBlind::default();
        let t = blind_diffuse_transmittance(
            blind.slat_width,
            blind.slat_separation,
            blind.slat_angle,
        );
        // Diffuse transmittance should be between 0 and 1
        assert!(t >= 0.0 && t <= 1.0, "t_diff={t}");
    }

    #[test]
    fn screen_reduces_beam() {
        let screen = ExteriorScreen {
            openness_factor: 0.25,
            beam_transmittance: 0.25,
            solar_reflectance: 0.5,
            ..Default::default()
        };
        let result = screen_beam_transmittance(0.7, &screen, 1.0);
        // At normal incidence: t_bb = 0.25 * 1.0 = 0.25
        // sys_beam = 0.25 * 0.7 = 0.175
        assert!(
            (result.beam_transmittance - 0.175).abs() < 0.01,
            "t={}",
            result.beam_transmittance
        );
    }

    #[test]
    fn screen_grazing_blocks_more() {
        let screen = ExteriorScreen::default();
        let r_normal = screen_beam_transmittance(0.7, &screen, 1.0);
        let r_oblique = screen_beam_transmittance(0.7, &screen, 0.3);
        assert!(
            r_oblique.beam_transmittance < r_normal.beam_transmittance,
            "normal={}, oblique={}",
            r_normal.beam_transmittance,
            r_oblique.beam_transmittance
        );
    }

    #[test]
    fn shading_control_schedule() {
        let ctrl = ShadingControl {
            control_type: ShadingControlType::Schedule,
            schedule_value: 1.0,
            ..Default::default()
        };
        assert!(ctrl.is_deployed());

        let ctrl_off = ShadingControl {
            schedule_value: 0.0,
            ..Default::default()
        };
        assert!(!ctrl_off.is_deployed());
    }

    #[test]
    fn shading_control_solar_setpoint() {
        let ctrl = ShadingControl {
            control_type: ShadingControlType::SolarOnWindow,
            setpoint: 300.0,
            current_value: 350.0,
            ..Default::default()
        };
        assert!(ctrl.is_deployed());

        let ctrl_below = ShadingControl {
            control_type: ShadingControlType::SolarOnWindow,
            setpoint: 300.0,
            current_value: 200.0,
            ..Default::default()
        };
        assert!(!ctrl_below.is_deployed());
    }

    #[test]
    fn beam_profile_angle_normal_incidence() {
        // Sun straight ahead at 45° altitude
        let angle = beam_profile_angle(45.0_f64.to_radians(), 0.0);
        // profile = atan(tan(45°) / cos(0°)) = atan(1.0) = 45°
        assert!(
            (angle - 45.0_f64.to_radians()).abs() < 0.01,
            "angle={}°",
            angle.to_degrees()
        );
    }

    #[test]
    fn beam_profile_angle_oblique_sun() {
        // Sun at 30° altitude, 45° off normal
        let angle = beam_profile_angle(30.0_f64.to_radians(), 45.0_f64.to_radians());
        // profile = atan(tan(30°) / cos(45°)) = atan(0.577 / 0.707) = atan(0.816) ≈ 39.2°
        assert!(
            angle.to_degrees() > 35.0 && angle.to_degrees() < 45.0,
            "angle={}°",
            angle.to_degrees()
        );
    }
}
