//! HVAC convergence monitoring.
//!
//! Tracks zone loads and system capacities across iterations to detect
//! convergence and oscillation.

/// Convergence check result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvergenceStatus {
    /// All loads within tolerance.
    Converged,
    /// Still converging — monotonically decreasing residual.
    Converging,
    /// Oscillating between two states.
    Oscillating,
    /// Not converging (increasing residual).
    Diverging,
}

/// Tracks convergence of a single variable across HVAC iterations.
#[derive(Debug, Clone)]
pub struct ConvergenceTracker {
    /// History of values across iterations.
    history: Vec<f64>,
    /// Tolerance for convergence.
    tolerance: f64,
    /// Maximum iterations to track.
    max_history: usize,
}

impl ConvergenceTracker {
    pub fn new(tolerance: f64) -> Self {
        Self {
            history: Vec::new(),
            tolerance,
            max_history: 30,
        }
    }

    /// Reset for a new timestep.
    pub fn reset(&mut self) {
        self.history.clear();
    }

    /// Record a new iteration value.
    pub fn record(&mut self, value: f64) {
        if self.history.len() >= self.max_history {
            self.history.remove(0);
        }
        self.history.push(value);
    }

    /// Check the current convergence status.
    pub fn status(&self) -> ConvergenceStatus {
        let n = self.history.len();
        if n < 2 {
            return ConvergenceStatus::Converging;
        }

        let current = self.history[n - 1];
        let previous = self.history[n - 2];
        let change = (current - previous).abs();

        if change < self.tolerance {
            return ConvergenceStatus::Converged;
        }

        // Check for oscillation (alternating pattern)
        if n >= 3 {
            let before_previous = self.history[n - 3];
            let prev_change = (previous - before_previous).abs();
            let sign_current = (current - previous).signum();
            let sign_prev = (previous - before_previous).signum();

            if sign_current != sign_prev && prev_change > self.tolerance {
                return ConvergenceStatus::Oscillating;
            }
        }

        // Check for diverging (increasing residual)
        if n >= 3 {
            let before_previous = self.history[n - 3];
            let prev_change = (previous - before_previous).abs();
            if change > prev_change * 1.1 {
                return ConvergenceStatus::Diverging;
            }
        }

        ConvergenceStatus::Converging
    }

    /// Current value.
    pub fn current(&self) -> Option<f64> {
        self.history.last().copied()
    }

    /// Number of iterations recorded.
    pub fn iteration_count(&self) -> usize {
        self.history.len()
    }
}

/// Multi-zone convergence monitor.
#[derive(Debug, Clone)]
pub struct ZoneConvergenceMonitor {
    /// Per-zone temperature trackers.
    pub zone_temps: Vec<ConvergenceTracker>,
    /// Per-zone load trackers.
    pub zone_loads: Vec<ConvergenceTracker>,
}

impl ZoneConvergenceMonitor {
    pub fn new(num_zones: usize, temp_tolerance: f64, load_tolerance: f64) -> Self {
        Self {
            zone_temps: (0..num_zones).map(|_| ConvergenceTracker::new(temp_tolerance)).collect(),
            zone_loads: (0..num_zones).map(|_| ConvergenceTracker::new(load_tolerance)).collect(),
        }
    }

    /// Reset all trackers for a new timestep.
    pub fn reset(&mut self) {
        for t in &mut self.zone_temps {
            t.reset();
        }
        for t in &mut self.zone_loads {
            t.reset();
        }
    }

    /// Record iteration values for a zone.
    pub fn record_zone(&mut self, zone_index: usize, temperature: f64, load: f64) {
        if zone_index < self.zone_temps.len() {
            self.zone_temps[zone_index].record(temperature);
            self.zone_loads[zone_index].record(load);
        }
    }

    /// Check if all zones have converged.
    pub fn all_converged(&self) -> bool {
        self.zone_temps.iter().all(|t| t.status() == ConvergenceStatus::Converged)
            && self.zone_loads.iter().all(|t| t.status() == ConvergenceStatus::Converged)
    }

    /// Check if any zone is oscillating.
    pub fn any_oscillating(&self) -> bool {
        self.zone_temps.iter().any(|t| t.status() == ConvergenceStatus::Oscillating)
            || self.zone_loads.iter().any(|t| t.status() == ConvergenceStatus::Oscillating)
    }

    /// Maximum residual across all zones.
    pub fn max_residual(&self) -> f64 {
        let mut max_r = 0.0_f64;
        for tracker in self.zone_loads.iter() {
            let n = tracker.iteration_count();
            if n >= 2 {
                let current = tracker.current().unwrap_or(0.0);
                if let Some(&prev) = tracker.history.get(n - 2) {
                    max_r = max_r.max((current - prev).abs());
                }
            }
        }
        max_r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_converges() {
        let mut tracker = ConvergenceTracker::new(0.1);
        tracker.record(100.0);
        tracker.record(50.0);
        tracker.record(25.05);
        tracker.record(25.0);
        assert_eq!(tracker.status(), ConvergenceStatus::Converged);
    }

    #[test]
    fn tracker_oscillating() {
        let mut tracker = ConvergenceTracker::new(0.1);
        tracker.record(10.0);
        tracker.record(20.0);
        tracker.record(10.0);
        assert_eq!(tracker.status(), ConvergenceStatus::Oscillating);
    }

    #[test]
    fn tracker_converging() {
        let mut tracker = ConvergenceTracker::new(0.01);
        tracker.record(100.0);
        tracker.record(50.0);
        assert_eq!(tracker.status(), ConvergenceStatus::Converging);
    }

    #[test]
    fn tracker_reset() {
        let mut tracker = ConvergenceTracker::new(0.1);
        tracker.record(10.0);
        tracker.record(20.0);
        assert_eq!(tracker.iteration_count(), 2);
        tracker.reset();
        assert_eq!(tracker.iteration_count(), 0);
    }

    #[test]
    fn zone_monitor_all_converged() {
        let mut monitor = ZoneConvergenceMonitor::new(2, 0.1, 1.0);
        // Zone 0
        monitor.record_zone(0, 22.0, 100.0);
        monitor.record_zone(0, 22.0, 100.0);
        // Zone 1
        monitor.record_zone(1, 24.0, 200.0);
        monitor.record_zone(1, 24.0, 200.0);

        assert!(monitor.all_converged());
        assert!(!monitor.any_oscillating());
    }

    #[test]
    fn zone_monitor_partial_convergence() {
        let mut monitor = ZoneConvergenceMonitor::new(2, 0.01, 0.01);
        // Zone 0: converged
        monitor.record_zone(0, 22.0, 100.0);
        monitor.record_zone(0, 22.0, 100.0);
        // Zone 1: not converged
        monitor.record_zone(1, 24.0, 200.0);
        monitor.record_zone(1, 26.0, 300.0);

        assert!(!monitor.all_converged());
    }
}
