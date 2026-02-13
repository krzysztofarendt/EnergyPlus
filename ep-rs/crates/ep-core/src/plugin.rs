//! Plugin and extension model for user-defined components and callbacks.

use crate::error::SimResult;
use crate::state::SimulationState;

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

/// Trait for EMS-like scripting callbacks.
pub trait ScriptCallback: Send + Sync {
    fn on_calling_point(&mut self, point: CallingPoint, state: &mut SimulationState) -> SimResult<()>;
}
