//! Functional Mock-up Interface (FMI) co-simulation support.
//!
//! Provides FMI 2.0 variable exchange, model description parsing,
//! and co-simulation step management. Supports both FMU import
//! (running external FMUs) and export (exposing EnergyPlus as FMU).

pub mod exchange;
pub mod model;

pub use exchange::{ExchangeVariable, VariableDirection, ExchangeManager};
pub use model::{FmiModelDescription, FmiVariable, FmiCausality, FmiVariability};
