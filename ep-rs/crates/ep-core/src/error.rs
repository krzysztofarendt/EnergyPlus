//! Structured error types for the simulation engine.

use thiserror::Error;

/// Convenience alias for simulation results.
pub type SimResult<T> = Result<T, SimError>;

/// Top-level simulation error type.
#[derive(Error, Debug)]
pub enum SimError {
    #[error("Input error in {object_type} '{object_name}': {message}")]
    InputError {
        object_type: String,
        object_name: String,
        message: String,
    },

    #[error("Convergence failure in {solver}: {message} (after {iterations} iterations)")]
    ConvergenceError {
        solver: String,
        message: String,
        iterations: u32,
    },

    #[error("Numerical error: {0}")]
    NumericalError(String),

    #[error("Weather data error: {0}")]
    WeatherError(String),

    #[error("Schedule error: {0}")]
    ScheduleError(String),

    #[error("File I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("FMI co-simulation error: {0}")]
    FmiError(String),

    #[error("{0}")]
    Other(String),
}

impl SimError {
    /// Create an input error with context.
    pub fn input(object_type: impl Into<String>, object_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InputError {
            object_type: object_type.into(),
            object_name: object_name.into(),
            message: message.into(),
        }
    }

    /// Create a convergence error.
    pub fn convergence(solver: impl Into<String>, message: impl Into<String>, iterations: u32) -> Self {
        Self::ConvergenceError {
            solver: solver.into(),
            message: message.into(),
            iterations,
        }
    }
}
