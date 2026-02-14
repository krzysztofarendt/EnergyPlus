//! EMS actuator model.
//!
//! Actuators write control values to simulation components,
//! overriding their normal behavior when active.

/// Actuator override state.
#[derive(Debug, Clone, Copy)]
pub enum ActuatorState {
    /// Normal simulation control.
    Normal,
    /// EMS-overridden with specific value.
    Overridden(f64),
}

impl Default for ActuatorState {
    fn default() -> Self {
        Self::Normal
    }
}

/// An EMS actuator that writes to a simulation variable.
#[derive(Debug, Clone)]
pub struct Actuator {
    pub name: String,
    /// Component type (e.g., "Schedule:Compact", "Lights").
    pub component_type: String,
    /// Control type (e.g., "Schedule Value", "Electricity Rate").
    pub control_type: String,
    /// Index into the variable manager for the control variable.
    pub variable_index: Option<usize>,
    /// Whether the actuator is currently active.
    pub is_active: bool,
    /// Current override state.
    pub state: ActuatorState,
}

impl Actuator {
    pub fn new(
        name: impl Into<String>,
        component_type: impl Into<String>,
        control_type: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            component_type: component_type.into(),
            control_type: control_type.into(),
            variable_index: None,
            is_active: false,
            state: ActuatorState::Normal,
        }
    }

    /// Activate the actuator with a specific value.
    pub fn activate(&mut self, value: f64) {
        self.is_active = true;
        self.state = ActuatorState::Overridden(value);
    }

    /// Deactivate, returning to normal simulation control.
    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.state = ActuatorState::Normal;
    }

    /// Get the current actuated value, if overridden.
    pub fn overridden_value(&self) -> Option<f64> {
        match self.state {
            ActuatorState::Overridden(v) => Some(v),
            ActuatorState::Normal => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actuator_lifecycle() {
        let mut act = Actuator::new("Act1", "Lights", "Electricity Rate");
        assert!(!act.is_active);
        assert!(act.overridden_value().is_none());

        act.activate(500.0);
        assert!(act.is_active);
        assert!((act.overridden_value().unwrap() - 500.0).abs() < 1e-10);

        act.deactivate();
        assert!(!act.is_active);
        assert!(act.overridden_value().is_none());
    }

    #[test]
    fn actuator_activate_deactivate_cycle() {
        let mut act = Actuator::new("CycleAct", "Coil", "Heating Rate");

        // Initially inactive
        assert!(!act.is_active);
        assert!(matches!(act.state, ActuatorState::Normal));

        // Activate with 100.0
        act.activate(100.0);
        assert!(act.is_active);
        assert!((act.overridden_value().unwrap() - 100.0).abs() < 1e-10);

        // Deactivate
        act.deactivate();
        assert!(!act.is_active);
        assert!(act.overridden_value().is_none());

        // Re-activate with a different value
        act.activate(200.0);
        assert!(act.is_active);
        assert!((act.overridden_value().unwrap() - 200.0).abs() < 1e-10);

        // Deactivate again
        act.deactivate();
        assert!(!act.is_active);
        assert!(matches!(act.state, ActuatorState::Normal));
    }
}
