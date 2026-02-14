//! Output variable definitions and value accumulation.

/// Reporting frequency for output variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReportFreq {
    EachCall,
    TimeStep,
    Hourly,
    Daily,
    Monthly,
    RunPeriod,
    Annual,
}

impl ReportFreq {
    pub fn name(&self) -> &'static str {
        match self {
            ReportFreq::EachCall => "Each Call",
            ReportFreq::TimeStep => "TimeStep",
            ReportFreq::Hourly => "Hourly",
            ReportFreq::Daily => "Daily",
            ReportFreq::Monthly => "Monthly",
            ReportFreq::RunPeriod => "Run Period",
            ReportFreq::Annual => "Annual",
        }
    }

    /// Parse from a loose string (case-insensitive, accepts common aliases).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "detailed" | "each call" | "eachcall" => Some(ReportFreq::EachCall),
            "timestep" | "time step" => Some(ReportFreq::TimeStep),
            "hourly" => Some(ReportFreq::Hourly),
            "daily" => Some(ReportFreq::Daily),
            "monthly" => Some(ReportFreq::Monthly),
            "runperiod" | "run period" | "environment" => Some(ReportFreq::RunPeriod),
            "annual" | "yearly" => Some(ReportFreq::Annual),
            _ => None,
        }
    }
}

/// How a variable's value is aggregated across timesteps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreType {
    /// Time-weighted average over the reporting interval.
    Average,
    /// Sum over the reporting interval.
    Sum,
}

/// Which simulation loop the variable belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeStepType {
    Zone,
    System,
}

/// Timestamp for min/max tracking and ESO output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimeStamp {
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

impl TimeStamp {
    /// Encode as a single integer for compact storage: MMDDHHMI.
    pub fn encode(&self) -> u64 {
        (self.month as u64) * 1_000_000
            + (self.day as u64) * 10_000
            + (self.hour as u64) * 100
            + self.minute as u64
    }
}

/// A registered output variable with accumulation state.
#[derive(Debug, Clone)]
pub struct OutputVariable {
    pub report_id: usize,
    pub name: String,
    pub key: String,
    pub units: String,
    pub store_type: StoreType,
    pub time_step_type: TimeStepType,
    pub freq: ReportFreq,
    /// Current instantaneous value (set each call).
    pub value: f64,
    /// Accumulated stored value for the current reporting interval.
    pub stored_value: f64,
    /// Number of stored samples in the current interval.
    pub num_stored: usize,
    /// Minimum value in current interval.
    pub min_value: f64,
    pub min_timestamp: TimeStamp,
    /// Maximum value in current interval.
    pub max_value: f64,
    pub max_timestamp: TimeStamp,
    /// Indices of meters this variable feeds into.
    pub meter_indices: Vec<usize>,
}

impl OutputVariable {
    pub fn new(
        report_id: usize,
        name: &str,
        key: &str,
        units: &str,
        store_type: StoreType,
        time_step_type: TimeStepType,
    ) -> Self {
        Self {
            report_id,
            name: name.to_string(),
            key: key.to_string(),
            units: units.to_string(),
            store_type,
            time_step_type,
            freq: ReportFreq::Hourly,
            value: 0.0,
            stored_value: 0.0,
            num_stored: 0,
            min_value: f64::MAX,
            min_timestamp: TimeStamp::default(),
            max_value: f64::MIN,
            max_timestamp: TimeStamp::default(),
            meter_indices: Vec::new(),
        }
    }

    /// Set the current instantaneous value.
    pub fn set_value(&mut self, val: f64) {
        self.value = val;
    }

    /// Accumulate the current value into the stored interval.
    /// For Average variables, `time_fraction` weights the sample.
    /// For Sum variables, the value is added directly.
    pub fn accumulate(&mut self, time_fraction: f64, timestamp: &TimeStamp) {
        match self.store_type {
            StoreType::Average => {
                self.stored_value += self.value * time_fraction;
            }
            StoreType::Sum => {
                self.stored_value += self.value;
            }
        }
        self.num_stored += 1;

        if self.value < self.min_value {
            self.min_value = self.value;
            self.min_timestamp = timestamp.clone();
        }
        if self.value > self.max_value {
            self.max_value = self.value;
            self.max_timestamp = timestamp.clone();
        }
    }

    /// Report the accumulated value and reset for the next interval.
    /// Returns the reportable value.
    pub fn report_and_reset(&mut self) -> f64 {
        let result = self.stored_value;
        self.stored_value = 0.0;
        self.num_stored = 0;
        self.min_value = f64::MAX;
        self.min_timestamp = TimeStamp::default();
        self.max_value = f64::MIN;
        self.max_timestamp = TimeStamp::default();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulate_average() {
        let mut var = OutputVariable::new(1, "Zone Mean Air Temperature", "Zone1", "C", StoreType::Average, TimeStepType::Zone);
        let ts = TimeStamp { month: 1, day: 1, hour: 1, minute: 0 };

        var.set_value(20.0);
        var.accumulate(0.5, &ts);
        var.set_value(22.0);
        var.accumulate(0.5, &ts);

        // Average: 20*0.5 + 22*0.5 = 21.0
        assert!((var.stored_value - 21.0).abs() < 1e-10);
        assert_eq!(var.num_stored, 2);
    }

    #[test]
    fn accumulate_sum() {
        let mut var = OutputVariable::new(2, "Zone Electricity", "Zone1", "J", StoreType::Sum, TimeStepType::Zone);
        let ts = TimeStamp { month: 1, day: 1, hour: 1, minute: 0 };

        var.set_value(100.0);
        var.accumulate(1.0, &ts);
        var.set_value(200.0);
        var.accumulate(1.0, &ts);

        assert!((var.stored_value - 300.0).abs() < 1e-10);
    }

    #[test]
    fn min_max_tracking() {
        let mut var = OutputVariable::new(3, "Temp", "Z1", "C", StoreType::Average, TimeStepType::Zone);
        let ts1 = TimeStamp { month: 7, day: 15, hour: 6, minute: 0 };
        let ts2 = TimeStamp { month: 7, day: 15, hour: 14, minute: 0 };
        let ts3 = TimeStamp { month: 7, day: 15, hour: 20, minute: 0 };

        var.set_value(18.0);
        var.accumulate(1.0, &ts1);
        var.set_value(35.0);
        var.accumulate(1.0, &ts2);
        var.set_value(25.0);
        var.accumulate(1.0, &ts3);

        assert!((var.min_value - 18.0).abs() < 1e-10);
        assert_eq!(var.min_timestamp.hour, 6);
        assert!((var.max_value - 35.0).abs() < 1e-10);
        assert_eq!(var.max_timestamp.hour, 14);
    }

    #[test]
    fn report_and_reset() {
        let mut var = OutputVariable::new(4, "Temp", "Z1", "C", StoreType::Sum, TimeStepType::Zone);
        let ts = TimeStamp { month: 1, day: 1, hour: 1, minute: 0 };

        var.set_value(50.0);
        var.accumulate(1.0, &ts);
        var.set_value(30.0);
        var.accumulate(1.0, &ts);

        let val = var.report_and_reset();
        assert!((val - 80.0).abs() < 1e-10);
        assert_eq!(var.num_stored, 0);
        assert!((var.stored_value).abs() < 1e-10);
        assert_eq!(var.min_value, f64::MAX);
    }

    #[test]
    fn timestamp_encode() {
        let ts = TimeStamp { month: 7, day: 21, hour: 14, minute: 30 };
        assert_eq!(ts.encode(), 7_21_14_30);
        // More precise: 7*1_000_000 + 21*10_000 + 14*100 + 30
        assert_eq!(ts.encode(), 7_211_430);
    }

    #[test]
    fn report_freq_from_str() {
        assert_eq!(ReportFreq::from_str_loose("Hourly"), Some(ReportFreq::Hourly));
        assert_eq!(ReportFreq::from_str_loose("hourly"), Some(ReportFreq::Hourly));
        assert_eq!(ReportFreq::from_str_loose("TIMESTEP"), Some(ReportFreq::TimeStep));
        assert_eq!(ReportFreq::from_str_loose("Detailed"), Some(ReportFreq::EachCall));
        assert_eq!(ReportFreq::from_str_loose("RunPeriod"), Some(ReportFreq::RunPeriod));
        assert_eq!(ReportFreq::from_str_loose("Annual"), Some(ReportFreq::Annual));
        assert_eq!(ReportFreq::from_str_loose("invalid"), None);
    }

    #[test]
    fn zero_stored_edge_case() {
        let mut var = OutputVariable::new(5, "Flow", "Z1", "m3/s", StoreType::Average, TimeStepType::System);
        // Report without any accumulation
        let val = var.report_and_reset();
        assert!((val).abs() < 1e-10);
        assert_eq!(var.num_stored, 0);
    }
}
