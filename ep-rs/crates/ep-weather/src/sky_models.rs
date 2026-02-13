//! Sky temperature and diffuse radiation models.

use ep_units::*;
use crate::WeatherRecord;

/// Calculate sky temperature using the Clark-Allen model.
///
/// T_sky = T_db * (0.787 + 0.764 * ln(T_dp/273))^0.25
pub fn sky_temperature_clark_allen(record: &WeatherRecord) -> Temperature {
    let t_dp_k = record.dew_point.value(); // Already in Kelvin
    let t_db_k = record.dry_bulb.value();
    let emissivity = (0.787 + 0.764 * (t_dp_k / 273.0).ln()).max(0.0);
    Temperature::new(t_db_k * emissivity.powf(0.25))
}

/// Calculate sky temperature using the Berdahl-Martin model.
///
/// Uses dew point temperature and opaque sky cover.
pub fn sky_temperature_berdahl_martin(record: &WeatherRecord) -> Temperature {
    let t_dp_c = record.dew_point.to_celsius();
    let t_db_k = record.dry_bulb.value();

    // Clear sky emissivity
    let eps_clear = 0.741 + 0.0062 * t_dp_c;
    // Adjust for cloud cover (0-10 scale)
    let cloud_fraction = record.opaque_sky_cover / 10.0;
    let eps = eps_clear + 0.84 * cloud_fraction * (1.0 - eps_clear);
    let eps = eps.clamp(0.0, 1.0);

    Temperature::new(t_db_k * eps.powf(0.25))
}

/// Perez anisotropic sky diffuse model.
///
/// Returns the diffuse irradiance on a tilted surface given:
/// - `dhi`: Diffuse horizontal irradiance
/// - `dni`: Direct normal irradiance
/// - `zenith`: Solar zenith angle
/// - `tilt`: Surface tilt angle (from horizontal)
/// - `surface_azimuth_from_sun`: Azimuth difference between surface and sun
pub fn perez_diffuse(
    dhi: Irradiance,
    dni: Irradiance,
    zenith: Angle,
    tilt: Angle,
    aoi: Angle,
    extraterrestrial: Irradiance,
) -> Irradiance {
    if dhi.value() <= 0.0 || zenith.to_degrees() >= 90.0 {
        return Irradiance::ZERO;
    }

    let cos_zenith = zenith.cos().max(0.087); // Limit to ~5° from horizon
    let cos_tilt = tilt.cos();
    let sin_tilt = tilt.sin();

    // Clearness index
    let am = 1.0 / cos_zenith; // Air mass approximation
    let _ghi = dhi.value() + dni.value() * cos_zenith;
    let _eps = ((dhi.value() + dni.value()) / dhi.value() + 5.535e-6 * zenith.value().powi(3)) / (1.0 + 5.535e-6 * zenith.value().powi(3));

    // Brightness
    let delta = dhi.value() * am / extraterrestrial.value().max(1.0);
    let delta = delta.clamp(0.0, 1.0);

    // Simplified Perez coefficients (category 1 for clarity)
    // Full implementation would use 8 clearness bins
    let f11 = -0.008 + 0.588 * delta;
    let f12 = 0.06 + 0.072 * delta;
    let _f13 = -0.06 + 0.03 * delta;
    let _f21 = -0.06 + 0.07 * delta;
    let _f22 = -0.019 + 0.066 * delta;
    let _f23 = -0.05 + 0.04 * delta;

    let a = aoi.cos().max(0.0);
    let b = cos_zenith.max(0.087);

    // Isotropic component
    let iso = dhi.value() * (1.0 + cos_tilt) / 2.0;
    // Circumsolar component
    let circ = dhi.value() * f11.max(0.0) * a / b;
    // Horizon brightening
    let horiz = dhi.value() * f12 * sin_tilt;

    Irradiance::new((iso + circ + horiz).max(0.0))
}

/// Isotropic sky diffuse model (simplest model).
///
/// Assumes uniform diffuse radiation from all sky directions.
pub fn isotropic_diffuse(dhi: Irradiance, tilt: Angle) -> Irradiance {
    Irradiance::new(dhi.value() * (1.0 + tilt.cos()) / 2.0)
}

/// Ground-reflected radiation on a tilted surface.
pub fn ground_reflected(ghi: Irradiance, tilt: Angle, albedo: f64) -> Irradiance {
    Irradiance::new(ghi.value() * albedo * (1.0 - tilt.cos()) / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isotropic_horizontal() {
        let dhi = Irradiance::new(200.0);
        let result = isotropic_diffuse(dhi, Angle::from_degrees(0.0));
        assert!((result.value() - 200.0).abs() < 0.1);
    }

    #[test]
    fn isotropic_vertical() {
        let dhi = Irradiance::new(200.0);
        let result = isotropic_diffuse(dhi, Angle::from_degrees(90.0));
        assert!((result.value() - 100.0).abs() < 0.1);
    }

    #[test]
    fn ground_reflected_horizontal() {
        let ghi = Irradiance::new(500.0);
        // Horizontal surface sees no ground reflection
        let result = ground_reflected(ghi, Angle::from_degrees(0.0), 0.2);
        assert!(result.value().abs() < 0.1);
    }
}
