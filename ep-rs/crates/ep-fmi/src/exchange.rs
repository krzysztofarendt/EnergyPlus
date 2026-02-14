//! FMI variable exchange and co-simulation step management.
//!
//! Manages the exchange of variables between the simulation engine
//! and external FMUs (or exposing the engine as an FMU).

use std::collections::HashMap;

/// Direction of variable exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableDirection {
    /// Simulation → FMU (FMU input).
    ToFmu,
    /// FMU → Simulation (FMU output).
    FromFmu,
}

/// A variable being exchanged via FMI.
#[derive(Debug, Clone)]
pub struct ExchangeVariable {
    pub name: String,
    pub value_reference: u32,
    pub direction: VariableDirection,
    pub current_value: f64,
    /// Internal variable key (e.g., schedule name, actuator name).
    pub internal_key: String,
}

impl ExchangeVariable {
    pub fn new(
        name: impl Into<String>,
        vr: u32,
        direction: VariableDirection,
        internal_key: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            value_reference: vr,
            direction,
            current_value: 0.0,
            internal_key: internal_key.into(),
        }
    }
}

/// Co-simulation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoSimState {
    /// Not initialized.
    Uninitialized,
    /// Initialized and ready.
    Initialized,
    /// Running (stepping).
    Running,
    /// Terminated.
    Terminated,
    /// Error state.
    Error,
}

/// Manager for FMI variable exchange.
#[derive(Debug)]
pub struct ExchangeManager {
    pub variables: Vec<ExchangeVariable>,
    pub state: CoSimState,
    /// Current simulation time (seconds from start).
    pub current_time: f64,
    /// Communication step size (seconds).
    pub step_size: f64,
    /// Value buffer indexed by value_reference.
    values: HashMap<u32, f64>,
}

impl ExchangeManager {
    pub fn new(step_size: f64) -> Self {
        Self {
            variables: Vec::new(),
            state: CoSimState::Uninitialized,
            current_time: 0.0,
            step_size,
            values: HashMap::new(),
        }
    }

    /// Add an exchange variable.
    pub fn add_variable(&mut self, var: ExchangeVariable) {
        self.values.insert(var.value_reference, var.current_value);
        self.variables.push(var);
    }

    /// Initialize the exchange.
    pub fn initialize(&mut self) {
        for var in &self.variables {
            self.values.insert(var.value_reference, var.current_value);
        }
        self.state = CoSimState::Initialized;
        self.current_time = 0.0;
    }

    /// Get value by value reference.
    pub fn get_real(&self, vr: u32) -> Option<f64> {
        self.values.get(&vr).copied()
    }

    /// Set value by value reference.
    pub fn set_real(&mut self, vr: u32, value: f64) {
        self.values.insert(vr, value);
        // Update the variable struct too
        for var in &mut self.variables {
            if var.value_reference == vr {
                var.current_value = value;
            }
        }
    }

    /// Perform a co-simulation step.
    /// Returns Ok(()) if step completed successfully.
    pub fn do_step(&mut self, communication_step_size: f64) -> Result<(), String> {
        if self.state != CoSimState::Initialized && self.state != CoSimState::Running {
            return Err(format!("Cannot step in state {:?}", self.state));
        }
        self.state = CoSimState::Running;
        self.current_time += communication_step_size;
        Ok(())
    }

    /// Terminate the co-simulation.
    pub fn terminate(&mut self) {
        self.state = CoSimState::Terminated;
    }

    /// Get all input variables (ToFmu direction).
    pub fn inputs(&self) -> Vec<&ExchangeVariable> {
        self.variables.iter()
            .filter(|v| v.direction == VariableDirection::ToFmu)
            .collect()
    }

    /// Get all output variables (FromFmu direction).
    pub fn outputs(&self) -> Vec<&ExchangeVariable> {
        self.variables.iter()
            .filter(|v| v.direction == VariableDirection::FromFmu)
            .collect()
    }

    /// Collect all output values into a map.
    pub fn collect_outputs(&self) -> HashMap<u32, f64> {
        self.variables.iter()
            .filter(|v| v.direction == VariableDirection::FromFmu)
            .map(|v| (v.value_reference, v.current_value))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exchange_lifecycle() {
        let mut mgr = ExchangeManager::new(900.0);
        mgr.add_variable(ExchangeVariable::new("zone_temp", 1, VariableDirection::FromFmu, "Zone1:Temperature"));
        mgr.add_variable(ExchangeVariable::new("setpoint", 2, VariableDirection::ToFmu, "ThermostatSetpoint"));

        mgr.initialize();
        assert_eq!(mgr.state, CoSimState::Initialized);

        mgr.set_real(2, 22.0);
        assert!((mgr.get_real(2).unwrap() - 22.0).abs() < 1e-10);

        mgr.set_real(1, 23.5);
        assert!(mgr.do_step(900.0).is_ok());
        assert_eq!(mgr.state, CoSimState::Running);
        assert!((mgr.current_time - 900.0).abs() < 1e-10);

        mgr.terminate();
        assert_eq!(mgr.state, CoSimState::Terminated);
    }

    #[test]
    fn exchange_io_filtering() {
        let mut mgr = ExchangeManager::new(900.0);
        mgr.add_variable(ExchangeVariable::new("out1", 1, VariableDirection::FromFmu, "key1"));
        mgr.add_variable(ExchangeVariable::new("out2", 2, VariableDirection::FromFmu, "key2"));
        mgr.add_variable(ExchangeVariable::new("in1", 3, VariableDirection::ToFmu, "key3"));

        assert_eq!(mgr.inputs().len(), 1);
        assert_eq!(mgr.outputs().len(), 2);
    }

    #[test]
    fn step_before_init_fails() {
        let mut mgr = ExchangeManager::new(900.0);
        assert!(mgr.do_step(900.0).is_err());
    }

    #[test]
    fn multiple_steps() {
        let mut mgr = ExchangeManager::new(900.0);
        mgr.initialize();
        for _ in 0..4 {
            assert!(mgr.do_step(900.0).is_ok());
        }
        assert!((mgr.current_time - 3600.0).abs() < 1e-10); // 4 * 900 = 3600s = 1 hour
    }

    #[test]
    fn collect_outputs() {
        let mut mgr = ExchangeManager::new(900.0);
        mgr.add_variable(ExchangeVariable::new("temp", 1, VariableDirection::FromFmu, "key"));
        mgr.add_variable(ExchangeVariable::new("input", 2, VariableDirection::ToFmu, "key2"));
        mgr.initialize();
        mgr.set_real(1, 25.0);

        let outputs = mgr.collect_outputs();
        assert_eq!(outputs.len(), 1);
        assert!((outputs[&1] - 25.0).abs() < 1e-10);
    }
}
