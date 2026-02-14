//! Output reporting and meter system for EnergyPlus-rs.
//!
//! Provides:
//! - Output variable registration and value accumulation
//! - Meter hierarchy (resource / end-use / group)
//! - ESO, MTR, CSV, tabular, and SQL output file generation
//! - An `OutputManager` that coordinates all output subsystems

pub mod accumulator;
pub mod csv;
pub mod eso;
pub mod meters;
pub mod mtr;
pub mod sql;
pub mod tabular;
pub mod variables;

use variables::{OutputVariable, ReportFreq, StoreType, TimeStamp, TimeStepType};

/// A user request for output reporting (from Output:Variable or Output:Meter).
#[derive(Debug, Clone)]
pub struct OutputRequest {
    /// Key pattern ("*" for all, or a specific key).
    pub key: String,
    /// Variable name to match.
    pub variable_name: String,
    /// Requested reporting frequency.
    pub freq: ReportFreq,
}

/// Coordinates output variable registration, requests, and reporting.
pub struct OutputManager {
    /// All registered output variables.
    pub variables: Vec<OutputVariable>,
    /// Meter manager.
    pub meter_manager: meters::MeterManager,
    /// Pending output requests (from IDF Output:Variable objects).
    pub requests: Vec<OutputRequest>,
    /// Next report ID to assign.
    next_report_id: usize,
}

impl OutputManager {
    pub fn new() -> Self {
        Self {
            variables: Vec::new(),
            meter_manager: meters::MeterManager::new(),
            requests: Vec::new(),
            next_report_id: 1,
        }
    }

    /// Register a new output variable. Returns its index in the variables vec.
    pub fn register_variable(
        &mut self,
        name: &str,
        key: &str,
        units: &str,
        store_type: StoreType,
        time_step_type: TimeStepType,
    ) -> usize {
        let report_id = self.next_report_id;
        self.next_report_id += 1;
        let var = OutputVariable::new(report_id, name, key, units, store_type, time_step_type);
        let idx = self.variables.len();
        self.variables.push(var);
        idx
    }

    /// Add an output request (e.g., from an Output:Variable IDF object).
    pub fn add_request(&mut self, request: OutputRequest) {
        self.requests.push(request);
    }

    /// Resolve pending requests against registered variables.
    /// Sets the reporting frequency on matched variables.
    pub fn resolve_requests(&mut self) {
        for req in &self.requests {
            let name_upper = req.variable_name.to_uppercase();
            let key_upper = req.key.to_uppercase();

            for var in &mut self.variables {
                let name_match = var.name.to_uppercase() == name_upper;
                let key_match = key_upper == "*" || var.key.to_uppercase() == key_upper;

                if name_match && key_match {
                    var.freq = req.freq;
                }
            }
        }
    }

    /// Update all variables of the given time_step_type by accumulating
    /// their current values.
    pub fn update_data(
        &mut self,
        time_step_type: TimeStepType,
        time_fraction: f64,
        timestamp: &TimeStamp,
    ) {
        for var in &mut self.variables {
            if var.time_step_type == time_step_type {
                var.accumulate(time_fraction, timestamp);
            }
        }
    }

    /// Collect reportable values for all variables at the given frequency,
    /// resetting them for the next interval.
    pub fn report_variables(&mut self, freq: ReportFreq) -> Vec<(usize, f64)> {
        let mut results = Vec::new();
        for var in &mut self.variables {
            if var.freq == freq {
                let val = var.report_and_reset();
                results.push((var.report_id, val));
            }
        }
        results
    }

    /// Number of registered variables.
    pub fn variable_count(&self) -> usize {
        self.variables.len()
    }
}

impl Default for OutputManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_update_and_report() {
        let mut mgr = OutputManager::new();
        let idx = mgr.register_variable(
            "Zone Mean Air Temperature",
            "Zone1",
            "C",
            StoreType::Average,
            TimeStepType::Zone,
        );
        assert_eq!(idx, 0);
        assert_eq!(mgr.variable_count(), 1);

        // Set value and accumulate
        mgr.variables[idx].set_value(22.0);
        let ts = TimeStamp { month: 1, day: 1, hour: 1, minute: 0 };
        mgr.update_data(TimeStepType::Zone, 1.0, &ts);

        // Add request and resolve
        mgr.add_request(OutputRequest {
            key: "*".to_string(),
            variable_name: "Zone Mean Air Temperature".to_string(),
            freq: ReportFreq::Hourly,
        });
        mgr.resolve_requests();

        assert_eq!(mgr.variables[0].freq, ReportFreq::Hourly);

        let results = mgr.report_variables(ReportFreq::Hourly);
        assert_eq!(results.len(), 1);
        assert!((results[0].1 - 22.0).abs() < 1e-10);
    }

    #[test]
    fn resolve_requests_wildcard() {
        let mut mgr = OutputManager::new();
        mgr.register_variable("Temp", "Z1", "C", StoreType::Average, TimeStepType::Zone);
        mgr.register_variable("Temp", "Z2", "C", StoreType::Average, TimeStepType::Zone);
        mgr.register_variable("Humidity", "Z1", "%", StoreType::Average, TimeStepType::Zone);

        mgr.add_request(OutputRequest {
            key: "*".to_string(),
            variable_name: "Temp".to_string(),
            freq: ReportFreq::Daily,
        });
        mgr.resolve_requests();

        assert_eq!(mgr.variables[0].freq, ReportFreq::Daily);
        assert_eq!(mgr.variables[1].freq, ReportFreq::Daily);
        // Humidity should still be default (Hourly)
        assert_eq!(mgr.variables[2].freq, ReportFreq::Hourly);
    }

    #[test]
    fn resolve_requests_specific_key() {
        let mut mgr = OutputManager::new();
        mgr.register_variable("Temp", "Z1", "C", StoreType::Average, TimeStepType::Zone);
        mgr.register_variable("Temp", "Z2", "C", StoreType::Average, TimeStepType::Zone);

        mgr.add_request(OutputRequest {
            key: "Z1".to_string(),
            variable_name: "Temp".to_string(),
            freq: ReportFreq::Monthly,
        });
        mgr.resolve_requests();

        assert_eq!(mgr.variables[0].freq, ReportFreq::Monthly);
        assert_eq!(mgr.variables[1].freq, ReportFreq::Hourly); // unchanged
    }

    #[test]
    fn update_only_matching_timestep_type() {
        let mut mgr = OutputManager::new();
        let zone_idx = mgr.register_variable("ZoneTemp", "Z1", "C", StoreType::Sum, TimeStepType::Zone);
        let sys_idx = mgr.register_variable("SysFlow", "AHU1", "m3/s", StoreType::Sum, TimeStepType::System);

        mgr.variables[zone_idx].set_value(100.0);
        mgr.variables[sys_idx].set_value(200.0);

        let ts = TimeStamp { month: 1, day: 1, hour: 1, minute: 0 };
        mgr.update_data(TimeStepType::Zone, 1.0, &ts);

        // Only zone var should have accumulated
        assert!((mgr.variables[zone_idx].stored_value - 100.0).abs() < 1e-10);
        assert!((mgr.variables[sys_idx].stored_value).abs() < 1e-10);
    }

    #[test]
    fn report_id_auto_increment() {
        let mut mgr = OutputManager::new();
        mgr.register_variable("A", "K1", "W", StoreType::Sum, TimeStepType::Zone);
        mgr.register_variable("B", "K2", "W", StoreType::Sum, TimeStepType::Zone);
        mgr.register_variable("C", "K3", "W", StoreType::Sum, TimeStepType::Zone);

        assert_eq!(mgr.variables[0].report_id, 1);
        assert_eq!(mgr.variables[1].report_id, 2);
        assert_eq!(mgr.variables[2].report_id, 3);
    }
}
