//! Pump models for plant and condenser water loops.
//!
//! Supports constant-speed and variable-speed pumps. Power is calculated
//! from rated conditions with part-load power curves. Motor heat is
//! added to the fluid.

/// Pump type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpType {
    /// Constant speed: runs at design flow whenever on.
    ConstantSpeed,
    /// Variable speed: modulates flow with VFD or bypass.
    VariableSpeed,
}

/// Pump specification.
#[derive(Debug, Clone)]
pub struct Pump {
    pub name: String,
    pub pump_type: PumpType,
    /// Design volume flow rate (m3/s).
    pub design_flow_rate: f64,
    /// Design pump head (Pa).
    pub design_head: f64,
    /// Design (rated) power consumption (W).
    pub rated_power: f64,
    /// Motor efficiency (0-1).
    pub motor_efficiency: f64,
    /// Fraction of motor heat to fluid (0-1).
    pub motor_heat_to_fluid: f64,
    /// Part-load power coefficients [c0, c1, c2, c3] for variable speed.
    /// FracFullLoadPower = c0 + c1*PLR + c2*PLR^2 + c3*PLR^3
    pub plf_coefficients: [f64; 4],
    /// Minimum flow fraction for variable-speed pumps.
    pub min_flow_fraction: f64,
}

/// Pump calculation result.
#[derive(Debug, Clone, Copy)]
pub struct PumpResult {
    /// Electrical power consumption (W).
    pub power: f64,
    /// Heat added to fluid (W).
    pub heat_to_fluid: f64,
    /// Flow rate through pump (kg/s).
    pub mass_flow_rate: f64,
    /// Fluid temperature rise (C).
    pub delta_temp: f64,
    /// Part-load ratio.
    pub part_load_ratio: f64,
}

impl Pump {
    /// Create a constant-speed pump.
    pub fn constant_speed(
        name: impl Into<String>,
        design_flow_rate: f64,
        design_head: f64,
        motor_efficiency: f64,
    ) -> Self {
        // Rated power = V_dot * Head / eta_motor
        let rated_power = if motor_efficiency > 0.0 {
            design_flow_rate * design_head / motor_efficiency
        } else {
            0.0
        };

        Self {
            name: name.into(),
            pump_type: PumpType::ConstantSpeed,
            design_flow_rate,
            design_head,
            rated_power,
            motor_efficiency,
            motor_heat_to_fluid: 1.0,
            plf_coefficients: [1.0, 0.0, 0.0, 0.0],
            min_flow_fraction: 1.0,
        }
    }

    /// Create a variable-speed pump.
    pub fn variable_speed(
        name: impl Into<String>,
        design_flow_rate: f64,
        design_head: f64,
        motor_efficiency: f64,
        min_flow_fraction: f64,
    ) -> Self {
        let rated_power = if motor_efficiency > 0.0 {
            design_flow_rate * design_head / motor_efficiency
        } else {
            0.0
        };

        Self {
            name: name.into(),
            pump_type: PumpType::VariableSpeed,
            design_flow_rate,
            design_head,
            rated_power,
            motor_efficiency,
            motor_heat_to_fluid: 1.0,
            // Default VSD coefficients (ASHRAE 90.1 curve)
            plf_coefficients: [0.0015, 0.0233, -0.0506, 1.0258],
            min_flow_fraction,
        }
    }

    /// Calculate pump performance.
    pub fn calculate(
        &self,
        mass_flow_rate: f64,
        fluid_density: f64,
        fluid_temp: f64,
    ) -> PumpResult {
        if mass_flow_rate <= 1e-10 || self.rated_power <= 0.0 {
            return PumpResult {
                power: 0.0,
                heat_to_fluid: 0.0,
                mass_flow_rate: 0.0,
                delta_temp: 0.0,
                part_load_ratio: 0.0,
            };
        }

        let design_mass_flow = self.design_flow_rate * fluid_density;
        let plr = if design_mass_flow > 1e-10 {
            (mass_flow_rate / design_mass_flow).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let power = match self.pump_type {
            PumpType::ConstantSpeed => {
                // Constant speed: full power when on
                self.rated_power
            }
            PumpType::VariableSpeed => {
                let plr_used = plr.max(self.min_flow_fraction);
                let c = &self.plf_coefficients;
                let frac = c[0] + c[1] * plr_used + c[2] * plr_used * plr_used
                    + c[3] * plr_used * plr_used * plr_used;
                self.rated_power * frac.max(0.0)
            }
        };

        // Heat to fluid
        let shaft_power = power * self.motor_efficiency;
        let motor_loss = power - shaft_power;
        let heat_to_fluid = shaft_power + motor_loss * self.motor_heat_to_fluid;

        // Temperature rise
        let cp = ep_psychrometrics::cp_water(fluid_temp);
        let delta_temp = if mass_flow_rate > 1e-10 {
            heat_to_fluid / (mass_flow_rate * cp)
        } else {
            0.0
        };

        PumpResult {
            power,
            heat_to_fluid,
            mass_flow_rate,
            delta_temp,
            part_load_ratio: plr,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_speed_pump_basic() {
        let p = Pump::constant_speed("CHWP", 0.01, 200_000.0, 0.9);
        // Rated power = 0.01 * 200000 / 0.9 = 2222.2 W
        assert!((p.rated_power - 2222.2).abs() < 1.0, "P={}", p.rated_power);
    }

    #[test]
    fn constant_speed_pump_on() {
        let p = Pump::constant_speed("Test", 0.01, 200_000.0, 0.9);
        let result = p.calculate(10.0, 1000.0, 7.0);
        // At any flow rate, constant speed = rated power
        assert!((result.power - p.rated_power).abs() < 1.0);
        assert!(result.heat_to_fluid > 0.0);
        assert!(result.delta_temp > 0.0);
    }

    #[test]
    fn pump_no_flow() {
        let p = Pump::constant_speed("Test", 0.01, 200_000.0, 0.9);
        let result = p.calculate(0.0, 1000.0, 7.0);
        assert!(result.power.abs() < 1e-10);
        assert!(result.delta_temp.abs() < 1e-10);
    }

    #[test]
    fn variable_speed_saves_energy() {
        let p = Pump::variable_speed("VSD", 0.01, 200_000.0, 0.9, 0.1);

        let full = p.calculate(10.0, 1000.0, 7.0);
        let half = p.calculate(5.0, 1000.0, 7.0);

        assert!(half.power < full.power, "full={}, half={}", full.power, half.power);
        assert!(half.power > 0.0);
    }

    #[test]
    fn variable_speed_min_flow() {
        let p = Pump::variable_speed("VSD", 0.01, 200_000.0, 0.9, 0.3);

        // Very low flow
        let result = p.calculate(0.5, 1000.0, 7.0);
        // Should still consume some power (min flow fraction)
        assert!(result.power > 0.0);
    }

    #[test]
    fn pump_heat_to_fluid() {
        let p = Pump::constant_speed("Test", 0.01, 200_000.0, 0.9);
        let result = p.calculate(10.0, 1000.0, 20.0);

        // motor_heat_to_fluid = 1.0, so all power goes to fluid
        // heat = shaft_power + motor_loss * 1.0 = power
        assert!((result.heat_to_fluid - result.power).abs() < 1.0,
                "heat={}, power={}", result.heat_to_fluid, result.power);
    }

    #[test]
    fn pump_partial_motor_heat() {
        let mut p = Pump::constant_speed("Test", 0.01, 200_000.0, 0.9);
        p.motor_heat_to_fluid = 0.0; // motor outside fluid

        let result = p.calculate(10.0, 1000.0, 20.0);
        // Only shaft power goes to fluid
        let shaft = result.power * 0.9;
        assert!((result.heat_to_fluid - shaft).abs() < 1.0,
                "heat={}, shaft={}", result.heat_to_fluid, shaft);
    }

    #[test]
    fn pump_temp_rise_energy_balance() {
        let p = Pump::constant_speed("Test", 0.01, 200_000.0, 0.9);
        let mdot = 10.0;
        let t_in = 15.0;
        let result = p.calculate(mdot, 1000.0, t_in);

        let cp = ep_psychrometrics::cp_water(t_in);
        let expected_dt = result.heat_to_fluid / (mdot * cp);
        assert!((result.delta_temp - expected_dt).abs() < 0.001,
                "dt={}, expected={}", result.delta_temp, expected_dt);
    }
}
