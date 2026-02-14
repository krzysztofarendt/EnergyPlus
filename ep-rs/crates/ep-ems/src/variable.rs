//! ERL variable management.
//!
//! Variables hold runtime values (numbers, null, error). The variable
//! manager provides named lookup, built-in constants, and scope support.

/// Value that an ERL variable can hold.
#[derive(Debug, Clone)]
pub enum ErlValue {
    Null,
    Number(f64),
    Error(String),
}

impl Default for ErlValue {
    fn default() -> Self {
        Self::Null
    }
}

impl ErlValue {
    pub fn as_number(&self) -> f64 {
        match self {
            Self::Number(n) => *n,
            _ => 0.0,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    pub fn is_true(&self) -> bool {
        match self {
            Self::Number(n) => *n == 1.0,
            _ => false,
        }
    }
}

/// A named ERL variable.
#[derive(Debug, Clone)]
pub struct ErlVariable {
    pub name: String,
    pub value: ErlValue,
    pub read_only: bool,
    pub initialized: bool,
}

/// Manages the collection of ERL variables.
#[derive(Debug, Default)]
pub struct VariableManager {
    variables: Vec<ErlVariable>,
}

impl VariableManager {
    /// Register built-in constants and simulation variables.
    pub fn register_builtins(&mut self) {
        // Constants
        self.add_readonly("Null", ErlValue::Null);
        self.add_readonly("False", ErlValue::Number(0.0));
        self.add_readonly("True", ErlValue::Number(1.0));
        self.add_readonly("Off", ErlValue::Number(0.0));
        self.add_readonly("On", ErlValue::Number(1.0));
        self.add_readonly("PI", ErlValue::Number(std::f64::consts::PI));

        // Time variables (updated each timestep by the simulation manager)
        let time_vars = [
            "Year", "Month", "DayOfMonth", "DayOfWeek", "DayOfYear",
            "Hour", "Minute", "CurrentTime", "TimeStepNum",
            "DaylightSavings", "Holiday", "WarmupFlag", "SunIsUp",
            "IsRaining", "SystemTimeStep", "ZoneTimeStep",
            "CurrentEnvironment", "TimeStepsPerHour",
        ];
        for name in &time_vars {
            self.add(name, true);
            let idx = self.variables.len() - 1;
            self.variables[idx].value = ErlValue::Number(0.0);
            self.variables[idx].initialized = true;
        }
    }

    /// Add a new user variable, returning its index.
    pub fn add(&mut self, name: &str, read_only: bool) -> usize {
        let idx = self.variables.len();
        self.variables.push(ErlVariable {
            name: name.to_string(),
            value: ErlValue::Null,
            read_only,
            initialized: false,
        });
        idx
    }

    fn add_readonly(&mut self, name: &str, value: ErlValue) {
        self.variables.push(ErlVariable {
            name: name.to_string(),
            value,
            read_only: true,
            initialized: true,
        });
    }

    /// Find a variable by name, returning its index.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.variables.iter().position(|v| v.name.eq_ignore_ascii_case(name))
    }

    /// Get a variable's value by index.
    pub fn get(&self, index: usize) -> Option<&ErlValue> {
        self.variables.get(index).map(|v| &v.value)
    }

    /// Set a variable's value by index.
    pub fn set(&mut self, index: usize, value: ErlValue) {
        if let Some(var) = self.variables.get_mut(index) {
            if !var.read_only {
                var.value = value;
                var.initialized = true;
            }
        }
    }

    /// Number of variables.
    pub fn count(&self) -> usize {
        self.variables.len()
    }

    /// Check if a variable is initialized.
    pub fn is_initialized(&self, index: usize) -> bool {
        self.variables.get(index).map_or(false, |v| v.initialized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_constants() {
        let mut vm = VariableManager::default();
        vm.register_builtins();

        let pi_idx = vm.find("PI").unwrap();
        assert!((vm.get(pi_idx).unwrap().as_number() - std::f64::consts::PI).abs() < 1e-15);

        let true_idx = vm.find("True").unwrap();
        assert!(vm.get(true_idx).unwrap().is_true());

        let false_idx = vm.find("False").unwrap();
        assert!(!vm.get(false_idx).unwrap().is_true());
    }

    #[test]
    fn user_variable_lifecycle() {
        let mut vm = VariableManager::default();
        let idx = vm.add("my_var", false);
        assert!(!vm.is_initialized(idx));

        vm.set(idx, ErlValue::Number(3.14));
        assert!(vm.is_initialized(idx));
        assert!((vm.get(idx).unwrap().as_number() - 3.14).abs() < 1e-10);
    }

    #[test]
    fn readonly_protection() {
        let mut vm = VariableManager::default();
        vm.register_builtins();
        let pi_idx = vm.find("PI").unwrap();
        vm.set(pi_idx, ErlValue::Number(99.0)); // Should be ignored
        assert!((vm.get(pi_idx).unwrap().as_number() - std::f64::consts::PI).abs() < 1e-15);
    }

    #[test]
    fn case_insensitive_find() {
        let mut vm = VariableManager::default();
        vm.add("MyVariable", false);
        assert!(vm.find("myvariable").is_some());
        assert!(vm.find("MYVARIABLE").is_some());
    }

    #[test]
    fn erl_value_null_as_number() {
        let val = ErlValue::Null;
        assert!((val.as_number() - 0.0).abs() < 1e-15);
    }

    #[test]
    fn erl_value_error_as_number() {
        let val = ErlValue::Error("some error".into());
        assert!((val.as_number() - 0.0).abs() < 1e-15);
    }

    #[test]
    fn variable_manager_find_nonexistent() {
        let vm = VariableManager::default();
        assert!(vm.find("nonexistent").is_none());
    }
}
