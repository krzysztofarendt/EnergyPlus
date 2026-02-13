//! Weather data processing and solar calculations for EnergyPlus-rs.
//!
//! Includes EPW file parsing, solar position calculations, sky models,
//! and ground temperature models. Ported from EnergyPlus WeatherManager.cc.

use ep_units::*;
use std::path::Path;

pub mod design_day;
pub mod epw;
pub mod ground_temperature;
pub mod sky_models;
pub mod solar_position;

/// Site location data.
#[derive(Debug, Clone)]
pub struct Location {
    pub name: String,
    pub latitude: Angle,
    pub longitude: Angle,
    pub time_zone: f64,
    pub elevation: Length,
    pub station_id: String,
}

impl Default for Location {
    fn default() -> Self {
        Self {
            name: String::new(),
            latitude: Angle::new(0.0),
            longitude: Angle::new(0.0),
            time_zone: 0.0,
            elevation: Length::new(0.0),
            station_id: String::new(),
        }
    }
}

/// Single timestep weather data.
#[derive(Debug, Clone)]
pub struct WeatherRecord {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub dry_bulb: Temperature,
    pub dew_point: Temperature,
    pub rel_humidity: RelativeHumidity,
    pub pressure: Pressure,
    pub direct_normal_irradiance: Irradiance,
    pub diffuse_horizontal_irradiance: Irradiance,
    pub global_horizontal_irradiance: Irradiance,
    pub wind_speed: Velocity,
    pub wind_direction: Angle,
    pub sky_cover: f64,
    pub opaque_sky_cover: f64,
    pub visibility: Length,
    pub ceiling_height: Length,
    pub precipitation: f64,
    pub snow_depth: Length,
    pub is_rain: bool,
    pub is_snow: bool,
}

/// Solar position for a given time and location.
#[derive(Debug, Clone)]
pub struct SolarPosition {
    pub altitude: Angle,
    pub azimuth: Angle,
    pub zenith: Angle,
    pub hour_angle: Angle,
    pub declination: Angle,
    pub equation_of_time: Duration,
    pub sun_is_up: bool,
}

/// Parsed EPW weather file.
#[derive(Debug, Clone)]
pub struct WeatherFile {
    pub location: Location,
    pub records: Vec<WeatherRecord>,
}

impl WeatherFile {
    /// Parse an EPW file from a path.
    pub fn from_path(path: &Path) -> Result<Self, ep_core::error::SimError> {
        epw::parse_epw(path)
    }

    /// Get the record for a specific month, day, hour.
    pub fn record_at(&self, month: u8, day: u8, hour: u8) -> Option<&WeatherRecord> {
        self.records.iter().find(|r| r.month == month && r.day == day && r.hour == hour)
    }
}

/// Sky temperature model selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyTempModel {
    ClarkAllen,
    Brunt,
    Idso,
    BerdahlMartin,
}

/// Ground temperature model trait.
pub trait GroundTemperatureModel: Send + Sync {
    fn temperature_at_depth(&self, depth: Length, day_of_year: u16) -> Temperature;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_location() {
        let loc = Location::default();
        assert_eq!(loc.latitude.value(), 0.0);
    }
}
