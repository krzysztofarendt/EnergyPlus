//! Plant loop topology structures.
//!
//! A plant loop consists of supply and demand half-loops, each with
//! branches connected by splitters and mixers. The loop solver
//! iterates between half-loops to converge flows and temperatures.

/// Plant loop flow scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlowScheme {
    /// Single setpoint: one setpoint controls the entire loop.
    #[default]
    SingleSetpoint,
    /// Dual setpoint: heating and cooling setpoints.
    DualSetpoint,
}

/// Identifies which half of a plant loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoopSideId {
    Supply,
    Demand,
}

/// Flow lock state for a loop side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlowLockState {
    /// During pump query — flows are not yet committed.
    #[default]
    PumpQuery,
    /// Flows are unlocked — components can request changes.
    Unlocked,
    /// Flows are locked — no further changes allowed.
    Locked,
}

/// A single component on a branch.
#[derive(Debug, Clone)]
pub struct PlantBranchComponent {
    pub name: String,
    pub component_type: String,
    pub inlet_node: usize,
    pub outlet_node: usize,
    pub is_on: bool,
    pub my_load: f64,
}

/// A branch in the plant loop.
#[derive(Debug, Clone)]
pub struct PlantBranch {
    pub name: String,
    pub components: Vec<PlantBranchComponent>,
    pub inlet_node: usize,
    pub outlet_node: usize,
    pub is_bypass: bool,
    pub max_flow: f64,
    pub current_flow: f64,
    pub requested_flow: f64,
}

impl PlantBranch {
    pub fn new(name: impl Into<String>, inlet_node: usize, outlet_node: usize) -> Self {
        Self {
            name: name.into(),
            components: Vec::new(),
            inlet_node,
            outlet_node,
            is_bypass: false,
            max_flow: 0.0,
            current_flow: 0.0,
            requested_flow: 0.0,
        }
    }
}

/// Splitter: one inlet branch splits to multiple outlet branches.
#[derive(Debug, Clone)]
pub struct PlantSplitter {
    pub name: String,
    pub inlet_node: usize,
    pub outlet_branches: Vec<usize>,
}

/// Mixer: multiple inlet branches merge to one outlet branch.
#[derive(Debug, Clone)]
pub struct PlantMixer {
    pub name: String,
    pub outlet_node: usize,
    pub inlet_branches: Vec<usize>,
}

/// A half-loop (supply or demand side).
#[derive(Debug, Clone)]
pub struct HalfLoop {
    pub side: LoopSideId,
    pub branches: Vec<PlantBranch>,
    pub splitter: Option<PlantSplitter>,
    pub mixer: Option<PlantMixer>,
    pub flow_lock: FlowLockState,
    pub inlet_node: usize,
    pub outlet_node: usize,
    pub flow_request: f64,
}

impl HalfLoop {
    pub fn new(side: LoopSideId, inlet: usize, outlet: usize) -> Self {
        Self {
            side,
            branches: Vec::new(),
            splitter: None,
            mixer: None,
            flow_lock: FlowLockState::PumpQuery,
            inlet_node: inlet,
            outlet_node: outlet,
            flow_request: 0.0,
        }
    }
}

/// A complete plant loop with supply and demand sides.
#[derive(Debug, Clone)]
pub struct PlantLoop {
    pub name: String,
    pub fluid_type: PlantFluidType,
    pub supply_side: HalfLoop,
    pub demand_side: HalfLoop,
    /// Loop setpoint temperature (C).
    pub setpoint_temp: f64,
    /// Loop design flow rate (kg/s).
    pub design_flow_rate: f64,
    /// Loop maximum flow rate (kg/s).
    pub max_flow_rate: f64,
    /// Loop minimum flow rate (kg/s).
    pub min_flow_rate: f64,
    /// Loop volume (m3).
    pub loop_volume: f64,
    /// Current loop temperature (C).
    pub loop_temp: f64,
    /// Last timestep loop temperature (C).
    pub last_loop_temp: f64,
    /// Whether this loop needs to run.
    pub needs_flow: bool,
    /// Current iteration flow convergence.
    pub flow_converged: bool,
    /// Current iteration temperature convergence.
    pub temp_converged: bool,
}

/// Plant loop fluid types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlantFluidType {
    #[default]
    Water,
    Steam,
    Glycol,
}

impl PlantLoop {
    pub fn new(
        name: impl Into<String>,
        supply_inlet: usize,
        supply_outlet: usize,
        demand_inlet: usize,
        demand_outlet: usize,
    ) -> Self {
        Self {
            name: name.into(),
            fluid_type: PlantFluidType::Water,
            supply_side: HalfLoop::new(LoopSideId::Supply, supply_inlet, supply_outlet),
            demand_side: HalfLoop::new(LoopSideId::Demand, demand_inlet, demand_outlet),
            setpoint_temp: 80.0,
            design_flow_rate: 0.0,
            max_flow_rate: 0.0,
            min_flow_rate: 0.0,
            loop_volume: 0.0,
            loop_temp: 20.0,
            last_loop_temp: 20.0,
            needs_flow: false,
            flow_converged: false,
            temp_converged: false,
        }
    }

    /// Check if the loop has converged (both flow and temperature).
    pub fn converged(&self) -> bool {
        self.flow_converged && self.temp_converged
    }

    /// Save the current loop state for the next timestep.
    pub fn save_timestep(&mut self) {
        self.last_loop_temp = self.loop_temp;
    }

    /// Distribute total flow across parallel branches based on requests.
    pub fn distribute_branch_flows(branches: &mut [PlantBranch], total_flow: f64) {
        let total_requested: f64 = branches.iter().map(|b| b.requested_flow).sum();

        if total_requested > 1e-10 && total_flow > 1e-10 {
            let ratio = total_flow / total_requested;
            for branch in branches.iter_mut() {
                branch.current_flow = branch.requested_flow * ratio.min(1.0);
            }
        } else if total_flow > 1e-10 && !branches.is_empty() {
            // Equal distribution
            let per_branch = total_flow / branches.len() as f64;
            for branch in branches.iter_mut() {
                branch.current_flow = per_branch;
            }
        } else {
            for branch in branches.iter_mut() {
                branch.current_flow = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plant_loop_creation() {
        let loop_ = PlantLoop::new("HW Loop", 0, 1, 2, 3);
        assert_eq!(loop_.supply_side.side, LoopSideId::Supply);
        assert_eq!(loop_.demand_side.side, LoopSideId::Demand);
        assert!(!loop_.converged());
    }

    #[test]
    fn half_loop_creation() {
        let hl = HalfLoop::new(LoopSideId::Supply, 10, 20);
        assert_eq!(hl.flow_lock, FlowLockState::PumpQuery);
        assert_eq!(hl.inlet_node, 10);
    }

    #[test]
    fn branch_creation() {
        let branch = PlantBranch::new("Boiler Branch", 5, 6);
        assert!(!branch.is_bypass);
        assert!(branch.components.is_empty());
    }

    #[test]
    fn save_timestep() {
        let mut loop_ = PlantLoop::new("Test", 0, 1, 2, 3);
        loop_.loop_temp = 75.0;
        loop_.save_timestep();
        assert!((loop_.last_loop_temp - 75.0).abs() < 1e-10);
    }

    #[test]
    fn distribute_flows_proportional() {
        let mut branches = vec![
            PlantBranch::new("B1", 0, 1),
            PlantBranch::new("B2", 2, 3),
        ];
        branches[0].requested_flow = 0.3;
        branches[1].requested_flow = 0.7;

        PlantLoop::distribute_branch_flows(&mut branches, 1.0);
        assert!((branches[0].current_flow - 0.3).abs() < 0.01);
        assert!((branches[1].current_flow - 0.7).abs() < 0.01);
    }

    #[test]
    fn distribute_flows_limited() {
        let mut branches = vec![
            PlantBranch::new("B1", 0, 1),
            PlantBranch::new("B2", 2, 3),
        ];
        branches[0].requested_flow = 0.6;
        branches[1].requested_flow = 1.4;

        // Only 1.0 available, requests total 2.0
        PlantLoop::distribute_branch_flows(&mut branches, 1.0);
        assert!((branches[0].current_flow - 0.3).abs() < 0.01,
                "b1={}", branches[0].current_flow);
        assert!((branches[1].current_flow - 0.7).abs() < 0.01,
                "b2={}", branches[1].current_flow);
    }

    #[test]
    fn distribute_flows_zero() {
        let mut branches = vec![PlantBranch::new("B1", 0, 1)];
        branches[0].requested_flow = 0.5;
        PlantLoop::distribute_branch_flows(&mut branches, 0.0);
        assert!(branches[0].current_flow.abs() < 1e-10);
    }
}
