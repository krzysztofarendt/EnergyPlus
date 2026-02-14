//! Combustion generator models.
//!
//! Internal combustion engine, combustion turbine, and micro-CHP.
//! Each generator converts fuel to electricity with optional heat recovery.

/// Generator fuel type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuelType {
    NaturalGas,
    Diesel,
    Gasoline,
    Propane,
    Hydrogen,
}

/// Generator type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratorType {
    /// Internal combustion engine.
    ICEngine,
    /// Combustion turbine.
    CombustionTurbine,
    /// Micro combined heat and power.
    MicroCHP,
}

/// Generator specification.
#[derive(Debug, Clone)]
pub struct Generator {
    pub name: String,
    pub generator_type: GeneratorType,
    pub fuel_type: FuelType,
    /// Rated electrical power output (W).
    pub rated_power: f64,
    /// Minimum part-load ratio.
    pub min_part_load: f64,
    /// Maximum part-load ratio.
    pub max_part_load: f64,
    /// Electrical efficiency at rated conditions.
    pub rated_electrical_efficiency: f64,
    /// Thermal efficiency (heat recovery fraction of fuel input).
    pub rated_thermal_efficiency: f64,
    /// Fuel higher heating value (J/kg).
    pub fuel_hhv: f64,
}

/// Generator calculation result.
#[derive(Debug, Clone, Copy)]
pub struct GeneratorResult {
    /// Electrical power produced (W).
    pub electrical_power: f64,
    /// Fuel consumption rate (W, thermal).
    pub fuel_rate: f64,
    /// Fuel mass flow (kg/s).
    pub fuel_mass_flow: f64,
    /// Recoverable thermal power (W).
    pub thermal_power: f64,
    /// Exhaust temperature (C).
    pub exhaust_temp: f64,
    /// Part-load ratio.
    pub part_load_ratio: f64,
    /// Electrical efficiency at operating point.
    pub electrical_efficiency: f64,
}

impl Generator {
    /// Create a natural gas IC engine generator.
    pub fn ic_engine(name: impl Into<String>, rated_power: f64) -> Self {
        Self {
            name: name.into(),
            generator_type: GeneratorType::ICEngine,
            fuel_type: FuelType::NaturalGas,
            rated_power,
            min_part_load: 0.3,
            max_part_load: 1.0,
            rated_electrical_efficiency: 0.35,
            rated_thermal_efficiency: 0.40,
            fuel_hhv: 50_000_000.0, // ~50 MJ/kg for natural gas
        }
    }

    /// Create a combustion turbine generator.
    pub fn combustion_turbine(name: impl Into<String>, rated_power: f64) -> Self {
        Self {
            name: name.into(),
            generator_type: GeneratorType::CombustionTurbine,
            fuel_type: FuelType::NaturalGas,
            rated_power,
            min_part_load: 0.2,
            max_part_load: 1.0,
            rated_electrical_efficiency: 0.30,
            rated_thermal_efficiency: 0.45,
            fuel_hhv: 50_000_000.0,
        }
    }

    /// Create a micro-CHP unit.
    pub fn micro_chp(name: impl Into<String>, rated_power: f64) -> Self {
        Self {
            name: name.into(),
            generator_type: GeneratorType::MicroCHP,
            fuel_type: FuelType::NaturalGas,
            rated_power,
            min_part_load: 0.4,
            max_part_load: 1.0,
            rated_electrical_efficiency: 0.25,
            rated_thermal_efficiency: 0.55,
            fuel_hhv: 50_000_000.0,
        }
    }

    /// Calculate generator output.
    ///
    /// `power_request` — desired electrical output (W).
    /// `ambient_temp` — outdoor temperature (C), affects CT efficiency.
    pub fn calculate(&self, power_request: f64, _ambient_temp: f64) -> GeneratorResult {
        if power_request <= 0.0 {
            return GeneratorResult {
                electrical_power: 0.0,
                fuel_rate: 0.0,
                fuel_mass_flow: 0.0,
                thermal_power: 0.0,
                exhaust_temp: 0.0,
                part_load_ratio: 0.0,
                electrical_efficiency: 0.0,
            };
        }

        let plr = (power_request / self.rated_power).clamp(self.min_part_load, self.max_part_load);
        let electrical_power = self.rated_power * plr;

        // Part-load efficiency correction (simplified quadratic)
        let eff_correction = 1.0 - 0.1 * (1.0 - plr).powi(2);
        let electrical_efficiency = self.rated_electrical_efficiency * eff_correction;

        let fuel_rate = electrical_power / electrical_efficiency;
        let fuel_mass_flow = fuel_rate / self.fuel_hhv;
        let thermal_power = fuel_rate * self.rated_thermal_efficiency;

        // Simplified exhaust temperature (higher at full load)
        let exhaust_temp = match self.generator_type {
            GeneratorType::ICEngine => 400.0 + 100.0 * plr,
            GeneratorType::CombustionTurbine => 450.0 + 150.0 * plr,
            GeneratorType::MicroCHP => 200.0 + 50.0 * plr,
        };

        GeneratorResult {
            electrical_power,
            fuel_rate,
            fuel_mass_flow,
            thermal_power,
            exhaust_temp,
            part_load_ratio: plr,
            electrical_efficiency,
        }
    }
}

/// DC-to-AC inverter for power conversion.
#[derive(Debug, Clone)]
pub struct Inverter {
    pub name: String,
    /// Rated power (W).
    pub rated_power: f64,
    /// Efficiency (0-1), can be fixed or curve-based.
    pub efficiency: f64,
}

impl Inverter {
    pub fn new(name: impl Into<String>, rated_power: f64, efficiency: f64) -> Self {
        Self {
            name: name.into(),
            rated_power,
            efficiency,
        }
    }

    /// Convert DC power to AC power.
    pub fn convert(&self, dc_power: f64) -> (f64, f64) {
        let dc_in = dc_power.min(self.rated_power).max(0.0);
        let ac_out = dc_in * self.efficiency;
        let loss = dc_in - ac_out;
        (ac_out, loss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ic_engine_full_load() {
        let gen = Generator::ic_engine("ICE-1", 100_000.0);
        let result = gen.calculate(100_000.0, 25.0);
        assert!((result.electrical_power - 100_000.0).abs() < 100.0);
        assert!((result.part_load_ratio - 1.0).abs() < 0.01);
        assert!(result.fuel_rate > result.electrical_power); // Fuel > elec (eff < 1)
        assert!(result.thermal_power > 0.0);
        assert!(result.exhaust_temp > 400.0);
    }

    #[test]
    fn ic_engine_part_load() {
        let gen = Generator::ic_engine("ICE", 100_000.0);
        let result = gen.calculate(50_000.0, 25.0);
        assert!((result.part_load_ratio - 0.5).abs() < 0.01);
        assert!(result.electrical_efficiency > 0.0);
        assert!(result.electrical_efficiency < gen.rated_electrical_efficiency * 1.01);
    }

    #[test]
    fn generator_below_min_load() {
        let gen = Generator::ic_engine("ICE", 100_000.0); // min PLR = 0.3
        let result = gen.calculate(10_000.0, 25.0); // 10% requested
        assert!((result.part_load_ratio - 0.3).abs() < 0.01); // Clamped to 30%
    }

    #[test]
    fn generator_off() {
        let gen = Generator::ic_engine("ICE", 100_000.0);
        let result = gen.calculate(0.0, 25.0);
        assert!(result.electrical_power.abs() < 1e-10);
        assert!(result.fuel_rate.abs() < 1e-10);
    }

    #[test]
    fn combustion_turbine() {
        let gen = Generator::combustion_turbine("CT", 500_000.0);
        let result = gen.calculate(500_000.0, 25.0);
        assert!(result.electrical_power > 0.0);
        assert!(result.thermal_power > 0.0);
        assert!(result.exhaust_temp > 500.0);
    }

    #[test]
    fn micro_chp_high_thermal() {
        let gen = Generator::micro_chp("CHP", 5_000.0);
        let result = gen.calculate(5_000.0, 25.0);
        // Micro-CHP: thermal > electrical
        assert!(result.thermal_power > result.electrical_power,
                "thermal={}W, elec={}W", result.thermal_power, result.electrical_power);
    }

    #[test]
    fn energy_balance() {
        let gen = Generator::ic_engine("ICE", 100_000.0);
        let result = gen.calculate(80_000.0, 25.0);
        let total_output = result.electrical_power + result.thermal_power;
        // Total useful output should be less than fuel input
        assert!(total_output < result.fuel_rate * 1.01,
                "output={}W, fuel={}W", total_output, result.fuel_rate);
    }

    #[test]
    fn inverter_conversion() {
        let inv = Inverter::new("Inv-1", 10_000.0, 0.96);
        let (ac, loss) = inv.convert(5_000.0);
        assert!((ac - 4800.0).abs() < 1.0);
        assert!((loss - 200.0).abs() < 1.0);
    }

    #[test]
    fn inverter_over_rated() {
        let inv = Inverter::new("Inv", 5000.0, 0.96);
        let (ac, _loss) = inv.convert(10_000.0);
        assert!(ac <= 5000.0 * 0.96 + 1.0);
    }
}
