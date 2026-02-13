//! Solar position calculations using Spencer's equations.
//!
//! Reference: EnergyPlus Engineering Reference, Section: Climate Calculations

use ep_units::*;

use crate::SolarPosition;

/// Calculate solar position for a given time and location.
///
/// Uses Spencer's equations for declination and equation of time.
///
/// # Arguments
/// * `latitude` - Site latitude (radians, positive north)
/// * `longitude` - Site longitude (radians, positive east)
/// * `time_zone` - Time zone offset from UTC (hours)
/// * `day_of_year` - Day of year (1-366)
/// * `hour` - Hour of day (fractional, 0.0-24.0)
pub fn solar_position(latitude: Angle, longitude: Angle, time_zone: f64, day_of_year: u16, hour: f64) -> SolarPosition {
    // Day angle (radians)
    let day_angle = 2.0 * std::f64::consts::PI * (day_of_year as f64 - 1.0) / 365.0;

    // Equation of time (minutes) - Spencer's equation
    let eot_minutes = 229.18
        * (0.000075 + 0.001868 * day_angle.cos() - 0.032077 * day_angle.sin()
            - 0.014615 * (2.0 * day_angle).cos()
            - 0.04089 * (2.0 * day_angle).sin());

    let eot = Duration::from_minutes(eot_minutes);

    // Solar declination (radians) - Spencer's equation
    let decl_rad = 0.006918 - 0.399912 * day_angle.cos() + 0.070257 * day_angle.sin()
        - 0.006758 * (2.0 * day_angle).cos()
        + 0.000907 * (2.0 * day_angle).sin()
        - 0.002697 * (3.0 * day_angle).cos()
        + 0.00148 * (3.0 * day_angle).sin();
    let declination = Angle::new(decl_rad);

    // Solar time
    let standard_meridian = Angle::from_degrees(15.0 * time_zone);
    let solar_time_hours = hour + eot_minutes / 60.0 + (longitude.to_degrees() - standard_meridian.to_degrees()) / 15.0;

    // Hour angle (radians): 0 at solar noon, negative in morning, positive in afternoon
    let hour_angle = Angle::from_degrees(15.0 * (solar_time_hours - 12.0));

    // Solar altitude (elevation angle)
    let sin_alt = latitude.sin() * declination.sin() + latitude.cos() * declination.cos() * hour_angle.cos();
    let altitude = Angle::asin(sin_alt.clamp(-1.0, 1.0));

    // Solar zenith
    let zenith = Angle::from_degrees(90.0) - altitude;

    // Solar azimuth (from south, positive west)
    let sin_zenith = zenith.sin();

    let cos_azimuth = if sin_zenith.abs() > 1.0e-10 {
        ((sin_alt * latitude.sin() - declination.sin()) / (sin_zenith.abs() * latitude.cos())).clamp(-1.0, 1.0)
    } else {
        1.0
    };

    let mut azimuth = Angle::acos(cos_azimuth);
    if hour_angle.value() > 0.0 {
        azimuth = Angle::new(2.0 * std::f64::consts::PI - azimuth.value());
    }

    let sun_is_up = altitude.to_degrees() > -0.833; // Account for refraction

    SolarPosition {
        altitude,
        azimuth,
        zenith,
        hour_angle,
        declination,
        equation_of_time: eot,
        sun_is_up,
    }
}

/// Calculate extraterrestrial normal radiation (W/m²) for a given day.
///
/// Uses the solar constant (1367 W/m²) and the Earth's orbital eccentricity.
pub fn extraterrestrial_radiation(day_of_year: u16) -> Irradiance {
    let solar_constant = 1367.0; // W/m²
    let day_angle = 2.0 * std::f64::consts::PI * (day_of_year as f64 - 1.0) / 365.0;
    // Eccentricity correction factor
    let eccentricity = 1.000110 + 0.034221 * day_angle.cos() + 0.001280 * day_angle.sin()
        + 0.000719 * (2.0 * day_angle).cos()
        + 0.000077 * (2.0 * day_angle).sin();
    Irradiance::new(solar_constant * eccentricity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solar_position_equinox_equator() {
        // March 20 (day 79), equator, solar noon
        let pos = solar_position(
            Angle::from_degrees(0.0),  // equator
            Angle::from_degrees(0.0),  // prime meridian
            0.0,                       // UTC
            79,                        // ~March 20
            12.0,                      // noon
        );
        // At equinox on equator at solar noon, altitude should be close to 90°
        assert!(pos.altitude.to_degrees() > 85.0, "alt={}", pos.altitude.to_degrees());
        assert!(pos.sun_is_up);
    }

    #[test]
    fn solar_position_midnight() {
        // Any day, midnight should have sun below horizon
        let pos = solar_position(
            Angle::from_degrees(40.0),
            Angle::from_degrees(-105.0),
            -7.0,
            172,
            0.0,
        );
        assert!(!pos.sun_is_up || pos.altitude.to_degrees() < 0.0);
    }

    #[test]
    fn extraterrestrial_radiation_range() {
        // Should be around 1367 W/m² with ±3.3% variation
        for day in 1..=365 {
            let etr = extraterrestrial_radiation(day);
            assert!(etr.value() > 1320.0 && etr.value() < 1420.0, "day={day}, etr={}", etr.value());
        }
    }

    #[test]
    fn equation_of_time_range() {
        // EoT should be within ±17 minutes
        for day in 1..=365 {
            let pos = solar_position(Angle::from_degrees(0.0), Angle::from_degrees(0.0), 0.0, day, 12.0);
            let eot_min = pos.equation_of_time.to_minutes();
            assert!(eot_min.abs() < 17.0, "day={day}, eot={eot_min}");
        }
    }
}
