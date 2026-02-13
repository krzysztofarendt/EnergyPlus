//! Heating and cooling coil models for EnergyPlus-rs.
//!
//! - Water coils: effectiveness-NTU method for sensible/latent heat transfer
//! - DX cooling coils: rated capacity with performance curves (CapFTemp, EIRFTemp, EIRFPLR)
//! - Simple heating coils: electric or gas with efficiency
//! - Air-to-air heat exchangers: sensible and latent effectiveness

pub mod dx;
pub mod heating;
pub mod hx;
pub mod water;
