//! Hot water boiler model.
//!
//! Calculates fuel consumption from load, efficiency curve (PLR-based),
//! and nominal efficiency. Supports on/off and modulating operation.

use ep_curves::Curve;

/// Boiler fuel type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoilerFuelType {
    NaturalGas,
    Electricity,
    Propane,
    FuelOilNo1,
    FuelOilNo2,
}

/// Boiler specification.
#[derive(Debug, Clone)]
pub struct Boiler {
    pub name: String,
    pub fuel_type: BoilerFuelType,
    /// Nominal capacity (W).
    pub nominal_capacity: f64,
    /// Nominal thermal efficiency (0-1).
    pub nominal_efficiency: f64,
    /// Design water outlet temperature (C).
    pub design_outlet_temp: f64,
    /// Design water flow rate (kg/s).
    pub design_water_flow: f64,
    /// Minimum part-load ratio.
    pub min_plr: f64,
    /// Maximum part-load ratio.
    pub max_plr: f64,
    /// Parasitic electric load (W).
    pub parasitic_electric: f64,
}

/// Boiler calculation result.
#[derive(Debug, Clone, Copy)]
pub struct BoilerResult {
    /// Heat delivered to water (W).
    pub heating_rate: f64,
    /// Fuel consumption (W, higher heating value).
    pub fuel_rate: f64,
    /// Electric parasitic power (W).
    pub electric_power: f64,
    /// Water outlet temperature (C).
    pub outlet_temp: f64,
    /// Part-load ratio.
    pub part_load_ratio: f64,
    /// Operating efficiency.
    pub efficiency: f64,
}

impl Boiler {
    pub fn new(
        name: impl Into<String>,
        capacity: f64,
        efficiency: f64,
        design_outlet_temp: f64,
        design_water_flow: f64,
    ) -> Self {
        Self {
            name: name.into(),
            fuel_type: BoilerFuelType::NaturalGas,
            nominal_capacity: capacity,
            nominal_efficiency: efficiency.clamp(0.01, 1.0),
            design_outlet_temp,
            design_water_flow,
            min_plr: 0.0,
            max_plr: 1.0,
            parasitic_electric: 0.0,
        }
    }

    /// Calculate boiler performance at current conditions.
    ///
    /// `load` is the heating load requested (W, positive).
    /// `efficiency_curve` modifies nominal efficiency as f(PLR).
    pub fn calculate(
        &self,
        water_inlet_temp: f64,
        water_mass_flow: f64,
        load: f64,
        efficiency_curve: Option<&Curve>,
    ) -> BoilerResult {
        if water_mass_flow <= 1e-10 || load <= 0.0 || self.nominal_capacity <= 0.0 {
            return BoilerResult {
                heating_rate: 0.0,
                fuel_rate: 0.0,
                electric_power: 0.0,
                outlet_temp: water_inlet_temp,
                part_load_ratio: 0.0,
                efficiency: self.nominal_efficiency,
            };
        }

        // Part-load ratio
        let plr = (load / self.nominal_capacity).clamp(self.min_plr, self.max_plr);

        // Actual heat output
        let heating = self.nominal_capacity * plr;

        // Efficiency modifier from curve
        let eff_modifier = match efficiency_curve {
            Some(curve) => curve.evaluate1(plr).max(0.01),
            None => 1.0,
        };

        let operating_eff = (self.nominal_efficiency * eff_modifier).clamp(0.01, 1.0);

        // Fuel consumption: FuelUsed = BoilerLoad / (EffCurve * NomEffic)
        let fuel_rate = heating / operating_eff;

        // Water outlet temperature
        let cp_water = ep_psychrometrics::cp_water(water_inlet_temp);
        let outlet_temp = water_inlet_temp + heating / (water_mass_flow * cp_water);

        BoilerResult {
            heating_rate: heating,
            fuel_rate,
            electric_power: self.parasitic_electric * plr,
            outlet_temp,
            part_load_ratio: plr,
            efficiency: operating_eff,
        }
    }
}

// ---------------------------------------------------------------------------
// Steam Boiler
// ---------------------------------------------------------------------------

/// Steam boiler — generates steam from water with fuel consumption.
#[derive(Debug, Clone)]
pub struct SteamBoiler {
    pub name: String,
    pub fuel_type: BoilerFuelType,
    /// Nominal steam generation capacity (W, thermal).
    pub nominal_capacity: f64,
    /// Nominal thermal efficiency (0-1).
    pub nominal_efficiency: f64,
    /// Design steam temperature (C).
    pub design_steam_temp: f64,
    /// Parasitic electric load (W).
    pub parasitic_electric: f64,
}

/// Steam boiler result.
#[derive(Debug, Clone, Copy)]
pub struct SteamBoilerResult {
    /// Steam generation rate (W, thermal).
    pub steam_rate: f64,
    /// Fuel consumption (W).
    pub fuel_rate: f64,
    /// Electric parasitic power (W).
    pub electric_power: f64,
    /// Part-load ratio.
    pub plr: f64,
    /// Operating efficiency.
    pub efficiency: f64,
}

impl SteamBoiler {
    pub fn new(
        name: impl Into<String>,
        capacity: f64,
        efficiency: f64,
    ) -> Self {
        Self {
            name: name.into(),
            fuel_type: BoilerFuelType::NaturalGas,
            nominal_capacity: capacity,
            nominal_efficiency: efficiency.clamp(0.01, 1.0),
            design_steam_temp: 100.0,
            parasitic_electric: 0.0,
        }
    }

    /// Calculate steam boiler performance.
    ///
    /// `load` — steam heating load (W, positive).
    pub fn calculate(
        &self,
        load: f64,
        efficiency_curve: Option<&Curve>,
    ) -> SteamBoilerResult {
        if load <= 0.0 || self.nominal_capacity <= 0.0 {
            return SteamBoilerResult {
                steam_rate: 0.0, fuel_rate: 0.0, electric_power: 0.0,
                plr: 0.0, efficiency: self.nominal_efficiency,
            };
        }

        let plr = (load / self.nominal_capacity).clamp(0.0, 1.0);
        let steam_rate = self.nominal_capacity * plr;

        let eff_modifier = match efficiency_curve {
            Some(curve) => curve.evaluate1(plr).max(0.01),
            None => 1.0,
        };
        let efficiency = (self.nominal_efficiency * eff_modifier).clamp(0.01, 1.0);
        let fuel_rate = steam_rate / efficiency;

        SteamBoilerResult {
            steam_rate,
            fuel_rate,
            electric_power: self.parasitic_electric * plr,
            plr,
            efficiency,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boiler_basic() {
        let b = Boiler::new("HW Boiler", 100_000.0, 0.80, 82.0, 2.0);
        assert!((b.nominal_capacity - 100_000.0).abs() < 1e-10);
        assert!((b.nominal_efficiency - 0.80).abs() < 1e-10);
    }

    #[test]
    fn boiler_full_load() {
        let b = Boiler::new("Test", 100_000.0, 0.80, 82.0, 2.0);
        let result = b.calculate(60.0, 2.0, 100_000.0, None);

        assert!((result.heating_rate - 100_000.0).abs() < 1.0);
        assert!((result.part_load_ratio - 1.0).abs() < 0.01);
        // Fuel = 100000 / 0.80 = 125000
        assert!((result.fuel_rate - 125_000.0).abs() < 100.0,
                "fuel={}", result.fuel_rate);
        assert!(result.outlet_temp > 60.0, "T_out={}", result.outlet_temp);
    }

    #[test]
    fn boiler_part_load() {
        let b = Boiler::new("Test", 100_000.0, 0.80, 82.0, 2.0);
        let result = b.calculate(60.0, 2.0, 50_000.0, None);

        assert!((result.part_load_ratio - 0.5).abs() < 0.01);
        assert!((result.heating_rate - 50_000.0).abs() < 1.0);
    }

    #[test]
    fn boiler_no_load() {
        let b = Boiler::new("Test", 100_000.0, 0.80, 82.0, 2.0);
        let result = b.calculate(60.0, 2.0, 0.0, None);
        assert!(result.heating_rate.abs() < 1e-10);
        assert!((result.outlet_temp - 60.0).abs() < 1e-10);
    }

    #[test]
    fn boiler_no_flow() {
        let b = Boiler::new("Test", 100_000.0, 0.80, 82.0, 2.0);
        let result = b.calculate(60.0, 0.0, 100_000.0, None);
        assert!(result.heating_rate.abs() < 1e-10);
    }

    #[test]
    fn boiler_with_efficiency_curve() {
        let b = Boiler::new("Test", 100_000.0, 0.80, 82.0, 2.0);
        // Simple linear curve: eff_modifier = 0.9 + 0.1*PLR
        let curve = Curve::linear(0.9, 0.1);
        let result = b.calculate(60.0, 2.0, 50_000.0, Some(&curve));

        // PLR = 0.5, curve value = 0.9 + 0.1*0.5 = 0.95
        // Operating eff = 0.80 * 0.95 = 0.76
        assert!((result.efficiency - 0.76).abs() < 0.01, "eff={}", result.efficiency);
        let expected_fuel = 50_000.0 / 0.76;
        assert!((result.fuel_rate - expected_fuel).abs() < 100.0,
                "fuel={}", result.fuel_rate);
    }

    #[test]
    fn boiler_outlet_temp_energy_balance() {
        let b = Boiler::new("Test", 50_000.0, 0.90, 82.0, 1.0);
        let result = b.calculate(70.0, 1.0, 50_000.0, None);

        let cp = ep_psychrometrics::cp_water(70.0);
        let expected_dt = 50_000.0 / (1.0 * cp);
        assert!((result.outlet_temp - (70.0 + expected_dt)).abs() < 0.1,
                "T_out={}, expected={}", result.outlet_temp, 70.0 + expected_dt);
    }

    #[test]
    fn boiler_parasitic() {
        let mut b = Boiler::new("Test", 100_000.0, 0.80, 82.0, 2.0);
        b.parasitic_electric = 200.0;
        let result = b.calculate(60.0, 2.0, 100_000.0, None);
        assert!((result.electric_power - 200.0).abs() < 1.0);
    }

    #[test]
    fn boiler_min_plr_clamp() {
        let mut b = Boiler::new("Test", 100_000.0, 0.80, 82.0, 2.0);
        b.min_plr = 0.2;

        // Request only 5000W = 5% of capacity, below min_plr of 0.2
        let result = b.calculate(60.0, 2.0, 5_000.0, None);

        // PLR should be clamped to min_plr = 0.2
        assert!(
            (result.part_load_ratio - 0.2).abs() < 0.01,
            "PLR={}, expected 0.2",
            result.part_load_ratio
        );
        // Heating output = capacity * min_plr = 100_000 * 0.2 = 20_000
        assert!(
            (result.heating_rate - 20_000.0).abs() < 100.0,
            "heating={}",
            result.heating_rate
        );
    }

    #[test]
    fn boiler_max_plr_clamp() {
        let b = Boiler::new("Test", 100_000.0, 0.80, 82.0, 2.0);

        // Request 200_000W, double the capacity
        let result = b.calculate(60.0, 2.0, 200_000.0, None);

        // PLR should be clamped to max_plr = 1.0
        assert!(
            (result.part_load_ratio - 1.0).abs() < 0.01,
            "PLR={}",
            result.part_load_ratio
        );
        // Output limited to nominal capacity
        assert!(
            (result.heating_rate - 100_000.0).abs() < 100.0,
            "heating={}",
            result.heating_rate
        );
    }

    #[test]
    fn boiler_efficiency_clamp() {
        // Construct with efficiency > 1.0; it gets clamped to 1.0
        let b = Boiler::new("OverEff", 100_000.0, 1.5, 82.0, 2.0);
        assert!(
            (b.nominal_efficiency - 1.0).abs() < 1e-10,
            "eff={}",
            b.nominal_efficiency
        );

        // At full load with efficiency=1.0, fuel_rate == heating_rate
        let result = b.calculate(60.0, 2.0, 100_000.0, None);
        assert!(
            (result.fuel_rate - result.heating_rate).abs() < 100.0,
            "fuel={}, heat={}",
            result.fuel_rate,
            result.heating_rate
        );
    }

    #[test]
    fn boiler_negative_load() {
        let b = Boiler::new("Test", 100_000.0, 0.80, 82.0, 2.0);

        // Negative load → zero output (early return path)
        let result = b.calculate(60.0, 2.0, -50_000.0, None);

        assert!(result.heating_rate.abs() < 1e-10, "heating={}", result.heating_rate);
        assert!(result.fuel_rate.abs() < 1e-10, "fuel={}", result.fuel_rate);
        assert!(
            (result.outlet_temp - 60.0).abs() < 1e-10,
            "T_out={}",
            result.outlet_temp
        );
        assert!(result.part_load_ratio.abs() < 1e-10);
    }

    // ====================================================================
    // Steam Boiler Tests
    // ====================================================================

    #[test]
    fn steam_boiler_basic() {
        let sb = SteamBoiler::new("STM-1", 500_000.0, 0.85);
        assert!((sb.nominal_capacity - 500_000.0).abs() < 1e-10);
    }

    #[test]
    fn steam_boiler_full_load() {
        let sb = SteamBoiler::new("STM", 500_000.0, 0.85);
        let result = sb.calculate(500_000.0, None);
        assert!((result.steam_rate - 500_000.0).abs() < 100.0);
        assert!((result.plr - 1.0).abs() < 0.01);
        // fuel = 500000 / 0.85 ≈ 588235
        assert!(result.fuel_rate > 500_000.0, "fuel={}", result.fuel_rate);
    }

    #[test]
    fn steam_boiler_part_load() {
        let sb = SteamBoiler::new("STM", 500_000.0, 0.85);
        let result = sb.calculate(250_000.0, None);
        assert!((result.plr - 0.5).abs() < 0.01);
    }

    #[test]
    fn steam_boiler_no_load() {
        let sb = SteamBoiler::new("STM", 500_000.0, 0.85);
        let result = sb.calculate(0.0, None);
        assert!(result.steam_rate.abs() < 1e-10);
        assert!(result.fuel_rate.abs() < 1e-10);
    }
}
