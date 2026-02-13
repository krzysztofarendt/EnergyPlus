//! HVAC air-side system models for EnergyPlus-rs.
//!
//! Modules:
//! - `air_loop`: Air loop (AHU) topology and control
//! - `zone_equipment`: Zone-level HVAC equipment (terminal units, baseboard)
//! - `unitary`: Unitary systems (packaged DX, heat pumps, furnaces)
//! - `setpoint`: Setpoint manager models
//! - `controller`: Air-side controller logic (OA, coil)

pub mod air_loop;
pub mod controller;
pub mod setpoint;
pub mod unitary;
pub mod zone_equipment;
