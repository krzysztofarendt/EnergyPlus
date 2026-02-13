//! Component traits for HVAC and plant equipment.

use crate::error::SimResult;
use crate::state::SimulationState;
use ep_units::*;

/// Every HVAC/plant component implements this trait.
pub trait HvacComponent: Send + Sync {
    /// Human-readable name for diagnostics.
    fn name(&self) -> &str;

    /// Component type identifier.
    fn component_type(&self) -> ComponentType;

    /// Initialize component state at start of environment or after sizing.
    fn initialize(&mut self, state: &SimulationState, first_hvac_iteration: bool) -> SimResult<()>;

    /// Run the component model for the current timestep.
    fn simulate(
        &mut self,
        state: &mut SimulationState,
        first_hvac_iteration: bool,
        load: Power,
        run: bool,
    ) -> SimResult<()>;

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
    fn size(&mut self, state: &SimulationState) -> SimResult<()>;
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

/// Component type identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentType {
    Boiler,
    Chiller,
    CoolingTower,
    Pump,
    Fan,
    CoilCooling,
    CoilHeating,
    HeatExchanger,
    WaterHeater,
    Generator,
    Other,
}
