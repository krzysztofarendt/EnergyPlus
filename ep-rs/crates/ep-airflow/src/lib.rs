//! Multizone pressure-based airflow network solver for EnergyPlus-rs.
//!
//! Models airflow through a building as a network of nodes and links.
//! Each link contains a component (crack, opening, duct, etc.) that
//! relates pressure drop to airflow via the power law or other relations.
//!
//! The system of nonlinear equations is solved using Newton-Raphson iteration
//! with a skyline LU factorization of the Jacobian.

pub mod components;
pub mod network;
pub mod solver;
