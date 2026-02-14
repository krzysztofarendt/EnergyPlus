//! Energy Management System (EMS) runtime.
//!
//! Provides sensors, actuators, Erl program parsing/execution,
//! trend variables, and calling-point management. The Erl language
//! supports arithmetic, comparison, logical operators, math functions,
//! psychrometric functions, and conditional/loop control flow.

pub mod actuator;
pub mod program;
pub mod sensor;
pub mod trend;
pub mod variable;

pub use actuator::{Actuator, ActuatorState};
pub use program::{ErlInstruction, ErlKeyword, ErlProgram, ProgramManager};
pub use sensor::Sensor;
pub use trend::TrendVariable;
pub use variable::{ErlValue, ErlVariable, VariableManager};

use ep_core::plugin::CallingPoint;

/// EMS manager that holds all programs, variables, sensors, and actuators.
#[derive(Debug, Default)]
pub struct EmsManager {
    pub variables: VariableManager,
    pub sensors: Vec<Sensor>,
    pub actuators: Vec<Actuator>,
    pub trends: Vec<TrendVariable>,
    pub program_managers: Vec<ProgramManager>,
    pub programs: Vec<ErlProgram>,
}

impl EmsManager {
    pub fn new() -> Self {
        let mut mgr = Self::default();
        mgr.variables.register_builtins();
        mgr
    }

    /// Execute all program managers at the given calling point.
    pub fn execute_at_calling_point(&mut self, point: CallingPoint) {
        // Update sensor values into variables
        for sensor in &self.sensors {
            if let Some(var_idx) = sensor.variable_index {
                self.variables.set(var_idx, ErlValue::Number(sensor.current_value));
            }
        }

        // Find managers for this calling point and run their programs
        let program_indices: Vec<Vec<usize>> = self.program_managers.iter()
            .filter(|pm| pm.calling_point == point)
            .map(|pm| pm.program_indices.clone())
            .collect();

        for indices in &program_indices {
            for &prog_idx in indices {
                if prog_idx < self.programs.len() {
                    self.programs[prog_idx].execute(&mut self.variables);
                }
            }
        }

        // Push actuator values from variables
        for actuator in &mut self.actuators {
            if actuator.is_active {
                if let Some(var_idx) = actuator.variable_index {
                    if let Some(val) = self.variables.get(var_idx) {
                        actuator.state = ActuatorState::Overridden(val.as_number());
                    }
                }
            }
        }
    }

    /// Update trend variables at end of timestep.
    pub fn update_trends(&mut self) {
        for trend in &mut self.trends {
            if let Some(var_idx) = trend.variable_index {
                if let Some(val) = self.variables.get(var_idx) {
                    trend.push(val.as_number());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ems_manager_creation() {
        let mgr = EmsManager::new();
        // Builtins should be registered
        assert!(mgr.variables.count() > 0);
    }

    #[test]
    fn ems_sensor_to_variable() {
        let mut mgr = EmsManager::new();
        let var_idx = mgr.variables.add("my_sensor_var", false);
        mgr.sensors.push(Sensor {
            name: "Zone Temp Sensor".into(),
            output_var_name: "Zone Mean Air Temperature".into(),
            key_name: "Zone1".into(),
            variable_index: Some(var_idx),
            current_value: 23.5,
        });

        mgr.execute_at_calling_point(CallingPoint::BeginTimestep);
        let val = mgr.variables.get(var_idx).unwrap();
        assert!((val.as_number() - 23.5).abs() < 1e-10);
    }

    #[test]
    fn ems_actuator_override() {
        let mut mgr = EmsManager::new();
        let var_idx = mgr.variables.add("actuator_var", false);
        mgr.variables.set(var_idx, ErlValue::Number(42.0));
        mgr.actuators.push(Actuator {
            name: "Light Override".into(),
            component_type: "Lights".into(),
            control_type: "Electricity Rate".into(),
            variable_index: Some(var_idx),
            is_active: true,
            state: ActuatorState::Normal,
        });

        mgr.execute_at_calling_point(CallingPoint::BeforeHvac);
        match mgr.actuators[0].state {
            ActuatorState::Overridden(v) => assert!((v - 42.0).abs() < 1e-10),
            _ => panic!("Expected overridden state"),
        }
    }

    #[test]
    fn ems_update_trends() {
        let mut mgr = EmsManager::new();
        let var_idx = mgr.variables.add("trend_var", false);
        mgr.variables.set(var_idx, ErlValue::Number(77.0));
        let mut trend = TrendVariable::new("MyTrend", 5);
        trend.variable_index = Some(var_idx);
        mgr.trends.push(trend);

        mgr.update_trends();

        assert!((mgr.trends[0].value_at(0).unwrap() - 77.0).abs() < 1e-10);
    }

    #[test]
    fn ems_execute_at_calling_point() {
        use crate::program::{ErlExpression, ErlInstruction, ErlKeyword, ErlOp, ErlProgram};

        let mut mgr = EmsManager::new();
        let var_idx = mgr.variables.add("out", false);

        // Build a program: SET out = 99.0
        let mut prog = ErlProgram::new("SetOut");
        let lit_idx = prog.add_literal(99.0);
        let expr_idx = prog.add_expression(ErlExpression {
            op: ErlOp::Literal(lit_idx),
            operands: vec![],
        });
        prog.instructions.push(ErlInstruction {
            keyword: ErlKeyword::Set,
            arg1: var_idx,
            arg2: expr_idx,
        });
        mgr.programs.push(prog);

        mgr.program_managers.push(ProgramManager {
            name: "PM1".into(),
            calling_point: CallingPoint::BeginTimestep,
            program_indices: vec![0],
        });

        mgr.execute_at_calling_point(CallingPoint::BeginTimestep);
        assert!((mgr.variables.get(var_idx).unwrap().as_number() - 99.0).abs() < 1e-10);
    }

    #[test]
    fn ems_inactive_actuator() {
        let mut mgr = EmsManager::new();
        let var_idx = mgr.variables.add("act_var", false);
        mgr.variables.set(var_idx, ErlValue::Number(50.0));
        mgr.actuators.push(Actuator {
            name: "InactiveAct".into(),
            component_type: "Lights".into(),
            control_type: "Electricity Rate".into(),
            variable_index: Some(var_idx),
            is_active: false,
            state: ActuatorState::Normal,
        });

        mgr.execute_at_calling_point(CallingPoint::BeforeHvac);
        // Actuator is inactive, state should remain Normal
        assert!(matches!(mgr.actuators[0].state, ActuatorState::Normal));
    }

    #[test]
    fn ems_multiple_programs() {
        use crate::program::{ErlExpression, ErlInstruction, ErlKeyword, ErlOp, ErlProgram};

        let mut mgr = EmsManager::new();
        let a_idx = mgr.variables.add("a", false);
        let b_idx = mgr.variables.add("b", false);

        // Program 0: SET a = 10
        let mut prog0 = ErlProgram::new("SetA");
        let lit_idx0 = prog0.add_literal(10.0);
        let expr_idx0 = prog0.add_expression(ErlExpression {
            op: ErlOp::Literal(lit_idx0),
            operands: vec![],
        });
        prog0.instructions.push(ErlInstruction {
            keyword: ErlKeyword::Set,
            arg1: a_idx,
            arg2: expr_idx0,
        });
        mgr.programs.push(prog0);

        // Program 1: SET b = 20
        let mut prog1 = ErlProgram::new("SetB");
        let lit_idx1 = prog1.add_literal(20.0);
        let expr_idx1 = prog1.add_expression(ErlExpression {
            op: ErlOp::Literal(lit_idx1),
            operands: vec![],
        });
        prog1.instructions.push(ErlInstruction {
            keyword: ErlKeyword::Set,
            arg1: b_idx,
            arg2: expr_idx1,
        });
        mgr.programs.push(prog1);

        mgr.program_managers.push(ProgramManager {
            name: "PM".into(),
            calling_point: CallingPoint::EndTimestep,
            program_indices: vec![0, 1],
        });

        mgr.execute_at_calling_point(CallingPoint::EndTimestep);
        assert!((mgr.variables.get(a_idx).unwrap().as_number() - 10.0).abs() < 1e-10);
        assert!((mgr.variables.get(b_idx).unwrap().as_number() - 20.0).abs() < 1e-10);
    }
}
