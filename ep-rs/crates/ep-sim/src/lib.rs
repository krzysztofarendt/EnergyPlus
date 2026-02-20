//! Simulation orchestrator — warmup, sizing, run periods, design days,
//! timestep control, and convergence management.
//!
//! Models the EnergyPlus SimulationManager nested loop structure:
//! Environment → Day → Hour → Timestep → HVAC iterations.

pub mod bestest600;
pub mod building;
pub mod convergence;
pub mod environment;
pub mod input_translator;
pub mod sizing;
pub mod warmup;

use ep_core::state::{EnvironmentType, SimulationState};
/// Simulation configuration.
#[derive(Debug, Clone)]
pub struct SimulationConfig {
    /// Timesteps per hour (1, 2, 4, 6, 10, 12, 15, 20, 30, 60).
    pub timesteps_per_hour: u8,
    /// Maximum number of warmup days.
    pub max_warmup_days: u32,
    /// Minimum number of warmup days.
    pub min_warmup_days: u32,
    /// Convergence tolerance for warmup (fraction).
    pub warmup_tolerance: f64,
    /// Maximum HVAC iterations per timestep.
    pub max_hvac_iterations: u32,
    /// HVAC convergence tolerance (W).
    pub hvac_tolerance: f64,
    /// Whether to run sizing calculations.
    pub do_zone_sizing: bool,
    pub do_system_sizing: bool,
    pub do_plant_sizing: bool,
    /// Run control flags.
    pub run_design_days: bool,
    pub run_weather_periods: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            timesteps_per_hour: 4,
            max_warmup_days: 25,
            min_warmup_days: 6,
            warmup_tolerance: 0.04, // 4% relative
            max_hvac_iterations: 20,
            hvac_tolerance: 0.5, // W
            do_zone_sizing: false,
            do_system_sizing: false,
            do_plant_sizing: false,
            run_design_days: true,
            run_weather_periods: true,
        }
    }
}

/// Callback trait for the simulation loop. Subsystems implement this
/// to participate in the simulation.
pub trait SimulationCallback {
    /// Called at the start of each environment.
    fn begin_environment(&mut self, _state: &mut SimulationState) {}
    /// Called at the start of each day.
    fn begin_day(&mut self, _state: &mut SimulationState) {}
    /// Called at the start of each hour.
    fn begin_hour(&mut self, _state: &mut SimulationState) {}
    /// Called at each timestep for heat balance.
    fn do_timestep(&mut self, _state: &mut SimulationState) {}
    /// Called for HVAC iteration. Returns residual load (W).
    fn do_hvac_iteration(&mut self, _state: &mut SimulationState) -> f64 {
        0.0
    }
    /// Called at the end of each timestep.
    fn end_timestep(&mut self, _state: &mut SimulationState) {}
    /// Called at the end of each day (for warmup checking).
    fn end_day(&mut self, _state: &mut SimulationState) {}
    /// Called at the end of each environment.
    fn end_environment(&mut self, _state: &mut SimulationState) {}
}

/// Run period definition.
#[derive(Debug, Clone)]
pub struct RunPeriod {
    pub name: String,
    pub start_month: u8,
    pub start_day: u8,
    pub end_month: u8,
    pub end_day: u8,
    pub start_year: Option<i32>,
    pub use_weather_file_holidays: bool,
    pub use_weather_file_daylight_saving: bool,
    pub apply_weekend_holiday_rule: bool,
    pub num_times_to_repeat: u32,
}

impl RunPeriod {
    pub fn new(name: impl Into<String>, start_month: u8, start_day: u8, end_month: u8, end_day: u8) -> Self {
        Self {
            name: name.into(),
            start_month,
            start_day,
            end_month,
            end_day,
            start_year: None,
            use_weather_file_holidays: false,
            use_weather_file_daylight_saving: false,
            apply_weekend_holiday_rule: false,
            num_times_to_repeat: 1,
        }
    }

    /// Total number of simulation days in this run period.
    pub fn total_days(&self, leap_year: bool) -> u32 {
        let start_doy = ep_core::time::day_of_year(self.start_month, self.start_day, leap_year);
        let end_doy = ep_core::time::day_of_year(self.end_month, self.end_day, leap_year);
        let days_in_year = if leap_year { 366 } else { 365 };

        if end_doy >= start_doy {
            (end_doy - start_doy + 1) as u32
        } else {
            (days_in_year - start_doy + end_doy + 1) as u32
        }
    }
}

/// The main simulation driver.
#[derive(Debug)]
pub struct SimulationDriver {
    pub config: SimulationConfig,
    pub run_periods: Vec<RunPeriod>,
    pub design_days: Vec<DesignDayRef>,
}

/// Reference to a design day (index into weather module's list).
#[derive(Debug, Clone, Copy)]
pub struct DesignDayRef {
    pub index: usize,
    pub env_type: EnvironmentType,
}

impl SimulationDriver {
    pub fn new(config: SimulationConfig) -> Self {
        Self {
            config,
            run_periods: Vec::new(),
            design_days: Vec::new(),
        }
    }

    /// Run the full simulation loop.
    pub fn run(&self, state: &mut SimulationState, callback: &mut dyn SimulationCallback) -> SimulationResult {
        let mut result = SimulationResult::default();

        // Phase 1: Design day environments (for sizing)
        if self.config.run_design_days {
            for dd in &self.design_days {
                self.run_environment(
                    state,
                    callback,
                    dd.env_type,
                    &format!("DesignDay-{}", dd.index),
                    1, // design days are 1 day
                    &mut result,
                );
            }
        }

        // Phase 2: Weather run periods
        if self.config.run_weather_periods {
            for rp in &self.run_periods {
                let total_days = rp.total_days(state.clock.is_leap_year);
                for _ in 0..rp.num_times_to_repeat {
                    self.run_environment(
                        state,
                        callback,
                        EnvironmentType::WeatherRunPeriod,
                        &rp.name,
                        total_days,
                        &mut result,
                    );
                }
            }
        }

        result
    }

    /// Run a single environment (design day or run period).
    fn run_environment(
        &self,
        state: &mut SimulationState,
        callback: &mut dyn SimulationCallback,
        env_type: EnvironmentType,
        name: &str,
        total_days: u32,
        result: &mut SimulationResult,
    ) {
        state.environment.current_type = Some(env_type);
        state.environment.name = name.to_string();
        state.environment.total_days = total_days;
        state.environment.current_day = 0;
        state.flags.begin_environment = true;

        // Warmup phase
        let warmup_days = warmup::run_warmup(
            state,
            callback,
            &self.config,
        );
        result.warmup_days_used.push(warmup_days);

        // Begin environment callback
        callback.begin_environment(state);
        state.flags.begin_environment = false;
        state.flags.warmup = false;

        // Day loop
        for day in 1..=total_days {
            state.environment.current_day = day;
            state.flags.begin_day = true;
            callback.begin_day(state);
            state.flags.begin_day = false;

            self.run_day(state, callback, result);

            callback.end_day(state);
            state.clock.advance_day();
        }

        callback.end_environment(state);
        result.environments_completed += 1;
    }

    /// Run a single day (24 hours).
    fn run_day(
        &self,
        state: &mut SimulationState,
        callback: &mut dyn SimulationCallback,
        result: &mut SimulationResult,
    ) {
        let tph = self.config.timesteps_per_hour;

        for hour in 0..24u8 {
            state.clock.hour_of_day = hour;
            state.flags.begin_hour = true;
            callback.begin_hour(state);
            state.flags.begin_hour = false;

            for ts in 0..tph {
                state.clock.timestep_in_hour = ts;
                state.flags.begin_timestep = true;

                // Heat balance
                callback.do_timestep(state);
                state.flags.begin_timestep = false;

                // HVAC iteration loop
                let iterations = self.run_hvac_iterations(state, callback);
                result.total_timesteps += 1;
                result.total_hvac_iterations += iterations as u64;

                callback.end_timestep(state);
            }
        }
    }

    /// Run HVAC iterations until convergence or max iterations.
    fn run_hvac_iterations(
        &self,
        state: &mut SimulationState,
        callback: &mut dyn SimulationCallback,
    ) -> u32 {
        let mut iterations = 0;

        for i in 0..self.config.max_hvac_iterations {
            state.flags.first_hvac_iteration = i == 0;
            state.flags.hvac_converged = false;

            let residual = callback.do_hvac_iteration(state);
            iterations = i + 1;

            if residual.abs() < self.config.hvac_tolerance {
                state.flags.hvac_converged = true;
                break;
            }
        }

        iterations
    }
}

/// Simulation result summary.
#[derive(Debug, Clone, Default)]
pub struct SimulationResult {
    /// Number of environments completed.
    pub environments_completed: u32,
    /// Warmup days used for each environment.
    pub warmup_days_used: Vec<u32>,
    /// Total timesteps simulated.
    pub total_timesteps: u64,
    /// Total HVAC iterations across all timesteps.
    pub total_hvac_iterations: u64,
}

impl SimulationResult {
    /// Average HVAC iterations per timestep.
    pub fn avg_hvac_iterations(&self) -> f64 {
        if self.total_timesteps == 0 {
            0.0
        } else {
            self.total_hvac_iterations as f64 / self.total_timesteps as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NullCallback;
    impl SimulationCallback for NullCallback {}

    #[test]
    fn run_period_total_days() {
        let rp = RunPeriod::new("Annual", 1, 1, 12, 31);
        assert_eq!(rp.total_days(false), 365);
        assert_eq!(rp.total_days(true), 366);
    }

    #[test]
    fn run_period_partial_year() {
        let rp = RunPeriod::new("Summer", 6, 1, 8, 31);
        // Jun(30) + Jul(31) + Aug(31) = 92
        assert_eq!(rp.total_days(false), 92);
    }

    #[test]
    fn run_period_wrap_around() {
        let rp = RunPeriod::new("Winter", 11, 1, 2, 28);
        // Nov(30) + Dec(31) + Jan(31) + Feb(28) = 120
        assert_eq!(rp.total_days(false), 120);
    }

    #[test]
    fn simulation_config_defaults() {
        let config = SimulationConfig::default();
        assert_eq!(config.timesteps_per_hour, 4);
        assert_eq!(config.max_warmup_days, 25);
        assert_eq!(config.max_hvac_iterations, 20);
    }

    #[test]
    fn simulation_result_avg_iterations() {
        let result = SimulationResult {
            environments_completed: 1,
            warmup_days_used: vec![6],
            total_timesteps: 100,
            total_hvac_iterations: 250,
        };
        assert!((result.avg_hvac_iterations() - 2.5).abs() < 1e-10);
    }

    #[test]
    fn run_design_day_environment() {
        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_warmup_days: 2,
            min_warmup_days: 1,
            run_design_days: true,
            run_weather_periods: false,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.design_days.push(DesignDayRef {
            index: 0,
            env_type: EnvironmentType::DesignDay,
        });

        let mut state = SimulationState::new(1);
        let mut callback = NullCallback;
        let result = driver.run(&mut state, &mut callback);

        assert_eq!(result.environments_completed, 1);
        assert_eq!(result.total_timesteps, 24); // 1 day, 1 ts/hr, 24 hours
    }

    #[test]
    fn run_short_period() {
        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_warmup_days: 1,
            min_warmup_days: 1,
            run_design_days: false,
            run_weather_periods: true,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.run_periods.push(RunPeriod::new("Jan1-3", 1, 1, 1, 3));

        let mut state = SimulationState::new(1);
        let mut callback = NullCallback;
        let result = driver.run(&mut state, &mut callback);

        assert_eq!(result.environments_completed, 1);
        assert_eq!(result.total_timesteps, 72); // 3 days * 24 hr * 1 ts/hr
    }

    #[test]
    fn hvac_convergence_callback() {
        struct ConvergingCallback {
            iteration_count: u32,
        }
        impl SimulationCallback for ConvergingCallback {
            fn do_hvac_iteration(&mut self, _state: &mut SimulationState) -> f64 {
                self.iteration_count += 1;
                // Converge on 3rd iteration
                if self.iteration_count % 3 == 0 { 0.0 } else { 10.0 }
            }
        }

        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_hvac_iterations: 10,
            max_warmup_days: 1,
            min_warmup_days: 1,
            run_design_days: true,
            run_weather_periods: false,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.design_days.push(DesignDayRef {
            index: 0,
            env_type: EnvironmentType::DesignDay,
        });

        let mut state = SimulationState::new(1);
        let mut callback = ConvergingCallback { iteration_count: 0 };
        let result = driver.run(&mut state, &mut callback);

        // Each timestep should take 3 iterations to converge
        assert_eq!(result.total_timesteps, 24);
        assert_eq!(result.total_hvac_iterations, 72); // 24 * 3
    }

    #[test]
    fn run_period_single_day() {
        let rp = RunPeriod::new("Jan1", 1, 1, 1, 1);
        assert_eq!(rp.total_days(false), 1);
    }

    #[test]
    fn run_period_leap_feb() {
        let rp = RunPeriod::new("Feb", 2, 1, 2, 29);
        assert_eq!(rp.total_days(true), 29);
    }

    #[test]
    fn simulation_result_zero_timesteps() {
        let result = SimulationResult {
            environments_completed: 0,
            warmup_days_used: vec![],
            total_timesteps: 0,
            total_hvac_iterations: 0,
        };
        assert!((result.avg_hvac_iterations() - 0.0).abs() < 1e-10);
    }

    #[test]
    fn run_multiple_design_days() {
        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_warmup_days: 1,
            min_warmup_days: 1,
            run_design_days: true,
            run_weather_periods: false,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.design_days.push(DesignDayRef {
            index: 0,
            env_type: EnvironmentType::DesignDay,
        });
        driver.design_days.push(DesignDayRef {
            index: 1,
            env_type: EnvironmentType::DesignDay,
        });

        let mut state = SimulationState::new(1);
        let mut callback = NullCallback;
        let result = driver.run(&mut state, &mut callback);

        assert_eq!(result.environments_completed, 2);
    }

    #[test]
    fn run_period_repeat() {
        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_warmup_days: 1,
            min_warmup_days: 1,
            run_design_days: false,
            run_weather_periods: true,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        let mut rp = RunPeriod::new("Jan1-3", 1, 1, 1, 3);
        rp.num_times_to_repeat = 2;
        driver.run_periods.push(rp);

        let mut state = SimulationState::new(1);
        let mut callback = NullCallback;
        let result = driver.run(&mut state, &mut callback);

        // 2 repeats → 2 environments completed
        assert_eq!(result.environments_completed, 2);
        // Each environment: 3 days * 24 hr * 1 ts/hr = 72 timesteps
        assert_eq!(result.total_timesteps, 144);
    }

    #[test]
    fn hvac_max_iterations_reached() {
        struct NonConvergingCallback;
        impl SimulationCallback for NonConvergingCallback {
            fn do_hvac_iteration(&mut self, _state: &mut SimulationState) -> f64 {
                // Always return a large residual — never converges
                1000.0
            }
        }

        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_hvac_iterations: 5,
            max_warmup_days: 1,
            min_warmup_days: 1,
            run_design_days: true,
            run_weather_periods: false,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.design_days.push(DesignDayRef {
            index: 0,
            env_type: EnvironmentType::DesignDay,
        });

        let mut state = SimulationState::new(1);
        let mut callback = NonConvergingCallback;
        let result = driver.run(&mut state, &mut callback);

        assert_eq!(result.total_timesteps, 24);
        // Each timestep hits the max of 5 iterations
        assert_eq!(result.total_hvac_iterations, 24 * 5);
    }
}
