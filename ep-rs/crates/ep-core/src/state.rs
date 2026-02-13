//! Top-level simulation state.

use crate::diagnostics::DiagnosticCollector;
use crate::time::SimulationClock;

/// Top-level simulation state. Constructed once, passed by mutable reference
/// through the solver pipeline. Subsystem state is grouped logically.
pub struct SimulationState {
    /// Simulation clock and timestep management.
    pub clock: SimulationClock,

    /// Environment metadata (current design day or weather period).
    pub environment: EnvironmentState,

    /// Simulation control flags.
    pub flags: SimulationFlags,

    /// Diagnostic message accumulator.
    pub diagnostics: DiagnosticCollector,
}

impl SimulationState {
    /// Create a new simulation state with default settings.
    pub fn new(timesteps_per_hour: u8) -> Self {
        Self {
            clock: SimulationClock::new(timesteps_per_hour),
            environment: EnvironmentState::default(),
            flags: SimulationFlags::default(),
            diagnostics: DiagnosticCollector::default(),
        }
    }
}

/// Control flags replacing EnergyPlus's scattered boolean globals.
#[derive(Debug, Default)]
pub struct SimulationFlags {
    pub warmup: bool,
    pub sizing: bool,
    pub begin_environment: bool,
    pub begin_day: bool,
    pub begin_hour: bool,
    pub begin_timestep: bool,
    pub first_hvac_iteration: bool,
    pub hvac_converged: bool,
}

/// Environment type being simulated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentType {
    DesignDay,
    WeatherRunPeriod,
    SizingPeriodDesignDay,
    SizingPeriodWeatherRunPeriod,
}

/// Current environment state.
#[derive(Debug)]
pub struct EnvironmentState {
    pub current_type: Option<EnvironmentType>,
    pub name: String,
    pub total_days: u32,
    pub current_day: u32,
}

impl Default for EnvironmentState {
    fn default() -> Self {
        Self {
            current_type: None,
            name: String::new(),
            total_days: 0,
            current_day: 0,
        }
    }
}
