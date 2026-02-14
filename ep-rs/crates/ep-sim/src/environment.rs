//! Environment management — design days and run periods.
//!
//! An "environment" in EnergyPlus is a contiguous simulation period:
//! a single design day or a weather-file run period.

use ep_core::state::EnvironmentType;
use ep_core::time::Weekday;

/// Environment definition for the simulation.
#[derive(Debug, Clone)]
pub struct Environment {
    pub name: String,
    pub env_type: EnvironmentType,
    pub start_month: u8,
    pub start_day: u8,
    pub end_month: u8,
    pub end_day: u8,
    pub start_day_of_week: Weekday,
    pub total_days: u32,
    /// Is this a sizing period?
    pub is_sizing: bool,
}

impl Environment {
    /// Create a design day environment.
    pub fn design_day(name: impl Into<String>, month: u8, day: u8) -> Self {
        Self {
            name: name.into(),
            env_type: EnvironmentType::DesignDay,
            start_month: month,
            start_day: day,
            end_month: month,
            end_day: day,
            start_day_of_week: Weekday::Monday,
            total_days: 1,
            is_sizing: false,
        }
    }

    /// Create a sizing design day environment.
    pub fn sizing_design_day(name: impl Into<String>, month: u8, day: u8) -> Self {
        Self {
            name: name.into(),
            env_type: EnvironmentType::SizingPeriodDesignDay,
            start_month: month,
            start_day: day,
            end_month: month,
            end_day: day,
            start_day_of_week: Weekday::Monday,
            total_days: 1,
            is_sizing: true,
        }
    }

    /// Create a run period environment.
    pub fn run_period(
        name: impl Into<String>,
        start_month: u8,
        start_day: u8,
        end_month: u8,
        end_day: u8,
        total_days: u32,
    ) -> Self {
        Self {
            name: name.into(),
            env_type: EnvironmentType::WeatherRunPeriod,
            start_month,
            start_day,
            end_month,
            end_day,
            start_day_of_week: Weekday::Monday,
            total_days,
            is_sizing: false,
        }
    }
}

/// Environment queue — orders environments for simulation.
#[derive(Debug, Clone, Default)]
pub struct EnvironmentQueue {
    pub environments: Vec<Environment>,
    pub current_index: usize,
}

impl EnvironmentQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an environment to the queue.
    pub fn add(&mut self, env: Environment) {
        self.environments.push(env);
    }

    /// Get the next environment to simulate.
    pub fn next(&mut self) -> Option<&Environment> {
        if self.current_index < self.environments.len() {
            let env = &self.environments[self.current_index];
            self.current_index += 1;
            Some(env)
        } else {
            None
        }
    }

    /// Reset the queue to the beginning.
    pub fn reset(&mut self) {
        self.current_index = 0;
    }

    /// Number of remaining environments.
    pub fn remaining(&self) -> usize {
        self.environments.len().saturating_sub(self.current_index)
    }

    /// Total number of simulation days across all environments.
    pub fn total_days(&self) -> u32 {
        self.environments.iter().map(|e| e.total_days).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_day_environment() {
        let env = Environment::design_day("Summer DD", 7, 21);
        assert_eq!(env.total_days, 1);
        assert_eq!(env.env_type, EnvironmentType::DesignDay);
        assert!(!env.is_sizing);
    }

    #[test]
    fn sizing_design_day() {
        let env = Environment::sizing_design_day("Cooling DD", 7, 21);
        assert_eq!(env.env_type, EnvironmentType::SizingPeriodDesignDay);
        assert!(env.is_sizing);
    }

    #[test]
    fn environment_queue() {
        let mut queue = EnvironmentQueue::new();
        queue.add(Environment::design_day("DD1", 7, 21));
        queue.add(Environment::design_day("DD2", 1, 21));
        queue.add(Environment::run_period("Annual", 1, 1, 12, 31, 365));

        assert_eq!(queue.remaining(), 3);
        assert_eq!(queue.total_days(), 367);

        let env1 = queue.next().unwrap();
        assert_eq!(env1.name, "DD1");
        assert_eq!(queue.remaining(), 2);

        let env2 = queue.next().unwrap();
        assert_eq!(env2.name, "DD2");

        let env3 = queue.next().unwrap();
        assert_eq!(env3.name, "Annual");

        assert!(queue.next().is_none());
    }

    #[test]
    fn queue_reset() {
        let mut queue = EnvironmentQueue::new();
        queue.add(Environment::design_day("DD", 7, 21));
        queue.next();
        assert_eq!(queue.remaining(), 0);

        queue.reset();
        assert_eq!(queue.remaining(), 1);
    }
}
