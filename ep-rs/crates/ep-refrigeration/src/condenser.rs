//! Condenser models for refrigeration systems.
//!
//! Supports air-cooled, evaporatively-cooled, and water-cooled condensers.

/// Condenser type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CondenserType {
    AirCooled,
    EvaporativelyCooled,
    WaterCooled,
}

/// Condenser fan speed control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanSpeedControl {
    Fixed,
    TwoSpeed,
    VariableSpeed,
}

/// Refrigeration condenser.
#[derive(Debug, Clone)]
pub struct Condenser {
    pub name: String,
    pub condenser_type: CondenserType,
    /// Rated heat rejection capacity (W).
    pub rated_capacity: f64,
    /// Rated condensing temperature (C).
    pub rated_condensing_temp: f64,
    /// Rated ambient temperature or entering water temp (C).
    pub rated_ambient_temp: f64,
    /// Rated fan power (W).
    pub rated_fan_power: f64,
    /// Fan speed control.
    pub fan_control: FanSpeedControl,
    /// Minimum condensing temperature (C).
    pub min_condensing_temp: f64,
    /// Approach temperature (K) — Tcond - Tambient at rated.
    pub rated_approach: f64,
    /// Evaporative effectiveness (for evap-cooled, 0-1).
    pub evap_effectiveness: f64,
    /// Pump power for evaporative cooler (W).
    pub evap_pump_power: f64,
}

/// Condenser calculation result.
#[derive(Debug, Clone, Copy, Default)]
pub struct CondenserResult {
    /// Actual condensing temperature (C).
    pub condensing_temp: f64,
    /// Heat rejected (W).
    pub heat_rejected: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Evaporative pump power (W).
    pub pump_power: f64,
    /// Total electric power (W).
    pub total_power: f64,
}

impl Condenser {
    pub fn air_cooled(name: impl Into<String>, rated_capacity: f64) -> Self {
        Self {
            name: name.into(),
            condenser_type: CondenserType::AirCooled,
            rated_capacity,
            rated_condensing_temp: 35.0,
            rated_ambient_temp: 25.0,
            rated_fan_power: rated_capacity * 0.015, // ~1.5% of capacity
            fan_control: FanSpeedControl::VariableSpeed,
            min_condensing_temp: 10.0,
            rated_approach: 10.0,
            evap_effectiveness: 0.0,
            evap_pump_power: 0.0,
        }
    }

    pub fn evap_cooled(name: impl Into<String>, rated_capacity: f64) -> Self {
        Self {
            name: name.into(),
            condenser_type: CondenserType::EvaporativelyCooled,
            rated_capacity,
            rated_condensing_temp: 29.4,
            rated_ambient_temp: 23.9,
            rated_fan_power: rated_capacity * 0.01,
            fan_control: FanSpeedControl::VariableSpeed,
            min_condensing_temp: 10.0,
            rated_approach: 5.5,
            evap_effectiveness: 0.9,
            evap_pump_power: 100.0,
        }
    }

    /// Calculate condenser performance.
    ///
    /// `heat_rejection_load` — total heat to reject from compressors (W).
    /// `ambient_db` — outdoor dry-bulb temperature (C).
    /// `ambient_wb` — outdoor wet-bulb temperature (C).
    pub fn calculate(
        &self,
        heat_rejection_load: f64,
        ambient_db: f64,
        ambient_wb: f64,
    ) -> CondenserResult {
        if heat_rejection_load <= 0.0 {
            return CondenserResult::default();
        }

        // Effective ambient for condenser
        let effective_ambient = match self.condenser_type {
            CondenserType::AirCooled => ambient_db,
            CondenserType::EvaporativelyCooled => {
                // Evaporative cooling approaches wet-bulb
                ambient_db - self.evap_effectiveness * (ambient_db - ambient_wb)
            }
            CondenserType::WaterCooled => ambient_db, // Simplified
        };

        // Condensing temperature = ambient + approach, scaled by load fraction
        let load_fraction = (heat_rejection_load / self.rated_capacity).min(1.0);
        let approach = self.rated_approach * load_fraction;
        let condensing_temp = (effective_ambient + approach).max(self.min_condensing_temp);

        // Fan power scales with load
        let fan_fraction = match self.fan_control {
            FanSpeedControl::Fixed => 1.0,
            FanSpeedControl::TwoSpeed => {
                if load_fraction > 0.5 { 1.0 } else { 0.5 }
            }
            FanSpeedControl::VariableSpeed => load_fraction.max(0.1),
        };
        let fan_power = self.rated_fan_power * fan_fraction;

        let pump_power = match self.condenser_type {
            CondenserType::EvaporativelyCooled => self.evap_pump_power,
            _ => 0.0,
        };

        CondenserResult {
            condensing_temp,
            heat_rejected: heat_rejection_load,
            fan_power,
            pump_power,
            total_power: fan_power + pump_power,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_cooled_at_rated() {
        let cond = Condenser::air_cooled("Cond-1", 50000.0);
        let result = cond.calculate(50000.0, 25.0, 20.0);

        assert!((result.condensing_temp - 35.0).abs() < 0.5);
        assert!(result.fan_power > 0.0);
        assert!((result.heat_rejected - 50000.0).abs() < 1.0);
    }

    #[test]
    fn air_cooled_part_load() {
        let cond = Condenser::air_cooled("Cond-1", 50000.0);
        let full = cond.calculate(50000.0, 25.0, 20.0);
        let part = cond.calculate(25000.0, 25.0, 20.0);

        // Part load: lower condensing temp, less fan power
        assert!(part.condensing_temp < full.condensing_temp);
        assert!(part.fan_power < full.fan_power);
    }

    #[test]
    fn evap_cooled_lower_condensing() {
        let air = Condenser::air_cooled("Air", 50000.0);
        let evap = Condenser::evap_cooled("Evap", 50000.0);

        let air_result = air.calculate(50000.0, 35.0, 25.0);
        let evap_result = evap.calculate(50000.0, 35.0, 25.0);

        // Evap-cooled should have lower condensing temp
        assert!(evap_result.condensing_temp < air_result.condensing_temp);
    }

    #[test]
    fn condenser_min_temp() {
        let cond = Condenser::air_cooled("Cond-1", 50000.0);
        let result = cond.calculate(5000.0, -10.0, -15.0);

        // Should not go below minimum condensing temp
        assert!(result.condensing_temp >= cond.min_condensing_temp);
    }

    #[test]
    fn condenser_zero_load() {
        let cond = Condenser::air_cooled("Cond-1", 50000.0);
        let result = cond.calculate(0.0, 25.0, 20.0);
        assert!(result.total_power.abs() < 1e-10);
    }
}
