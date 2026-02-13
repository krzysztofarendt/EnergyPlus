//! Plant loop infrastructure and equipment for EnergyPlus-rs.
//!
//! Modules:
//! - `loop_topology`: Plant loop, half-loop, branch/splitter/mixer structures
//! - `boiler`: Hot water boiler with efficiency curve
//! - `chiller`: Electric chiller with EIR curves
//! - `tower`: Cooling tower (single-speed, variable-speed)
//! - `pump`: Constant and variable speed pumps
//! - `water_heater`: Mixed and stratified water heater tanks

pub mod boiler;
pub mod chiller;
pub mod loop_topology;
pub mod pump;
pub mod tower;
pub mod water_heater;
