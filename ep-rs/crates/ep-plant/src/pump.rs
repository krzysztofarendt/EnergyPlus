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

/// Headered pumps — multiple parallel pumps staged on/off based on demand.
///
/// Pumps are identical and stage on one at a time as flow demand increases.
/// Each pump is either fully on or fully off (for constant-speed headers)
/// or the last pump modulates (for variable-speed headers).
#[derive(Debug, Clone)]
pub struct HeaderedPumps {
    pub name: String,
    /// Number of parallel pumps.
    pub num_pumps: usize,
    /// Pump type (all pumps are identical).
    pub pump_type: PumpType,
    /// Design flow rate per pump (m3/s).
    pub per_pump_flow_rate: f64,
    /// Design head (Pa) — same for all pumps.
    pub design_head: f64,
    /// Rated power per pump (W).
    pub rated_power_per_pump: f64,
    /// Motor efficiency (0-1).
    pub motor_efficiency: f64,
    /// Fraction of motor heat to fluid (0-1).
    pub motor_heat_to_fluid: f64,
    /// Part-load power coefficients for VSD [c0..c3].
    pub plf_coefficients: [f64; 4],
    /// Minimum flow fraction for VSD.
    pub min_flow_fraction: f64,
}

/// Headered pump calculation result.
#[derive(Debug, Clone, Copy)]
pub struct HeaderedPumpResult {
    /// Total power (W).
    pub power: f64,
    /// Total heat to fluid (W).
    pub heat_to_fluid: f64,
    /// Total mass flow rate (kg/s).
    pub mass_flow_rate: f64,
    /// Temperature rise (C).
    pub delta_temp: f64,
    /// Number of pumps running.
    pub pumps_running: usize,
}

impl HeaderedPumps {
    /// Create headered constant-speed pumps.
    pub fn constant_speed(
        name: impl Into<String>,
        num_pumps: usize,
        per_pump_flow_rate: f64,
        design_head: f64,
        motor_efficiency: f64,
    ) -> Self {
        let rated = if motor_efficiency > 0.0 {
            per_pump_flow_rate * design_head / motor_efficiency
        } else {
            0.0
        };
        Self {
            name: name.into(),
            num_pumps: num_pumps.max(1),
            pump_type: PumpType::ConstantSpeed,
            per_pump_flow_rate,
            design_head,
            rated_power_per_pump: rated,
            motor_efficiency,
            motor_heat_to_fluid: 1.0,
            plf_coefficients: [1.0, 0.0, 0.0, 0.0],
            min_flow_fraction: 1.0,
        }
    }

    /// Create headered variable-speed pumps.
    pub fn variable_speed(
        name: impl Into<String>,
        num_pumps: usize,
        per_pump_flow_rate: f64,
        design_head: f64,
        motor_efficiency: f64,
        min_flow_fraction: f64,
    ) -> Self {
        let rated = if motor_efficiency > 0.0 {
            per_pump_flow_rate * design_head / motor_efficiency
        } else {
            0.0
        };
        Self {
            name: name.into(),
            num_pumps: num_pumps.max(1),
            pump_type: PumpType::VariableSpeed,
            per_pump_flow_rate,
            design_head,
            rated_power_per_pump: rated,
            motor_efficiency,
            motor_heat_to_fluid: 1.0,
            plf_coefficients: [0.0015, 0.0233, -0.0506, 1.0258],
            min_flow_fraction,
        }
    }

    /// Calculate headered pump performance.
    ///
    /// Stages pumps on/off based on required flow. For constant-speed headers,
    /// each pump runs at design flow. For variable-speed, the last pump modulates.
    pub fn calculate(
        &self,
        mass_flow_rate: f64,
        fluid_density: f64,
        fluid_temp: f64,
    ) -> HeaderedPumpResult {
        if mass_flow_rate <= 1e-10 || self.rated_power_per_pump <= 0.0 {
            return HeaderedPumpResult {
                power: 0.0,
                heat_to_fluid: 0.0,
                mass_flow_rate: 0.0,
                delta_temp: 0.0,
                pumps_running: 0,
            };
        }

        let per_pump_mass_flow = self.per_pump_flow_rate * fluid_density;
        let total_design_flow = per_pump_mass_flow * self.num_pumps as f64;
        let actual_flow = mass_flow_rate.min(total_design_flow);

        // Determine how many pumps to run
        let pumps_needed = if per_pump_mass_flow > 1e-10 {
            ((actual_flow / per_pump_mass_flow).ceil() as usize).clamp(1, self.num_pumps)
        } else {
            1
        };

        let total_power;

        match self.pump_type {
            PumpType::ConstantSpeed => {
                // Each running pump at full power
                total_power = self.rated_power_per_pump * pumps_needed as f64;
            }
            PumpType::VariableSpeed => {
                // Full-speed pumps + one modulating pump
                let full_speed_pumps = if pumps_needed > 1 { pumps_needed - 1 } else { 0 };
                let remaining_flow = actual_flow - per_pump_mass_flow * full_speed_pumps as f64;
                let last_pump_plr = if per_pump_mass_flow > 1e-10 {
                    (remaining_flow / per_pump_mass_flow)
                        .clamp(self.min_flow_fraction, 1.0)
                } else {
                    self.min_flow_fraction
                };

                let c = &self.plf_coefficients;
                let frac = c[0]
                    + c[1] * last_pump_plr
                    + c[2] * last_pump_plr * last_pump_plr
                    + c[3] * last_pump_plr * last_pump_plr * last_pump_plr;

                total_power = self.rated_power_per_pump * full_speed_pumps as f64
                    + self.rated_power_per_pump * frac.max(0.0);
            }
        }

        // Heat to fluid
        let shaft = total_power * self.motor_efficiency;
        let motor_loss = total_power - shaft;
        let heat_to_fluid = shaft + motor_loss * self.motor_heat_to_fluid;

        let cp = ep_psychrometrics::cp_water(fluid_temp);
        let delta_temp = if actual_flow > 1e-10 {
            heat_to_fluid / (actual_flow * cp)
        } else {
            0.0
        };

        HeaderedPumpResult {
            power: total_power,
            heat_to_fluid,
            mass_flow_rate: actual_flow,
            delta_temp,
            pumps_running: pumps_needed,
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

    #[test]
    fn variable_speed_plr_clamped() {
        let p = Pump::variable_speed("VSD-Clamp", 0.01, 200_000.0, 0.9, 0.1);
        // design_mass_flow = 0.01 * 1000 = 10 kg/s
        // Provide flow > design → PLR should be clamped to 1.0
        let result = p.calculate(15.0, 1000.0, 7.0);

        assert!(
            (result.part_load_ratio - 1.0).abs() < 0.01,
            "PLR={}, expected 1.0 (clamped)",
            result.part_load_ratio
        );
    }

    #[test]
    fn pump_zero_motor_efficiency() {
        // Motor efficiency = 0 → rated_power = 0 → power = 0 (graceful)
        let p = Pump::constant_speed("Zero-Eff", 0.01, 200_000.0, 0.0);
        assert!(p.rated_power.abs() < 1e-10, "rated_power={}", p.rated_power);

        let result = p.calculate(10.0, 1000.0, 7.0);
        assert!(result.power.abs() < 1e-10, "power={}", result.power);
        assert!(
            result.heat_to_fluid.abs() < 1e-10,
            "heat={}",
            result.heat_to_fluid
        );
    }

    // ─── HeaderedPumps ───

    #[test]
    fn headered_cs_one_pump_at_low_flow() {
        // 3 constant-speed pumps, each 0.01 m3/s
        let hp = HeaderedPumps::constant_speed("Header", 3, 0.01, 200_000.0, 0.9);

        // Request flow that needs only 1 pump
        let result = hp.calculate(5.0, 1000.0, 7.0);
        assert_eq!(result.pumps_running, 1, "pumps={}", result.pumps_running);
    }

    #[test]
    fn headered_cs_stages_on() {
        let hp = HeaderedPumps::constant_speed("Header", 3, 0.01, 200_000.0, 0.9);
        // per_pump_mass_flow = 0.01 * 1000 = 10 kg/s

        // Low flow → 1 pump
        let r1 = hp.calculate(5.0, 1000.0, 7.0);
        assert_eq!(r1.pumps_running, 1);

        // Mid flow → 2 pumps
        let r2 = hp.calculate(15.0, 1000.0, 7.0);
        assert_eq!(r2.pumps_running, 2);

        // High flow → 3 pumps
        let r3 = hp.calculate(25.0, 1000.0, 7.0);
        assert_eq!(r3.pumps_running, 3);
    }

    #[test]
    fn headered_cs_power_scales_with_pumps() {
        let hp = HeaderedPumps::constant_speed("Header", 3, 0.01, 200_000.0, 0.9);

        let r1 = hp.calculate(5.0, 1000.0, 7.0);
        let r3 = hp.calculate(25.0, 1000.0, 7.0);

        // 3 pumps running should draw 3x the power of 1 pump
        assert!(
            (r3.power - 3.0 * r1.power).abs() < 1.0,
            "P1={}, P3={}",
            r1.power,
            r3.power
        );
    }

    #[test]
    fn headered_vsd_saves_energy() {
        let hp = HeaderedPumps::variable_speed("VSD-Header", 2, 0.01, 200_000.0, 0.9, 0.1);

        let full = hp.calculate(20.0, 1000.0, 7.0);
        let half = hp.calculate(5.0, 1000.0, 7.0);

        assert!(half.power < full.power, "full={}, half={}", full.power, half.power);
    }

    #[test]
    fn headered_no_flow() {
        let hp = HeaderedPumps::constant_speed("Header", 3, 0.01, 200_000.0, 0.9);
        let result = hp.calculate(0.0, 1000.0, 7.0);
        assert_eq!(result.pumps_running, 0);
        assert!(result.power.abs() < 1e-10);
    }

    #[test]
    fn headered_flow_capped() {
        let hp = HeaderedPumps::constant_speed("Header", 2, 0.01, 200_000.0, 0.9);
        // Request more than 2 pumps can deliver (20 kg/s)
        let result = hp.calculate(30.0, 1000.0, 7.0);
        assert_eq!(result.pumps_running, 2);
        assert!(
            (result.mass_flow_rate - 20.0).abs() < 0.1,
            "flow={}, expected 20.0",
            result.mass_flow_rate
        );
    }
}
