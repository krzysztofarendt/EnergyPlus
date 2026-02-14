//! FMI model description and variable metadata.
//!
//! Represents the modelDescription.xml structure from FMI 2.0,
//! defining variables, their causality, variability, and data types.

/// FMI variable causality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FmiCausality {
    /// Variable set by the environment (input to FMU).
    Input,
    /// Variable computed by the FMU (output from FMU).
    Output,
    /// Internal variable, not exchanged.
    Local,
    /// Fixed parameter.
    Parameter,
}

/// FMI variable variability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FmiVariability {
    /// Value never changes.
    Constant,
    /// Value changes only at events.
    Discrete,
    /// Value can change at any time.
    Continuous,
    /// Value is fixed after initialization.
    Fixed,
}

/// An FMI scalar variable.
#[derive(Debug, Clone)]
pub struct FmiVariable {
    pub name: String,
    pub value_reference: u32,
    pub causality: FmiCausality,
    pub variability: FmiVariability,
    pub description: String,
    pub unit: String,
    pub initial_value: f64,
}

impl FmiVariable {
    pub fn input(name: impl Into<String>, vr: u32) -> Self {
        Self {
            name: name.into(),
            value_reference: vr,
            causality: FmiCausality::Input,
            variability: FmiVariability::Continuous,
            description: String::new(),
            unit: String::new(),
            initial_value: 0.0,
        }
    }

    pub fn output(name: impl Into<String>, vr: u32) -> Self {
        Self {
            name: name.into(),
            value_reference: vr,
            causality: FmiCausality::Output,
            variability: FmiVariability::Continuous,
            description: String::new(),
            unit: String::new(),
            initial_value: 0.0,
        }
    }

    pub fn with_initial(mut self, value: f64) -> Self {
        self.initial_value = value;
        self
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = unit.into();
        self
    }
}

/// FMI model description — metadata about an FMU.
#[derive(Debug, Clone)]
pub struct FmiModelDescription {
    pub model_name: String,
    pub guid: String,
    pub fmi_version: String,
    pub variables: Vec<FmiVariable>,
    /// Step size for co-simulation (seconds).
    pub default_step_size: f64,
}

impl FmiModelDescription {
    pub fn new(model_name: impl Into<String>, guid: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            guid: guid.into(),
            fmi_version: "2.0".into(),
            variables: Vec::new(),
            default_step_size: 900.0, // 15 minutes
        }
    }

    pub fn add_variable(&mut self, var: FmiVariable) {
        self.variables.push(var);
    }

    pub fn inputs(&self) -> impl Iterator<Item = &FmiVariable> {
        self.variables.iter().filter(|v| v.causality == FmiCausality::Input)
    }

    pub fn outputs(&self) -> impl Iterator<Item = &FmiVariable> {
        self.variables.iter().filter(|v| v.causality == FmiCausality::Output)
    }

    pub fn find_by_name(&self, name: &str) -> Option<&FmiVariable> {
        self.variables.iter().find(|v| v.name == name)
    }

    pub fn find_by_reference(&self, vr: u32) -> Option<&FmiVariable> {
        self.variables.iter().find(|v| v.value_reference == vr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_description_basic() {
        let mut desc = FmiModelDescription::new("TestModel", "abc-123");
        desc.add_variable(FmiVariable::input("zone_temp", 1).with_unit("K"));
        desc.add_variable(FmiVariable::output("heating_power", 2).with_unit("W"));
        desc.add_variable(FmiVariable::input("setpoint", 3).with_initial(293.15));

        assert_eq!(desc.inputs().count(), 2);
        assert_eq!(desc.outputs().count(), 1);
        assert!(desc.find_by_name("zone_temp").is_some());
        assert!(desc.find_by_reference(2).is_some());
    }

    #[test]
    fn variable_builders() {
        let v = FmiVariable::input("temp", 1)
            .with_unit("C")
            .with_initial(20.0);
        assert_eq!(v.causality, FmiCausality::Input);
        assert!((v.initial_value - 20.0).abs() < 1e-10);
        assert_eq!(v.unit, "C");
    }
}
