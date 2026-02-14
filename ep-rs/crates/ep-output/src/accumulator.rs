//! Period accumulators for aggregating values over reporting intervals.

use crate::variables::TimeStamp;

/// Accumulates a value over a reporting period, tracking sum, count,
/// and min/max with timestamps.
#[derive(Debug, Clone)]
pub struct PeriodAccumulator {
    pub value_sum: f64,
    pub count: usize,
    pub min_value: f64,
    pub min_timestamp: TimeStamp,
    pub max_value: f64,
    pub max_timestamp: TimeStamp,
}

impl Default for PeriodAccumulator {
    fn default() -> Self {
        Self {
            value_sum: 0.0,
            count: 0,
            min_value: f64::MAX,
            min_timestamp: TimeStamp::default(),
            max_value: f64::MIN,
            max_timestamp: TimeStamp::default(),
        }
    }
}

impl PeriodAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a value sample at the given timestamp.
    pub fn add(&mut self, value: f64, timestamp: &TimeStamp) {
        self.value_sum += value;
        self.count += 1;
        if value < self.min_value {
            self.min_value = value;
            self.min_timestamp = timestamp.clone();
        }
        if value > self.max_value {
            self.max_value = value;
            self.max_timestamp = timestamp.clone();
        }
    }

    /// Get the average value, or 0.0 if no samples.
    pub fn average(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.value_sum / self.count as f64
        }
    }

    /// Reset the accumulator for a new period.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_accumulation() {
        let mut acc = PeriodAccumulator::new();
        let ts1 = TimeStamp { month: 1, day: 1, hour: 1, minute: 0 };
        let ts2 = TimeStamp { month: 1, day: 1, hour: 2, minute: 0 };
        let ts3 = TimeStamp { month: 1, day: 1, hour: 3, minute: 0 };

        acc.add(10.0, &ts1);
        acc.add(20.0, &ts2);
        acc.add(15.0, &ts3);

        assert_eq!(acc.count, 3);
        assert!((acc.value_sum - 45.0).abs() < 1e-10);
        assert!((acc.average() - 15.0).abs() < 1e-10);
        assert!((acc.min_value - 10.0).abs() < 1e-10);
        assert!((acc.max_value - 20.0).abs() < 1e-10);
        assert_eq!(acc.min_timestamp.hour, 1);
        assert_eq!(acc.max_timestamp.hour, 2);
    }

    #[test]
    fn reset_clears_state() {
        let mut acc = PeriodAccumulator::new();
        let ts = TimeStamp { month: 1, day: 1, hour: 1, minute: 0 };
        acc.add(42.0, &ts);
        acc.reset();

        assert_eq!(acc.count, 0);
        assert!((acc.value_sum).abs() < 1e-10);
        assert_eq!(acc.min_value, f64::MAX);
        assert_eq!(acc.max_value, f64::MIN);
    }

    #[test]
    fn single_value() {
        let mut acc = PeriodAccumulator::new();
        let ts = TimeStamp { month: 6, day: 15, hour: 12, minute: 30 };
        acc.add(100.0, &ts);

        assert_eq!(acc.count, 1);
        assert!((acc.average() - 100.0).abs() < 1e-10);
        assert!((acc.min_value - 100.0).abs() < 1e-10);
        assert!((acc.max_value - 100.0).abs() < 1e-10);
    }

    #[test]
    fn empty_average_is_zero() {
        let acc = PeriodAccumulator::new();
        assert!((acc.average()).abs() < 1e-10);
    }
}
