//! On-site power generation models.
//!
//! Photovoltaic panels (simple and one-diode), wind turbines,
//! battery storage (simple bucket and kinetic), electric load centers,
//! generators (combustion, fuel cell), and power conversion (inverters).

pub mod battery;
pub mod generator;
pub mod load_center;
pub mod photovoltaic;
pub mod wind;
