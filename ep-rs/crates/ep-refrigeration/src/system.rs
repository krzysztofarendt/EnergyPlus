//! Refrigeration system — assembles compressors, condenser, cases, and walk-ins.
//!
//! Coordinates the energy balance: total case load → compressor power
//! → condenser heat rejection.

use crate::case::{DisplayCase, CaseResult};
use crate::compressor::Compressor;
use crate::condenser::Condenser;
use crate::walkin::{WalkIn, WalkInResult};

/// Complete refrigeration system (compressor rack).
#[derive(Debug, Clone)]
pub struct RefrigerationSystem {
    pub name: String,
    /// Suction (evaporating) temperature (C).
    pub suction_temp: f64,
    pub compressors: Vec<Compressor>,
    pub condenser: Condenser,
    pub cases: Vec<DisplayCase>,
    pub walkins: Vec<WalkIn>,
}

/// System-level result.
#[derive(Debug, Clone, Default)]
pub struct SystemResult {
    /// Total case cooling load (W).
    pub total_case_load: f64,
    /// Total walk-in cooling load (W).
    pub total_walkin_load: f64,
    /// Total refrigeration load (W).
    pub total_load: f64,
    /// Total compressor power (W).
    pub compressor_power: f64,
    /// Total compressor COP.
    pub system_cop: f64,
    /// Condenser heat rejection (W).
    pub condenser_heat_rejection: f64,
    /// Condenser fan power (W).
    pub condenser_power: f64,
    /// Condensing temperature (C).
    pub condensing_temp: f64,
    /// Individual case results.
    pub case_results: Vec<CaseResult>,
    /// Individual walk-in results.
    pub walkin_results: Vec<WalkInResult>,
}

impl RefrigerationSystem {
    pub fn new(name: impl Into<String>, suction_temp: f64, condenser: Condenser) -> Self {
        Self {
            name: name.into(),
            suction_temp,
            compressors: Vec::new(),
            condenser,
            cases: Vec::new(),
            walkins: Vec::new(),
        }
    }

    /// Run the full system calculation.
    ///
    /// `zone_temp` — zone temperature for cases/walk-ins (C).
    /// `zone_rh` — zone relative humidity (fraction).
    /// `ground_temp` — ground temperature for walk-ins (C).
    /// `ambient_db` — outdoor dry-bulb for condenser (C).
    /// `ambient_wb` — outdoor wet-bulb for condenser (C).
    pub fn calculate(
        &self,
        zone_temp: f64,
        zone_rh: f64,
        ground_temp: f64,
        ambient_db: f64,
        ambient_wb: f64,
    ) -> SystemResult {
        // Calculate case loads
        let case_results: Vec<CaseResult> = self.cases.iter()
            .map(|c| c.calculate(zone_temp, zone_rh))
            .collect();
        let total_case_load: f64 = case_results.iter().map(|r| r.total_cooling_load).sum();

        // Calculate walk-in loads
        let walkin_results: Vec<WalkInResult> = self.walkins.iter()
            .map(|w| w.calculate(zone_temp, ground_temp))
            .collect();
        let total_walkin_load: f64 = walkin_results.iter().map(|r| r.total_cooling_load).sum();

        let total_load = total_case_load + total_walkin_load;

        if total_load <= 0.0 || self.compressors.is_empty() {
            return SystemResult {
                total_case_load,
                total_walkin_load,
                total_load,
                case_results,
                walkin_results,
                ..Default::default()
            };
        }

        // First pass: get condensing temperature from condenser
        // Estimate heat rejection = load + compressor power (iterate once)
        let estimated_compressor_power = total_load / 3.0; // Rough COP ~3
        let estimated_rejection = total_load + estimated_compressor_power;
        let condenser_result = self.condenser.calculate(estimated_rejection, ambient_db, ambient_wb);

        // Distribute load across compressors
        let total_compressor_capacity: f64 = self.compressors.iter()
            .map(|c| c.rated_capacity)
            .sum();
        let mut total_compressor_power = 0.0;

        for comp in &self.compressors {
            let share = if total_compressor_capacity > 0.0 {
                comp.rated_capacity / total_compressor_capacity
            } else {
                1.0 / self.compressors.len() as f64
            };
            let comp_load = total_load * share;
            let plr = (comp_load / comp.rated_capacity).min(1.0);
            let result = comp.calculate(
                self.suction_temp,
                condenser_result.condensing_temp,
                plr,
            );
            total_compressor_power += result.power;
        }

        let actual_rejection = total_load + total_compressor_power;
        let system_cop = if total_compressor_power > 0.0 {
            total_load / total_compressor_power
        } else {
            0.0
        };

        SystemResult {
            total_case_load,
            total_walkin_load,
            total_load,
            compressor_power: total_compressor_power,
            system_cop,
            condenser_heat_rejection: actual_rejection,
            condenser_power: condenser_result.total_power,
            condensing_temp: condenser_result.condensing_temp,
            case_results,
            walkin_results,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::condenser::Condenser;

    #[test]
    fn system_basic() {
        let condenser = Condenser::air_cooled("Cond-1", 50000.0);
        let mut sys = RefrigerationSystem::new("Rack-1", -6.7, condenser);
        sys.compressors.push(Compressor::new("Comp-1", 15000.0, 5000.0));
        sys.cases.push(DisplayCase::new("Case-1", 3.0, -5.0));

        let result = sys.calculate(24.0, 0.55, 18.0, 25.0, 20.0);

        assert!(result.total_case_load > 0.0);
        assert!(result.compressor_power > 0.0);
        assert!(result.system_cop > 0.0);
        assert!(result.condenser_heat_rejection > result.total_load);
    }

    #[test]
    fn system_with_walkin() {
        let condenser = Condenser::air_cooled("Cond-1", 80000.0);
        let mut sys = RefrigerationSystem::new("Rack-1", -6.7, condenser);
        sys.compressors.push(Compressor::new("Comp-1", 20000.0, 6000.0));
        sys.cases.push(DisplayCase::new("Case-1", 3.0, -5.0));
        sys.walkins.push(WalkIn::new("WI-1", 5000.0, 2.0));

        let result = sys.calculate(24.0, 0.55, 18.0, 25.0, 20.0);

        assert!(result.total_walkin_load > 0.0);
        assert!(result.total_load > result.total_case_load);
    }

    #[test]
    fn system_energy_balance() {
        let condenser = Condenser::air_cooled("Cond-1", 50000.0);
        let mut sys = RefrigerationSystem::new("Rack-1", -6.7, condenser);
        sys.compressors.push(Compressor::new("Comp-1", 15000.0, 5000.0));
        sys.cases.push(DisplayCase::new("Case-1", 3.0, -5.0));

        let result = sys.calculate(24.0, 0.55, 18.0, 25.0, 20.0);

        // Heat rejection = cooling load + compressor work
        let expected_rejection = result.total_load + result.compressor_power;
        assert!((result.condenser_heat_rejection - expected_rejection).abs() < 1.0);
    }

    #[test]
    fn system_no_load() {
        let condenser = Condenser::air_cooled("Cond-1", 50000.0);
        let sys = RefrigerationSystem::new("Rack-1", -6.7, condenser);

        let result = sys.calculate(24.0, 0.55, 18.0, 25.0, 20.0);
        assert!(result.total_load.abs() < 1e-10);
        assert!(result.compressor_power.abs() < 1e-10);
    }
}
