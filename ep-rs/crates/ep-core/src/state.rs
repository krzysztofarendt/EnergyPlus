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

    /// Current outdoor weather conditions.
    pub outdoor: OutdoorConditions,

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
            outdoor: OutdoorConditions::default(),
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

/// Current outdoor weather conditions for a single timestep.
#[derive(Debug, Clone)]
pub struct OutdoorConditions {
    /// Dry-bulb temperature (C).
    pub dry_bulb: f64,
    /// Humidity ratio (kg/kg).
    pub humidity_ratio: f64,
    /// Wind speed (m/s).
    pub wind_speed: f64,
    /// Wind direction (degrees from north, clockwise).
    pub wind_direction: f64,
    /// Direct normal beam solar radiation (W/m²).
    pub beam_solar: f64,
    /// Diffuse horizontal solar radiation (W/m²).
    pub diffuse_solar: f64,
    /// Sky temperature (C).
    pub sky_temperature: f64,
    /// Barometric pressure (Pa).
    pub barometric_pressure: f64,
}

impl Default for OutdoorConditions {
    fn default() -> Self {
        Self {
            dry_bulb: 20.0,
            humidity_ratio: 0.008,
            wind_speed: 0.0,
            wind_direction: 0.0,
            beam_solar: 0.0,
            diffuse_solar: 0.0,
            sky_temperature: 10.0,
            barometric_pressure: 101325.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outdoor_conditions_defaults() {
        let oc = OutdoorConditions::default();
        assert!((oc.dry_bulb - 20.0).abs() < 1e-10);
        assert!((oc.barometric_pressure - 101325.0).abs() < 0.1);
        assert!((oc.humidity_ratio - 0.008).abs() < 1e-6);
    }

    #[test]
    fn outdoor_conditions_custom() {
        let oc = OutdoorConditions {
            dry_bulb: 35.0,
            humidity_ratio: 0.012,
            wind_speed: 5.0,
            wind_direction: 180.0,
            beam_solar: 800.0,
            diffuse_solar: 150.0,
            sky_temperature: 25.0,
            barometric_pressure: 100000.0,
        };
        assert!((oc.dry_bulb - 35.0).abs() < 1e-10);
        assert!((oc.beam_solar - 800.0).abs() < 1e-10);
        assert!((oc.wind_direction - 180.0).abs() < 1e-10);
    }

    #[test]
    fn simulation_state_construction() {
        let state = SimulationState::new(4);
        assert_eq!(state.flags.warmup, false);
        assert_eq!(state.flags.sizing, false);
        assert!(state.environment.current_type.is_none());
    }
}
