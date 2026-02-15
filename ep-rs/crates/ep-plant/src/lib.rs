//! Plant loop infrastructure and equipment for EnergyPlus-rs.
//!
//! Modules:
//! - `loop_topology`: Plant loop, half-loop, branch/splitter/mixer structures
//! - `loop_solver`: Plant loop solver with half-loop iteration and convergence
//! - `boiler`: Hot water boiler with efficiency curve
//! - `chiller`: Electric chillers (EIR, reformulated EIR, absorption, constant COP)
//! - `tower`: Cooling tower (single-speed, variable-speed)
//! - `pump`: Constant, variable speed, and headered pumps
//! - `water_heater`: Mixed and stratified water heater tanks
//! - `heat_pump`: Water-to-water and EIR heat pump models
//! - `ghx`: Ground heat exchangers (vertical borehole, slinky, surface)
//! - `storage`: Thermal storage (stratified tank, ice storage)
//! - `pipe`: Pipe models (adiabatic, heat transfer)

pub mod boiler;
pub mod chiller;
pub mod district;
pub mod evap_cooler;
pub mod fluid_hx;
pub mod ghx;
pub mod heat_pump;
pub mod loop_solver;
pub mod loop_topology;
pub mod pipe;
pub mod pump;
pub mod storage;
pub mod tower;
pub mod water_heater;
