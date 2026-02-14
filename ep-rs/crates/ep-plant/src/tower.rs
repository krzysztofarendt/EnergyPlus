//! Cooling tower model using simplified Merkel/NTU approach.
//!
//! Supports single-speed and variable-speed towers. Calculates approach
//! temperature and heat rejection based on entering water temperature,
//! wet-bulb temperature, and airflow.

/// Cooling tower speed control type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TowerSpeedControl {
    /// Single speed: fan runs at full speed or off.
    #[default]
    SingleSpeed,
    /// Two-speed: fan runs at full or half speed.
    TwoSpeed,
    /// Variable speed: fan modulates continuously.
    VariableSpeed,
}

/// Cooling tower specification.
#[derive(Debug, Clone)]
pub struct CoolingTower {
    pub name: String,
    pub speed_control: TowerSpeedControl,
    /// Design water flow rate (kg/s).
    pub design_water_flow: f64,
    /// Design air flow rate (m3/s).
    pub design_air_flow: f64,
    /// Design fan power (W).
    pub design_fan_power: f64,
    /// UA-value at design conditions (W/K).
    pub design_ua: f64,
    /// Design inlet water temperature (C).
    pub design_inlet_temp: f64,
    /// Design wet-bulb temperature (C).
    pub design_wb: f64,
    /// Design approach (C) — T_water_out - T_wb.
    pub design_approach: f64,
    /// Design range (C) — T_water_in - T_water_out.
    pub design_range: f64,
    /// Minimum air flow ratio for variable speed.
    pub min_air_flow_ratio: f64,
}

/// Cooling tower calculation result.
#[derive(Debug, Clone, Copy)]
pub struct CoolingTowerResult {
    /// Heat rejection rate (W).
    pub heat_rejection_rate: f64,
    /// Water outlet temperature (C).
    pub outlet_water_temp: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Air flow ratio (0-1).
    pub air_flow_ratio: f64,
    /// Approach temperature (C) — outlet water - wb.
    pub approach: f64,
}

impl CoolingTower {
    pub fn new(
        name: impl Into<String>,
        design_water_flow: f64,
        design_air_flow: f64,
        design_fan_power: f64,
    ) -> Self {
        Self {
            name: name.into(),
            speed_control: TowerSpeedControl::SingleSpeed,
            design_water_flow,
            design_air_flow,
            design_fan_power,
            design_ua: 0.0,
            design_inlet_temp: 35.0,
            design_wb: 25.6,
            design_approach: 3.9,
            design_range: 5.6,
            min_air_flow_ratio: 0.2,
        }
    }

    /// Create a variable-speed cooling tower.
    pub fn variable_speed(
        name: impl Into<String>,
        design_water_flow: f64,
        design_air_flow: f64,
        design_fan_power: f64,
    ) -> Self {
        let mut tower = Self::new(name, design_water_flow, design_air_flow, design_fan_power);
        tower.speed_control = TowerSpeedControl::VariableSpeed;
        tower
    }

    /// Calculate tower performance using simplified approach.
    ///
    /// Uses a simplified Merkel method: the tower cools water from the inlet
    /// temperature toward the wet-bulb temperature, limited by effectiveness.
    pub fn calculate(
        &self,
        water_inlet_temp: f64,
        water_mass_flow: f64,
        wb_temp: f64,
        load_to_reject: f64,
    ) -> CoolingTowerResult {
        if water_mass_flow <= 1e-10 || load_to_reject <= 0.0 {
            return CoolingTowerResult {
                heat_rejection_rate: 0.0,
                outlet_water_temp: water_inlet_temp,
                fan_power: 0.0,
                air_flow_ratio: 0.0,
                approach: water_inlet_temp - wb_temp,
            };
        }

        // Maximum possible heat rejection: cool water down to wb + approach
        let cp_water = ep_psychrometrics::cp_water(water_inlet_temp);
        let min_outlet = wb_temp + 1.0; // Minimum approach ~1C
        let q_max = water_mass_flow * cp_water * (water_inlet_temp - min_outlet).max(0.0);

        if q_max <= 0.0 {
            return CoolingTowerResult {
                heat_rejection_rate: 0.0,
                outlet_water_temp: water_inlet_temp,
                fan_power: 0.0,
                air_flow_ratio: 0.0,
                approach: water_inlet_temp - wb_temp,
            };
        }

        // Determine air flow ratio needed
        let load_fraction = (load_to_reject / q_max).clamp(0.0, 1.0);

        let (air_flow_ratio, fan_power) = match self.speed_control {
            TowerSpeedControl::SingleSpeed => {
                // On-off: either full speed or off
                if load_to_reject > 0.0 {
                    (1.0, self.design_fan_power)
                } else {
                    (0.0, 0.0)
                }
            }
            TowerSpeedControl::TwoSpeed => {
                if load_fraction > 0.5 {
                    (1.0, self.design_fan_power)
                } else {
                    (0.5, self.design_fan_power * 0.125) // cube law: 0.5^3
                }
            }
            TowerSpeedControl::VariableSpeed => {
                // Fan modulates; approximate speed ratio from load
                let ratio = load_fraction.clamp(self.min_air_flow_ratio, 1.0);
                let fan_power = self.design_fan_power * ratio * ratio * ratio;
                (ratio, fan_power)
            }
        };

        // Actual heat rejection — limited by load and maximum capacity
        let q_actual = load_to_reject.min(q_max * air_flow_ratio.sqrt());

        // Outlet water temperature
        let outlet_temp = water_inlet_temp - q_actual / (water_mass_flow * cp_water);
        let approach = outlet_temp - wb_temp;

        CoolingTowerResult {
            heat_rejection_rate: q_actual,
            outlet_water_temp: outlet_temp,
            fan_power,
            air_flow_ratio,
            approach,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tower_basic() {
        let t = CoolingTower::new("CT-1", 50.0, 30.0, 15000.0);
        assert!((t.design_fan_power - 15000.0).abs() < 1e-10);
        assert_eq!(t.speed_control, TowerSpeedControl::SingleSpeed);
    }

    #[test]
    fn tower_full_load() {
        let t = CoolingTower::new("CT", 50.0, 30.0, 15000.0);
        let result = t.calculate(35.0, 50.0, 25.0, 500_000.0);

        assert!(result.heat_rejection_rate > 0.0);
        assert!(result.outlet_water_temp < 35.0, "T_out={}", result.outlet_water_temp);
        assert!(result.outlet_water_temp > 25.0, "T_out={}", result.outlet_water_temp);
        assert!((result.fan_power - 15000.0).abs() < 1.0);
    }

    #[test]
    fn tower_no_load() {
        let t = CoolingTower::new("CT", 50.0, 30.0, 15000.0);
        let result = t.calculate(35.0, 50.0, 25.0, 0.0);
        assert!(result.heat_rejection_rate.abs() < 1e-10);
        assert!((result.outlet_water_temp - 35.0).abs() < 1e-10);
        assert!(result.fan_power.abs() < 1e-10);
    }

    #[test]
    fn tower_no_flow() {
        let t = CoolingTower::new("CT", 50.0, 30.0, 15000.0);
        let result = t.calculate(35.0, 0.0, 25.0, 500_000.0);
        assert!(result.heat_rejection_rate.abs() < 1e-10);
    }

    #[test]
    fn tower_approach_positive() {
        let t = CoolingTower::new("CT", 50.0, 30.0, 15000.0);
        let result = t.calculate(35.0, 50.0, 25.0, 300_000.0);
        assert!(result.approach > 0.0, "approach={}", result.approach);
    }

    #[test]
    fn variable_speed_saves_energy() {
        let t = CoolingTower::variable_speed("CT-VS", 50.0, 30.0, 15000.0);
        let result_full = t.calculate(35.0, 50.0, 25.0, 500_000.0);
        let result_part = t.calculate(35.0, 50.0, 25.0, 100_000.0);

        assert!(result_part.fan_power < result_full.fan_power,
                "full={}, part={}", result_full.fan_power, result_part.fan_power);
    }

    #[test]
    fn two_speed_steps() {
        let mut t = CoolingTower::new("CT-2S", 50.0, 30.0, 15000.0);
        t.speed_control = TowerSpeedControl::TwoSpeed;

        // Use high enough load to exceed 50% load fraction
        let result_high = t.calculate(35.0, 50.0, 25.0, 1_500_000.0);
        let result_low = t.calculate(35.0, 50.0, 25.0, 50_000.0);

        // High load: full speed power
        assert!((result_high.fan_power - 15000.0).abs() < 1.0,
                "P_high={}", result_high.fan_power);
        // Low load: half speed power = 0.125 * design
        assert!((result_low.fan_power - 15000.0 * 0.125).abs() < 1.0,
                "P_low={}", result_low.fan_power);
    }

    #[test]
    fn tower_water_temp_near_wetbulb() {
        // When inlet water temp is close to wetbulb, approach is small
        // and heat rejection should be small
        let t = CoolingTower::new("CT-Small", 50.0, 30.0, 15000.0);
        let wb = 25.0;
        // Inlet only 1.5 C above wetbulb + min_approach (1.0 C)
        // min_outlet = wb + 1.0 = 26.0, inlet = 27.5
        // q_max = mdot * cp * (27.5 - 26.0) = 50 * 4180 * 1.5 = 313500
        let result = t.calculate(27.5, 50.0, wb, 100_000.0);

        // Heat rejection should be positive but limited
        assert!(result.heat_rejection_rate > 0.0, "Q={}", result.heat_rejection_rate);
        // Approach should be small and positive
        assert!(result.approach > 0.0, "approach={}", result.approach);
        assert!(result.approach < 5.0, "approach too big={}", result.approach);
    }

    #[test]
    fn tower_energy_balance() {
        // Verify outlet = inlet - q/(mdot*cp)
        let t = CoolingTower::new("CT-Balance", 50.0, 30.0, 15000.0);
        let inlet_temp = 35.0;
        let mdot = 50.0;
        let wb = 25.0;
        let load = 300_000.0;

        let result = t.calculate(inlet_temp, mdot, wb, load);

        let cp = ep_psychrometrics::cp_water(inlet_temp);
        let expected_outlet = inlet_temp - result.heat_rejection_rate / (mdot * cp);

        assert!(
            (result.outlet_water_temp - expected_outlet).abs() < 0.01,
            "T_out={}, expected={}",
            result.outlet_water_temp,
            expected_outlet
        );
    }

    #[test]
    fn tower_variable_speed_min_airflow() {
        let mut t = CoolingTower::variable_speed("CT-VS-Min", 50.0, 30.0, 15000.0);
        t.min_air_flow_ratio = 0.3;

        // Very small load → air_flow_ratio should be clamped to min_air_flow_ratio
        let result = t.calculate(35.0, 50.0, 25.0, 1.0);

        assert!(
            result.air_flow_ratio >= 0.3 - 1e-10,
            "air_flow_ratio={}, expected >= 0.3",
            result.air_flow_ratio
        );
        // Fan power at min ratio: design_power * 0.3^3 = 15000 * 0.027 = 405
        let expected_power = 15000.0 * 0.3_f64.powi(3);
        assert!(
            (result.fan_power - expected_power).abs() < 1.0,
            "fan_power={}, expected={}",
            result.fan_power,
            expected_power
        );
    }
}
