//! District heating and cooling models.
//!
//! Provides infinite-capacity heating or cooling at a setpoint temperature.

/// District cooling plant.
#[derive(Debug, Clone)]
pub struct DistrictCooling {
    pub name: String,
    /// Nominal capacity (W).
    pub nominal_capacity: f64,
    /// Chilled water setpoint temperature (C).
    pub chw_setpoint: f64,
}

/// District cooling result.
#[derive(Debug, Clone, Copy)]
pub struct DistrictCoolingResult {
    /// Cooling rate delivered (W).
    pub cooling_rate: f64,
    /// Water outlet temperature (C).
    pub outlet_temp: f64,
}

impl DistrictCooling {
    pub fn new(name: impl Into<String>, nominal_capacity: f64, chw_setpoint: f64) -> Self {
        Self { name: name.into(), nominal_capacity, chw_setpoint }
    }

    /// Calculate district cooling performance.
    pub fn calculate(
        &self,
        water_inlet_temp: f64,
        water_mass_flow: f64,
        load: f64,
    ) -> DistrictCoolingResult {
        if water_mass_flow <= 1e-10 || load <= 0.0 {
            return DistrictCoolingResult { cooling_rate: 0.0, outlet_temp: water_inlet_temp };
        }
        let cooling = load.min(self.nominal_capacity);
        let cp = ep_psychrometrics::cp_water(water_inlet_temp);
        let outlet = water_inlet_temp - cooling / (water_mass_flow * cp);
        let outlet = outlet.max(self.chw_setpoint);
        let actual_cooling = water_mass_flow * cp * (water_inlet_temp - outlet);
        DistrictCoolingResult { cooling_rate: actual_cooling, outlet_temp: outlet }
    }
}

/// District heating plant.
#[derive(Debug, Clone)]
pub struct DistrictHeating {
    pub name: String,
    /// Nominal capacity (W).
    pub nominal_capacity: f64,
    /// Hot water setpoint temperature (C).
    pub hw_setpoint: f64,
}

/// District heating result.
#[derive(Debug, Clone, Copy)]
pub struct DistrictHeatingResult {
    /// Heating rate delivered (W).
    pub heating_rate: f64,
    /// Water outlet temperature (C).
    pub outlet_temp: f64,
}

impl DistrictHeating {
    pub fn new(name: impl Into<String>, nominal_capacity: f64, hw_setpoint: f64) -> Self {
        Self { name: name.into(), nominal_capacity, hw_setpoint }
    }

    pub fn calculate(
        &self,
        water_inlet_temp: f64,
        water_mass_flow: f64,
        load: f64,
    ) -> DistrictHeatingResult {
        if water_mass_flow <= 1e-10 || load <= 0.0 {
            return DistrictHeatingResult { heating_rate: 0.0, outlet_temp: water_inlet_temp };
        }
        let heating = load.min(self.nominal_capacity);
        let cp = ep_psychrometrics::cp_water(water_inlet_temp);
        let outlet = water_inlet_temp + heating / (water_mass_flow * cp);
        let outlet = outlet.min(self.hw_setpoint);
        let actual_heating = water_mass_flow * cp * (outlet - water_inlet_temp);
        DistrictHeatingResult { heating_rate: actual_heating.max(0.0), outlet_temp: outlet }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn district_cooling_basic() {
        let dc = DistrictCooling::new("DC-1", 1_000_000.0, 6.7);
        let result = dc.calculate(12.0, 10.0, 200_000.0);
        assert!(result.cooling_rate > 0.0, "Q={}", result.cooling_rate);
        assert!(result.outlet_temp < 12.0, "T={}", result.outlet_temp);
        assert!(result.outlet_temp >= 6.7, "T should not go below setpoint: {}", result.outlet_temp);
    }

    #[test]
    fn district_cooling_no_load() {
        let dc = DistrictCooling::new("DC-1", 1_000_000.0, 6.7);
        let result = dc.calculate(12.0, 10.0, 0.0);
        assert!(result.cooling_rate.abs() < 1e-10);
    }

    #[test]
    fn district_heating_basic() {
        let dh = DistrictHeating::new("DH-1", 1_000_000.0, 82.0);
        let result = dh.calculate(60.0, 5.0, 100_000.0);
        assert!(result.heating_rate > 0.0, "Q={}", result.heating_rate);
        assert!(result.outlet_temp > 60.0, "T={}", result.outlet_temp);
        assert!(result.outlet_temp <= 82.0, "T should not exceed setpoint: {}", result.outlet_temp);
    }

    #[test]
    fn district_heating_no_load() {
        let dh = DistrictHeating::new("DH-1", 1_000_000.0, 82.0);
        let result = dh.calculate(60.0, 5.0, 0.0);
        assert!(result.heating_rate.abs() < 1e-10);
    }

    #[test]
    fn district_cooling_no_flow() {
        let dc = DistrictCooling::new("DC-1", 1_000_000.0, 6.7);
        let result = dc.calculate(12.0, 0.0, 200_000.0);
        assert!(result.cooling_rate.abs() < 1e-10);
    }

    #[test]
    fn district_heating_energy_balance() {
        let dh = DistrictHeating::new("DH-1", 1_000_000.0, 82.0);
        let result = dh.calculate(60.0, 5.0, 50_000.0);
        let cp = ep_psychrometrics::cp_water(60.0);
        let q_check = 5.0 * cp * (result.outlet_temp - 60.0);
        assert!((q_check - result.heating_rate).abs() / result.heating_rate.abs().max(1.0) < 0.01,
                "q_check={}, reported={}", q_check, result.heating_rate);
    }
}
