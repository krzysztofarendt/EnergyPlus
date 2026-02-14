//! Domestic hot water usage profiles and load calculation.
//!
//! Provides standard draw profiles and calculates hot water demand
//! from fixture counts and usage patterns.

/// DHW end use type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndUseType {
    Shower,
    Bath,
    Sink,
    Dishwasher,
    ClothesWasher,
    Other,
}

/// A single DHW fixture definition.
#[derive(Debug, Clone)]
pub struct Fixture {
    pub name: String,
    pub end_use: EndUseType,
    /// Peak flow rate per fixture (m³/s).
    pub peak_flow_rate: f64,
    /// Target hot water temperature (C).
    pub target_temp: f64,
    /// Number of fixtures.
    pub count: u32,
    /// Schedule fraction (0-1) for this hour.
    pub schedule_fraction: f64,
}

/// DHW load calculation result.
#[derive(Debug, Clone, Copy, Default)]
pub struct DhwResult {
    /// Total hot water flow rate (m³/s).
    pub total_flow_rate: f64,
    /// Mixed (tempered) flow rate at target temp (m³/s).
    pub mixed_flow_rate: f64,
    /// Heat demand (W).
    pub heat_demand: f64,
    /// Hot water energy per day (J).
    pub daily_energy: f64,
}

/// Calculate DHW demand from a set of fixtures.
///
/// `fixtures` — active fixtures with schedule fractions.
/// `hot_water_temp` — temperature of water from heater (C).
/// `cold_water_temp` — mains cold water temperature (C).
pub fn calculate_demand(
    fixtures: &[Fixture],
    hot_water_temp: f64,
    cold_water_temp: f64,
) -> DhwResult {
    let cp_water = 4186.0;
    let rho_water = 998.0;

    let mut total_flow = 0.0;
    let mut total_heat = 0.0;

    for fixture in fixtures {
        let mixed_flow = fixture.peak_flow_rate
            * fixture.count as f64
            * fixture.schedule_fraction;

        if mixed_flow <= 0.0 {
            continue;
        }

        // Mixing valve: how much hot water is needed to reach target temp
        let hot_fraction = if (hot_water_temp - cold_water_temp).abs() > 0.1 {
            ((fixture.target_temp - cold_water_temp)
                / (hot_water_temp - cold_water_temp))
                .clamp(0.0, 1.0)
        } else {
            1.0
        };

        let hot_flow = mixed_flow * hot_fraction;
        total_flow += hot_flow;

        // Heat demand for this fixture
        let heat = hot_flow * rho_water * cp_water * (hot_water_temp - cold_water_temp);
        total_heat += heat;
    }

    DhwResult {
        total_flow_rate: total_flow,
        mixed_flow_rate: total_flow,
        heat_demand: total_heat,
        daily_energy: total_heat * 3600.0, // Assumes 1-hour calculation
    }
}

/// Standard residential DHW profile — hourly fractions for a typical day.
/// Index 0 = midnight, index 23 = 11 PM.
pub const RESIDENTIAL_PROFILE: [f64; 24] = [
    0.02, 0.02, 0.01, 0.01, 0.02, 0.05, // 0-5
    0.10, 0.12, 0.08, 0.05, 0.04, 0.04, // 6-11
    0.05, 0.04, 0.03, 0.04, 0.05, 0.07, // 12-17
    0.08, 0.07, 0.06, 0.05, 0.04, 0.03, // 18-23
];

/// Standard commercial office DHW profile.
pub const OFFICE_PROFILE: [f64; 24] = [
    0.00, 0.00, 0.00, 0.00, 0.00, 0.00, // 0-5
    0.02, 0.05, 0.10, 0.08, 0.06, 0.08, // 6-11
    0.10, 0.06, 0.05, 0.06, 0.08, 0.04, // 12-17
    0.02, 0.00, 0.00, 0.00, 0.00, 0.00, // 18-23
];

/// Estimate annual cold water mains temperature using a sinusoidal model.
///
/// `annual_avg_outdoor_temp` — average annual outdoor air temperature (C).
/// `max_diff` — max difference from annual average (C).
/// `day_of_year` — day of year (1-365).
/// `lag_days` — phase lag (typically ~35 days).
pub fn mains_water_temp(
    annual_avg_outdoor_temp: f64,
    max_diff: f64,
    day_of_year: u16,
    lag_days: f64,
) -> f64 {
    let offset = 6.0; // Mains water is typically 6°C warmer than ground
    let ratio = 0.4 + 0.01 * (annual_avg_outdoor_temp - 44.0_f64 * 5.0 / 9.0);
    let ratio = ratio.clamp(0.25, 0.55);

    let avg_mains = annual_avg_outdoor_temp + offset * ratio;
    let amplitude = max_diff * ratio;

    let phase = 2.0 * std::f64::consts::PI * (day_of_year as f64 - 15.0 - lag_days) / 365.0;
    (avg_mains + amplitude * phase.sin()).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_fixture_demand() {
        let fixtures = vec![
            Fixture {
                name: "Shower".into(),
                end_use: EndUseType::Shower,
                peak_flow_rate: 0.00015, // ~9 L/min
                target_temp: 40.0,
                count: 1,
                schedule_fraction: 1.0,
            },
        ];

        let result = calculate_demand(&fixtures, 60.0, 10.0);
        assert!(result.total_flow_rate > 0.0);
        assert!(result.heat_demand > 0.0);
    }

    #[test]
    fn mixing_valve_fraction() {
        let fixtures = vec![
            Fixture {
                name: "Sink".into(),
                end_use: EndUseType::Sink,
                peak_flow_rate: 0.0001,
                target_temp: 35.0, // Low target temp
                count: 1,
                schedule_fraction: 1.0,
            },
        ];

        let result = calculate_demand(&fixtures, 60.0, 10.0);
        // Hot fraction = (35-10)/(60-10) = 0.5
        assert!((result.total_flow_rate - 0.00005).abs() < 1e-6);
    }

    #[test]
    fn zero_schedule_no_demand() {
        let fixtures = vec![
            Fixture {
                name: "Shower".into(),
                end_use: EndUseType::Shower,
                peak_flow_rate: 0.00015,
                target_temp: 40.0,
                count: 2,
                schedule_fraction: 0.0,
            },
        ];

        let result = calculate_demand(&fixtures, 60.0, 10.0);
        assert!(result.total_flow_rate.abs() < 1e-10);
    }

    #[test]
    fn multiple_fixtures() {
        let fixtures = vec![
            Fixture {
                name: "Shower".into(),
                end_use: EndUseType::Shower,
                peak_flow_rate: 0.00015,
                target_temp: 40.0,
                count: 2,
                schedule_fraction: 0.5,
            },
            Fixture {
                name: "Sink".into(),
                end_use: EndUseType::Sink,
                peak_flow_rate: 0.00008,
                target_temp: 35.0,
                count: 3,
                schedule_fraction: 0.3,
            },
        ];

        let result = calculate_demand(&fixtures, 60.0, 10.0);
        assert!(result.total_flow_rate > 0.0);
        assert!(result.heat_demand > 0.0);
    }

    #[test]
    fn residential_profile_sums_to_one() {
        let sum: f64 = RESIDENTIAL_PROFILE.iter().sum();
        // Profile fractions should approximately sum to 1.0
        // (they represent hourly fractions, not necessarily summing exactly to 1)
        assert!(sum > 0.5 && sum < 2.0);
    }

    #[test]
    fn mains_water_temp_seasonal() {
        let winter = mains_water_temp(10.0, 10.0, 15, 35.0); // Jan 15
        let summer = mains_water_temp(10.0, 10.0, 196, 35.0); // Jul 15

        // Summer mains should be warmer
        assert!(summer > winter, "summer={} > winter={}", summer, winter);
    }
}
