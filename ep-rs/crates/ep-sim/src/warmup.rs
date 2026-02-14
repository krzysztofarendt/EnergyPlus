//! Warmup convergence logic.
//!
//! Repeats the first simulation day until zone temperatures and
//! heating/cooling loads converge between successive warmup days.

use ep_core::state::SimulationState;
use crate::{SimulationCallback, SimulationConfig};

/// Warmup convergence tracker.
#[derive(Debug, Clone)]
pub struct WarmupTracker {
    /// Previous day's min zone temperature (C).
    pub prev_min_temp: f64,
    /// Previous day's max zone temperature (C).
    pub prev_max_temp: f64,
    /// Previous day's total heating load (W).
    pub prev_heating_load: f64,
    /// Previous day's total cooling load (W).
    pub prev_cooling_load: f64,
    /// Current day's values.
    pub curr_min_temp: f64,
    pub curr_max_temp: f64,
    pub curr_heating_load: f64,
    pub curr_cooling_load: f64,
    /// Number of consecutive converged days.
    pub converged_days: u32,
    /// Days needed for warmup convergence (typically 3 consecutive converged days).
    pub required_converged_days: u32,
}

impl Default for WarmupTracker {
    fn default() -> Self {
        Self {
            prev_min_temp: -999.0,
            prev_max_temp: -999.0,
            prev_heating_load: -999.0,
            prev_cooling_load: -999.0,
            curr_min_temp: 0.0,
            curr_max_temp: 0.0,
            curr_heating_load: 0.0,
            curr_cooling_load: 0.0,
            converged_days: 0,
            required_converged_days: 3,
        }
    }
}

impl WarmupTracker {
    /// Begin a new warmup day — reset current values.
    pub fn begin_day(&mut self) {
        self.curr_min_temp = f64::MAX;
        self.curr_max_temp = f64::MIN;
        self.curr_heating_load = 0.0;
        self.curr_cooling_load = 0.0;
    }

    /// Update with a zone temperature reading.
    pub fn update_temperature(&mut self, temp_c: f64) {
        self.curr_min_temp = self.curr_min_temp.min(temp_c);
        self.curr_max_temp = self.curr_max_temp.max(temp_c);
    }

    /// Update with a heating/cooling load.
    pub fn update_loads(&mut self, heating: f64, cooling: f64) {
        self.curr_heating_load += heating;
        self.curr_cooling_load += cooling;
    }

    /// Check if the warmup day has converged compared to previous day.
    pub fn check_convergence(&mut self, tolerance: f64) -> bool {
        // Skip first day (no previous data)
        if self.prev_min_temp < -900.0 {
            self.advance_day();
            return false;
        }

        let temp_converged = self.check_relative(self.curr_min_temp, self.prev_min_temp, tolerance)
            && self.check_relative(self.curr_max_temp, self.prev_max_temp, tolerance);

        let load_converged = self.check_relative(self.curr_heating_load, self.prev_heating_load, tolerance)
            && self.check_relative(self.curr_cooling_load, self.prev_cooling_load, tolerance);

        if temp_converged && load_converged {
            self.converged_days += 1;
        } else {
            self.converged_days = 0;
        }

        self.advance_day();
        self.converged_days >= self.required_converged_days
    }

    fn advance_day(&mut self) {
        self.prev_min_temp = self.curr_min_temp;
        self.prev_max_temp = self.curr_max_temp;
        self.prev_heating_load = self.curr_heating_load;
        self.prev_cooling_load = self.curr_cooling_load;
    }

    fn check_relative(&self, current: f64, previous: f64, tolerance: f64) -> bool {
        if previous.abs() < 1e-10 {
            current.abs() < 1e-10
        } else {
            ((current - previous) / previous).abs() < tolerance
        }
    }
}

/// Run the warmup phase for an environment.
/// Returns the number of warmup days executed.
pub fn run_warmup(
    state: &mut SimulationState,
    callback: &mut dyn SimulationCallback,
    config: &SimulationConfig,
) -> u32 {
    state.flags.warmup = true;
    let mut tracker = WarmupTracker::default();
    let tph = config.timesteps_per_hour;

    for day in 1..=config.max_warmup_days {
        tracker.begin_day();
        state.flags.begin_day = true;
        callback.begin_day(state);
        state.flags.begin_day = false;

        // Run through all hours and timesteps
        for hour in 0..24u8 {
            state.clock.hour_of_day = hour;
            for ts in 0..tph {
                state.clock.timestep_in_hour = ts;
                callback.do_timestep(state);
                let _ = callback.do_hvac_iteration(state);
            }
        }

        callback.end_day(state);

        // Check convergence after minimum warmup days
        if day >= config.min_warmup_days {
            if tracker.check_convergence(config.warmup_tolerance) {
                return day;
            }
        } else {
            tracker.check_convergence(config.warmup_tolerance);
        }
    }

    config.max_warmup_days
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warmup_tracker_convergence() {
        let mut tracker = WarmupTracker::default();
        tracker.required_converged_days = 2;

        // Day 1: no previous data
        tracker.begin_day();
        tracker.update_temperature(20.0);
        tracker.update_temperature(25.0);
        tracker.update_loads(1000.0, 500.0);
        assert!(!tracker.check_convergence(0.04));

        // Day 2: same values -> converged day 1
        tracker.begin_day();
        tracker.update_temperature(20.0);
        tracker.update_temperature(25.0);
        tracker.update_loads(1000.0, 500.0);
        assert!(!tracker.check_convergence(0.04)); // Need 2 consecutive

        // Day 3: same values -> converged day 2 -> done
        tracker.begin_day();
        tracker.update_temperature(20.0);
        tracker.update_temperature(25.0);
        tracker.update_loads(1000.0, 500.0);
        assert!(tracker.check_convergence(0.04));
    }

    #[test]
    fn warmup_tracker_divergence_resets() {
        let mut tracker = WarmupTracker::default();
        tracker.required_converged_days = 2;

        // Day 1
        tracker.begin_day();
        tracker.update_temperature(20.0);
        tracker.update_temperature(25.0);
        tracker.update_loads(1000.0, 0.0);
        tracker.check_convergence(0.04);

        // Day 2: converged
        tracker.begin_day();
        tracker.update_temperature(20.0);
        tracker.update_temperature(25.0);
        tracker.update_loads(1000.0, 0.0);
        tracker.check_convergence(0.04);
        assert_eq!(tracker.converged_days, 1);

        // Day 3: diverged — big load change
        tracker.begin_day();
        tracker.update_temperature(20.0);
        tracker.update_temperature(25.0);
        tracker.update_loads(5000.0, 0.0);
        tracker.check_convergence(0.04);
        assert_eq!(tracker.converged_days, 0); // Reset
    }

    #[test]
    fn warmup_tracker_zero_loads() {
        let mut tracker = WarmupTracker::default();
        tracker.required_converged_days = 1;

        // Day 1: zero loads
        tracker.begin_day();
        tracker.update_temperature(22.0);
        tracker.update_loads(0.0, 0.0);
        tracker.check_convergence(0.04);

        // Day 2: still zero loads -> converged
        tracker.begin_day();
        tracker.update_temperature(22.0);
        tracker.update_loads(0.0, 0.0);
        assert!(tracker.check_convergence(0.04));
    }

    #[test]
    fn warmup_begin_day_resets() {
        let mut tracker = WarmupTracker::default();
        tracker.curr_min_temp = 10.0;
        tracker.curr_max_temp = 30.0;
        tracker.curr_heating_load = 5000.0;
        tracker.curr_cooling_load = 3000.0;

        tracker.begin_day();

        assert_eq!(tracker.curr_min_temp, f64::MAX);
        assert_eq!(tracker.curr_max_temp, f64::MIN);
        assert!((tracker.curr_heating_load - 0.0).abs() < 1e-10);
        assert!((tracker.curr_cooling_load - 0.0).abs() < 1e-10);
    }

    #[test]
    fn warmup_update_temperature_tracking() {
        let mut tracker = WarmupTracker::default();
        tracker.begin_day();

        tracker.update_temperature(20.0);
        tracker.update_temperature(25.0);
        tracker.update_temperature(18.0);
        tracker.update_temperature(22.0);

        assert!((tracker.curr_min_temp - 18.0).abs() < 1e-10);
        assert!((tracker.curr_max_temp - 25.0).abs() < 1e-10);
    }

    #[test]
    fn warmup_update_loads_accumulation() {
        let mut tracker = WarmupTracker::default();
        tracker.begin_day();

        tracker.update_loads(1000.0, 500.0);
        tracker.update_loads(2000.0, 300.0);

        assert!((tracker.curr_heating_load - 3000.0).abs() < 1e-10);
        assert!((tracker.curr_cooling_load - 800.0).abs() < 1e-10);
    }

    #[test]
    fn warmup_convergence_tight_tolerance() {
        let mut tracker = WarmupTracker::default();
        tracker.required_converged_days = 1;

        // Day 1
        tracker.begin_day();
        tracker.update_temperature(20.0);
        tracker.update_temperature(25.0);
        tracker.update_loads(1000.0, 500.0);
        tracker.check_convergence(0.001);

        // Day 2: 0.5% temperature difference (min: 20.0 vs 20.1, max: 25.0 vs 25.125)
        tracker.begin_day();
        tracker.update_temperature(20.1);
        tracker.update_temperature(25.125);
        tracker.update_loads(1000.0, 500.0);
        // 0.5% relative diff exceeds 0.1% tolerance
        assert!(!tracker.check_convergence(0.001));
    }

    #[test]
    fn warmup_convergence_fail_tolerance() {
        let mut tracker = WarmupTracker::default();
        tracker.required_converged_days = 1;

        // Day 1
        tracker.begin_day();
        tracker.update_temperature(20.0);
        tracker.update_temperature(25.0);
        tracker.update_loads(1000.0, 500.0);
        tracker.check_convergence(0.04);

        // Day 2: 5% temperature difference
        tracker.begin_day();
        tracker.update_temperature(19.0); // 5% below 20.0
        tracker.update_temperature(25.0);
        tracker.update_loads(1000.0, 500.0);
        // 5% relative diff exceeds 4% tolerance
        assert!(!tracker.check_convergence(0.04));
    }

    #[test]
    fn run_warmup_min_days_enforced() {
        struct NullCallback;
        impl SimulationCallback for NullCallback {}

        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_warmup_days: 25,
            min_warmup_days: 5,
            warmup_tolerance: 0.04,
            ..Default::default()
        };

        let mut state = SimulationState::new(1);
        let mut callback = NullCallback;

        // NullCallback does nothing, so tracker gets zero loads and zero-ish temps.
        // Even if convergence could happen early, min_warmup_days must be respected.
        let warmup_days = run_warmup(&mut state, &mut callback, &config);

        // Must run at least min_warmup_days (5)
        assert!(warmup_days >= config.min_warmup_days,
                "warmup ran {} days but min is {}", warmup_days, config.min_warmup_days);
    }
}
