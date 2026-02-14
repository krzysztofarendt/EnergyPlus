//! EMS trend variable — time-series logging of ERL variable values.
//!
//! Trend variables maintain a circular buffer of historical values,
//! supporting lookback analysis (average, min, max, sum, direction).

/// A trend variable that logs values over time.
#[derive(Debug, Clone)]
pub struct TrendVariable {
    pub name: String,
    /// Index into the variable manager.
    pub variable_index: Option<usize>,
    /// Circular buffer of logged values (index 0 = most recent).
    values: Vec<f64>,
    /// Maximum depth (number of timesteps retained).
    pub log_depth: usize,
    /// Number of values actually recorded.
    count: usize,
}

impl TrendVariable {
    pub fn new(name: impl Into<String>, log_depth: usize) -> Self {
        Self {
            name: name.into(),
            variable_index: None,
            values: vec![0.0; log_depth],
            log_depth,
            count: 0,
        }
    }

    /// Push a new value, shifting older values forward.
    pub fn push(&mut self, value: f64) {
        // Shift existing values right
        for i in (1..self.log_depth).rev() {
            self.values[i] = self.values[i - 1];
        }
        if self.log_depth > 0 {
            self.values[0] = value;
        }
        if self.count < self.log_depth {
            self.count += 1;
        }
    }

    /// Get value at a past timestep (0 = most recent, 1 = one step back, etc.).
    pub fn value_at(&self, steps_back: usize) -> Option<f64> {
        if steps_back < self.count {
            Some(self.values[steps_back])
        } else {
            None
        }
    }

    /// Average of the last `n` values.
    pub fn average(&self, n: usize) -> Option<f64> {
        let n = n.min(self.count);
        if n == 0 {
            return None;
        }
        let sum: f64 = self.values[..n].iter().sum();
        Some(sum / n as f64)
    }

    /// Maximum of the last `n` values.
    pub fn max(&self, n: usize) -> Option<f64> {
        let n = n.min(self.count);
        if n == 0 {
            return None;
        }
        self.values[..n].iter().copied().reduce(f64::max)
    }

    /// Minimum of the last `n` values.
    pub fn min(&self, n: usize) -> Option<f64> {
        let n = n.min(self.count);
        if n == 0 {
            return None;
        }
        self.values[..n].iter().copied().reduce(f64::min)
    }

    /// Sum of the last `n` values.
    pub fn sum(&self, n: usize) -> Option<f64> {
        let n = n.min(self.count);
        if n == 0 {
            return None;
        }
        Some(self.values[..n].iter().sum())
    }

    /// Linear regression slope (direction) of the last `n` values.
    /// Positive = increasing, negative = decreasing.
    pub fn direction(&self, n: usize) -> Option<f64> {
        let n = n.min(self.count);
        if n < 2 {
            return None;
        }
        // Linear regression: y = a + b*x where x = 0..n-1
        let n_f = n as f64;
        let sum_x: f64 = (0..n).map(|i| i as f64).sum();
        let sum_y: f64 = self.values[..n].iter().sum();
        let sum_xy: f64 = (0..n).map(|i| i as f64 * self.values[i]).sum();
        let sum_x2: f64 = (0..n).map(|i| (i as f64) * (i as f64)).sum();

        let denom = n_f * sum_x2 - sum_x * sum_x;
        if denom.abs() < 1e-20 {
            return Some(0.0);
        }
        // Note: since index 0 is most recent and index n-1 is oldest,
        // a positive slope means values are increasing toward the past,
        // so we negate to get the temporal direction.
        Some(-(n_f * sum_xy - sum_x * sum_y) / denom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trend_push_and_read() {
        let mut trend = TrendVariable::new("T1", 5);
        trend.push(10.0);
        trend.push(20.0);
        trend.push(30.0);

        assert!((trend.value_at(0).unwrap() - 30.0).abs() < 1e-10);
        assert!((trend.value_at(1).unwrap() - 20.0).abs() < 1e-10);
        assert!((trend.value_at(2).unwrap() - 10.0).abs() < 1e-10);
        assert!(trend.value_at(3).is_none());
    }

    #[test]
    fn trend_overflow() {
        let mut trend = TrendVariable::new("T", 3);
        trend.push(1.0);
        trend.push(2.0);
        trend.push(3.0);
        trend.push(4.0); // Oldest (1.0) dropped

        assert!((trend.value_at(0).unwrap() - 4.0).abs() < 1e-10);
        assert!((trend.value_at(2).unwrap() - 2.0).abs() < 1e-10);
    }

    #[test]
    fn trend_statistics() {
        let mut trend = TrendVariable::new("T", 10);
        for v in [10.0, 20.0, 30.0, 40.0, 50.0] {
            trend.push(v);
        }

        assert!((trend.average(5).unwrap() - 30.0).abs() < 1e-10);
        assert!((trend.max(5).unwrap() - 50.0).abs() < 1e-10);
        assert!((trend.min(5).unwrap() - 10.0).abs() < 1e-10);
        assert!((trend.sum(5).unwrap() - 150.0).abs() < 1e-10);
    }

    #[test]
    fn trend_direction() {
        let mut trend = TrendVariable::new("T", 10);
        // Push increasing values: 1, 2, 3, 4, 5
        // In buffer: [5, 4, 3, 2, 1]
        // Temporal direction is increasing → positive
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            trend.push(v);
        }
        let dir = trend.direction(5).unwrap();
        assert!(dir > 0.0, "direction={}", dir);
    }

    #[test]
    fn trend_empty_stats() {
        let trend = TrendVariable::new("T", 5);
        assert!(trend.average(3).is_none());
        assert!(trend.max(3).is_none());
        assert!(trend.direction(3).is_none());
    }
}
