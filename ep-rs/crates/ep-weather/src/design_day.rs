//! Design day generation.
//!
//! Generates hourly weather data from ASHRAE design day specifications.

use ep_units::*;

/// Design day specification.
#[derive(Debug, Clone)]
pub struct DesignDaySpec {
    pub name: String,
    pub day_type: DesignDayType,
    pub month: u8,
    pub day: u8,
    pub max_dry_bulb: Temperature,
    pub daily_dry_bulb_range: f64,
    pub dry_bulb_range_type: DryBulbRangeType,
    pub humidity_condition: HumidityCondition,
    pub barometric_pressure: Pressure,
    pub wind_speed: Velocity,
    pub wind_direction: Angle,
    pub solar_model: DesignDaySolarModel,
    pub rain: bool,
    pub snow_on_ground: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignDayType {
    SummerExtreme,
    SummerTypical,
    WinterExtreme,
    WinterTypical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DryBulbRangeType {
    DefaultMultipliers,
    MultiplierSchedule,
    TemperatureProfileSchedule,
}

#[derive(Debug, Clone)]
pub enum HumidityCondition {
    WetBulb(f64),
    DewPoint(f64),
    HumidityRatio(f64),
    Enthalpy(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignDaySolarModel {
    AshraeClearSky,
    AshraeTau,
    AshraeTau2017,
    ZhangHuang,
    Schedule,
}

/// ASHRAE clear-sky default daily temperature range multipliers.
/// These define the shape of the diurnal temperature cycle (0 = max DB, 1 = min DB).
pub const ASHRAE_DB_RANGE_MULTIPLIERS: [f64; 24] = [
    0.87, 0.92, 0.96, 1.00, 0.98, 0.93, // Hours 1-6
    0.84, 0.71, 0.56, 0.39, 0.23, 0.13, // Hours 7-12
    0.05, 0.00, 0.03, 0.10, 0.21, 0.34, // Hours 13-18
    0.47, 0.58, 0.68, 0.76, 0.82, 0.87, // Hours 19-24
];

/// Generate hourly dry-bulb temperatures for a design day.
pub fn generate_hourly_temperatures(spec: &DesignDaySpec) -> Vec<Temperature> {
    let max_db = spec.max_dry_bulb.to_celsius();
    let range = spec.daily_dry_bulb_range;

    let mut temps = Vec::with_capacity(24);
    for hour in 0..24 {
        let multiplier = ASHRAE_DB_RANGE_MULTIPLIERS[hour];
        let t_db = max_db - multiplier * range;
        temps.push(Temperature::from_celsius(t_db));
    }
    temps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_day_temperatures() {
        let spec = DesignDaySpec {
            name: "Summer Design Day".to_string(),
            day_type: DesignDayType::SummerExtreme,
            month: 7,
            day: 21,
            max_dry_bulb: Temperature::from_celsius(35.0),
            daily_dry_bulb_range: 10.0,
            dry_bulb_range_type: DryBulbRangeType::DefaultMultipliers,
            humidity_condition: HumidityCondition::WetBulb(20.0),
            barometric_pressure: Pressure::STANDARD_ATMOSPHERE,
            wind_speed: Velocity::new(4.0),
            wind_direction: Angle::from_degrees(180.0),
            solar_model: DesignDaySolarModel::AshraeClearSky,
            rain: false,
            snow_on_ground: false,
        };

        let temps = generate_hourly_temperatures(&spec);
        assert_eq!(temps.len(), 24);

        // Hour 14 (index 13) should have multiplier 0.0 -> max temperature
        assert!((temps[13].to_celsius() - 35.0).abs() < 0.1);

        // Hour 4 (index 3) should have multiplier 1.0 -> min temperature
        assert!((temps[3].to_celsius() - 25.0).abs() < 0.1);
    }
}
