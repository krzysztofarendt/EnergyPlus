//! Secondary refrigeration loop model.
//!
//! Models glycol or CO2 secondary loops that transfer heat from
//! display cases/walk-ins to a central compressor rack.

/// Secondary fluid type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondaryFluid {
    Glycol,
    CO2,
    Brine,
}

/// Pump control for secondary loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpControl {
    Constant,
    Variable,
}

/// Secondary refrigeration loop.
#[derive(Debug, Clone)]
pub struct SecondaryLoop {
    pub name: String,
    pub fluid_type: SecondaryFluid,
    /// Design evaporating temperature at the heat exchanger (C).
    pub evap_temp: f64,
    /// Approach temperature: Tfluid_out - Tevap (K).
    pub approach_temp: f64,
    /// Design circulating flow rate (m³/s).
    pub design_flow_rate: f64,
    /// Pump power (W).
    pub pump_power: f64,
    /// Pump control type.
    pub pump_control: PumpControl,
    /// Pipe heat gain per unit length (W/m).
    pub pipe_heat_gain_per_length: f64,
    /// Total pipe length (m).
    pub pipe_length: f64,
    /// Heat exchanger effectiveness (0-1).
    pub hx_effectiveness: f64,
    /// Fluid specific heat (J/kg·K).
    pub fluid_cp: f64,
    /// Fluid density (kg/m³).
    pub fluid_density: f64,
}

/// Secondary loop result.
#[derive(Debug, Clone, Copy, Default)]
pub struct SecondaryResult {
    /// Total load on the primary system (W).
    pub primary_load: f64,
    /// Load from cases/walk-ins (W).
    pub case_load: f64,
    /// Pipe heat gain (W).
    pub pipe_heat_gain: f64,
    /// Pump heat gain (W).
    pub pump_heat_gain: f64,
    /// Pump electric power (W).
    pub pump_power: f64,
    /// Fluid supply temperature (C).
    pub supply_temp: f64,
    /// Fluid return temperature (C).
    pub return_temp: f64,
}

impl SecondaryLoop {
    pub fn new(name: impl Into<String>, evap_temp: f64) -> Self {
        Self {
            name: name.into(),
            fluid_type: SecondaryFluid::Glycol,
            evap_temp,
            approach_temp: 2.0,
            design_flow_rate: 0.002,
            pump_power: 1000.0,
            pump_control: PumpControl::Constant,
            pipe_heat_gain_per_length: 5.0,
            pipe_length: 50.0,
            hx_effectiveness: 0.8,
            fluid_cp: 3400.0,  // Propylene glycol ~30%
            fluid_density: 1040.0,
        }
    }

    /// Calculate secondary loop performance.
    ///
    /// `total_case_load` — sum of cooling loads from cases and walk-ins (W).
    pub fn calculate(&self, total_case_load: f64) -> SecondaryResult {
        if total_case_load <= 0.0 {
            return SecondaryResult {
                supply_temp: self.evap_temp + self.approach_temp,
                ..Default::default()
            };
        }

        // Pipe heat gain
        let pipe_heat_gain = self.pipe_heat_gain_per_length * self.pipe_length;

        // Pump heat gain (all pump power becomes heat in the fluid)
        let load_fraction: f64 = 1.0; // Simplified
        let pump_power = match self.pump_control {
            PumpControl::Constant => self.pump_power,
            PumpControl::Variable => self.pump_power * load_fraction.max(0.3),
        };
        let pump_heat_gain = pump_power;

        // Total load on primary = case load + parasitic gains
        let primary_load = total_case_load + pipe_heat_gain + pump_heat_gain;

        // Fluid temperatures
        let supply_temp = self.evap_temp + self.approach_temp;
        let mass_flow = self.design_flow_rate * self.fluid_density;
        let dt = if mass_flow * self.fluid_cp > 0.0 {
            primary_load / (mass_flow * self.fluid_cp)
        } else {
            0.0
        };
        let return_temp = supply_temp + dt;

        SecondaryResult {
            primary_load,
            case_load: total_case_load,
            pipe_heat_gain,
            pump_heat_gain,
            pump_power,
            supply_temp,
            return_temp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secondary_loop_basic() {
        let loop_ = SecondaryLoop::new("Secondary-1", -6.7);
        let result = loop_.calculate(10000.0);

        assert!(result.primary_load > 10000.0); // Includes parasitic gains
        assert!(result.pipe_heat_gain > 0.0);
        assert!(result.pump_heat_gain > 0.0);
        assert!(result.return_temp > result.supply_temp);
    }

    #[test]
    fn secondary_loop_zero_load() {
        let loop_ = SecondaryLoop::new("Secondary-1", -6.7);
        let result = loop_.calculate(0.0);

        assert!(result.primary_load.abs() < 1e-10);
        assert!(result.pump_power.abs() < 1e-10);
    }

    #[test]
    fn secondary_parasitics_add_load() {
        let loop_ = SecondaryLoop::new("Secondary-1", -6.7);
        let result = loop_.calculate(10000.0);

        let expected_parasitics = loop_.pipe_heat_gain_per_length * loop_.pipe_length
            + loop_.pump_power;
        assert!((result.primary_load - 10000.0 - expected_parasitics).abs() < 1.0);
    }
}
