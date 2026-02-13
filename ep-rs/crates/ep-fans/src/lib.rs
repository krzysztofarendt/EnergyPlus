//! Fan models for EnergyPlus-rs.
//!
//! Implements constant volume, variable volume (VAV), on-off, and
//! component-model fans. Fan power is calculated from pressure rise,
//! efficiency, and motor heat gain to the airstream.


/// Fan type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanType {
    /// Constant volume: runs at design flow whenever on.
    ConstantVolume,
    /// Variable volume: modulates flow between min and max.
    VariableVolume,
    /// On-off: cycles between full flow and zero flow.
    OnOff,
}

/// Fan operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FanOperatingMode {
    /// Fan runs continuously when system is on.
    #[default]
    Continuous,
    /// Fan cycles on/off based on part-load ratio.
    Cycling,
}

/// Fan specification and state.
#[derive(Debug, Clone)]
pub struct Fan {
    /// Fan name.
    pub name: String,
    /// Fan type.
    pub fan_type: FanType,
    /// Design pressure rise (Pa).
    pub design_pressure_rise: f64,
    /// Design maximum air flow rate (m3/s).
    pub design_flow_rate: f64,
    /// Total fan efficiency (0-1).
    pub fan_efficiency: f64,
    /// Motor efficiency (0-1).
    pub motor_efficiency: f64,
    /// Fraction of motor heat in airstream (0-1).
    pub motor_in_airstream_fraction: f64,
    /// Minimum flow fraction for VAV fans (0-1).
    pub min_flow_fraction: f64,
    /// Part-load power coefficients [c0, c1, c2, c3, c4] for VAV.
    /// Power fraction = c0 + c1*PLR + c2*PLR^2 + c3*PLR^3 + c4*PLR^4
    pub plf_coefficients: [f64; 5],

    // --- Operating state ---
    /// Current inlet air temperature (C).
    pub inlet_temp: f64,
    /// Current inlet humidity ratio (kg/kg).
    pub inlet_humidity_ratio: f64,
    /// Current inlet air density (kg/m3).
    pub inlet_density: f64,
    /// Current air mass flow rate (kg/s).
    pub mass_flow_rate: f64,
}

/// Fan calculation result.
#[derive(Debug, Clone, Copy)]
pub struct FanResult {
    /// Fan electrical power consumption (W).
    pub power: f64,
    /// Outlet air temperature (C).
    pub outlet_temp: f64,
    /// Outlet enthalpy (J/kg).
    pub outlet_enthalpy: f64,
    /// Heat added to airstream (W).
    pub heat_to_air: f64,
    /// Part-load ratio (0-1).
    pub part_load_ratio: f64,
    /// Run-time fraction (0-1) for on-off fans.
    pub runtime_fraction: f64,
}

impl Fan {
    /// Create a constant-volume fan.
    pub fn constant_volume(
        name: impl Into<String>,
        design_flow_rate: f64,
        design_pressure_rise: f64,
        fan_efficiency: f64,
        motor_efficiency: f64,
    ) -> Self {
        Self {
            name: name.into(),
            fan_type: FanType::ConstantVolume,
            design_pressure_rise,
            design_flow_rate,
            fan_efficiency,
            motor_efficiency,
            motor_in_airstream_fraction: 1.0,
            min_flow_fraction: 1.0,
            plf_coefficients: [1.0, 0.0, 0.0, 0.0, 0.0],
            inlet_temp: 20.0,
            inlet_humidity_ratio: 0.008,
            inlet_density: 1.2,
            mass_flow_rate: 0.0,
        }
    }

    /// Create a variable-volume fan.
    pub fn variable_volume(
        name: impl Into<String>,
        design_flow_rate: f64,
        design_pressure_rise: f64,
        fan_efficiency: f64,
        motor_efficiency: f64,
        min_flow_fraction: f64,
    ) -> Self {
        Self {
            name: name.into(),
            fan_type: FanType::VariableVolume,
            design_pressure_rise,
            design_flow_rate,
            fan_efficiency,
            motor_efficiency,
            motor_in_airstream_fraction: 1.0,
            min_flow_fraction,
            // Default VAV coefficients (outlet damper)
            plf_coefficients: [0.0408, 0.088, -0.0729, 0.9437, 0.0],
            inlet_temp: 20.0,
            inlet_humidity_ratio: 0.008,
            inlet_density: 1.2,
            mass_flow_rate: 0.0,
        }
    }

    /// Create an on-off fan.
    pub fn on_off(
        name: impl Into<String>,
        design_flow_rate: f64,
        design_pressure_rise: f64,
        fan_efficiency: f64,
        motor_efficiency: f64,
    ) -> Self {
        Self {
            name: name.into(),
            fan_type: FanType::OnOff,
            design_pressure_rise,
            design_flow_rate,
            fan_efficiency,
            motor_efficiency,
            motor_in_airstream_fraction: 1.0,
            min_flow_fraction: 0.0,
            plf_coefficients: [1.0, 0.0, 0.0, 0.0, 0.0],
            inlet_temp: 20.0,
            inlet_humidity_ratio: 0.008,
            inlet_density: 1.2,
            mass_flow_rate: 0.0,
        }
    }

    /// Design nominal power consumption (W).
    pub fn design_power(&self) -> f64 {
        if self.fan_efficiency > 0.0 {
            self.design_flow_rate * self.design_pressure_rise / self.fan_efficiency
        } else {
            0.0
        }
    }

    /// Calculate fan performance at current operating point.
    pub fn calculate(&self, run: bool) -> FanResult {
        if !run || self.mass_flow_rate <= 1e-10 {
            return FanResult {
                power: 0.0,
                outlet_temp: self.inlet_temp,
                outlet_enthalpy: ep_psychrometrics::enthalpy(self.inlet_temp, self.inlet_humidity_ratio),
                heat_to_air: 0.0,
                part_load_ratio: 0.0,
                runtime_fraction: 0.0,
            };
        }

        let design_mass_flow = self.design_flow_rate * self.inlet_density;
        let nominal_power = self.design_power();

        let (power, plr, rtf) = match self.fan_type {
            FanType::ConstantVolume => {
                // Constant volume: full power when on
                let plr = if design_mass_flow > 1e-10 {
                    (self.mass_flow_rate / design_mass_flow).min(1.0)
                } else {
                    0.0
                };
                // Power proportional to flow (linear)
                (nominal_power * plr, plr, 1.0)
            }
            FanType::VariableVolume => {
                let plr = if design_mass_flow > 1e-10 {
                    (self.mass_flow_rate / design_mass_flow).clamp(self.min_flow_fraction, 1.0)
                } else {
                    0.0
                };
                let c = &self.plf_coefficients;
                let frac_power = c[0] + c[1] * plr + c[2] * plr * plr
                    + c[3] * plr * plr * plr + c[4] * plr * plr * plr * plr;
                (nominal_power * frac_power.max(0.0), plr, 1.0)
            }
            FanType::OnOff => {
                let plr = if design_mass_flow > 1e-10 {
                    (self.mass_flow_rate / design_mass_flow).min(1.0)
                } else {
                    0.0
                };
                // On-off: runs at full power for a fraction of the timestep
                let rtf = plr; // Simple: runtime = PLR
                (nominal_power * rtf, plr, rtf)
            }
        };

        // Heat to airstream
        let shaft_power = power * self.motor_efficiency;
        let motor_loss = power - shaft_power;
        let heat_to_air = shaft_power + motor_loss * self.motor_in_airstream_fraction;

        // Temperature rise
        let cp = ep_psychrometrics::cp_air(self.inlet_humidity_ratio);
        let delta_t = if self.mass_flow_rate > 1e-10 {
            heat_to_air / (self.mass_flow_rate * cp)
        } else {
            0.0
        };

        let outlet_temp = self.inlet_temp + delta_t;
        let outlet_enthalpy = ep_psychrometrics::enthalpy(outlet_temp, self.inlet_humidity_ratio);

        FanResult {
            power,
            outlet_temp,
            outlet_enthalpy,
            heat_to_air,
            part_load_ratio: plr,
            runtime_fraction: rtf,
        }
    }
}

/// Calculate fan power from pressure-based approach.
///
/// P = V_dot * dP / (eta_fan * eta_motor)
pub fn fan_power_from_pressure(
    volume_flow_rate: f64,
    pressure_rise: f64,
    fan_efficiency: f64,
    motor_efficiency: f64,
) -> f64 {
    if fan_efficiency > 0.0 && motor_efficiency > 0.0 {
        volume_flow_rate * pressure_rise / (fan_efficiency * motor_efficiency)
    } else {
        0.0
    }
}

/// Fan affinity laws: power scales with cube of speed ratio.
///
/// P2 = P1 * (N2/N1)^3
pub fn affinity_power(design_power: f64, speed_ratio: f64) -> f64 {
    design_power * speed_ratio * speed_ratio * speed_ratio
}

/// Fan affinity laws: flow scales linearly with speed ratio.
///
/// Q2 = Q1 * (N2/N1)
pub fn affinity_flow(design_flow: f64, speed_ratio: f64) -> f64 {
    design_flow * speed_ratio
}

/// Fan affinity laws: pressure scales with square of speed ratio.
///
/// dP2 = dP1 * (N2/N1)^2
pub fn affinity_pressure(design_pressure: f64, speed_ratio: f64) -> f64 {
    design_pressure * speed_ratio * speed_ratio
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_volume_fan_basic() {
        let fan = Fan::constant_volume("Supply Fan", 1.0, 600.0, 0.7, 0.9);
        assert!((fan.design_power() - 1.0 * 600.0 / 0.7).abs() < 0.1);
    }

    #[test]
    fn constant_volume_fan_off() {
        let fan = Fan::constant_volume("CV Fan", 1.0, 600.0, 0.7, 0.9);
        let result = fan.calculate(false);
        assert!(result.power.abs() < 1e-10);
        assert!((result.outlet_temp - fan.inlet_temp).abs() < 1e-10);
    }

    #[test]
    fn constant_volume_fan_on() {
        let mut fan = Fan::constant_volume("CV Fan", 1.0, 600.0, 0.7, 0.9);
        fan.mass_flow_rate = 1.2; // design flow * density
        fan.inlet_temp = 20.0;
        let result = fan.calculate(true);
        // Power should be design_power * PLR = 857.1 * 1.0
        assert!(result.power > 800.0 && result.power < 900.0, "P={}", result.power);
        // Temperature should rise due to motor heat
        assert!(result.outlet_temp > fan.inlet_temp, "T_out={}", result.outlet_temp);
        assert!(result.heat_to_air > 0.0);
    }

    #[test]
    fn variable_volume_fan_part_load() {
        let mut fan = Fan::variable_volume("VAV Fan", 2.0, 800.0, 0.7, 0.9, 0.3);
        fan.inlet_density = 1.2;
        fan.mass_flow_rate = 1.2; // 50% of design mass flow (2.0 * 1.2 = 2.4)
        fan.inlet_temp = 15.0;

        let result = fan.calculate(true);
        let full_power = fan.design_power();
        // At 50% PLR with default coefficients, power should be less than full
        assert!(result.power < full_power, "P={}, full={}", result.power, full_power);
        assert!(result.power > 0.0);
        assert!((result.part_load_ratio - 0.5).abs() < 0.01, "PLR={}", result.part_load_ratio);
    }

    #[test]
    fn variable_volume_fan_min_flow() {
        let mut fan = Fan::variable_volume("VAV Fan", 2.0, 800.0, 0.7, 0.9, 0.3);
        fan.inlet_density = 1.2;
        // Very low flow — should be clamped to min_flow_fraction
        fan.mass_flow_rate = 0.1;
        let result = fan.calculate(true);
        assert!((result.part_load_ratio - 0.3).abs() < 0.01, "PLR={}", result.part_load_ratio);
    }

    #[test]
    fn on_off_fan() {
        let mut fan = Fan::on_off("Cycling Fan", 0.5, 400.0, 0.65, 0.85);
        fan.inlet_density = 1.2;
        fan.mass_flow_rate = 0.3; // 50% of design
        let result = fan.calculate(true);
        let design_power = fan.design_power();
        assert!((result.runtime_fraction - 0.5).abs() < 0.01, "RTF={}", result.runtime_fraction);
        assert!((result.power - design_power * 0.5).abs() < 1.0, "P={}", result.power);
    }

    #[test]
    fn fan_temperature_rise() {
        let mut fan = Fan::constant_volume("Test", 1.0, 1000.0, 0.6, 0.85);
        fan.motor_in_airstream_fraction = 1.0;
        fan.mass_flow_rate = 1.2;
        fan.inlet_temp = 20.0;
        let result = fan.calculate(true);
        // All motor heat goes to air: heat = power (since motor_in_airstream=1.0)
        let cp = ep_psychrometrics::cp_air(0.008);
        let expected_dt = result.power / (1.2 * cp);
        assert!((result.outlet_temp - (20.0 + expected_dt)).abs() < 0.01,
                "T_out={}, expected={}", result.outlet_temp, 20.0 + expected_dt);
    }

    #[test]
    fn fan_power_from_pressure_calc() {
        let p = fan_power_from_pressure(1.0, 600.0, 0.7, 0.9);
        // P = 1.0 * 600 / (0.7 * 0.9) = 952.38 W
        assert!((p - 952.38).abs() < 1.0, "P={}", p);
    }

    #[test]
    fn affinity_laws() {
        let design_power = 1000.0;
        let design_flow = 2.0;
        let design_pressure = 600.0;

        // At half speed
        assert!((affinity_power(design_power, 0.5) - 125.0).abs() < 1e-10);
        assert!((affinity_flow(design_flow, 0.5) - 1.0).abs() < 1e-10);
        assert!((affinity_pressure(design_pressure, 0.5) - 150.0).abs() < 1e-10);
    }

    #[test]
    fn motor_heat_fraction() {
        let mut fan = Fan::constant_volume("Test", 1.0, 600.0, 0.7, 0.9);
        fan.motor_in_airstream_fraction = 0.0; // Motor outside airstream
        fan.mass_flow_rate = 1.2;
        let result = fan.calculate(true);
        // Only shaft power goes to air: shaft_power = power * motor_eff
        let shaft_power = result.power * 0.9;
        assert!((result.heat_to_air - shaft_power).abs() < 1.0,
                "heat={}, shaft={}", result.heat_to_air, shaft_power);
    }

    #[test]
    fn vav_power_curve_shape() {
        // VAV power should decrease at lower flow fractions (not linearly)
        let mut fan = Fan::variable_volume("VAV", 2.0, 800.0, 0.7, 0.9, 0.3);
        fan.inlet_density = 1.2;

        fan.mass_flow_rate = 2.4; // 100% flow
        let p100 = fan.calculate(true).power;

        fan.mass_flow_rate = 1.2; // 50% flow
        let p50 = fan.calculate(true).power;

        fan.mass_flow_rate = 0.72; // 30% flow (min)
        let p30 = fan.calculate(true).power;

        // Power at 50% should be much less than 50% of full (cube law effect)
        assert!(p50 < 0.5 * p100, "p50={}, p100={}", p50, p100);
        assert!(p30 < p50, "p30={}, p50={}", p30, p50);
        assert!(p30 > 0.0);
    }
}
