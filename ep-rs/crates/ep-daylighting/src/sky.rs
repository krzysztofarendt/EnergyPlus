//! CIE sky luminance distributions.
//!
//! Four standard sky types used for daylighting factor calculations:
//! - Clear: direct sunlight dominant, graduated luminance distribution
//! - Clear turbid: clear with haze
//! - Intermediate: partly cloudy
//! - Overcast: uniform cloud cover, luminance varies only with altitude

use std::f64::consts::PI;

use crate::SkyType;

/// CIE sky luminance at a given sky point.
///
/// Returns normalized luminance (relative to zenith luminance = 1.0 cd/m2).
///
/// # Arguments
/// * `sky_type` - Sky luminance distribution model
/// * `altitude` - Altitude angle of sky element (radians, 0=horizon, pi/2=zenith)
/// * `azimuth` - Azimuth angle of sky element (radians)
/// * `sun_altitude` - Solar altitude angle (radians)
/// * `sun_azimuth` - Solar azimuth angle (radians)
pub fn sky_luminance(
    sky_type: SkyType,
    altitude: f64,
    azimuth: f64,
    sun_altitude: f64,
    sun_azimuth: f64,
) -> f64 {
    if altitude <= 0.0 {
        return 0.0;
    }

    // Angle between sky element and sun
    let gamma = angular_distance(altitude, azimuth, sun_altitude, sun_azimuth);
    let cos_gamma = gamma.cos();

    match sky_type {
        SkyType::Clear => clear_sky_luminance(altitude, cos_gamma, gamma, sun_altitude),
        SkyType::ClearTurbid => clear_turbid_luminance(altitude, cos_gamma, gamma, sun_altitude),
        SkyType::Intermediate => intermediate_luminance(altitude, cos_gamma, gamma, sun_altitude),
        SkyType::Overcast => overcast_luminance(altitude),
    }
}

/// Angular distance between two sky points.
fn angular_distance(alt1: f64, azi1: f64, alt2: f64, azi2: f64) -> f64 {
    let cos_d = alt1.sin() * alt2.sin() + alt1.cos() * alt2.cos() * (azi1 - azi2).cos();
    cos_d.clamp(-1.0, 1.0).acos()
}

/// CIE Clear sky luminance (Type 12).
///
/// L = (0.91 + 10*exp(-3*gamma) + 0.45*cos^2(gamma)) * (1 - exp(-0.32/sin(phi)))
fn clear_sky_luminance(altitude: f64, _cos_gamma: f64, gamma: f64, sun_altitude: f64) -> f64 {
    let sin_phi = altitude.sin().max(0.01);
    let cos_g = gamma.cos();

    // Scattering indicatrix
    let f_gamma = 0.91 + 10.0 * (-3.0 * gamma).exp() + 0.45 * cos_g * cos_g;

    // Gradation function
    let f_phi = 1.0 - (-0.32 / sin_phi).exp();

    // Zenith normalization
    let gamma_z = (PI / 2.0 - sun_altitude).max(0.0);
    let cos_gz = gamma_z.cos();
    let f_z = 0.91 + 10.0 * (-3.0 * gamma_z).exp() + 0.45 * cos_gz * cos_gz;
    let phi_z = 1.0 - (-0.32_f64).exp();

    let zenith = f_z * phi_z;
    if zenith > 1e-10 {
        (f_gamma * f_phi / zenith).max(0.0)
    } else {
        1.0
    }
}

/// CIE Clear Turbid sky luminance (Type 11).
fn clear_turbid_luminance(altitude: f64, _cos_gamma: f64, gamma: f64, sun_altitude: f64) -> f64 {
    let sin_phi = altitude.sin().max(0.01);
    let cos_g = gamma.cos();

    let f_gamma = 0.856 + 16.0 * (-3.0 * gamma).exp() + 0.3 * cos_g * cos_g;
    let f_phi = 1.0 - (-0.32 / sin_phi).exp();

    let gamma_z = (PI / 2.0 - sun_altitude).max(0.0);
    let cos_gz = gamma_z.cos();
    let f_z = 0.856 + 16.0 * (-3.0 * gamma_z).exp() + 0.3 * cos_gz * cos_gz;
    let phi_z = 1.0 - (-0.32_f64).exp();

    let zenith = f_z * phi_z;
    if zenith > 1e-10 {
        (f_gamma * f_phi / zenith).max(0.0)
    } else {
        1.0
    }
}

/// CIE Intermediate sky luminance (Type 8).
fn intermediate_luminance(altitude: f64, _cos_gamma: f64, gamma: f64, sun_altitude: f64) -> f64 {
    let sin_phi = altitude.sin().max(0.01);
    let cos_g = gamma.cos();

    // Simplified intermediate model
    let f_gamma = 1.35 * (1.0 + 5.0 * (-3.0 * gamma).exp() + 0.1 * cos_g * cos_g);
    let f_phi = (0.45 + 0.55 * sin_phi).max(0.0);

    let gamma_z = (PI / 2.0 - sun_altitude).max(0.0);
    let cos_gz = gamma_z.cos();
    let f_z = 1.35 * (1.0 + 5.0 * (-3.0 * gamma_z).exp() + 0.1 * cos_gz * cos_gz);
    let phi_z = (0.45_f64 + 0.55).max(0.0);

    let zenith = f_z * phi_z;
    if zenith > 1e-10 {
        (f_gamma * f_phi / zenith).max(0.0)
    } else {
        1.0
    }
}

/// CIE Overcast sky luminance (Type 1).
///
/// L = (1 + 2*sin(phi)) / 3
///
/// Only depends on altitude, uniform in azimuth.
fn overcast_luminance(altitude: f64) -> f64 {
    (1.0 + 2.0 * altitude.sin()) / 3.0
}

/// Integrate sky luminance over hemisphere to get horizontal illuminance.
///
/// E_h = integral(L(phi,theta) * sin(phi) * cos(phi) * dphi * dtheta)
///
/// Uses numerical integration with nth azimuth and nph altitude steps.
pub fn horizontal_sky_illuminance(
    sky_type: SkyType,
    sun_altitude: f64,
    sun_azimuth: f64,
    nth: usize,
    nph: usize,
) -> f64 {
    if sun_altitude <= 0.0 && sky_type != SkyType::Overcast {
        return 0.0;
    }

    let d_theta = 2.0 * PI / nth as f64;
    let d_phi = (PI / 2.0) / nph as f64;
    let mut illum = 0.0;

    for ith in 0..nth {
        let theta = (ith as f64 + 0.5) * d_theta;
        for iph in 0..nph {
            let phi = (iph as f64 + 0.5) * d_phi;
            let lum = sky_luminance(sky_type, phi, theta, sun_altitude, sun_azimuth);
            // Contribution: L * sin(phi) * cos(phi) * d_omega
            illum += lum * phi.sin() * phi.cos() * d_phi * d_theta;
        }
    }

    illum.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overcast_sky_uniform_azimuth() {
        // Overcast sky should not depend on azimuth
        let l1 = sky_luminance(SkyType::Overcast, PI / 4.0, 0.0, PI / 4.0, 0.0);
        let l2 = sky_luminance(SkyType::Overcast, PI / 4.0, PI, PI / 4.0, 0.0);
        assert!((l1 - l2).abs() < 1e-10, "l1={l1}, l2={l2}");
    }

    #[test]
    fn overcast_sky_zenith_brightest() {
        // Overcast sky: zenith (phi=pi/2) should be brightest
        let l_zenith = sky_luminance(SkyType::Overcast, PI / 2.0, 0.0, PI / 4.0, 0.0);
        let l_horizon = sky_luminance(SkyType::Overcast, 0.1, 0.0, PI / 4.0, 0.0);
        assert!(l_zenith > l_horizon, "zenith={l_zenith}, horizon={l_horizon}");
        // At zenith: (1 + 2*1) / 3 = 1.0
        assert!((l_zenith - 1.0).abs() < 0.01, "l_zenith={l_zenith}");
        // At horizon: (1 + 0) / 3 ≈ 0.33
        assert!(l_horizon < 0.5);
    }

    #[test]
    fn clear_sky_bright_near_sun() {
        let sun_alt = PI / 4.0;
        let sun_azi = PI; // south
        // Near the sun
        let l_near = sky_luminance(SkyType::Clear, sun_alt, sun_azi, sun_alt, sun_azi);
        // Away from sun (opposite azimuth)
        let l_far = sky_luminance(SkyType::Clear, sun_alt, 0.0, sun_alt, sun_azi);
        assert!(l_near > l_far, "near_sun={l_near}, far={l_far}");
    }

    #[test]
    fn clear_turbid_brighter_near_sun() {
        let l_near = sky_luminance(SkyType::ClearTurbid, PI / 4.0, PI, PI / 4.0, PI);
        let l_far = sky_luminance(SkyType::ClearTurbid, PI / 4.0, 0.0, PI / 4.0, PI);
        assert!(l_near > l_far);
    }

    #[test]
    fn angular_distance_same_point() {
        let d = angular_distance(PI / 4.0, 0.0, PI / 4.0, 0.0);
        assert!(d.abs() < 1e-10);
    }

    #[test]
    fn angular_distance_opposite_horizons() {
        let d = angular_distance(0.0, 0.0, 0.0, PI);
        assert!((d - PI).abs() < 0.01);
    }

    #[test]
    fn horizontal_illuminance_overcast() {
        // Overcast sky should give consistent illuminance
        let e = horizontal_sky_illuminance(SkyType::Overcast, PI / 4.0, 0.0, 18, 8);
        assert!(e > 0.0, "e={e}");
        // Normalized to zenith=1.0: E_h ≈ 7/9 * pi ≈ 2.44
        assert!(e > 1.5 && e < 4.0, "e={e}");
    }

    #[test]
    fn horizontal_illuminance_clear() {
        let e = horizontal_sky_illuminance(SkyType::Clear, PI / 3.0, PI, 18, 8);
        assert!(e > 0.0);
    }

    #[test]
    fn zero_altitude_no_illuminance() {
        // When sun is below horizon, clear sky gives no illuminance
        let e = horizontal_sky_illuminance(SkyType::Clear, -0.1, 0.0, 18, 8);
        assert!((e).abs() < 1e-10);
    }
}
