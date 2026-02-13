//! EPW (EnergyPlus Weather) file parser.
//!
//! Parses TMY3-format EPW files with hourly or sub-hourly weather records.

use ep_core::error::SimError;
use ep_units::*;
use std::path::Path;

use crate::{Location, WeatherFile, WeatherRecord};

/// Number of header lines in an EPW file.
const EPW_HEADER_LINES: usize = 8;

/// Parse an EPW file from a path.
pub fn parse_epw(path: &Path) -> Result<WeatherFile, SimError> {
    let content = std::fs::read_to_string(path).map_err(SimError::IoError)?;
    parse_epw_string(&content)
}

/// Parse an EPW file from a string.
pub fn parse_epw_string(content: &str) -> Result<WeatherFile, SimError> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.len() < EPW_HEADER_LINES + 1 {
        return Err(SimError::WeatherError("EPW file too short".to_string()));
    }

    // Parse location from first header line
    let location = parse_location_line(lines[0])?;

    // Parse data records (after header lines)
    let mut records = Vec::new();
    for (i, line) in lines.iter().enumerate().skip(EPW_HEADER_LINES) {
        match parse_data_line(line) {
            Ok(record) => records.push(record),
            Err(e) => {
                return Err(SimError::WeatherError(format!("Error on line {}: {e}", i + 1)));
            }
        }
    }

    Ok(WeatherFile { location, records })
}

/// Parse the LOCATION header line.
fn parse_location_line(line: &str) -> Result<Location, SimError> {
    let fields: Vec<&str> = line.split(',').collect();
    if fields.len() < 10 || !fields[0].eq_ignore_ascii_case("LOCATION") {
        return Err(SimError::WeatherError("Invalid LOCATION header".to_string()));
    }

    Ok(Location {
        name: format!("{}, {}", fields[1].trim(), fields[3].trim()),
        latitude: Angle::from_degrees(
            fields[6]
                .trim()
                .parse::<f64>()
                .map_err(|_| SimError::WeatherError("Invalid latitude".to_string()))?,
        ),
        longitude: Angle::from_degrees(
            fields[7]
                .trim()
                .parse::<f64>()
                .map_err(|_| SimError::WeatherError("Invalid longitude".to_string()))?,
        ),
        time_zone: fields[8]
            .trim()
            .parse::<f64>()
            .map_err(|_| SimError::WeatherError("Invalid time zone".to_string()))?,
        elevation: Length::new(
            fields[9]
                .trim()
                .parse::<f64>()
                .map_err(|_| SimError::WeatherError("Invalid elevation".to_string()))?,
        ),
        station_id: fields[5].trim().to_string(),
    })
}

/// Parse a single data record line.
fn parse_data_line(line: &str) -> Result<WeatherRecord, String> {
    let fields: Vec<&str> = line.split(',').collect();
    if fields.len() < 35 {
        return Err(format!("Expected at least 35 fields, got {}", fields.len()));
    }

    let parse_f64 = |idx: usize, name: &str| -> Result<f64, String> {
        fields[idx]
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("Invalid {name}: '{}'", fields[idx].trim()))
    };

    let parse_i32 = |idx: usize, name: &str| -> Result<i32, String> {
        fields[idx]
            .trim()
            .parse::<i32>()
            .map_err(|_| format!("Invalid {name}: '{}'", fields[idx].trim()))
    };

    Ok(WeatherRecord {
        year: parse_i32(0, "year")?,
        month: parse_i32(1, "month")? as u8,
        day: parse_i32(2, "day")? as u8,
        hour: parse_i32(3, "hour")? as u8,
        minute: parse_i32(4, "minute")? as u8,
        dry_bulb: Temperature::from_celsius(parse_f64(6, "dry_bulb")?),
        dew_point: Temperature::from_celsius(parse_f64(7, "dew_point")?),
        rel_humidity: RelativeHumidity::new(parse_f64(8, "rel_humidity")?),
        pressure: Pressure::new(parse_f64(9, "pressure")?),
        global_horizontal_irradiance: Irradiance::new(parse_f64(13, "ghi")?),
        direct_normal_irradiance: Irradiance::new(parse_f64(14, "dni")?),
        diffuse_horizontal_irradiance: Irradiance::new(parse_f64(15, "dhi")?),
        wind_direction: Angle::from_degrees(parse_f64(20, "wind_dir")?),
        wind_speed: Velocity::new(parse_f64(21, "wind_speed")?),
        sky_cover: parse_f64(22, "sky_cover").unwrap_or(0.0),
        opaque_sky_cover: parse_f64(23, "opaque_sky_cover").unwrap_or(0.0),
        visibility: Length::new(parse_f64(24, "visibility").unwrap_or(9999.0) * 1000.0), // km to m
        ceiling_height: Length::new(parse_f64(25, "ceiling_height").unwrap_or(99999.0)),
        precipitation: parse_f64(33, "precipitation").unwrap_or(0.0),
        snow_depth: Length::new(parse_f64(30, "snow_depth").unwrap_or(0.0) / 100.0), // cm to m
        is_rain: fields.get(33).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0) > 0.0,
        is_snow: fields.get(30).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0) > 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_location_header() {
        let line = "LOCATION,Denver,CO,USA,TMY3,725650,39.75,-104.87,-7.0,1609.0";
        let loc = parse_location_line(line).unwrap();
        assert!(loc.name.contains("Denver"));
        assert!((loc.latitude.to_degrees() - 39.75).abs() < 0.01);
        assert!((loc.longitude.to_degrees() - (-104.87)).abs() < 0.01);
        assert!((loc.time_zone - (-7.0)).abs() < 0.01);
        assert!((loc.elevation.value() - 1609.0).abs() < 0.1);
    }
}
