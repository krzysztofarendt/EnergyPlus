//! Incident solar radiation calculations on tilted surfaces.
//!
//! Includes angle of incidence calculation, diffuse sky models,
//! and ground-reflected radiation.

use ep_units::*;

/// Calculate cosine of angle of incidence on a tilted surface.
///
/// # Arguments
/// * `sun_altitude` - Solar altitude angle
/// * `sun_azimuth` - Solar azimuth (from south, positive west)
/// * `surface_tilt` - Surface tilt from horizontal
/// * `surface_azimuth` - Surface azimuth (from south, positive west)
pub fn cos_angle_of_incidence(
    sun_altitude: Angle,
    sun_azimuth: Angle,
    surface_tilt: Angle,
    surface_azimuth: Angle,
) -> f64 {
    let sin_alt = sun_altitude.sin();
    let cos_alt = sun_altitude.cos();
    let sin_tilt = surface_tilt.sin();
    let cos_tilt = surface_tilt.cos();

    // Azimuth difference between sun and surface
    let delta_azimuth = Angle::new(sun_azimuth.value() - surface_azimuth.value());

    sin_alt * cos_tilt + cos_alt * sin_tilt * delta_azimuth.cos()
}

/// Isotropic sky diffuse radiation on a tilted surface.
///
/// D_tilt = D_horiz * (1 + cos(tilt)) / 2
pub fn isotropic_diffuse(dhi: Irradiance, cos_tilt: f64) -> Irradiance {
    Irradiance::new(dhi.value() * (1.0 + cos_tilt) / 2.0)
}

/// Ground-reflected radiation on a tilted surface.
///
/// R = GHI * albedo * (1 - cos(tilt)) / 2
pub fn ground_reflected(ghi: Irradiance, cos_tilt: f64, albedo: f64) -> Irradiance {
    Irradiance::new(ghi.value() * albedo * (1.0 - cos_tilt) / 2.0)
}

/// Beam radiation on a tilted surface.
///
/// B_tilt = DNI * max(cos_incidence, 0)
pub fn beam_on_tilted(dni: Irradiance, cos_incidence: f64) -> Irradiance {
    Irradiance::new(dni.value() * cos_incidence.max(0.0))
}

/// Calculate the angle of incidence for tracking surfaces (direct beam optimization).
///
/// For surfaces that track to minimize incidence angle:
/// cos(theta) = cos(zenith) for 2-axis tracking.
pub fn cos_incidence_two_axis_tracking(zenith: Angle) -> f64 {
    zenith.cos().max(0.0)
}

/// HDKR (Hay-Davies-Klucher-Reindl) diffuse model.
///
/// A more accurate anisotropic sky model that accounts for
/// circumsolar and horizon brightening effects.
pub fn hdkr_diffuse(
    dhi: Irradiance,
    dni: Irradiance,
    ghi: Irradiance,
    cos_incidence: f64,
    cos_tilt: f64,
    sin_tilt: f64,
    cos_zenith: f64,
    extraterrestrial: Irradiance,
) -> Irradiance {
    if dhi.value() <= 0.0 || cos_zenith <= 0.0 || extraterrestrial.value() <= 0.0 {
        return isotropic_diffuse(dhi, cos_tilt);
    }

    // Anisotropy index
    let ai = (dni.value() * cos_zenith / extraterrestrial.value()).min(1.0);

    // Modulating factor for horizon brightening
    let f_hb = if ghi.value() > 0.0 {
        (dni.value() / ghi.value()).sqrt().min(1.0)
    } else {
        0.0
    };

    // Geometric factor
    let r_b = if cos_zenith > 0.087 {
        cos_incidence.max(0.0) / cos_zenith
    } else {
        cos_incidence.max(0.0) / 0.087
    };

    let iso_part = (1.0 - ai) * (1.0 + cos_tilt) / 2.0;
    let circ_part = ai * r_b;
    let horiz_part = (1.0 - ai) * f_hb * sin_tilt;

    Irradiance::new(dhi.value() * (iso_part + circ_part + horiz_part).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aoi_normal_incidence() {
        // Sun directly above, horizontal surface
        let cos_aoi = cos_angle_of_incidence(
            Angle::from_degrees(90.0), // Sun at zenith
            Angle::from_degrees(0.0),
            Angle::from_degrees(0.0),  // Horizontal surface
            Angle::from_degrees(0.0),
        );
        assert!((cos_aoi - 1.0).abs() < 0.01, "cos_aoi={cos_aoi}");
    }

    #[test]
    fn aoi_grazing_incidence() {
        // Sun at horizon, vertical wall facing away
        let cos_aoi = cos_angle_of_incidence(
            Angle::from_degrees(0.0),   // Sun at horizon
            Angle::from_degrees(0.0),   // From south
            Angle::from_degrees(90.0),  // Vertical
            Angle::from_degrees(180.0), // Facing north (away from sun)
        );
        assert!(cos_aoi < 0.01, "cos_aoi={cos_aoi}");
    }

    #[test]
    fn isotropic_diffuse_horizontal() {
        let dhi = Irradiance::new(200.0);
        let result = isotropic_diffuse(dhi, 1.0); // cos(0) = 1, horizontal
        assert!((result.value() - 200.0).abs() < 0.1);
    }

    #[test]
    fn isotropic_diffuse_vertical() {
        let dhi = Irradiance::new(200.0);
        let result = isotropic_diffuse(dhi, 0.0); // cos(90) = 0, vertical
        assert!((result.value() - 100.0).abs() < 0.1);
    }

    #[test]
    fn ground_reflected_horizontal() {
        let ghi = Irradiance::new(500.0);
        // Horizontal surface: no ground reflection
        let result = ground_reflected(ghi, 1.0, 0.2);
        assert!(result.value().abs() < 0.1);
    }

    #[test]
    fn ground_reflected_vertical() {
        let ghi = Irradiance::new(500.0);
        // Vertical surface: half of ground reflection
        let result = ground_reflected(ghi, 0.0, 0.2);
        assert!((result.value() - 50.0).abs() < 0.1);
    }

    #[test]
    fn beam_on_tilted_positive() {
        let dni = Irradiance::new(800.0);
        let result = beam_on_tilted(dni, 0.866); // cos(30°)
        assert!((result.value() - 692.8).abs() < 0.1);
    }

    #[test]
    fn beam_on_tilted_negative_aoi() {
        let dni = Irradiance::new(800.0);
        let result = beam_on_tilted(dni, -0.5); // Behind surface
        assert!(result.value().abs() < 1e-10);
    }
}
