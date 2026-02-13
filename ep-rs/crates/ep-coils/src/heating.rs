//! Simple heating coil models (electric, gas, steam).
//!
//! Electric coils have efficiency = 1.0. Gas coils use a burner
//! efficiency curve as a function of part-load ratio.

/// Heating coil fuel type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeatingFuelType {
    Electric,
    NaturalGas,
    Propane,
    Steam,
}

/// Simple heating coil specification.
#[derive(Debug, Clone)]
pub struct HeatingCoil {
    /// Coil name.
    pub name: String,
    /// Fuel type.
    pub fuel_type: HeatingFuelType,
    /// Nominal heating capacity (W).
    pub nominal_capacity: f64,
    /// Nominal efficiency (0-1). Electric = 1.0.
    pub nominal_efficiency: f64,
    /// Parasitic electric load when coil is on (W).
    pub parasitic_on: f64,
    /// Parasitic electric load when coil is off (W).
    pub parasitic_off: f64,
}

/// Heating coil calculation result.
#[derive(Debug, Clone, Copy)]
pub struct HeatingCoilResult {
    /// Heating delivered to air (W).
    pub heating_rate: f64,
    /// Fuel consumption rate (W, higher heating value).
    pub fuel_rate: f64,
    /// Electric power consumption (W) — always electric for electric coils,
    /// parasitic only for gas coils.
    pub electric_power: f64,
    /// Air outlet temperature (C).
    pub outlet_temp: f64,
    /// Air outlet humidity ratio (kg/kg) — unchanged for sensible-only coils.
    pub outlet_humidity_ratio: f64,
    /// Part-load ratio (0-1).
    pub part_load_ratio: f64,
    /// Efficiency at operating conditions.
    pub efficiency: f64,
}

impl HeatingCoil {
    /// Create an electric heating coil.
    pub fn electric(name: impl Into<String>, capacity: f64) -> Self {
        Self {
            name: name.into(),
            fuel_type: HeatingFuelType::Electric,
            nominal_capacity: capacity,
            nominal_efficiency: 1.0,
            parasitic_on: 0.0,
            parasitic_off: 0.0,
        }
    }

    /// Create a gas heating coil.
    pub fn gas(name: impl Into<String>, capacity: f64, efficiency: f64) -> Self {
        Self {
            name: name.into(),
            fuel_type: HeatingFuelType::NaturalGas,
            nominal_capacity: capacity,
            nominal_efficiency: efficiency.clamp(0.01, 1.0),
            parasitic_on: 0.0,
            parasitic_off: 0.0,
        }
    }

    /// Calculate coil performance.
    ///
    /// `load_requested` is positive for heating demand.
    /// `efficiency_modifier` adjusts nominal efficiency (e.g., from a PLR curve).
    pub fn calculate(
        &self,
        air_inlet_temp: f64,
        air_inlet_w: f64,
        air_mass_flow: f64,
        load_requested: f64,
        efficiency_modifier: f64,
    ) -> HeatingCoilResult {
        if air_mass_flow <= 1e-10 || load_requested <= 0.0 {
            return HeatingCoilResult {
                heating_rate: 0.0,
                fuel_rate: 0.0,
                electric_power: self.parasitic_off,
                outlet_temp: air_inlet_temp,
                outlet_humidity_ratio: air_inlet_w,
                part_load_ratio: 0.0,
                efficiency: self.nominal_efficiency,
            };
        }

        // Available capacity
        let available = self.nominal_capacity;

        // Part-load ratio
        let plr = (load_requested / available).clamp(0.0, 1.0);

        // Actual heating delivered
        let heating = available * plr;

        // Efficiency at operating point
        let eff = (self.nominal_efficiency * efficiency_modifier.max(0.0)).clamp(0.01, 1.0);

        // Fuel consumption
        let fuel_rate = heating / eff;

        // Electric power
        let electric_power = match self.fuel_type {
            HeatingFuelType::Electric => fuel_rate,
            _ => self.parasitic_on * plr,
        };

        // Outlet temperature
        let cp = ep_psychrometrics::cp_air(air_inlet_w);
        let outlet_temp = air_inlet_temp + heating / (air_mass_flow * cp);

        HeatingCoilResult {
            heating_rate: heating,
            fuel_rate,
            electric_power,
            outlet_temp,
            outlet_humidity_ratio: air_inlet_w,
            part_load_ratio: plr,
            efficiency: eff,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electric_coil_basic() {
        let coil = HeatingCoil::electric("Elec Htg", 10000.0);
        assert_eq!(coil.fuel_type, HeatingFuelType::Electric);
        assert!((coil.nominal_efficiency - 1.0).abs() < 1e-10);
    }

    #[test]
    fn electric_coil_full_load() {
        let coil = HeatingCoil::electric("Elec", 10000.0);
        let result = coil.calculate(15.0, 0.006, 1.0, 10000.0, 1.0);
        assert!((result.heating_rate - 10000.0).abs() < 1.0);
        assert!((result.fuel_rate - 10000.0).abs() < 1.0); // eff=1.0
        assert!((result.electric_power - 10000.0).abs() < 1.0);
        assert!((result.part_load_ratio - 1.0).abs() < 0.01);
        assert!(result.outlet_temp > 15.0, "T_out={}", result.outlet_temp);
    }

    #[test]
    fn electric_coil_part_load() {
        let coil = HeatingCoil::electric("Elec", 10000.0);
        let result = coil.calculate(15.0, 0.006, 1.0, 5000.0, 1.0);
        assert!((result.heating_rate - 5000.0).abs() < 1.0);
        assert!((result.part_load_ratio - 0.5).abs() < 0.01);
    }

    #[test]
    fn electric_coil_no_load() {
        let coil = HeatingCoil::electric("Elec", 10000.0);
        let result = coil.calculate(15.0, 0.006, 1.0, 0.0, 1.0);
        assert!(result.heating_rate.abs() < 1e-10);
        assert!((result.outlet_temp - 15.0).abs() < 1e-10);
    }

    #[test]
    fn gas_coil_efficiency() {
        let coil = HeatingCoil::gas("Gas Htg", 20000.0, 0.80);
        let result = coil.calculate(10.0, 0.005, 1.5, 20000.0, 1.0);
        assert!((result.heating_rate - 20000.0).abs() < 1.0);
        // Fuel = heating / efficiency = 20000 / 0.80 = 25000
        assert!((result.fuel_rate - 25000.0).abs() < 10.0,
                "fuel={}", result.fuel_rate);
        assert!((result.efficiency - 0.80).abs() < 0.01);
    }

    #[test]
    fn gas_coil_efficiency_modifier() {
        let coil = HeatingCoil::gas("Gas", 20000.0, 0.80);
        // At part-load, efficiency drops by 10%
        let result = coil.calculate(10.0, 0.005, 1.5, 10000.0, 0.9);
        let expected_eff = 0.80 * 0.9; // 0.72
        assert!((result.efficiency - expected_eff).abs() < 0.01,
                "eff={}", result.efficiency);
        let expected_fuel = 10000.0 / expected_eff;
        assert!((result.fuel_rate - expected_fuel).abs() < 10.0,
                "fuel={}", result.fuel_rate);
    }

    #[test]
    fn gas_coil_parasitic() {
        let mut coil = HeatingCoil::gas("Gas", 20000.0, 0.80);
        coil.parasitic_on = 100.0;
        coil.parasitic_off = 10.0;

        // When running
        let on_result = coil.calculate(10.0, 0.005, 1.5, 20000.0, 1.0);
        assert!((on_result.electric_power - 100.0).abs() < 1.0);

        // When off
        let off_result = coil.calculate(10.0, 0.005, 1.5, 0.0, 1.0);
        assert!((off_result.electric_power - 10.0).abs() < 1.0);
    }

    #[test]
    fn outlet_temp_energy_balance() {
        let coil = HeatingCoil::electric("Test", 5000.0);
        let w = 0.008;
        let t_in = 12.0;
        let mdot = 0.8;
        let result = coil.calculate(t_in, w, mdot, 5000.0, 1.0);

        let cp = ep_psychrometrics::cp_air(w);
        let expected_dt = 5000.0 / (mdot * cp);
        assert!((result.outlet_temp - (t_in + expected_dt)).abs() < 0.01,
                "T_out={}, expected={}", result.outlet_temp, t_in + expected_dt);
    }

    #[test]
    fn humidity_ratio_unchanged() {
        let coil = HeatingCoil::electric("Test", 5000.0);
        let result = coil.calculate(12.0, 0.009, 0.8, 5000.0, 1.0);
        assert!((result.outlet_humidity_ratio - 0.009).abs() < 1e-10);
    }
}
