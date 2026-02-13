# EnergyPlus Rust Rewrite Plan

## Project Charter for a Modern Building Energy Simulation Engine

**Version:** 1.0
**Date:** 2026-02-12
**Status:** Draft

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Module Decomposition](#2-module-decomposition)
3. [Phased Roadmap](#3-phased-roadmap)
4. [Validation & Testing Strategy](#4-validation--testing-strategy)
5. [Build & Tooling](#5-build--tooling)
6. [Comparison with Existing Efforts](#6-comparison-with-existing-efforts)
7. [Open Questions & Decision Points](#7-open-questions--decision-points)

---

## 1. Architecture Overview

### 1.1 Current State Assessment

EnergyPlus is approximately **798,000 lines of C++17** across 304 implementation files and 366 headers,
plus **200,000 lines of legacy Fortran** in auxiliary utilities. It bundles 28 third-party libraries and
has 457,000 lines of test code across 271 unit test files. The largest single file (`OutputReportTabular.cc`)
is nearly 20,000 lines. Ten files exceed 10,000 lines each.

The codebase follows a **Central State Pattern**: a single `EnergyPlusData` struct holds 150+
`unique_ptr` members, each containing module-specific state. Every function takes
`EnergyPlusData &state` as its first parameter. Equipment modules follow a consistent
GetInput/Init/Calc/Update lifecycle managed through the `PlantComponent` virtual interface.

The simulation runs a nested loop: Environment -> Day -> Hour -> TimeStep. Within each timestep,
the sequence is: `ManageWeather()` -> `ManageHeatBalance()` -> `ManageHVAC()`. The plant solver
uses a half-loop iteration scheme with 2-8 sub-iterations for convergence.

### 1.2 Crate Structure

The Rust engine uses a Cargo workspace organized as a mono-repo. Each crate has a focused
responsibility and minimal dependencies on sibling crates.

```
ep-rs/
+-- Cargo.toml                  # Workspace root
+-- crates/
|   +-- ep-units/               # Physical units type system
|   +-- ep-core/                # Shared types, error handling, node/loop abstractions
|   +-- ep-psychrometrics/      # Psychrometric calculations
|   +-- ep-fluids/              # Fluid properties (water, glycol, refrigerants)
|   +-- ep-curves/              # Performance curve evaluation
|   +-- ep-weather/             # EPW parsing, solar position, sky models
|   +-- ep-schedule/            # Schedule types and evaluation
|   +-- ep-io/                  # Input parsing (IDF/JSON/new format), output framework
|   +-- ep-materials/           # Material and construction definitions
|   +-- ep-surfaces/            # Surface geometry, view factors, shading geometry
|   +-- ep-solar/               # Solar/shading calculations, sun position
|   +-- ep-envelope/            # Surface heat balance (CTF, CondFD), convection
|   +-- ep-windows/             # Fenestration optics and thermal, glazing layers
|   +-- ep-daylighting/         # Daylighting calculations, glare, controls
|   +-- ep-zone/                # Zone air heat balance, predictor-corrector
|   +-- ep-airflow/             # Airflow network, infiltration, ventilation
|   +-- ep-hvac/                # HVAC air-side components (fans, coils, AHUs)
|   +-- ep-plant/               # Plant loops, solvers, water-side equipment
|   +-- ep-refrigeration/       # Supermarket refrigeration, walk-ins
|   +-- ep-water/               # DHW, water heaters, solar thermal
|   +-- ep-generation/          # PV, wind, fuel cells, batteries, inverters
|   +-- ep-ground/              # Ground heat transfer (Kiva, slab, basement)
|   +-- ep-ems/                 # Energy Management System, scripting
|   +-- ep-demand/              # Demand-side management, load control
|   +-- ep-output/              # Output variables, meters, reports, SQL
|   +-- ep-fmi/                 # FMI co-simulation interface
|   +-- ep-sizing/              # Equipment and system autosizing
|   +-- ep-sim/                 # Simulation manager, orchestrator
|   +-- ep-api/                 # C API and Python bindings (PyO3)
+-- tests/
|   +-- regression/             # Regression tests against E+ reference outputs
|   +-- bestest/                # ASHRAE Standard 140 validation
|   +-- integration/            # Full-model integration tests
+-- docs/                       # mdBook user guide and engineering reference
+-- tools/
|   +-- idf-convert/            # IDF-to-new-format converter
|   +-- ep-diff/                # Output comparison tool
```

### 1.3 Type Safety: Physical Units System

A compile-time units system prevents the single most common class of physics bugs:
mixing incompatible quantities. We use newtypes with zero-cost abstraction.

```rust
// crates/ep-units/src/lib.rs

use std::ops::{Add, Sub, Mul, Div, Neg};

/// Macro to define a physical quantity newtype wrapping f64.
macro_rules! quantity {
    ($name:ident, $unit_str:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
        #[repr(transparent)]
        pub struct $name(pub f64);

        impl $name {
            pub const ZERO: Self = Self(0.0);

            #[inline(always)]
            pub fn new(val: f64) -> Self { Self(val) }

            #[inline(always)]
            pub fn value(self) -> f64 { self.0 }

            #[inline(always)]
            pub fn abs(self) -> Self { Self(self.0.abs()) }

            pub fn is_finite(self) -> bool { self.0.is_finite() }
        }

        impl Add for $name {
            type Output = Self;
            #[inline(always)]
            fn add(self, rhs: Self) -> Self { Self(self.0 + rhs.0) }
        }

        impl Sub for $name {
            type Output = Self;
            #[inline(always)]
            fn sub(self, rhs: Self) -> Self { Self(self.0 - rhs.0) }
        }

        impl Neg for $name {
            type Output = Self;
            #[inline(always)]
            fn neg(self) -> Self { Self(-self.0) }
        }

        impl Mul<f64> for $name {
            type Output = Self;
            #[inline(always)]
            fn mul(self, rhs: f64) -> Self { Self(self.0 * rhs) }
        }

        impl Div<f64> for $name {
            type Output = Self;
            #[inline(always)]
            fn div(self, rhs: f64) -> Self { Self(self.0 / rhs) }
        }

        impl Div<$name> for $name {
            type Output = f64;
            #[inline(always)]
            fn div(self, rhs: $name) -> f64 { self.0 / rhs.0 }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{} {}", self.0, $unit_str)
            }
        }
    };
}

// Thermodynamic quantities
quantity!(Temperature, "K");        // Always stored in Kelvin internally
quantity!(TempDelta, "deltaK");     // Temperature difference (K or degC)
quantity!(Power, "W");
quantity!(Energy, "J");
quantity!(HeatFlux, "W/m2");
quantity!(ThermalConductivity, "W/(m*K)");
quantity!(ThermalResistance, "m2*K/W");
quantity!(SpecificHeat, "J/(kg*K)");
quantity!(Enthalpy, "J/kg");

// Flow quantities
quantity!(MassFlowRate, "kg/s");
quantity!(VolumeFlowRate, "m3/s");
quantity!(Pressure, "Pa");
quantity!(Velocity, "m/s");

// Geometry
quantity!(Area, "m2");
quantity!(Length, "m");
quantity!(Angle, "rad");

// Solar/optical
quantity!(Irradiance, "W/m2");
quantity!(Illuminance, "lux");
quantity!(Transmittance, "");       // dimensionless 0..1
quantity!(Absorptance, "");
quantity!(Emissivity, "");

// Time
quantity!(Duration, "s");

// Humidity
quantity!(HumidityRatio, "kg/kg");
quantity!(RelativeHumidity, "%");
quantity!(Density, "kg/m3");
quantity!(DynamicViscosity, "Pa*s");

// Convenience conversions
impl Temperature {
    /// Convert from Celsius to internal Kelvin representation.
    #[inline(always)]
    pub fn from_celsius(c: f64) -> Self { Self(c + 273.15) }

    /// Get value in Celsius for display/output.
    #[inline(always)]
    pub fn to_celsius(self) -> f64 { self.0 - 273.15 }

    /// Difference between two temperatures yields a TempDelta.
    #[inline(always)]
    pub fn delta(self, other: Temperature) -> TempDelta {
        TempDelta(self.0 - other.0)
    }
}

// Cross-quantity operations with explicit typed results
impl Mul<Area> for HeatFlux {
    type Output = Power;
    #[inline(always)]
    fn mul(self, rhs: Area) -> Power { Power(self.0 * rhs.0) }
}

impl Mul<Duration> for Power {
    type Output = Energy;
    #[inline(always)]
    fn mul(self, rhs: Duration) -> Energy { Energy(self.0 * rhs.0) }
}

impl Div<Area> for Power {
    type Output = HeatFlux;
    #[inline(always)]
    fn div(self, rhs: Area) -> HeatFlux { HeatFlux(self.0 / rhs.0) }
}

impl Mul<MassFlowRate> for Enthalpy {
    type Output = Power;
    #[inline(always)]
    fn mul(self, rhs: MassFlowRate) -> Power { Power(self.0 * rhs.0) }
}
```

### 1.4 State Management

The C++ codebase uses a monolithic `EnergyPlusData` god-object with 150+ members.
The Rust design replaces this with a structured `SimulationState` that groups related
state and makes ownership explicit.

```rust
// crates/ep-core/src/state.rs

use crate::time::SimulationClock;

/// Top-level simulation state. Constructed once, passed by mutable reference
/// through the solver pipeline. Subsystem state is grouped logically.
pub struct SimulationState {
    /// Simulation clock and timestep management.
    pub clock: SimulationClock,

    /// Environment metadata (current design day or weather period).
    pub environment: EnvironmentState,

    /// Weather data for current timestep.
    pub weather: WeatherState,

    /// Building geometry and surface state.
    pub surfaces: SurfaceState,

    /// Zone air state (temperatures, humidity, loads).
    pub zones: ZoneState,

    /// HVAC air-side system state.
    pub hvac: HvacState,

    /// Plant loop state (flow rates, temperatures, convergence).
    pub plant: PlantState,

    /// Output variable registry and current values.
    pub output: OutputState,

    /// Schedule evaluation cache for current timestep.
    pub schedules: ScheduleState,

    /// Simulation control flags.
    pub flags: SimulationFlags,

    /// Diagnostic message accumulator.
    pub diagnostics: DiagnosticCollector,
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

/// Timestep and simulation clock.
pub struct SimulationClock {
    pub current_time: chrono::NaiveDateTime,
    pub time_step: ep_units::Duration,
    pub hour_of_day: u8,
    pub timestep_in_hour: u8,
    pub timesteps_per_hour: u8,
    pub day_of_year: u16,
    pub day_of_week: Weekday,
    pub month: u8,
    pub day_of_month: u8,
    pub year: i32,
    pub is_leap_year: bool,
}
```

### 1.5 Component Model: Trait-Based Design

Equipment components implement traits rather than inheriting from a base class.
This enables static dispatch (zero-cost) for the common case while allowing
dynamic dispatch (`dyn Trait`) when heterogeneous collections are needed.

```rust
// crates/ep-core/src/component.rs

use crate::state::SimulationState;
use ep_units::*;

/// Every HVAC/plant component implements this trait.
pub trait HvacComponent: Send + Sync {
    /// Human-readable name for diagnostics.
    fn name(&self) -> &str;

    /// Component type identifier.
    fn component_type(&self) -> ComponentType;

    /// Initialize component state at start of environment or after sizing.
    fn initialize(
        &mut self,
        state: &SimulationState,
        first_hvac_iteration: bool,
    ) -> Result<(), SimError>;

    /// Run the component model for the current timestep.
    fn simulate(
        &mut self,
        state: &mut SimulationState,
        first_hvac_iteration: bool,
        load: Power,
        run: bool,
    ) -> Result<(), SimError>;

    /// Return the component's current capacity for load dispatch.
    fn available_capacity(&self) -> Power;
}

/// Plant-specific component trait extending HvacComponent.
pub trait PlantComponent: HvacComponent {
    /// The plant loop location where this component is connected.
    fn plant_location(&self) -> PlantLocation;

    /// Water-side flow request for the current timestep.
    fn design_flow_rate(&self) -> MassFlowRate;

    /// Sizing callback.
    fn size(&mut self, state: &SimulationState) -> Result<(), SimError>;

    /// Report output variables.
    fn report(&self, output: &mut OutputState);
}

/// Identifies a component's location in the plant topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlantLocation {
    pub loop_index: usize,
    pub side: LoopSide,
    pub branch_index: usize,
    pub component_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoopSide {
    Supply,
    Demand,
}
```

### 1.6 Solver Orchestration

The main simulation loop mirrors EnergyPlus's nested structure but uses Rust's
type system to enforce the correct calling sequence.

```rust
// crates/ep-sim/src/orchestrator.rs

use ep_core::state::SimulationState;
use ep_core::error::SimResult;

pub struct SimulationOrchestrator {
    weather_manager: ep_weather::WeatherManager,
    envelope_solver: ep_envelope::EnvelopeSolver,
    zone_solver: ep_zone::ZoneAirSolver,
    hvac_manager: ep_hvac::HvacManager,
    plant_manager: ep_plant::PlantManager,
    output_manager: ep_output::OutputManager,
    sizing_manager: ep_sizing::SizingManager,
}

impl SimulationOrchestrator {
    pub fn run(&mut self, state: &mut SimulationState) -> SimResult<()> {
        self.run_sizing_passes(state)?;

        // Environment loop (design days + weather periods)
        for env in state.environment.iter_environments() {
            state.flags.begin_environment = true;
            self.weather_manager.setup_environment(state, &env)?;
            self.run_warmup(state)?;

            // Day loop
            for _day in env.day_range() {
                state.flags.begin_day = true;

                // Hour loop
                for hour in 0..24u8 {
                    state.flags.begin_hour = true;
                    state.clock.hour_of_day = hour;

                    // Timestep loop
                    for ts in 0..state.clock.timesteps_per_hour {
                        state.clock.timestep_in_hour = ts;
                        state.flags.begin_timestep = true;

                        self.simulate_timestep(state)?;

                        state.flags.begin_timestep = false;
                        state.flags.begin_hour = false;
                        state.flags.begin_day = false;
                    }
                }
                state.clock.advance_day();
            }
            state.flags.begin_environment = false;
        }

        self.output_manager.write_final_reports(state)?;
        Ok(())
    }

    fn simulate_timestep(&mut self, state: &mut SimulationState) -> SimResult<()> {
        // 1. Weather
        self.weather_manager.update(state)?;

        // 2. Envelope heat balance (outside surfaces -> inside surfaces)
        self.envelope_solver.solve(state)?;

        // 3. Zone air heat balance + HVAC iteration
        self.solve_hvac_loop(state)?;

        // 4. Reporting
        self.output_manager.update_timestep(state)?;

        Ok(())
    }

    fn solve_hvac_loop(&mut self, state: &mut SimulationState) -> SimResult<()> {
        const MAX_HVAC_ITERATIONS: u32 = 20;

        for iteration in 0..MAX_HVAC_ITERATIONS {
            state.flags.first_hvac_iteration = iteration == 0;

            // Zone predictor
            self.zone_solver.predict(state)?;

            // Air-side systems
            let mut sim_air = true;
            let mut sim_zone_equip = true;
            let mut sim_plant = true;

            // Inner HVAC iteration (air loops <-> zone equipment <-> plant)
            while sim_air || sim_zone_equip || sim_plant {
                if sim_air {
                    self.hvac_manager.simulate_air_loops(state)?;
                }
                if sim_zone_equip {
                    self.hvac_manager.simulate_zone_equipment(state)?;
                }
                if sim_plant {
                    self.plant_manager.simulate(
                        state,
                        &mut sim_air,
                        &mut sim_zone_equip,
                        &mut sim_plant,
                    )?;
                }
            }

            // Zone corrector
            self.zone_solver.correct(state)?;

            if state.flags.hvac_converged {
                break;
            }
        }

        Ok(())
    }
}
```

### 1.7 Concurrency Model

EnergyPlus is fundamentally sequential within a timestep, but several computations
are embarrassingly parallel. The Rust engine uses Rayon for data-parallel operations.

**Parallel opportunities identified:**

| Operation | Parallelism Type | Expected Speedup |
|-----------|-----------------|-----------------|
| Surface outside heat balance | Per-surface | 2-4x on large models |
| Solar shading calculations | Per-surface pair | 2-6x |
| View factor computation (setup) | Per-enclosure | 3-8x |
| CTF coefficient generation | Per-construction | 2-4x |
| Daylighting reference points | Per-zone | 2-4x |
| Parametric/batch runs | Per-run | Near-linear |
| Weather file preprocessing | Per-month | 2-4x |

```rust
// Example: parallel surface heat balance using Rayon
use rayon::prelude::*;

fn calc_outside_surface_heat_balance(
    surfaces: &mut [SurfaceState],
    weather: &WeatherState,
    solar: &SolarState,
) {
    surfaces.par_iter_mut().for_each(|surf| {
        let q_solar = solar.incident_solar(surf.index);
        let q_lw_sky = calc_sky_longwave(surf, weather);
        let q_lw_ground = calc_ground_longwave(surf, weather);
        let h_conv = calc_exterior_convection(surf, weather);

        surf.outside_temp = solve_outside_balance(
            q_solar, q_lw_sky, q_lw_ground, h_conv, surf,
        );
    });
}
```

**Determinism guarantee:** Parallel operations must produce bit-identical results
regardless of thread scheduling. This is achieved by:
- Using indexed operations (not order-dependent reductions)
- Avoiding floating-point summation order dependence in parallel reductions
  (use Kahan summation or sort-before-sum)
- Providing a `--deterministic` flag that forces sequential execution

### 1.8 Error Handling

EnergyPlus uses `ShowFatalError()` / `ShowSevereError()` / `ShowWarningError()` functions
that write to stderr and sometimes abort. The Rust engine uses a structured error hierarchy.

```rust
// crates/ep-core/src/error.rs

use thiserror::Error;

pub type SimResult<T> = Result<T, SimError>;

#[derive(Error, Debug)]
pub enum SimError {
    // Fatal errors that abort the simulation
    #[error("Input error in {object_type} '{object_name}': {message}")]
    InputError {
        object_type: String,
        object_name: String,
        message: String,
    },

    #[error("Convergence failure in {solver}: {message}")]
    ConvergenceError {
        solver: String,
        message: String,
        iterations: u32,
    },

    #[error("Numerical error: {0}")]
    NumericalError(String),

    #[error("Weather data error: {0}")]
    WeatherError(String),

    #[error("File I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("FMI co-simulation error: {0}")]
    FmiError(String),
}

/// Non-fatal diagnostics accumulated during simulation.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub category: &'static str,
    pub message: String,
    /// Source location for traceability.
    pub location: Option<SourceLocation>,
    /// Recurrence count (for rate-limiting repeated warnings).
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Severe,
}

#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub file: &'static str,
    pub line: u32,
    pub module: &'static str,
}

/// Collector that rate-limits repeated warnings.
pub struct DiagnosticCollector {
    diagnostics: Vec<Diagnostic>,
    recurring: std::collections::HashMap<String, u32>,
    max_recurring: u32,
}

impl DiagnosticCollector {
    pub fn warn(&mut self, category: &'static str, message: impl Into<String>) {
        let msg = message.into();
        let count = self.recurring.entry(msg.clone()).or_insert(0);
        *count += 1;
        if *count <= self.max_recurring {
            self.diagnostics.push(Diagnostic {
                level: DiagnosticLevel::Warning,
                category,
                message: msg,
                location: None,
                count: *count,
            });
        }
    }

    pub fn severe(&mut self, category: &'static str, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Severe,
            category,
            message: message.into(),
            location: None,
            count: 1,
        });
    }
}
```

### 1.9 Plugin / Extension Model

Users can extend the engine without modifying core code. The trait-based design
supports three extension mechanisms:

```rust
// crates/ep-core/src/plugin.rs

/// Trait for user-defined HVAC components loadable at runtime.
pub trait UserComponent: HvacComponent {
    /// Called during input processing to configure the component.
    fn configure(&mut self, params: &serde_json::Value) -> SimResult<()>;
}

/// Trait for EMS-like scripting callbacks.
pub trait ScriptCallback: Send + Sync {
    fn on_calling_point(
        &mut self,
        point: CallingPoint,
        state: &mut SimulationState,
    ) -> SimResult<()>;
}

/// Calling points where scripts/plugins can intervene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallingPoint {
    BeginNewEnvironment,
    AfterWarmup,
    BeginTimestep,
    BeforeHvac,
    AfterHvac,
    EndTimestep,
    EndEnvironment,
    EndSimulation,
}

/// Registry for dynamically loaded components and callbacks.
pub struct PluginRegistry {
    component_factories:
        HashMap<String, Box<dyn Fn(&serde_json::Value) -> Box<dyn PlantComponent>>>,
    script_callbacks: Vec<(CallingPoint, Box<dyn ScriptCallback>)>,
}
```

---

## 2. Module Decomposition

### 2.1 Weather Processing (`ep-weather`)

**Scope:** EnergyPlus files: `WeatherManager.cc/hh` (8,867 lines), `DataEnvironment.hh`,
ground temperature models in `GroundTemperatureModeling/` directory (5 model implementations).

**Key algorithms:**
- EPW file parsing (TMY3 format, hourly or sub-hourly records)
- Solar position: Spencer's equations for declination, equation of time, hour angle
  (Engineering Reference Section: Climate Calculations)
- Sky models: Perez anisotropic sky diffuse model, isotropic, HDKR
- Ground temperatures: Kusuda-Achenbach correlation, Xing model, finite-difference 1D
- Sky temperature: Clark-Allen, Brunt, Idso, Berdahl-Martin correlations
- Design day generation: ASHRAE clear-sky solar model, Zhang-Huang, Tau/Tau2017
- Water mains temperature: correlation and schedule-based

**Rust crate public API sketch:**

```rust
// crates/ep-weather/src/lib.rs

pub mod epw;
pub mod solar_position;
pub mod sky_models;
pub mod ground_temperature;
pub mod design_day;

/// Parsed EPW weather file.
pub struct WeatherFile {
    pub location: Location,
    pub records: Vec<WeatherRecord>,
    pub design_conditions: Option<DesignConditions>,
    pub ground_temps: Option<MonthlyGroundTemps>,
}

/// Single timestep weather data.
#[derive(Debug, Clone)]
pub struct WeatherRecord {
    pub dry_bulb: Temperature,
    pub wet_bulb: Temperature,
    pub dew_point: Temperature,
    pub rel_humidity: RelativeHumidity,
    pub pressure: Pressure,
    pub direct_normal_irradiance: Irradiance,
    pub diffuse_horizontal_irradiance: Irradiance,
    pub global_horizontal_irradiance: Irradiance,
    pub wind_speed: Velocity,
    pub wind_direction: Angle,
    pub sky_cover: f64,           // 0-10 tenths
    pub visibility: Length,
    pub ceiling_height: Length,
    pub precipitation: f64,       // mm
    pub snow_depth: Length,
    pub is_rain: bool,
    pub is_snow: bool,
}

/// Solar position for a given time and location.
pub struct SolarPosition {
    pub altitude: Angle,
    pub azimuth: Angle,
    pub zenith: Angle,
    pub hour_angle: Angle,
    pub declination: Angle,
    pub equation_of_time: Duration,
    pub sun_is_up: bool,
}

pub struct WeatherManager {
    weather_file: Option<WeatherFile>,
    design_days: Vec<DesignDaySpec>,
    ground_model: Box<dyn GroundTemperatureModel>,
}

impl WeatherManager {
    pub fn load_epw(path: &std::path::Path) -> SimResult<WeatherFile>;
    pub fn update(&self, state: &mut SimulationState) -> SimResult<()>;
    pub fn solar_position(
        lat: Angle, lon: Angle, tz: f64,
        day_of_year: u16, hour: f64,
    ) -> SolarPosition;
    pub fn sky_temperature(weather: &WeatherRecord, model: SkyTempModel) -> Temperature;
}

pub trait GroundTemperatureModel: Send + Sync {
    fn temperature_at_depth(
        &self, depth: Length, day_of_year: u16,
    ) -> Temperature;
}
```

**Dependencies:** `ep-units`, `ep-core`

**Complexity estimate:**
- Lines of Rust: ~4,000-5,000
- Effort: 2-3 person-months
- Difficulty: Medium

**Migration strategy:** Can be ported and validated independently by comparing
parsed EPW outputs and solar position calculations against EnergyPlus reference values.
This is one of the first modules to port since it has no dependencies on HVAC or envelope.

**Risks:**
- EPW format has many edge cases (missing data markers, non-standard extensions)
- Solar position precision must match EnergyPlus to within 0.01 degrees for
  regression testing. Differences in trig implementations between C++ and Rust
  standard libraries can cause drift.
- Design day generation has complex humidity/pressure models with 5+ control types

### 2.2 Schedules & Internal Gains (`ep-schedule`)

**Scope:** EnergyPlus files: `ScheduleManager.cc/hh` (large module),
`InternalHeatGains.cc` (8,823 lines), `DataHeatBalance.hh` (zone gains data).

**Key algorithms:**
- Schedule hierarchy: Year -> Week (14 day types) -> Day (hourly/sub-hourly values)
- Compact schedule parsing
- Interpolation: None (step), Average, Linear
- File-based schedules (CSV external files)
- Internal gains: People (activity level, radiant/convective split),
  Lights (return air fraction), Equipment (electric, gas, steam, hot water, other)
- Infiltration models: Design flow rate, effective leakage area, flow coefficients

**Rust crate public API sketch:**

```rust
// crates/ep-schedule/src/lib.rs

/// A schedule that can be evaluated at any simulation time.
pub enum Schedule {
    Constant(f64),
    Year(YearSchedule),
    Compact(CompactSchedule),
    File(FileSchedule),
    External(ExternalSchedule),   // EMS/FMI actuated
}

impl Schedule {
    /// Get the schedule value for the given time.
    pub fn value_at(&self, clock: &SimulationClock) -> f64;

    /// Get min/max bounds for validation.
    pub fn bounds(&self) -> (f64, f64);
}

pub struct YearSchedule {
    pub name: String,
    pub schedule_type: Option<ScheduleTypeLimit>,
    weeks: Vec<WeekRule>,        // Date ranges -> week schedule
}

pub struct WeekSchedule {
    days: [DaySchedule; 14],     // One per DayType
}

pub struct DaySchedule {
    values: Vec<f64>,            // One per timestep in a day
    interpolation: Interpolation,
}

#[derive(Debug, Clone, Copy)]
pub enum DayType {
    Sunday, Monday, Tuesday, Wednesday, Thursday, Friday, Saturday,
    Holiday, SummerDesignDay, WinterDesignDay, CustomDay1, CustomDay2,
    AllDays, Weekdays, Weekends,
}

/// Internal heat gain definition for a zone.
pub struct InternalGain {
    pub gain_type: GainType,
    pub schedule: ScheduleRef,
    pub design_level: Power,
    pub radiant_fraction: f64,
    pub convective_fraction: f64,
    pub latent_fraction: f64,
    pub return_air_fraction: f64,  // For lights
    pub carbon_dioxide_rate: f64,  // For people
}

pub enum GainType {
    People { activity_schedule: ScheduleRef, count: f64 },
    Lights,
    ElectricEquipment,
    GasEquipment,
    HotWaterEquipment,
    SteamEquipment,
    OtherEquipment,
    InfiltrationDesignFlowRate {
        flow_rate: VolumeFlowRate,
        coefficients: [f64; 4],    // A + B*|Tz-To| + C*WindSpeed + D*WindSpeed^2
    },
}
```

**Dependencies:** `ep-units`, `ep-core`

**Complexity estimate:**
- Lines of Rust: ~3,000-4,000
- Effort: 2 person-months
- Difficulty: Low-Medium

**Migration strategy:** Schedules are self-contained and can be validated by
comparing evaluated schedule values at every timestep against EnergyPlus output.
Internal gains feed into the zone heat balance and can be validated via zone
load comparisons.

**Risks:**
- Compact schedule parsing is tricky (free-form text with Through/For/Until syntax)
- 14 day types with DST transitions create edge cases
- Schedule file I/O must handle various CSV formats and missing data

### 2.3 Surface Heat Balance (`ep-envelope`)

**Scope:** EnergyPlus files: `HeatBalanceSurfaceManager.cc` (10,094 lines),
`HeatBalanceIntRadExchange.cc`, `HeatBalFiniteDiffManager.cc` (CondFD),
`HeatBalanceHAMTManager.cc` (HAMT), `HeatBalanceMovableInsulation.cc`,
`ConvectionCoefficients.cc` (6,610 lines), `Construction.cc`, `Material.cc`,
`HeatBalanceManager.cc` (6,131 lines).

**Key algorithms:**
- **CTF (Conduction Transfer Functions):** State-space method for wall conduction.
  Transforms material layer properties into time-series transfer function
  coefficients (X, Y, Z series). Uses historical temperature data for fast
  evaluation. (Engineering Reference: Conduction Transfer Functions)
- **CondFD (Conduction Finite Difference):** Node-based 1D conduction with
  Crank-Nicolson or fully-implicit schemes. Supports phase change materials
  via enthalpy method. (Engineering Reference: Conduction Finite Difference)
- **HAMT (Heat and Moisture Transfer):** Coupled heat and moisture transport
  through building materials.
- **Outside surface heat balance:** Solar absorbed + LW sky radiation +
  LW ground radiation - convection = conduction flux
- **Inside surface heat balance:** Internal gains radiation + inter-surface
  LW exchange + interior convection = conduction flux
- **Interior radiant exchange:** Enclosure-based view factor method with
  radiosity formulation.
- **Convection correlations:** 15+ interior models (TARP, CeilingDiffuser,
  adaptive), 10+ exterior models (DOE-2, TARP, MoWiTT, adaptive)

**Rust crate public API sketch:**

```rust
// crates/ep-envelope/src/lib.rs

pub mod ctf;
pub mod cond_fd;
pub mod hamt;
pub mod convection;
pub mod radiant_exchange;
pub mod surface_balance;

/// Conduction model selector per surface.
pub enum ConductionModel {
    CTF(CtfCoefficients),
    CondFD(CondFdState),
    HAMT(HamtState),
}

/// CTF coefficients for a construction (precomputed during setup).
pub struct CtfCoefficients {
    pub outside_coeffs: Vec<f64>,  // X series
    pub inside_coeffs: Vec<f64>,   // Y series
    pub cross_coeffs: Vec<f64>,    // Z series
    pub flux_coeffs: Vec<f64>,     // Phi series
    pub num_history_terms: usize,
}

/// Per-surface conduction finite difference state.
pub struct CondFdState {
    pub scheme: CondFdScheme,
    pub nodes: Vec<CondFdNode>,
    pub node_temps: Vec<Temperature>,
    pub node_temps_old: Vec<Temperature>,
    pub node_enthalpies: Vec<Enthalpy>,
}

pub enum CondFdScheme {
    CrankNicolson,
    FullyImplicit,
}

/// Main envelope solver.
pub struct EnvelopeSolver {
    conduction_models: Vec<ConductionModel>,
    convection_config: ConvectionConfig,
    radiant_enclosures: Vec<RadiantEnclosure>,
}

impl EnvelopeSolver {
    /// Solve the complete surface heat balance for one timestep.
    pub fn solve(&mut self, state: &mut SimulationState) -> SimResult<()> {
        self.calc_outside_surface_balance(state)?;
        self.calc_inside_surface_balance(state)?;
        self.update_thermal_histories(state);
        Ok(())
    }

    /// Outside surface heat balance for all exterior surfaces.
    fn calc_outside_surface_balance(
        &mut self, state: &mut SimulationState,
    ) -> SimResult<()>;

    /// Inside surface heat balance with radiant exchange iteration.
    fn calc_inside_surface_balance(
        &mut self, state: &mut SimulationState,
    ) -> SimResult<()>;
}

/// Convection coefficient calculation.
pub trait ConvectionModel: Send + Sync {
    fn interior_coefficient(
        &self, surface: &SurfaceState, zone: &ZoneAirState,
    ) -> HeatFlux;

    fn exterior_coefficient(
        &self, surface: &SurfaceState, weather: &WeatherState,
    ) -> HeatFlux;
}

/// Enclosure for radiant exchange calculations.
pub struct RadiantEnclosure {
    pub surface_indices: Vec<usize>,
    pub view_factors: ndarray::Array2<f64>,
    pub script_f: ndarray::Array2<f64>,  // Radiosity script-F matrix
}
```

**Dependencies:** `ep-units`, `ep-core`, `ep-materials`, `ep-surfaces`, `ep-solar`

**Complexity estimate:**
- Lines of Rust: ~12,000-15,000
- Effort: 6-8 person-months
- Difficulty: **Very High**

**Migration strategy:** This is the most critical and complex subsystem. Port in stages:
1. CTF coefficient generation (can validate against E+ CTF output reports)
2. Outside surface heat balance (validate surface temperatures)
3. Interior radiant exchange (validate view factors, then enclosure loads)
4. Inside surface heat balance (validate zone loads)
5. CondFD and HAMT as secondary methods

**Risks and challenges:**
- CTF state-space method involves matrix eigenvalue decomposition with
  numerical stability concerns for thin/high-conductivity layers
- Interior radiant exchange requires solving a coupled system across all
  surfaces in an enclosure; the view factor matrix must be properly normalized
- Convection coefficient selection logic is extremely complex with 30+ models
  and adaptive selection algorithms
- The predictor step feeds from HVAC back into surface balance, creating
  a tight coupling loop
- Floating-point reproducibility: CTF history arrays accumulate over
  many timesteps; small differences compound

### 2.4 Fenestration & Window Models (`ep-windows`)

**Scope:** EnergyPlus files: `WindowManager.cc` (8,564 lines),
`WindowComplexManager.cc`, `WindowEquivalentLayer.cc` (8,108 lines),
`WindowManagerExteriorThermal.cc`, `WindowManagerExteriorOptical.cc`,
`DataSurfaces.hh` (window-specific structs), integration with
Windows-CalcEngine (Tarcog) third-party library.

**Key algorithms:**
- Multi-layer glazing optics: layer-by-layer transmittance, reflectance, absorptance
  using spectral data at 107 wavelength points (solar) and 81 points (photopic)
- Angular dependence: 10 incidence angles (0-90 degrees) with polynomial fits
- Gap gas properties: convection and conduction across glazing gaps (air, argon, krypton)
- Shading devices: blinds (slat angle), shades, screens, switchable glazing
- BSDF (Bidirectional Scattering Distribution Function) for complex fenestration:
  145-direction hemisphere sampling
- Window frame and divider thermal bridging
- Tarcog integration for ISO 15099 thermal calculations
- Solar distribution through windows onto interior surfaces

**Rust crate public API sketch:**

```rust
// crates/ep-windows/src/lib.rs

pub mod glazing;
pub mod spectral;
pub mod thermal;
pub mod shading;
pub mod bsdf;
pub mod frame;

/// Complete window system definition.
pub struct WindowSystem {
    pub layers: Vec<GlazingLayer>,
    pub gaps: Vec<GasGap>,
    pub frame: Option<FrameProperties>,
    pub divider: Option<DividerProperties>,
    pub shading: Option<ShadingDevice>,
}

/// Single glazing layer with spectral properties.
pub struct GlazingLayer {
    pub thickness: Length,
    pub conductivity: ThermalConductivity,
    pub emissivity_front: Emissivity,
    pub emissivity_back: Emissivity,
    /// Spectral data: (wavelength_um, transmittance, reflectance_front, reflectance_back)
    pub spectral_data: Option<Vec<SpectralPoint>>,
    /// Angle-dependent properties (10 angles from 0 to 90 degrees).
    pub angular_properties: AngularProperties,
}

/// Window thermal state for a single timestep.
pub struct WindowThermalState {
    pub layer_temperatures: Vec<Temperature>,  // Front and back of each layer
    pub heat_gain: Power,
    pub heat_loss: Power,
    pub solar_transmittance: f64,
    pub visible_transmittance: f64,
    pub u_value: f64,               // W/(m2*K)
    pub shgc: f64,                  // Solar Heat Gain Coefficient
}

pub trait WindowModel: Send + Sync {
    fn calc_solar_properties(
        &self, cos_incidence: f64,
    ) -> WindowSolarProperties;

    fn calc_thermal_balance(
        &mut self,
        exterior_temp: Temperature,
        interior_temp: Temperature,
        incident_solar: Irradiance,
        wind_speed: Velocity,
        interior_ir: HeatFlux,
    ) -> SimResult<WindowThermalState>;
}
```

**Dependencies:** `ep-units`, `ep-core`, `ep-materials`, `ep-solar`

**Complexity estimate:**
- Lines of Rust: ~10,000-12,000
- Effort: 5-7 person-months
- Difficulty: **Very High**

**Migration strategy:** Start with simple single-pane windows, then multi-pane,
then add shading devices, then BSDF. Validate against Window 7 / LBNL reference
data and EnergyPlus window output reports.

**Risks:**
- Spectral integration requires careful numerical quadrature
- BSDF matrix operations are large (145x145 per layer pair)
- Tarcog/Windows-CalcEngine is complex C++ code; may need FFI during transition
- Angular interpolation must exactly match E+ behavior for regression testing

### 2.5 Solar & Shading Calculations (`ep-solar`)

**Scope:** EnergyPlus files: `SolarShading.cc` (13,085 lines),
`SolarReflectionManager.cc`, integration with Penumbra library (OpenGL shading).

**Key algorithms:**
- Sun position calculation (Spencer, NREL SPA algorithm)
- Shadow casting: polygon clipping (Weiler-Atherton) for sunlit area determination
- Penumbra GPU-accelerated shadow calculation (optional)
- Anisotropic sky diffuse: Perez model decomposition into circumsolar,
  horizon brightening, and isotropic components
- Solar distribution: tracking beam solar through windows onto interior surfaces
- Reflected solar from ground and obstructions
- Shading surface schedule interaction

**Rust crate public API sketch:**

```rust
// crates/ep-solar/src/lib.rs

pub mod position;
pub mod shading;
pub mod diffuse_sky;
pub mod distribution;
pub mod reflection;

pub struct SolarCalculator {
    shading_engine: Box<dyn ShadingEngine>,
    sky_model: SkyDiffuseModel,
}

pub trait ShadingEngine: Send + Sync {
    /// Calculate the sunlit fraction for each surface at the given sun position.
    fn calculate_sunlit_fractions(
        &mut self,
        surfaces: &[SurfaceGeometry],
        sun_position: &SolarPosition,
    ) -> Vec<f64>;
}

/// Software polygon-clipping shading engine (always available).
pub struct PolygonClipShadingEngine;

/// GPU-accelerated shading engine (optional, requires OpenGL).
#[cfg(feature = "gpu-shading")]
pub struct GpuShadingEngine { /* Penumbra-equivalent */ }

pub enum SkyDiffuseModel {
    Isotropic,
    Perez,
    HDKR,    // Hay-Davies-Klucher-Reindl
}

/// Incident solar radiation on a surface.
#[derive(Debug, Clone)]
pub struct SurfaceSolarIncident {
    pub beam: Irradiance,
    pub diffuse_sky: Irradiance,
    pub diffuse_ground: Irradiance,
    pub reflected: Irradiance,
    pub total: Irradiance,
    pub cos_incidence: f64,
    pub sunlit_fraction: f64,
}
```

**Dependencies:** `ep-units`, `ep-core`, `ep-weather`, `ep-surfaces`

**Complexity estimate:**
- Lines of Rust: ~6,000-8,000
- Effort: 4-5 person-months
- Difficulty: High

**Migration strategy:** Solar position is straightforward; shading calculations
can be validated surface-by-surface against EnergyPlus shadow reports.

**Risks:**
- Polygon clipping algorithms have numerous degenerate cases (coplanar surfaces,
  near-zero-area slivers, numerical precision at polygon edges)
- Shading calculations run at a different frequency than the main timestep
  (often hourly or every N timesteps); must handle caching correctly
- GPU shading path adds an optional dependency that complicates CI

### 2.6 Daylighting (`ep-daylighting`)

**Scope:** EnergyPlus files: `DaylightingManager.cc` (10,099 lines),
`DaylightingDevices.cc`, `DataDaylighting.hh`, integration with DElight library.

**Key algorithms:**
- Split-flux method: Splits interior illuminance into direct-sun, sky, and
  inter-reflected components using daylight factors
- Radiosity-based inter-reflection for complex geometries
- Glare calculation: DGI (Daylight Glare Index), simplified DGP
- Daylight control: continuous dimming, stepped, on/off based on illuminance setpoints
- Tubular Daylighting Devices (TDD): dome transmittance, pipe losses, diffuser output
- Reference point illuminance calculation with obstruction handling

**Rust crate public API sketch:**

```rust
// crates/ep-daylighting/src/lib.rs

pub struct DaylightingZone {
    pub reference_points: Vec<ReferencePoint>,
    pub control: DaylightControl,
    pub windows: Vec<DaylightWindow>,
}

pub struct ReferencePoint {
    pub position: [f64; 3],        // x, y, z in zone coordinates
    pub illuminance_setpoint: Illuminance,
    pub fraction_controlled: f64,
}

pub enum DaylightControl {
    Continuous { min_power_fraction: f64, min_light_fraction: f64 },
    Stepped { steps: u32 },
    OnOff,
}

pub struct DaylightingManager {
    zones: Vec<DaylightingZone>,
    daylight_factors: Vec<DaylightFactors>,  // Precomputed per window/ref point
}

impl DaylightingManager {
    /// Precompute daylight factors (called once during setup).
    pub fn compute_daylight_factors(
        &mut self,
        surfaces: &SurfaceState,
        solar: &SolarState,
    ) -> SimResult<()>;

    /// Calculate illuminance and electric lighting reduction for timestep.
    pub fn calculate(
        &self,
        state: &mut SimulationState,
    ) -> SimResult<Vec<LightingReduction>>;
}
```

**Dependencies:** `ep-units`, `ep-core`, `ep-solar`, `ep-windows`, `ep-surfaces`

**Complexity estimate:**
- Lines of Rust: ~5,000-7,000
- Effort: 4-5 person-months
- Difficulty: High

**Migration strategy:** Validate daylight factors against DElight reference data
and EnergyPlus daylighting output. Start with simple split-flux, then add
radiosity inter-reflection.

**Risks:**
- Daylight factor precomputation is geometry-intensive
- Glare calculations involve view-direction-dependent luminance maps
- DElight integration may require FFI bridge during transition

### 2.7 Zone Air Heat Balance (`ep-zone`)

**Scope:** EnergyPlus files: `ZoneTempPredictorCorrector.cc` (7,222 lines),
`ZoneEquipmentManager.cc` (7,098 lines), `HeatBalanceAirManager.cc`,
`ZoneContaminantPredictorCorrector.cc`, `DataZoneEquipment.hh`.

**Key algorithms:**
- **Predictor-corrector method:** First predicts zone load assuming
  prior-timestep conditions, then corrects after HVAC system simulation
  (Engineering Reference: Zone Air Heat Balance)
- Zone air energy balance: Q_sys + Q_surfaces + Q_internal + Q_infiltration +
  Q_mixing + Q_ventilation = rho*V*Cp*(dT/dt)
- Numerical schemes: 3rd-order backward difference (default), Euler,
  analytical solution
- Zone mixing and cross-mixing between zones
- Return air path calculations
- Moisture balance (humidity ratio predictor-corrector)
- CO2/generic contaminant balance

**Rust crate public API sketch:**

```rust
// crates/ep-zone/src/lib.rs

pub struct ZoneAirSolver {
    zones: Vec<ZoneAirState>,
    solver_type: ZoneAirSolverType,
    history_depth: usize,           // 3 for 3rd-order backward difference
}

pub enum ZoneAirSolverType {
    ThirdOrderBackwardDifference,
    EulerMethod,
    AnalyticalSolution,
}

pub struct ZoneAirState {
    pub temperature: Temperature,
    pub humidity_ratio: HumidityRatio,
    pub co2_concentration: Option<f64>,

    // Historical values for multi-step methods
    temp_history: [Temperature; 4],
    humidity_history: [HumidityRatio; 4],

    // Loads
    pub system_sensible_load: Power,
    pub system_latent_load: Power,
    pub surface_convective_load: Power,
    pub internal_convective_load: Power,
    pub infiltration_load: Power,
    pub mixing_load: Power,
    pub ventilation_load: Power,

    // Zone parameters
    pub volume: f64,                  // m3
    pub multiplier: f64,
    pub air_mass: f64,                // kg
}

impl ZoneAirSolver {
    /// Predictor step: estimate zone load based on current conditions.
    pub fn predict(&mut self, state: &mut SimulationState) -> SimResult<()>;

    /// Corrector step: update zone temperature after HVAC response.
    pub fn correct(&mut self, state: &mut SimulationState) -> SimResult<()>;
}
```

**Dependencies:** `ep-units`, `ep-core`, `ep-schedule`, `ep-psychrometrics`

**Complexity estimate:**
- Lines of Rust: ~6,000-8,000
- Effort: 4-5 person-months
- Difficulty: High

**Migration strategy:** Validate by comparing zone temperatures and loads
timestep-by-timestep against EnergyPlus. Start with simple single-zone models,
then add mixing and multi-zone.

**Risks:**
- The predictor-corrector creates a tight feedback loop with HVAC; the zone solver
  cannot be validated in isolation from at least ideal-loads HVAC
- Third-order backward difference requires careful initialization during warmup
- Zone mixing creates inter-zone coupling that complicates parallel execution

### 2.8 HVAC Air-Side Systems (`ep-hvac`)

**Scope:** EnergyPlus files (partial list of the largest): `UnitarySystem.cc` (18,344 lines),
`DXCoils.cc` (18,068 lines), `Furnaces.cc` (11,199 lines),
`HVACVariableRefrigerantFlow.cc` (15,565 lines), `SimAirServingZones.cc` (7,767 lines),
`VariableSpeedCoils.cc` (7,723 lines), `WaterCoils.cc` (6,375 lines),
`SingleDuct.cc` (5,859 lines), `MixedAir.cc`, `Fans.cc`, `Humidifiers.cc`,
`HeatRecovery.cc`, `HVACManager.cc`, `HVACControllers.cc`,
`SetPointManager.cc`, `SystemAvailabilityManager.cc`,
plus `Coils/` directory (CoilCoolingDX, CoilCoolingDXCurveFitSpeed, etc.),
`Autosizing/` directory (44 files).

This is the largest subsystem by far, totaling over **100,000 lines** of C++ code.

**Key algorithms:**
- Fan models: constant volume, VAV (variable air volume), on/off, component model
  with motor/belt/VFD losses
- Cooling coil models: DX single/multi-speed, DX variable speed, chilled water,
  evaporative coolers
- Heating coil models: electric, gas, DX heat pump, hot water, steam, desuperheater
- Unitary systems: packaged DX equipment, furnaces, heat pumps (air-to-air,
  water-to-air), VRF (variable refrigerant flow)
- Air handling units: mixing box (OA/return), supply fan, cooling coil, heating coil,
  humidifier, heat recovery
- Air terminal units: single duct VAV, dual duct, induction, powered
- Controller logic: PI control, setpoint managers (scheduled, OA reset,
  warmest/coldest, follow-OA, return-air)
- System availability: night cycle, optimum start, scheduled, differential thermostat

**Rust crate public API sketch:**

```rust
// crates/ep-hvac/src/lib.rs

pub mod fan;
pub mod coil;
pub mod dx;
pub mod unitary;
pub mod air_loop;
pub mod terminal;
pub mod controller;
pub mod availability;
pub mod heat_recovery;
pub mod humidifier;
pub mod mixer;
pub mod sizing;

/// Air-side node state.
#[derive(Debug, Clone)]
pub struct AirNode {
    pub temp: Temperature,
    pub humidity_ratio: HumidityRatio,
    pub enthalpy: Enthalpy,
    pub mass_flow_rate: MassFlowRate,
    pub pressure: Pressure,
    pub quality: f64,             // For two-phase
}

/// Fan component.
pub struct Fan {
    pub name: String,
    pub fan_type: FanType,
    pub max_flow_rate: VolumeFlowRate,
    pub delta_pressure: Pressure,
    pub total_efficiency: f64,
    pub motor_efficiency: f64,
    pub motor_in_air_fraction: f64,
    // ... state variables
}

pub enum FanType {
    ConstantVolume,
    VariableVolume { min_flow_frac: f64, curve: CurveRef },
    OnOff,
    ComponentModel { /* belt, motor, VFD sub-models */ },
}

impl HvacComponent for Fan {
    fn simulate(
        &mut self, state: &mut SimulationState,
        first_hvac_iteration: bool, load: Power, run: bool,
    ) -> SimResult<()> {
        self.init(state, first_hvac_iteration);
        self.calc(state);
        self.update(state);
        Ok(())
    }
    // ...
}

/// DX cooling coil (single speed).
pub struct DxCoolingCoil {
    pub name: String,
    pub rated_capacity: Power,
    pub rated_cop: f64,
    pub rated_flow_rate: VolumeFlowRate,
    pub total_cooling_curve: CurveRef,     // f(Twb, Tdb_condenser)
    pub eir_curve: CurveRef,               // f(Twb, Tdb_condenser)
    pub plf_curve: CurveRef,               // f(part_load_ratio)
    // ... many more parameters
}
```

**Dependencies:** `ep-units`, `ep-core`, `ep-psychrometrics`, `ep-curves`,
`ep-schedule`, `ep-fluids`, `ep-sizing`

**Complexity estimate:**
- Lines of Rust: ~40,000-55,000
- Effort: **18-24 person-months**
- Difficulty: **Very High**

**Migration strategy:** This must be broken into sub-phases:
1. Fans (simplest, well-defined physics)
2. Simple coils (hot water, electric heating)
3. DX cooling coils (single speed, then multi-speed)
4. Unitary systems (package the above)
5. Air loops and controllers
6. Terminal units
7. VRF systems (last, most complex single module)

**Risks:**
- DXCoils.cc alone is 18,000 lines with dozens of performance curve lookups
- VRF systems have complex refrigerant-side models with phase-change physics
- Controller convergence logic is subtle (hunting, overshooting)
- Autosizing creates circular dependencies (equipment sizes depend on loads
  which depend on equipment)

### 2.9 HVAC Water-Side / Plant (`ep-plant`)

**Scope:** EnergyPlus files: `Plant/` directory (28 files),
`PlantChillers.cc` (7,542 lines), `Boilers.cc`, `BoilerSteam.cc`,
`CondenserLoopTowers.cc` (6,330 lines), `Pumps.cc`,
`PlantHeatExchangerFluidToFluid.cc`, `ChillerAbsorption.cc`,
`ChillerElectricEIR.cc`, `ChillerReformulatedEIR.cc`,
`HeatPumpWaterToWaterHEATING.cc`, `HeatPumpWaterToWaterCOOLING.cc`,
`PlantCentralGSHP.cc`, `WaterToAirHeatPump*.cc`,
`LowTempRadiantSystem.cc` (6,016 lines), `PlantPipingSystemsManager.cc` (6,111 lines).

**Key algorithms:**
- Plant loop solver: half-loop iteration with supply/demand sides,
  splitter-mixer topology, 2-8 sub-iterations (PlantLoopSolver.cc)
- Flow resolution: flow request aggregation from demand components,
  supply-side component dispatching
- Equipment operation schemes: load range, cooling/heating priority,
  optimal sequential loading, uncontrolled, user-defined
- Boiler model: efficiency curve f(PLR), hot water production
- Chiller models: EIR (Energy Input Ratio), reformulated EIR,
  absorption (single/double effect), electric, engine-driven
- Cooling tower: YorkCalc, CoolTools, variable speed fan models
- Pumps: constant speed, variable speed, pressure-based
- Demand-side components: coils, radiant systems, SWH
- Common pipe and two-way common pipe for primary-secondary loops

**Rust crate public API sketch:**

```rust
// crates/ep-plant/src/lib.rs

pub mod loop_solver;
pub mod boiler;
pub mod chiller;
pub mod cooling_tower;
pub mod pump;
pub mod heat_exchanger;
pub mod operation;
pub mod pipe;

/// A plant loop with supply and demand sides.
pub struct PlantLoop {
    pub name: String,
    pub loop_type: PlantLoopType,
    pub fluid: FluidType,
    pub sides: [LoopSide; 2],       // [Supply, Demand]
    pub operation_scheme: OperationScheme,
    pub max_flow_rate: MassFlowRate,
    pub min_flow_rate: MassFlowRate,
    pub setpoint_temp: Temperature,
    pub common_pipe: CommonPipeType,
}

pub struct LoopSide {
    pub branches: Vec<Branch>,
    pub splitter: Option<Splitter>,
    pub mixer: Option<Mixer>,
    pub inlet_node: NodeIndex,
    pub outlet_node: NodeIndex,
    pub needs_simulation: bool,
}

pub struct Branch {
    pub name: String,
    pub components: Vec<Box<dyn PlantComponent>>,
    pub flow_rate: MassFlowRate,
    pub pressure_drop: Pressure,
}

/// Plant loop solver.
pub struct PlantManager {
    loops: Vec<PlantLoop>,
    calling_order: Vec<HalfLoopId>,
    min_iterations: u32,            // default 2 (7 with common pipe)
    max_iterations: u32,            // default 8
}

impl PlantManager {
    pub fn simulate(
        &mut self,
        state: &mut SimulationState,
        sim_air: &mut bool,
        sim_zone_equip: &mut bool,
        sim_plant: &mut bool,
    ) -> SimResult<()> {
        let mut iteration = 0;
        loop {
            for half_loop_id in &self.calling_order {
                let (loop_idx, side) = half_loop_id.unpack();
                let plant_loop = &mut self.loops[loop_idx];
                plant_loop.sides[side as usize].solve(state)?;
            }

            iteration += 1;
            if iteration >= self.min_iterations && self.check_convergence() {
                break;
            }
            if iteration >= self.max_iterations {
                break;
            }
        }
        *sim_plant = false;
        Ok(())
    }
}

/// Hot water boiler model.
pub struct Boiler {
    pub name: String,
    pub nominal_capacity: Power,
    pub nominal_efficiency: f64,
    pub efficiency_curve: CurveRef,   // f(PLR)
    pub design_water_flow_rate: MassFlowRate,
    pub min_plr: f64,
    pub max_plr: f64,
    pub optimum_plr: f64,
    // Operating state
    fuel_used: Power,
    heating_output: Power,
    water_outlet_temp: Temperature,
}

impl PlantComponent for Boiler {
    fn simulate(
        &mut self, state: &mut SimulationState,
        first_hvac_iteration: bool, load: Power, run: bool,
    ) -> SimResult<()> {
        if !run || load <= Power::ZERO {
            self.fuel_used = Power::ZERO;
            self.heating_output = Power::ZERO;
            return Ok(());
        }

        let plr = (load / self.nominal_capacity).clamp(self.min_plr, self.max_plr);
        let efficiency = self.nominal_efficiency
            * state.curves.evaluate(self.efficiency_curve, plr);

        self.heating_output = self.nominal_capacity * plr;
        self.fuel_used = self.heating_output / efficiency;

        // Calculate outlet water temperature
        let cp = state.fluids.specific_heat(self.fluid, self.inlet_temp());
        let mdot = self.water_mass_flow_rate();
        if mdot > MassFlowRate::ZERO {
            let delta_t = TempDelta::new(self.heating_output.value() / (mdot.value() * cp.value()));
            self.water_outlet_temp = self.inlet_temp() + delta_t;
        }

        Ok(())
    }
    // ...
}
```

**Dependencies:** `ep-units`, `ep-core`, `ep-fluids`, `ep-curves`, `ep-schedule`

**Complexity estimate:**
- Lines of Rust: ~25,000-35,000
- Effort: **12-16 person-months**
- Difficulty: **Very High**

**Migration strategy:**
1. Plant loop topology and flow resolution (the framework)
2. Pumps (simple, well-defined)
3. Boilers (canonical equipment pattern)
4. Electric EIR chiller (most common chiller model)
5. Cooling towers
6. Remaining equipment

**Risks:**
- Plant loop convergence is the most difficult numerical problem in EnergyPlus
- Common pipe and two-way common pipe logic is notoriously fragile
- Equipment operation scheme dispatching has complex priority logic
- Chiller models involve thermodynamic property lookups that must match E+ exactly

### 2.10 Airflow Network (`ep-airflow`)

**Scope:** EnergyPlus files: `AirflowNetwork/` directory (6 files totaling ~130K+ lines),
`AirflowNetworkBalanceManager.cc`.

The AirflowNetwork module is the single largest subsystem by line count in its
directory files (Elements.hpp alone is ~87,000 lines), though much of this is
data tables and repetitive component definitions.

**Key algorithms:**
- Multi-zone pressure network: nodal pressure/mass-flow equations
- Newton-Raphson iterative solver for nonlinear flow equations
- Component models: cracks (power law), effective leakage areas, doors, windows,
  horizontal openings, duct segments, duct leakage, constant-volume fans
- Wind pressure coefficients on building surfaces
- Stack effect (buoyancy-driven flow)
- Duct heat gain/loss during air transport

**Rust crate public API sketch:**

```rust
// crates/ep-airflow/src/lib.rs

pub mod network;
pub mod elements;
pub mod solver;

/// Airflow network node (zone or external).
pub struct AirflowNode {
    pub name: String,
    pub node_type: AirflowNodeType,
    pub pressure: Pressure,
    pub temperature: Temperature,
    pub humidity_ratio: HumidityRatio,
    pub height: Length,
}

pub enum AirflowNodeType {
    Zone(usize),          // Zone index
    External,             // Outdoor node
    Other,                // Internal duct node
}

/// Airflow element (connection between nodes).
pub trait AirflowElement: Send + Sync {
    /// Calculate mass flow rate given pressure difference.
    fn flow_at_pressure(
        &self, dp: Pressure, density: Density, viscosity: DynamicViscosity,
    ) -> (MassFlowRate, f64);   // (flow, dF/dP for Jacobian)
}

/// Power-law crack model: m_dot = C * (dP)^n
pub struct CrackElement {
    pub coefficient: f64,
    pub exponent: f64,       // typically 0.65
    pub reference_conditions: AirProperties,
}

pub struct AirflowNetworkSolver {
    nodes: Vec<AirflowNode>,
    links: Vec<AirflowLink>,
    jacobian: ndarray::Array2<f64>,
    max_iterations: u32,
    convergence_tolerance: f64,
}

impl AirflowNetworkSolver {
    pub fn solve(&mut self, state: &SimulationState) -> SimResult<()>;
}
```

**Dependencies:** `ep-units`, `ep-core`, `ep-psychrometrics`

**Complexity estimate:**
- Lines of Rust: ~8,000-12,000
- Effort: 5-7 person-months
- Difficulty: High

**Migration strategy:** The airflow network is relatively self-contained. Validate
by comparing zone infiltration/ventilation rates and pressure distributions
against EnergyPlus.

**Risks:**
- Newton-Raphson convergence is sensitive to initial guesses and step sizing
- The Jacobian matrix can become singular for poorly-connected networks
- Wind pressure coefficient data tables are large and building-shape-dependent

### 2.11 Refrigeration Systems (`ep-refrigeration`)

**Scope:** EnergyPlus files: `RefrigeratedCase.cc` (16,340 lines).

**Key algorithms:**
- Display case heat balance: anti-sweat heaters, fan power, lighting, defrost
- Compressor rack: single/two-stage, variable-speed compressor curves
- Walk-in cooler/freezer: door openings, defrost, floor heaters
- Secondary loops: brine systems, cascade condensers
- Condenser: air-cooled, evaporative, water-cooled
- Subcooler, liquid suction heat exchanger
- Transcritical CO2 system models

**Complexity estimate:**
- Lines of Rust: ~8,000-10,000
- Effort: 4-6 person-months
- Difficulty: High

**Migration strategy:** Mostly independent from the main HVAC/plant loop.
Validate against dedicated refrigeration test cases.

**Risks:**
- `RefrigeratedCase.cc` is a single 16K-line monolith that conflates many
  different system types. Decomposition into clean Rust modules requires
  careful understanding of the internal state machine.
- Transcritical CO2 systems have unique thermodynamic property requirements

### 2.12 Water Systems (`ep-water`)

**Scope:** EnergyPlus files: `WaterThermalTanks.cc` (13,199 lines),
`WaterUse.cc`, `WaterManager.cc`, `SolarCollectors.cc`,
`PlantSolarCollectors.cc`, `IceThermalStorage.cc`.

**Key algorithms:**
- Stratified water tank model: 1D multi-node thermal model with
  mixing, heat loss, auxiliary heating, and use-side draw
- Mixed tank model: single-node with deadband control
- Heat pump water heater: wrapped DX coil with stratified tank
- Solar thermal collectors: flat plate (Hottel-Whillier), ICS (integrated collector storage),
  evacuated tube models
- Ice thermal storage: simple, detailed, and tabular models
- Water use connections, fixtures, and drain water heat recovery

**Complexity estimate:**
- Lines of Rust: ~8,000-10,000
- Effort: 4-6 person-months
- Difficulty: High

**Migration strategy:** Water heater models are self-contained with plant loop
interface. Validate stratified tank temperature profiles against EnergyPlus.

**Risks:**
- Stratified tank mixing algorithms are numerically sensitive
- Heat pump water heater wraps both a DX coil and a tank, creating a
  nested simulation loop
- Solar collector models have complex thermal-optical interactions

### 2.13 On-Site Generation (`ep-generation`)

**Scope:** EnergyPlus files: `Photovoltaics.cc`, `WindTurbine.cc`,
`FuelCellElectricGenerator.cc`, `MicroCHPElectricGenerator.cc`,
`ICEngineElectricGenerator.cc`, `MicroturbineElectricGenerator.cc`,
`ElectricPowerServiceManager.cc`, `GeneratorFuelSupply.cc`,
`GeneratorDynamicsManager.cc`.

**Key algorithms:**
- PV models: simple, Sandia, TRNSYS equivalent one-diode (I-V curve,
  Newton-Raphson for operating point)
- Wind turbine: power curve lookup, rotor height wind profile,
  Betz limit considerations
- Fuel cell: electrochemical model, thermal output
- Micro-CHP: thermal-following/electrical-following control
- IC engine/combustion turbine: efficiency curves, heat recovery
- Inverters: CEC lookup table, curve-based, constant efficiency
- Battery storage: simple charge/discharge model, kinetic battery model
- Load center distribution: AC/DC buses, baseload/demand-following dispatch

**Complexity estimate:**
- Lines of Rust: ~6,000-8,000
- Effort: 3-5 person-months
- Difficulty: Medium-High

**Migration strategy:** Each generator type is mostly independent. Validate
electrical output against EnergyPlus for standard test cases. The
ElectricPowerServiceManager (dispatch logic) should be ported last.

**Risks:**
- One-diode PV model requires Newton-Raphson convergence on the I-V curve
- Integration with SSC/SAM library may require FFI during transition
- Battery state-of-charge tracking requires careful timestep handling

### 2.14 Demand-Side Management / EMS (`ep-ems`, `ep-demand`)

**Scope:** EnergyPlus files: `EMSManager.cc`, `RuntimeLanguageProcessor.cc`,
`DataRuntimeLanguage.hh`, `DemandManager.cc`, `PluginManager.cc`.

**Key algorithms:**
- Erl scripting language: tokenizer, parser, stack-based evaluator with 64+
  built-in functions
- 21 calling points distributed throughout the simulation loop
- Actuator/sensor registration and lookup
- Demand manager: rolling-window demand calculation, sequential/optimal/all
  priority shedding
- Plugin manager: Python plugin execution via embedded interpreter

**Rust crate public API sketch:**

```rust
// crates/ep-ems/src/lib.rs

pub mod actuator;
pub mod sensor;
pub mod scripting;

/// EMS actuator that can override a simulation variable.
pub struct Actuator {
    pub component_type: String,
    pub unique_id: String,
    pub control_type: String,
    pub is_active: bool,
    pub value: f64,
    target: ActuatorTarget,     // Internal pointer to the controlled variable
}

/// EMS sensor that reads a simulation variable.
pub struct Sensor {
    pub variable_name: String,
    pub key: String,
    source: SensorSource,       // Internal pointer to the source variable
}

/// Embedded scripting engine (replaces Erl).
/// Uses Rhai for a safe, sandboxed scripting language.
pub struct ScriptEngine {
    engine: rhai::Engine,
    programs: Vec<CompiledProgram>,
    actuators: Vec<Actuator>,
    sensors: Vec<Sensor>,
}

/// Compiled EMS program bound to a calling point.
pub struct CompiledProgram {
    pub name: String,
    pub calling_point: CallingPoint,
    pub ast: rhai::AST,
}
```

**Dependencies:** `ep-units`, `ep-core`, `rhai` (scripting), `ep-schedule`

**Complexity estimate:**
- Lines of Rust: ~5,000-7,000
- Effort: 4-5 person-months
- Difficulty: High

**Migration strategy:** The EMS is deeply integrated with the simulation loop.
Port the actuator/sensor registry first, then the scripting engine. Consider
replacing Erl with Rhai (a Rust-native scripting language) or embedded Python
via PyO3.

**Risks:**
- EMS actuators can override almost any variable in the simulation, creating
  arbitrary coupling between subsystems
- Erl has quirky syntax that differs from standard scripting languages;
  backward compatibility requires either an Erl parser or a migration tool
- 21 calling points mean the EMS hooks into many places in the simulation loop

### 2.15 Output & Reporting (`ep-output`)

**Scope:** EnergyPlus files: `OutputProcessor.cc`, `OutputReportTabular.cc` (19,749 lines),
`OutputReportTabularAnnual.cc`, `OutputReportPredefined.cc`, `OutputReports.cc`,
`ResultsFramework.cc`, `SQLiteProcedures.cc`.

**Key algorithms:**
- Variable registration with pointer-to-source pattern
- Time-step aggregation: sum/average over timestep, hourly, daily, monthly, annual
- 49 end-use categories with meter hierarchy
- Tabular reports: zone component loads, envelope, HVAC sizing, economics
- SQL output: full schema with simulation metadata, variable dictionaries, time series
- ESO/MTR file format: legacy comma-delimited output

**Rust crate public API sketch:**

```rust
// crates/ep-output/src/lib.rs

pub mod variable;
pub mod meter;
pub mod tabular;
pub mod sql;
pub mod csv;

/// Output variable registered by a component.
pub struct OutputVariable {
    pub name: String,
    pub key: String,
    pub units: String,
    pub frequency: ReportFreq,
    pub store_type: StoreType,
    value: f64,
    accumulated: f64,
    min_value: f64,
    max_value: f64,
    count: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum ReportFreq {
    EachCall,
    TimeStep,
    Hourly,
    Daily,
    Monthly,
    RunPeriod,
    Annual,
}

#[derive(Debug, Clone, Copy)]
pub enum StoreType {
    Average,
    Sum,
}

pub struct OutputManager {
    variables: Vec<OutputVariable>,
    meters: Vec<Meter>,
    sql_writer: Option<SqlWriter>,
    csv_writer: Option<CsvWriter>,
}
```

**Dependencies:** `ep-core`, `rusqlite`, `csv`

**Complexity estimate:**
- Lines of Rust: ~8,000-12,000
- Effort: 4-6 person-months
- Difficulty: Medium-High

**Migration strategy:** Output is mostly independent of physics. Start with
CSV output, then SQL, then tabular reports. Validate by comparing output
files line-by-line.

**Risks:**
- `OutputReportTabular.cc` at 19,749 lines is the largest file in the codebase,
  generating dozens of standardized report tables with complex formatting
- Variable registration uses raw pointers in C++; the Rust version needs a
  safe alternative (indices into a variable store, or interior mutability patterns)

### 2.16 Input Processing (`ep-io`)

**Scope:** EnergyPlus files: `InputProcessing/` directory (InputProcessor, IdfParser,
InputValidation, CsvParser, DataStorage), `idd/` directory (schema),
`Energy+.schema.epJSON`.

**Key algorithms:**
- IDF tokenizer and parser (free-format, comma-delimited with `!` comments)
- IDF-to-JSON conversion using the IDD (Input Data Dictionary) schema
- JSON Schema validation
- Case-insensitive object/field lookup with caching
- Object factory pattern for typed object creation

**Rust crate public API sketch:**

```rust
// crates/ep-io/src/lib.rs

pub mod idf;
pub mod schema;
pub mod validation;
pub mod new_format;   // Future: TOML or custom DSL

/// Parsed input model.
pub struct InputModel {
    objects: HashMap<String, Vec<serde_json::Value>>,
    schema: InputSchema,
}

impl InputModel {
    /// Parse an IDF file.
    pub fn from_idf(path: &Path) -> SimResult<Self>;

    /// Parse a JSON input file.
    pub fn from_json(path: &Path) -> SimResult<Self>;

    /// Get all objects of a given type.
    pub fn get_objects(&self, object_type: &str) -> &[serde_json::Value];

    /// Validate against schema.
    pub fn validate(&self) -> Vec<ValidationError>;
}

/// Schema definition for input validation.
pub struct InputSchema {
    object_defs: HashMap<String, ObjectDefinition>,
}
```

**Dependencies:** `ep-core`, `serde`, `serde_json`

**Complexity estimate:**
- Lines of Rust: ~4,000-6,000
- Effort: 3-4 person-months
- Difficulty: Medium

**Migration strategy:** Start with epJSON (already JSON), then add IDF parser.
Consider defining a new input format alongside IDF compatibility.

**Risks:**
- IDF format has 30+ years of accumulated quirks
- ~900 object types with complex inter-object reference validation
- Backward compatibility is critical for ecosystem adoption

### 2.17 Co-Simulation / FMI (`ep-fmi`)

**Scope:** EnergyPlus files: `ExternalInterface.cc`, FMI third-party library,
BCVTB interface.

**Key algorithms:**
- FMI 2.0 co-simulation slave implementation
- Variable exchange at zone timestep boundaries
- Input mapping: schedule overrides, variable values, actuator values
- Synchronization with master simulator

**Complexity estimate:**
- Lines of Rust: ~2,000-3,000
- Effort: 2-3 person-months
- Difficulty: Medium

**Risks:**
- FMI specification compliance requires careful state management
- Must handle asynchronous timing with external simulators

### 2.18 Ground Heat Transfer (`ep-ground`)

**Scope:** EnergyPlus files: `HeatBalanceKivaManager.cc/hh`,
`GroundTemperatureModeling/` directory (5 model files),
`PlantPipingSystemsManager.cc` (6,111 lines),
plus legacy Fortran: `src/Basement/` and `src/Slab/` directories.

**Key algorithms:**
- Kiva: 2D finite-difference ground domain with foundation coupling
  (using the kiva third-party library)
- Site ground temperatures: shallow (Kusuda-Achenbach, Xing) and deep models
- Plant piping systems: buried pipe heat transfer
- Slab-on-grade: 3D heat transfer (legacy Fortran, replaced by Kiva)
- Basement: 3D basement heat transfer (legacy Fortran, replaced by Kiva)

**Complexity estimate:**
- Lines of Rust: ~4,000-6,000
- Effort: 3-4 person-months
- Difficulty: High

**Migration strategy:** Use FFI to call the existing Kiva C++ library initially.
Re-implement the simpler ground temperature models in pure Rust.

**Risks:**
- Kiva is a substantial C++ library; full Rust rewrite would be a project in itself
- Ground heat transfer has very long time constants requiring multi-year warmup

### 2.19 Simulation Manager / Orchestrator (`ep-sim`)

**Scope:** EnergyPlus files: `SimulationManager.cc`, `DataGlobals.hh`,
`HeatBalanceManager.cc` (6,131 lines), `NodeInputManager.cc`,
`BranchInputManager.cc`, `BranchNodeConnections.cc`,
sizing-related files in `Autosizing/` directory (44 files).

**Key algorithms:**
- Simulation loop orchestration (Environment -> Day -> Hour -> TimeStep)
- Warmup convergence detection
- Sizing passes: zone sizing, system sizing, plant sizing
- Design day simulation for sizing
- Node and branch connection validation
- Simulation cost estimation and lifecycle cost analysis

**Complexity estimate:**
- Lines of Rust: ~6,000-8,000
- Effort: 4-5 person-months
- Difficulty: High

**Migration strategy:** The orchestrator is the last piece to come together,
as it depends on all other subsystems. Build incrementally as subsystems
become available.

**Risks:**
- Warmup convergence criteria must exactly match EnergyPlus behavior
- Sizing passes involve iterative re-simulation with different conditions
- The orchestrator mediates all inter-subsystem coupling

---

## 3. Phased Roadmap

### Phase 0 -- Foundation (Months 1-3)

**Objective:** Establish the core infrastructure, build system, and foundational
crates that all subsequent phases depend on.

**Deliverables:**
- `ep-units`: Complete physical units type system with all quantities
- `ep-core`: Error handling, state structures, component traits, node abstractions
- `ep-psychrometrics`: All psychrometric functions (humidity calculations)
- `ep-fluids`: Water, glycol, steam, refrigerant property lookups
- `ep-curves`: Performance curve evaluation (biquadratic, cubic, table lookup, etc.)
- `ep-weather`: EPW parser, solar position, sky models, design day generation
- `ep-schedule`: Complete schedule system with all schedule types
- `ep-io`: IDF parser (read-only), epJSON parser, schema validation framework
- Cargo workspace with CI/CD pipeline
- Regression test infrastructure (`ep-diff` tool)

**Entry criteria:** Project kickoff, team assembled, development environment set up.

**Exit criteria:**
- All foundation crates compile and pass unit tests
- EPW files parse correctly and produce identical weather data to EnergyPlus
- Solar position matches EnergyPlus to within 0.01 degrees
- Schedules evaluate identically to EnergyPlus at every timestep
- Psychrometric functions match EnergyPlus to within machine epsilon
- CI pipeline runs on every commit

**Validation targets:**
- Compare parsed EPW data against EnergyPlus weather output reports
- Compare psychrometric function outputs against ASHRAE reference tables
- Compare schedule values against EnergyPlus schedule output

**Team:** 2-3 engineers (1 senior Rust, 1 building physics, 1 infrastructure)

**Key risks:**
- Floating-point differences between C++ and Rust standard library implementations
  may cause test failures; need to establish tolerance conventions early
- EPW format edge cases in real-world weather files

### Phase 1 -- Envelope (Months 4-8)

**Objective:** Implement the building envelope heat balance, which forms the
physical foundation for all subsequent energy calculations.

**Deliverables:**
- `ep-materials`: Material and construction definitions
- `ep-surfaces`: Surface geometry, vertex processing, view factors
- `ep-solar`: Solar/shading calculations, sunlit fractions
- `ep-envelope`: Complete surface heat balance (CTF + CondFD)
  - Outside surface heat balance
  - Inside surface heat balance
  - Interior radiant exchange (enclosure method)
  - Convection coefficient models (minimum: TARP interior, DOE-2 exterior)
- `ep-windows`: Basic window model (single/double pane, no complex fenestration)
- `ep-ground`: Kusuda-Achenbach ground temperature model

**Entry criteria:** Phase 0 complete and passing.

**Exit criteria:**
- Single-zone models with opaque surfaces produce zone loads within 1% of EnergyPlus
- CTF coefficients match EnergyPlus output for standard constructions
- Solar shading calculations match E+ for simple building geometries
- Simple window models match E+ solar heat gain and U-value calculations

**Validation targets:**
- ASHRAE Standard 140 cases 600-650 (opaque envelope, basic windows)
- EnergyPlus example files: `1ZoneUncontrolled.idf`, `5ZoneAirCooled.idf` (envelope only)

**Team:** 3-4 engineers (2 building physics, 1 numerical methods, 1 geometry)

**Key risks:**
- CTF state-space eigenvalue computation is numerically fragile
- View factor calculation for complex geometries requires robust polygon algorithms
- Interior radiant exchange convergence for highly-coupled enclosures

### Phase 2 -- Zone (Months 9-12)

**Objective:** Add zone air heat balance, basic HVAC (ideal loads), airflow,
and daylighting to enable complete zone-level energy calculations.

**Deliverables:**
- `ep-zone`: Zone air predictor-corrector, moisture balance, mixing/ventilation
- `ep-airflow`: Airflow network solver (simplified: infiltration + ventilation)
- `ep-daylighting`: Split-flux daylighting model, daylight controls
- Ideal loads air system (for testing zone balance without full HVAC)
- `ep-output`: Basic output variable system (CSV + SQL)

**Entry criteria:** Phase 1 complete and passing envelope validation.

**Exit criteria:**
- Multi-zone models with ideal loads match E+ zone temperatures within 0.1 degC
- Zone loads match within 1% for annual simulations
- Airflow network produces matching infiltration rates
- Daylighting reduces electric lighting loads correctly

**Validation targets:**
- ASHRAE Standard 140 cases 600-960 (complete BESTEST suite, opaque + windows)
- EnergyPlus example files with ideal loads
- Multi-zone models with zone mixing

**Team:** 3-4 engineers (2 building physics, 1 HVAC, 1 daylighting)

**Key risks:**
- Predictor-corrector feedback loop must converge stably
- Third-order backward difference initialization during warmup transitions

### Phase 3 -- HVAC (Months 13-20)

**Objective:** Implement the full HVAC simulation capability: air-side systems,
plant loops, and all major equipment types.

This is the longest and most complex phase, subdivided into sub-phases:

**Phase 3a -- HVAC Core (Months 13-15):**
- Fans (all types), simple heating/cooling coils (electric, hot water, chilled water)
- Air loops with OA mixer, supply fan, and coils
- Single-duct VAV terminals
- Setpoint managers and controllers
- Basic plant loop solver with pumps and boilers

**Phase 3b -- DX and Unitary (Months 16-18):**
- DX cooling coils (single-speed, multi-speed, variable speed)
- Unitary systems (furnaces, DX heat pumps)
- Heat recovery (air-to-air)
- Humidifiers
- EIR chillers and cooling towers
- Ice thermal storage

**Phase 3c -- Advanced HVAC (Months 19-20):**
- VRF systems
- Absorption chillers
- Low-temperature radiant systems
- Dedicated outdoor air systems (DOAS)
- Remaining terminal unit types
- Complete autosizing

**Entry criteria:** Phase 2 complete and passing zone validation.

**Exit criteria:**
- Standard HVAC configurations (VAV, CAV, DOAS, VRF) produce annual energy
  within 2% of EnergyPlus
- Plant loops converge reliably for all standard configurations
- Autosizing produces equipment capacities within 5% of EnergyPlus

**Validation targets:**
- EnergyPlus example files: `5ZoneAirCooled.idf`, `DOASDualDuctSchool.idf`,
  `HospitalLowEnergy.idf`, `LargeOfficeDetailed.idf`
- DOE commercial reference buildings (small, medium, large office)

**Team:** 5-7 engineers (3 HVAC, 2 plant, 1 controls, 1 integration/test)

**Key risks:**
- HVAC module count is staggering (100,000+ lines in E+)
- Controller convergence is subtle and model-dependent
- Plant loop solver convergence with diverse equipment mixes
- Autosizing circular dependencies

### Phase 4 -- Advanced Features (Months 21-26)

**Objective:** Complete the feature set with advanced capabilities.

**Deliverables:**
- `ep-ems`: Scripting engine (Rhai-based), actuator/sensor registry
- `ep-fmi`: FMI 2.0 co-simulation interface
- `ep-generation`: PV, wind, fuel cell, battery, inverter models
- `ep-refrigeration`: Supermarket refrigeration systems
- `ep-water`: DHW, solar thermal, stratified tanks
- `ep-demand`: Demand-side management
- `ep-windows`: Complex fenestration (BSDF), equivalent layer model
- `ep-daylighting`: Full radiosity model, TDD
- `ep-ground`: Kiva integration (FFI or rewrite)
- `ep-api`: C API and Python bindings (PyO3)

**Entry criteria:** Phase 3 complete and passing HVAC validation.

**Exit criteria:**
- All advanced features produce results within 2% of EnergyPlus
- Python API provides equivalent functionality to EnergyPlus Python API
- FMI co-simulation works with standard FMU test cases

**Validation targets:**
- EnergyPlus example files using advanced features
- PV/battery test cases from SAM validation suite
- FMI compliance tests

**Team:** 4-6 engineers (specialists per domain)

### Phase 5 -- Parity & Validation (Months 27-30)

**Objective:** Achieve full validation parity with EnergyPlus and production readiness.

**Deliverables:**
- Complete ASHRAE Standard 140 (BESTEST) compliance
- Regression testing against all 822 EnergyPlus example files
- Performance benchmarking (target: 2x faster than E+ for sequential, 4-8x with parallelism)
- Documentation: mdBook user guide, rustdoc API reference
- IDF-to-new-format migration tool
- OpenStudio SDK integration layer
- Release packaging and distribution

**Entry criteria:** Phase 4 complete, all major features implemented.

**Exit criteria:**
- BESTEST compliance for all applicable test cases
- 95%+ of example files produce results within 2% tolerance
- Performance meets or exceeds targets
- Documentation complete
- Package builds for Linux, macOS, Windows

**Validation targets:**
- Full BESTEST suite (Standard 140-2017 and later)
- All 822 EnergyPlus example IDF files
- DOE commercial prototype buildings
- ASHRAE 90.1 appendix G baseline models
- Empirical validation against measured building data (where available)

**Team:** 4-5 engineers (2 validation, 1 performance, 1 docs, 1 integration)

---

## 4. Validation & Testing Strategy

### 4.1 Regression Testing Against EnergyPlus

The primary validation method is regression testing: running identical input files
through both EnergyPlus and the Rust engine, then comparing outputs.

**`ep-diff` tool:**

```rust
// tools/ep-diff/src/main.rs

/// Compare two simulation output files within tolerance.
pub struct OutputComparison {
    pub file_a: PathBuf,           // EnergyPlus reference
    pub file_b: PathBuf,           // Rust engine output
    pub absolute_tolerance: f64,   // Default: 0.01
    pub relative_tolerance: f64,   // Default: 0.001 (0.1%)
    pub results: Vec<VariableComparison>,
}

pub struct VariableComparison {
    pub name: String,
    pub key: String,
    pub max_absolute_diff: f64,
    pub max_relative_diff: f64,
    pub rms_diff: f64,
    pub num_timesteps: usize,
    pub pass: bool,
}
```

**Tolerance tiers:**
- **Tier 1 (strict):** < 0.01% relative difference. For temperatures (degC),
  flow rates, energy totals. Required for foundation modules (weather, psychrometrics).
- **Tier 2 (standard):** < 1% relative difference. For zone loads, equipment
  energy consumption, heating/cooling demand. Required for BESTEST compliance.
- **Tier 3 (relaxed):** < 5% relative difference. For complex HVAC models
  where iterative convergence may differ. Acceptable during initial porting.

### 4.2 Property-Based Testing

Use `proptest` for numerical kernels:

```rust
// Example: property test for psychrometric functions
use proptest::prelude::*;

proptest! {
    #[test]
    fn humidity_ratio_roundtrip(
        tdb in -40.0f64..60.0,
        rh in 1.0f64..100.0,
        p in 80000.0f64..110000.0,
    ) {
        let w = humidity_ratio_from_rh(
            Temperature::from_celsius(tdb),
            RelativeHumidity::new(rh),
            Pressure::new(p),
        );
        let rh_back = rh_from_humidity_ratio(
            Temperature::from_celsius(tdb),
            w,
            Pressure::new(p),
        );
        prop_assert!((rh_back.value() - rh).abs() < 0.01);
    }

    #[test]
    fn ctf_energy_conservation(
        layers in prop::collection::vec(
            (0.01f64..1.0, 0.1..5.0, 500.0..2500.0, 500.0..2000.0),
            1..5
        ),
    ) {
        // CTF coefficients must conserve energy: sum of all response
        // factors should equal steady-state U-value
        let construction = build_construction_from_layers(&layers);
        let ctf = compute_ctf(&construction);
        let u_ctf = ctf_steady_state_u_value(&ctf);
        let u_analytic = analytic_u_value(&construction);
        prop_assert!((u_ctf - u_analytic).abs() / u_analytic < 0.001);
    }
}
```

### 4.3 ASHRAE Standard 140 (BESTEST) Roadmap

| Test Case | Description | Phase | Priority |
|-----------|-------------|-------|----------|
| 600-650 | Low-mass, lightweight building | Phase 1 | High |
| 900-960 | High-mass, heavyweight building | Phase 2 | High |
| HE100-HE230 | Heating equipment (furnaces, heat pumps) | Phase 3a | High |
| CE100-CE545 | Cooling equipment (DX coils) | Phase 3b | High |
| GC10-GC80 | Ground-coupled slab-on-grade | Phase 1-2 | Medium |
| IN-DEPTH | Window properties | Phase 1 | Medium |
| Airflow | Multi-zone airflow | Phase 2 | Medium |
| HVAC BESTEST | Full HVAC system tests | Phase 3c | High |

### 4.4 Continuous Integration Pipeline

```yaml
# .github/workflows/ci.yml
name: CI
on: [push, pull_request]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo check --workspace --all-features
      - run: cargo clippy --workspace -- -D warnings
      - run: cargo fmt --check

  test:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test --workspace

  benchmark:
    runs-on: ubuntu-latest
    if: github.event_name == 'pull_request'
    steps:
      - run: cargo bench --workspace -- --output-format bencher | tee bench.txt
      - uses: benchmark-action/github-action-benchmark@v1

  regression:
    runs-on: ubuntu-latest
    needs: test
    steps:
      - run: cargo build --release
      - run: python tests/regression/run_regression.py --tolerance 0.01
```

### 4.5 Fuzzing Strategy

Fuzz input parsing to find crashes and panics:

```rust
// fuzz/fuzz_targets/idf_parser.rs
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = ep_io::idf::parse_idf(text);
    }
});
```

Run with `cargo fuzz run idf_parser` using `cargo-fuzz`.

### 4.6 Numerical Tolerance

**Policy:** The Rust engine targets bit-for-bit reproducibility across runs
on the same platform. Cross-platform reproducibility is a non-goal due to
hardware floating-point differences, but results should be within Tier 1
tolerance across platforms.

**Implementation:**
- No `f64::fast_math` or `-ffast-math` equivalents
- Summation order is deterministic (fixed iteration order)
- Random seed is configurable for any stochastic operations
- Parallel operations use indexed access, not order-dependent reductions

---

## 5. Build & Tooling

### 5.1 Cargo Workspace Configuration

```toml
# Cargo.toml (workspace root)
[workspace]
resolver = "2"
members = [
    "crates/ep-units",
    "crates/ep-core",
    "crates/ep-psychrometrics",
    "crates/ep-fluids",
    "crates/ep-curves",
    "crates/ep-weather",
    "crates/ep-schedule",
    "crates/ep-io",
    "crates/ep-materials",
    "crates/ep-surfaces",
    "crates/ep-solar",
    "crates/ep-envelope",
    "crates/ep-windows",
    "crates/ep-daylighting",
    "crates/ep-zone",
    "crates/ep-airflow",
    "crates/ep-hvac",
    "crates/ep-plant",
    "crates/ep-refrigeration",
    "crates/ep-water",
    "crates/ep-generation",
    "crates/ep-ground",
    "crates/ep-ems",
    "crates/ep-demand",
    "crates/ep-output",
    "crates/ep-fmi",
    "crates/ep-sizing",
    "crates/ep-sim",
    "crates/ep-api",
    "tools/idf-convert",
    "tools/ep-diff",
]

[workspace.dependencies]
# Shared dependency versions
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
anyhow = "1"
rayon = "1"
ndarray = "0.16"
chrono = { version = "0.4", default-features = false }
rusqlite = { version = "0.32", features = ["bundled"] }
csv = "1"
log = "0.4"
rhai = "1"

[profile.release]
lto = "thin"
codegen-units = 4
opt-level = 3
```

### 5.2 Benchmarking Framework

Use `criterion` for micro-benchmarks:

```rust
// crates/ep-psychrometrics/benches/psychrometrics.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ep_psychrometrics::*;
use ep_units::*;

fn bench_saturation_pressure(c: &mut Criterion) {
    c.bench_function("saturation_pressure", |b| {
        b.iter(|| {
            for t in -40..60 {
                black_box(saturation_pressure(Temperature::from_celsius(t as f64)));
            }
        })
    });
}

fn bench_humidity_ratio(c: &mut Criterion) {
    c.bench_function("humidity_ratio_from_tdb_twb", |b| {
        b.iter(|| {
            black_box(humidity_ratio(
                Temperature::from_celsius(35.0),
                Temperature::from_celsius(25.0),
                Pressure::new(101325.0),
            ))
        })
    });
}

criterion_group!(benches, bench_saturation_pressure, bench_humidity_ratio);
criterion_main!(benches);
```

### 5.3 FFI Strategy for Transition Period

During the multi-year migration, some C/C++ libraries will be called via FFI:

```rust
// crates/ep-ground/src/kiva_ffi.rs

#[link(name = "kiva")]
extern "C" {
    fn kiva_create_instance(config: *const KivaConfig) -> *mut KivaInstance;
    fn kiva_set_boundary_conditions(
        instance: *mut KivaInstance,
        indoor_temp: f64,
        outdoor_temp: f64,
        wind_speed: f64,
    );
    fn kiva_solve(instance: *mut KivaInstance) -> f64;  // returns heat flux
    fn kiva_destroy(instance: *mut KivaInstance);
}

/// Safe wrapper around Kiva C library.
pub struct KivaFoundation {
    instance: *mut KivaInstance,
}

impl Drop for KivaFoundation {
    fn drop(&mut self) {
        unsafe { kiva_destroy(self.instance); }
    }
}

// Safety: KivaInstance is used single-threaded within the simulation loop.
unsafe impl Send for KivaFoundation {}
```

**Libraries requiring FFI during transition:**
- Kiva (ground heat transfer) -- until pure Rust implementation
- Windows-CalcEngine (Tarcog) -- until pure Rust window thermal model
- DElight (daylighting) -- until Rust radiosity implementation
- Penumbra (GPU shading) -- optional, may keep as C++ with FFI permanently

### 5.4 Python Bindings (PyO3)

```rust
// crates/ep-api/src/python.rs

use pyo3::prelude::*;

#[pyclass]
pub struct EnergyPlusSimulation {
    state: SimulationState,
    orchestrator: SimulationOrchestrator,
}

#[pymethods]
impl EnergyPlusSimulation {
    #[new]
    fn new() -> Self { /* ... */ }

    fn load_input(&mut self, path: &str) -> PyResult<()> { /* ... */ }

    fn run(&mut self) -> PyResult<()> {
        self.orchestrator.run(&mut self.state)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    fn get_variable(&self, name: &str, key: &str) -> PyResult<f64> { /* ... */ }

    fn set_actuator(&mut self, name: &str, value: f64) -> PyResult<()> { /* ... */ }
}

#[pymodule]
fn energyplus_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<EnergyPlusSimulation>()?;
    Ok(())
}
```

### 5.5 WebAssembly Target

For browser-based simulations (design tools, education):

```toml
# crates/ep-sim/Cargo.toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
```

Compile with: `cargo build --target wasm32-unknown-unknown --release`

Note: WebAssembly builds exclude FFI dependencies (Kiva, Penumbra) and
file I/O. Weather data and inputs must be provided as in-memory buffers.

### 5.6 Documentation Strategy

- **rustdoc:** Every public API has doc comments with examples
- **mdBook:** User guide covering installation, input format, running simulations
- **Engineering Reference:** Port key algorithm descriptions from EnergyPlus
  Engineering Reference, with references to the original document sections

---

## 6. Comparison with Existing Efforts

### 6.1 Spawn of EnergyPlus (LBNL)

Spawn was LBNL's project to couple EnergyPlus's envelope model with Modelica-based
HVAC simulation. It was partially funded by DOE but has seen limited adoption.

**Key differences from this Rust rewrite:**
- Spawn kept EnergyPlus's C++ envelope code and only replaced HVAC with Modelica.
  This rewrite replaces everything.
- Spawn required a Modelica runtime (OpenModelica or Dymola), adding significant
  complexity. The Rust engine is a single binary with no runtime dependencies.
- Spawn's two-language architecture created integration challenges at the
  envelope-HVAC boundary. A single-language Rust implementation avoids this.

### 6.2 Modelica-Based Tools (OpenModelica, Modelon)

Modelica tools like the LBNL Buildings library model HVAC systems as equation-based
component networks. They excel at controls research but struggle with whole-building
simulation performance and input complexity.

**Advantages of the Rust approach:**
- Compile to a single native binary (no Modelica runtime)
- Explicit solver control vs. Modelica's DAE solver overhead
- Better performance for annual simulations (10-100x faster than Modelica)
- Direct backward compatibility with EnergyPlus input files

### 6.3 PassiveLogic

PassiveLogic is developing a proprietary real-time physics engine for digital twins.
It targets real-time control rather than annual energy simulation.

**Advantages of the Rust approach:**
- Open source (BSD license) vs. proprietary
- Validated against ASHRAE Standard 140
- Full annual simulation capability
- Ecosystem compatibility (OpenStudio, Ladybug Tools)

### 6.4 OpenStudio SDK

OpenStudio is a middleware layer on top of EnergyPlus that provides building model
construction, measure-based workflows, and results processing.

**Integration strategy:** The Rust engine should provide a compatibility API
that allows OpenStudio to use it as a drop-in replacement for the EnergyPlus
executable. This requires:
- IDF input file compatibility
- SQL output file compatibility
- C API compatibility (for OpenStudio's EnergyPlus function calls)
- Command-line interface compatibility

### 6.5 Why Rust?

| Factor | C++ (status quo) | Rust | Modelica | Python |
|--------|-----------------|------|----------|--------|
| Memory safety | Manual | Compile-time | Runtime (GC) | Runtime (GC) |
| Performance | Excellent | Excellent | Good | Poor |
| Concurrency | Error-prone | Safe by design | Limited | GIL-limited |
| Build time | Minutes (with PCH) | Minutes | N/A | N/A |
| Ecosystem | Mature | Growing rapidly | Niche | Excellent |
| Error handling | Ad-hoc | Type-safe Result | Exceptions | Exceptions |
| Testing | GoogleTest | Built-in | Limited | pytest |
| WASM support | Difficult | Native | No | Pyodide |
| Package management | CMake (fragile) | Cargo (excellent) | Tool-specific | pip |

**Key Rust advantages for this project:**
1. **Memory safety without GC:** EnergyPlus has known memory bugs (use-after-free,
   buffer overflows) that Rust's ownership system prevents at compile time
2. **Fearless concurrency:** Safe parallelism for independent zone/surface calculations
3. **Type-safe units:** Compile-time unit checking prevents physics bugs
4. **Cargo ecosystem:** Vastly simpler dependency management than CMake
5. **WASM compilation:** Enables browser-based simulation tools
6. **Modern error handling:** `Result<T, E>` replaces the `ShowFatalError` pattern

---

## 7. Open Questions & Decision Points

### 7.1 Fixed vs. Variable Timestep

**Current EnergyPlus:** Fixed timestep (user-selected: 1, 2, 3, 4, 5, 6, 10, 12,
15, 20, 30, 60 minutes). HVAC uses a system timestep that can subdivide the zone timestep.

**Options:**
- **A) Keep fixed timestep:** Simpler, deterministic, matches E+ behavior for validation
- **B) Adaptive timestep:** Better accuracy during transients (sunrise, HVAC startup),
  but harder to validate and may affect output time alignment

**Recommendation:** Start with fixed timestep for validation parity.
Add adaptive timestep as an optional feature in Phase 5.

### 7.2 Input Format

**Options:**
- **A) IDF-only:** Maximum backward compatibility, but perpetuates a difficult format
- **B) IDF + new format:** Support both, with a migration tool
- **C) New format only:** Clean break, but alienates existing users

**Recommendation:** Option B. Support IDF for backward compatibility, but design
a new format (likely TOML-based or custom DSL) that is the primary input method.
Provide an `idf-convert` tool for migration.

```toml
# Example new format (TOML-based)
[building]
name = "Small Office"
north_axis = 0.0
terrain = "suburbs"

[[zone]]
name = "Core Zone"
volume = 500.0  # m3
floor_area = 200.0  # m2

[[zone.surface]]
name = "South Wall"
type = "wall"
construction = "Steel Frame Wall"
outside_boundary = "outdoors"
vertices = [
    [0.0, 0.0, 0.0],
    [10.0, 0.0, 0.0],
    [10.0, 0.0, 3.0],
    [0.0, 0.0, 3.0],
]
```

### 7.3 Scripting Language for EMS

**Options:**
- **A) Rhai:** Pure Rust, sandboxed, safe. But different syntax from Erl.
- **B) Lua (via rlua):** Widely used in game engines, fast. Familiar to many engineers.
- **C) Python (via PyO3):** Most familiar to building energy community. But heavy runtime.
- **D) Keep Erl:** Maximum compatibility. But Erl is a poor language.

**Recommendation:** Option A (Rhai) as the primary scripting language, with
Python (Option C) available as an optional plugin system. Provide an
Erl-to-Rhai transpiler for migration.

### 7.4 Parallelism vs. Determinism

**Constraint:** Building energy simulations are used for code compliance and
certification. Results must be reproducible.

**Policy:**
- Default mode: deterministic (sequential execution, bit-reproducible)
- Optional `--parallel` flag enables Rayon-based parallelism
- Parallel results must be within Tier 1 tolerance of sequential results
- Parallel results are reproducible for a given thread count and platform

### 7.5 Licensing

**Recommendation:** BSD-3-Clause or Apache-2.0 + MIT dual license, matching
EnergyPlus's open-source model. The engine should remain freely available
for commercial and academic use.

### 7.6 IDF Backward Compatibility

**Question:** How many versions of IDF should the Rust engine support?

**Recommendation:** Support the current IDF version (matching the latest EnergyPlus
release). Provide the `idf-convert` tool to upgrade older IDF files. Do not
attempt to support the full history of IDF format changes in the parser.

### 7.7 Relationship to EnergyPlus

**Question:** Is this a fork, a successor, or an independent project?

**Recommendation:** Independent project with EnergyPlus validation parity.
Position it as a next-generation engine that can serve the same use cases
but with better performance, safety, and extensibility. Maintain close
collaboration with the EnergyPlus development team at NREL/LBNL for
algorithm validation and testing methodology.

---

## Appendix A: Effort Summary

| Crate | Lines (est.) | Person-Months | Difficulty |
|-------|-------------|--------------|------------|
| ep-units | 500 | 0.5 | Low |
| ep-core | 2,000 | 1.5 | Medium |
| ep-psychrometrics | 800 | 0.5 | Low |
| ep-fluids | 2,000 | 1.5 | Medium |
| ep-curves | 1,500 | 1 | Low |
| ep-weather | 5,000 | 3 | Medium |
| ep-schedule | 3,500 | 2 | Low-Medium |
| ep-io | 5,000 | 3.5 | Medium |
| ep-materials | 2,000 | 1 | Low |
| ep-surfaces | 4,000 | 2.5 | Medium |
| ep-solar | 7,000 | 4.5 | High |
| ep-envelope | 14,000 | 7 | Very High |
| ep-windows | 11,000 | 6 | Very High |
| ep-daylighting | 6,000 | 4.5 | High |
| ep-zone | 7,000 | 4.5 | High |
| ep-airflow | 10,000 | 6 | High |
| ep-hvac | 48,000 | 21 | Very High |
| ep-plant | 30,000 | 14 | Very High |
| ep-refrigeration | 9,000 | 5 | High |
| ep-water | 9,000 | 5 | High |
| ep-generation | 7,000 | 4 | Medium-High |
| ep-ground | 5,000 | 3.5 | High |
| ep-ems | 6,000 | 4.5 | High |
| ep-demand | 2,000 | 1.5 | Medium |
| ep-output | 10,000 | 5 | Medium-High |
| ep-fmi | 2,500 | 2.5 | Medium |
| ep-sizing | 5,000 | 3 | High |
| ep-sim | 7,000 | 4.5 | High |
| ep-api | 3,000 | 2 | Medium |
| Tools | 3,000 | 2 | Low |
| **Total** | **~227,000** | **~127** | |

**Total estimated effort: ~127 person-months (10.6 person-years)**

With a core team of 5 engineers, the project would take approximately
**2.5 years** for Phase 0-4 and **3 years** to reach full parity (Phase 5).

With a team of 8 engineers (allowing more parallelism between subsystems),
the timeline compresses to approximately **2 years** to Phase 4 and
**2.5 years** to full parity.

---

## Appendix B: Dependency Graph

```
ep-units (no deps)
    |
ep-core (ep-units)
    |
    +-- ep-psychrometrics (ep-units, ep-core)
    +-- ep-fluids (ep-units, ep-core)
    +-- ep-curves (ep-units, ep-core)
    +-- ep-schedule (ep-units, ep-core)
    +-- ep-io (ep-core, serde, serde_json)
    +-- ep-output (ep-core, rusqlite, csv)
    |
ep-weather (ep-units, ep-core, ep-psychrometrics)
    |
ep-materials (ep-units, ep-core)
    |
ep-surfaces (ep-units, ep-core, ep-materials)
    |
ep-solar (ep-units, ep-core, ep-weather, ep-surfaces)
    |
ep-envelope (ep-units, ep-core, ep-materials, ep-surfaces, ep-solar)
    |
ep-windows (ep-units, ep-core, ep-materials, ep-solar)
    |
ep-daylighting (ep-units, ep-core, ep-solar, ep-windows, ep-surfaces)
    |
ep-zone (ep-units, ep-core, ep-schedule, ep-psychrometrics)
    |
ep-airflow (ep-units, ep-core, ep-psychrometrics)
    |
ep-hvac (ep-units, ep-core, ep-psychrometrics, ep-fluids, ep-curves, ep-schedule, ep-sizing)
    |
ep-plant (ep-units, ep-core, ep-fluids, ep-curves, ep-schedule)
    |
ep-sim (ALL crates)
    |
ep-api (ep-sim, pyo3)
```

---

## Appendix C: EnergyPlus Source File Mapping

The following table maps EnergyPlus C++ source files to their target Rust crate.
Files are listed in order of size (largest first).

| E+ Source File | Lines | Target Crate |
|---------------|-------|-------------|
| OutputReportTabular.cc | 19,749 | ep-output |
| UnitarySystem.cc | 18,344 | ep-hvac |
| DXCoils.cc | 18,068 | ep-hvac |
| RefrigeratedCase.cc | 16,340 | ep-refrigeration |
| HVACVariableRefrigerantFlow.cc | 15,565 | ep-hvac |
| SurfaceGeometry.cc | 15,535 | ep-surfaces |
| WaterThermalTanks.cc | 13,199 | ep-water |
| SolarShading.cc | 13,085 | ep-solar |
| Furnaces.cc | 11,199 | ep-hvac |
| DaylightingManager.cc | 10,099 | ep-daylighting |
| HeatBalanceSurfaceManager.cc | 10,094 | ep-envelope |
| WeatherManager.cc | 8,867 | ep-weather |
| InternalHeatGains.cc | 8,823 | ep-schedule |
| WindowManager.cc | 8,564 | ep-windows |
| WindowEquivalentLayer.cc | 8,108 | ep-windows |
| StandardRatings.cc | 7,816 | ep-hvac |
| SimAirServingZones.cc | 7,767 | ep-hvac |
| VariableSpeedCoils.cc | 7,723 | ep-hvac |
| PlantChillers.cc | 7,542 | ep-plant |
| ZoneTempPredictorCorrector.cc | 7,222 | ep-zone |
| ZoneEquipmentManager.cc | 7,098 | ep-hvac |
| FluidProperties.cc | 6,974 | ep-fluids |
| ConvectionCoefficients.cc | 6,610 | ep-envelope |
| WaterCoils.cc | 6,375 | ep-hvac |
| CondenserLoopTowers.cc | 6,330 | ep-plant |
| HeatBalanceManager.cc | 6,131 | ep-envelope |
| PlantPipingSystemsManager.cc | 6,111 | ep-ground |
| LowTempRadiantSystem.cc | 6,016 | ep-plant |
| SingleDuct.cc | 5,859 | ep-hvac |

---

*End of EnergyPlus Rust Rewrite Plan v1.0*
