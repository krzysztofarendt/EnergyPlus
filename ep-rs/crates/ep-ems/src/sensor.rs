//! EMS sensor model.
//!
//! Sensors read simulation output variables and make them available
//! to ERL programs through the variable manager.

/// An EMS sensor that reads an output variable.
#[derive(Debug, Clone)]
pub struct Sensor {
    pub name: String,
    /// Output variable name to read (e.g., "Zone Mean Air Temperature").
    pub output_var_name: String,
    /// Key name for the output variable (e.g., zone name).
    pub key_name: String,
    /// Index into the variable manager.
    pub variable_index: Option<usize>,
    /// Current value read from the simulation.
    pub current_value: f64,
}

impl Sensor {
    pub fn new(name: impl Into<String>, output_var: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            output_var_name: output_var.into(),
            key_name: key.into(),
            variable_index: None,
            current_value: 0.0,
        }
    }

    /// Update the sensor value from the simulation.
    pub fn update(&mut self, value: f64) {
        self.current_value = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensor_creation() {
        let sensor = Sensor::new("MySensor", "Zone Mean Air Temperature", "Zone1");
        assert_eq!(sensor.name, "MySensor");
        assert!((sensor.current_value).abs() < 1e-10);
    }

    #[test]
    fn sensor_update() {
        let mut sensor = Sensor::new("T", "Temperature", "Zone1");
        sensor.update(23.5);
        assert!((sensor.current_value - 23.5).abs() < 1e-10);
    }
}
