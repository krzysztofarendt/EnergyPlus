//! HVAC node and loop infrastructure for EnergyPlus-rs.
//!
//! Provides the core node data structure that all HVAC and plant components
//! connect through, plus loop topology types (branches, splitters, mixers).

use serde::{Deserialize, Serialize};

/// Fluid type flowing through a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum FluidType {
    #[default]
    Air,
    Water,
    Steam,
    Electric,
}

/// How a node is connected in the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConnectionType {
    Inlet,
    Outlet,
    Internal,
    ZoneNode,
    Sensor,
    Actuator,
    OutsideAir,
    ReliefAir,
    ZoneInlet,
    ZoneReturn,
    ZoneExhaust,
    SetPoint,
}

/// HVAC node — the fundamental data carrier between components.
///
/// Every HVAC/plant component reads from inlet nodes and writes to outlet nodes.
/// Nodes carry temperature, humidity, mass flow, pressure, enthalpy, and setpoints.
#[derive(Debug, Clone)]
pub struct Node {
    /// Node name (for diagnostics).
    pub name: String,
    /// Fluid type.
    pub fluid_type: FluidType,

    // --- Thermodynamic state ---
    /// Dry-bulb temperature (C).
    pub temp: f64,
    /// Minimum temperature limit (C).
    pub temp_min: f64,
    /// Maximum temperature limit (C).
    pub temp_max: f64,
    /// Temperature setpoint (C).
    pub temp_setpoint: f64,
    /// High setpoint for dual-setpoint control (C).
    pub temp_setpoint_hi: f64,
    /// Low setpoint for dual-setpoint control (C).
    pub temp_setpoint_lo: f64,
    /// Temperature from last timestep (C).
    pub temp_last_timestep: f64,

    // --- Enthalpy ---
    /// Specific enthalpy (J/kg).
    pub enthalpy: f64,
    /// Enthalpy from last timestep (J/kg).
    pub enthalpy_last_timestep: f64,

    // --- Mass flow ---
    /// Current mass flow rate (kg/s).
    pub mass_flow_rate: f64,
    /// Requested mass flow rate (kg/s).
    pub mass_flow_rate_request: f64,
    /// Minimum mass flow rate (kg/s).
    pub mass_flow_rate_min: f64,
    /// Maximum mass flow rate (kg/s).
    pub mass_flow_rate_max: f64,
    /// Minimum available mass flow rate (kg/s).
    pub mass_flow_rate_min_avail: f64,
    /// Maximum available mass flow rate (kg/s).
    pub mass_flow_rate_max_avail: f64,
    /// Mass flow rate setpoint (kg/s).
    pub mass_flow_rate_setpoint: f64,

    // --- Humidity ---
    /// Humidity ratio (kg water / kg dry air).
    pub humidity_ratio: f64,
    /// Minimum humidity ratio (kg/kg).
    pub humidity_ratio_min: f64,
    /// Maximum humidity ratio (kg/kg).
    pub humidity_ratio_max: f64,
    /// Humidity ratio setpoint (kg/kg).
    pub humidity_ratio_setpoint: f64,

    // --- Pressure & Quality ---
    /// Pressure (Pa).
    pub press: f64,
    /// Vapor quality (0-1, for two-phase fluids like steam/refrigerant).
    pub quality: f64,

    // --- Geometry ---
    /// Node height (m), -1.0 if not set.
    pub height: f64,

    // --- Contaminants ---
    /// CO2 concentration (ppm).
    pub co2: f64,
    /// CO2 setpoint (ppm).
    pub co2_setpoint: f64,
    /// Generic contaminant concentration (ppm).
    pub generic_contam: f64,
    /// Generic contaminant setpoint (ppm).
    pub generic_contam_setpoint: f64,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            name: String::new(),
            fluid_type: FluidType::Air,
            temp: 0.0,
            temp_min: 0.0,
            temp_max: 0.0,
            temp_setpoint: f64::MAX,
            temp_setpoint_hi: f64::MAX,
            temp_setpoint_lo: f64::MIN,
            temp_last_timestep: 0.0,
            enthalpy: 0.0,
            enthalpy_last_timestep: 0.0,
            mass_flow_rate: 0.0,
            mass_flow_rate_request: 0.0,
            mass_flow_rate_min: 0.0,
            mass_flow_rate_max: 0.0,
            mass_flow_rate_min_avail: 0.0,
            mass_flow_rate_max_avail: 0.0,
            mass_flow_rate_setpoint: 0.0,
            humidity_ratio: 0.0,
            humidity_ratio_min: 0.0,
            humidity_ratio_max: 1.0,
            humidity_ratio_setpoint: f64::MAX,
            press: 101325.0,
            quality: 0.0,
            height: -1.0,
            co2: 0.0,
            co2_setpoint: 0.0,
            generic_contam: 0.0,
            generic_contam_setpoint: 0.0,
        }
    }
}

impl Node {
    /// Create a new air node with a name.
    pub fn air(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fluid_type: FluidType::Air,
            ..Default::default()
        }
    }

    /// Create a new water node with a name.
    pub fn water(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fluid_type: FluidType::Water,
            ..Default::default()
        }
    }

    /// Create a new steam node with a name.
    pub fn steam(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fluid_type: FluidType::Steam,
            ..Default::default()
        }
    }

    /// Update enthalpy from temperature and humidity ratio (air nodes).
    pub fn update_enthalpy_from_temp(&mut self) {
        if self.fluid_type == FluidType::Air {
            self.enthalpy = ep_psychrometrics::enthalpy(self.temp, self.humidity_ratio);
        }
    }

    /// Update temperature from enthalpy and humidity ratio (air nodes).
    pub fn update_temp_from_enthalpy(&mut self) {
        if self.fluid_type == FluidType::Air {
            self.temp = ep_psychrometrics::t_db_from_enthalpy_w(
                self.enthalpy,
                self.humidity_ratio,
            );
        }
    }

    /// Save current state to "last timestep" history.
    pub fn save_timestep(&mut self) {
        self.temp_last_timestep = self.temp;
        self.enthalpy_last_timestep = self.enthalpy;
    }

    /// Check if setpoint is active (not defaulted).
    pub fn has_temp_setpoint(&self) -> bool {
        self.temp_setpoint < 1.0e10
    }

    /// Check if dual setpoints are active.
    pub fn has_dual_setpoints(&self) -> bool {
        self.temp_setpoint_hi < 1.0e10 && self.temp_setpoint_lo > -1.0e10
    }
}

/// Node index type for referencing nodes in a collection.
pub type NodeIndex = usize;

/// Collection of all HVAC/plant nodes in the simulation.
#[derive(Debug, Default)]
pub struct NodeManager {
    nodes: Vec<Node>,
    name_map: std::collections::HashMap<String, NodeIndex>,
}

impl NodeManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node and return its index.
    pub fn add(&mut self, node: Node) -> NodeIndex {
        let idx = self.nodes.len();
        self.name_map.insert(node.name.clone(), idx);
        self.nodes.push(node);
        idx
    }

    /// Get a node by index.
    pub fn get(&self, idx: NodeIndex) -> Option<&Node> {
        self.nodes.get(idx)
    }

    /// Get a mutable node by index.
    pub fn get_mut(&mut self, idx: NodeIndex) -> Option<&mut Node> {
        self.nodes.get_mut(idx)
    }

    /// Find a node index by name.
    pub fn find(&self, name: &str) -> Option<NodeIndex> {
        self.name_map.get(name).copied()
    }

    /// Number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether there are no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Save all nodes' timestep history.
    pub fn save_all_timesteps(&mut self) {
        for node in &mut self.nodes {
            node.save_timestep();
        }
    }

    /// Copy outlet node state to inlet node (for component connections).
    pub fn copy_node(&mut self, from: NodeIndex, to: NodeIndex) {
        if from == to || from >= self.nodes.len() || to >= self.nodes.len() {
            return;
        }
        let temp = self.nodes[from].temp;
        let enthalpy = self.nodes[from].enthalpy;
        let humidity_ratio = self.nodes[from].humidity_ratio;
        let quality = self.nodes[from].quality;
        let press = self.nodes[from].press;
        let co2 = self.nodes[from].co2;
        let generic_contam = self.nodes[from].generic_contam;
        // Do NOT copy mass flow — that's set by the solver
        let dst = &mut self.nodes[to];
        dst.temp = temp;
        dst.enthalpy = enthalpy;
        dst.humidity_ratio = humidity_ratio;
        dst.quality = quality;
        dst.press = press;
        dst.co2 = co2;
        dst.generic_contam = generic_contam;
    }
}

/// Loop side identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LoopSide {
    Supply,
    Demand,
}

/// Flow lock state for plant loop iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlowLock {
    /// Pumps querying min/max available.
    PumpQuery,
    /// Components can request flow freely.
    #[default]
    Unlocked,
    /// Flow rates are fixed for this iteration.
    Locked,
}

/// Component flow priority in a plant loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlowPriority {
    /// Must have flow, turns loop on (e.g., modulating valve).
    NeedyAndTurnsLoopOn,
    /// Requests additional flow if loop is already running.
    NeedyIfLoopOn,
    /// Passive, accepts whatever flow is available.
    #[default]
    TakesWhatGets,
}

/// Component on a branch.
#[derive(Debug, Clone)]
pub struct BranchComponent {
    /// Component name.
    pub name: String,
    /// Component type identifier.
    pub component_type: String,
    /// Inlet node index.
    pub inlet_node: NodeIndex,
    /// Outlet node index.
    pub outlet_node: NodeIndex,
    /// Flow priority.
    pub flow_priority: FlowPriority,
    /// Whether component is active.
    pub is_on: bool,
    /// Whether component is available.
    pub is_available: bool,
    /// Current load assigned by dispatch (W).
    pub my_load: f64,
    /// Maximum capacity (W).
    pub max_load: f64,
    /// Minimum capacity (W).
    pub min_load: f64,
    /// Optimal operating capacity (W).
    pub opt_load: f64,
}

impl BranchComponent {
    pub fn new(
        name: impl Into<String>,
        component_type: impl Into<String>,
        inlet_node: NodeIndex,
        outlet_node: NodeIndex,
    ) -> Self {
        Self {
            name: name.into(),
            component_type: component_type.into(),
            inlet_node,
            outlet_node,
            flow_priority: FlowPriority::TakesWhatGets,
            is_on: false,
            is_available: true,
            my_load: 0.0,
            max_load: 0.0,
            min_load: 0.0,
            opt_load: 0.0,
        }
    }
}

/// A branch in a loop (one or more components in series).
#[derive(Debug, Clone)]
pub struct Branch {
    /// Branch name.
    pub name: String,
    /// Inlet node index.
    pub inlet_node: NodeIndex,
    /// Outlet node index.
    pub outlet_node: NodeIndex,
    /// Components on this branch.
    pub components: Vec<BranchComponent>,
    /// Whether this is a bypass branch.
    pub is_bypass: bool,
    /// Requested mass flow rate (kg/s).
    pub requested_mass_flow: f64,
}

impl Branch {
    pub fn new(name: impl Into<String>, inlet: NodeIndex, outlet: NodeIndex) -> Self {
        Self {
            name: name.into(),
            inlet_node: inlet,
            outlet_node: outlet,
            components: Vec::new(),
            is_bypass: false,
            requested_mass_flow: 0.0,
        }
    }

    pub fn add_component(&mut self, comp: BranchComponent) {
        self.components.push(comp);
    }
}

/// Splitter: one inlet to multiple outlet branches.
#[derive(Debug, Clone, Default)]
pub struct Splitter {
    pub name: String,
    /// Inlet node index.
    pub inlet_node: NodeIndex,
    /// Inlet branch index.
    pub inlet_branch: usize,
    /// Outlet node indices.
    pub outlet_nodes: Vec<NodeIndex>,
    /// Outlet branch indices.
    pub outlet_branches: Vec<usize>,
}

/// Mixer: multiple inlet branches to one outlet.
#[derive(Debug, Clone, Default)]
pub struct Mixer {
    pub name: String,
    /// Outlet node index.
    pub outlet_node: NodeIndex,
    /// Outlet branch index.
    pub outlet_branch: usize,
    /// Inlet node indices.
    pub inlet_nodes: Vec<NodeIndex>,
    /// Inlet branch indices.
    pub inlet_branches: Vec<usize>,
}

/// Calculate mixed outlet conditions for a mixer node.
///
/// Mass-weighted average of temperature, humidity ratio, and enthalpy
/// from all inlet streams.
pub fn mix_nodes(nodes: &NodeManager, inlet_indices: &[NodeIndex], outlet: NodeIndex) -> Option<MixedState> {
    let mut total_mass_flow = 0.0;
    let mut sum_mdt = 0.0;
    let mut sum_mdw = 0.0;
    let mut sum_mdh = 0.0;
    let mut sum_co2 = 0.0;

    for &idx in inlet_indices {
        let node = nodes.get(idx)?;
        let mdot = node.mass_flow_rate;
        total_mass_flow += mdot;
        sum_mdt += mdot * node.temp;
        sum_mdw += mdot * node.humidity_ratio;
        sum_mdh += mdot * node.enthalpy;
        sum_co2 += mdot * node.co2;
    }

    if total_mass_flow > 1e-10 {
        Some(MixedState {
            temp: sum_mdt / total_mass_flow,
            humidity_ratio: sum_mdw / total_mass_flow,
            enthalpy: sum_mdh / total_mass_flow,
            mass_flow_rate: total_mass_flow,
            co2: sum_co2 / total_mass_flow,
        })
    } else {
        // No flow — use first inlet conditions or outlet's existing state
        let outlet_node = nodes.get(outlet)?;
        Some(MixedState {
            temp: outlet_node.temp,
            humidity_ratio: outlet_node.humidity_ratio,
            enthalpy: outlet_node.enthalpy,
            mass_flow_rate: 0.0,
            co2: outlet_node.co2,
        })
    }
}

/// Result of mixing multiple streams.
#[derive(Debug, Clone, Copy)]
pub struct MixedState {
    pub temp: f64,
    pub humidity_ratio: f64,
    pub enthalpy: f64,
    pub mass_flow_rate: f64,
    pub co2: f64,
}

/// Distribute flow from a splitter to outlet branches.
///
/// Total outlet flow must equal inlet flow (mass conservation).
/// Each outlet gets its requested fraction of the total.
pub fn split_flow(
    nodes: &mut NodeManager,
    inlet: NodeIndex,
    outlet_indices: &[NodeIndex],
    requested_flows: &[f64],
) {
    let inlet_node = match nodes.get(inlet) {
        Some(n) => n.clone(),
        None => return,
    };

    let total_requested: f64 = requested_flows.iter().sum();

    for (i, &out_idx) in outlet_indices.iter().enumerate() {
        if let Some(out_node) = nodes.get_mut(out_idx) {
            // Copy thermodynamic state from inlet
            out_node.temp = inlet_node.temp;
            out_node.enthalpy = inlet_node.enthalpy;
            out_node.humidity_ratio = inlet_node.humidity_ratio;
            out_node.quality = inlet_node.quality;
            out_node.press = inlet_node.press;

            // Set flow rate
            if total_requested > 1e-10 && i < requested_flows.len() {
                out_node.mass_flow_rate = requested_flows[i];
            } else if !outlet_indices.is_empty() {
                out_node.mass_flow_rate = inlet_node.mass_flow_rate / outlet_indices.len() as f64;
            }
        }
    }
}

/// Outside air mixer: mixes outdoor and return air streams.
#[derive(Debug, Clone)]
pub struct OutsideAirMixer {
    pub name: String,
    /// Mixed air (outlet) node.
    pub mixed_air_node: NodeIndex,
    /// Outside air (inlet) node.
    pub outside_air_node: NodeIndex,
    /// Return air (inlet) node.
    pub return_air_node: NodeIndex,
    /// Relief air (outlet) node.
    pub relief_air_node: NodeIndex,
}

impl OutsideAirMixer {
    /// Calculate mixed air conditions.
    ///
    /// Mixed = OA_fraction * OA + (1 - OA_fraction) * Return
    pub fn mix(&self, nodes: &mut NodeManager, oa_mass_flow: f64, return_mass_flow: f64) {
        let oa = match nodes.get(self.outside_air_node) {
            Some(n) => n.clone(),
            None => return,
        };
        let ret = match nodes.get(self.return_air_node) {
            Some(n) => n.clone(),
            None => return,
        };

        let total_flow = oa_mass_flow + return_mass_flow;

        if let Some(mixed) = nodes.get_mut(self.mixed_air_node) {
            mixed.mass_flow_rate = total_flow;
            if total_flow > 1e-10 {
                let oa_frac = oa_mass_flow / total_flow;
                let ret_frac = 1.0 - oa_frac;
                mixed.temp = oa_frac * oa.temp + ret_frac * ret.temp;
                mixed.humidity_ratio = oa_frac * oa.humidity_ratio + ret_frac * ret.humidity_ratio;
                mixed.enthalpy = oa_frac * oa.enthalpy + ret_frac * ret.enthalpy;
                mixed.co2 = oa_frac * oa.co2 + ret_frac * ret.co2;
                mixed.press = oa.press; // Use OA pressure
            } else {
                mixed.temp = ret.temp;
                mixed.humidity_ratio = ret.humidity_ratio;
                mixed.enthalpy = ret.enthalpy;
            }
        }

        // Relief air gets return air conditions at OA flow rate
        if let Some(relief) = nodes.get_mut(self.relief_air_node) {
            relief.temp = ret.temp;
            relief.humidity_ratio = ret.humidity_ratio;
            relief.enthalpy = ret.enthalpy;
            relief.mass_flow_rate = oa_mass_flow; // Relief = OA flow for mass balance
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_creation() {
        let n = Node::air("test_air_node");
        assert_eq!(n.name, "test_air_node");
        assert_eq!(n.fluid_type, FluidType::Air);
        assert!((n.press - 101325.0).abs() < 1.0);
        assert!(!n.has_temp_setpoint());
    }

    #[test]
    fn water_node() {
        let n = Node::water("chw_supply");
        assert_eq!(n.fluid_type, FluidType::Water);
    }

    #[test]
    fn node_manager_add_find() {
        let mut nm = NodeManager::new();
        let idx = nm.add(Node::air("supply_air"));
        assert_eq!(idx, 0);
        assert_eq!(nm.len(), 1);
        assert_eq!(nm.find("supply_air"), Some(0));
        assert_eq!(nm.find("nonexistent"), None);
    }

    #[test]
    fn node_manager_get_mut() {
        let mut nm = NodeManager::new();
        let idx = nm.add(Node::air("node1"));
        nm.get_mut(idx).unwrap().temp = 25.0;
        assert!((nm.get(idx).unwrap().temp - 25.0).abs() < 1e-10);
    }

    #[test]
    fn node_copy() {
        let mut nm = NodeManager::new();
        let src = nm.add(Node::air("src"));
        let dst = nm.add(Node::air("dst"));
        nm.get_mut(src).unwrap().temp = 30.0;
        nm.get_mut(src).unwrap().humidity_ratio = 0.01;
        nm.get_mut(src).unwrap().mass_flow_rate = 5.0;
        nm.copy_node(src, dst);
        assert!((nm.get(dst).unwrap().temp - 30.0).abs() < 1e-10);
        assert!((nm.get(dst).unwrap().humidity_ratio - 0.01).abs() < 1e-10);
        // Mass flow should NOT be copied
        assert!((nm.get(dst).unwrap().mass_flow_rate).abs() < 1e-10);
    }

    #[test]
    fn node_setpoint_detection() {
        let mut n = Node::air("test");
        assert!(!n.has_temp_setpoint());
        assert!(!n.has_dual_setpoints());
        n.temp_setpoint = 22.0;
        assert!(n.has_temp_setpoint());
        n.temp_setpoint_hi = 24.0;
        n.temp_setpoint_lo = 20.0;
        assert!(n.has_dual_setpoints());
    }

    #[test]
    fn save_timestep() {
        let mut n = Node::air("test");
        n.temp = 25.0;
        n.enthalpy = 50000.0;
        n.save_timestep();
        assert!((n.temp_last_timestep - 25.0).abs() < 1e-10);
        assert!((n.enthalpy_last_timestep - 50000.0).abs() < 1e-10);
    }

    #[test]
    fn mixer_mass_weighted_average() {
        let mut nm = NodeManager::new();
        let inlet1 = nm.add(Node::air("in1"));
        let inlet2 = nm.add(Node::air("in2"));
        let outlet = nm.add(Node::air("out"));

        nm.get_mut(inlet1).unwrap().temp = 20.0;
        nm.get_mut(inlet1).unwrap().mass_flow_rate = 1.0;
        nm.get_mut(inlet1).unwrap().humidity_ratio = 0.008;
        nm.get_mut(inlet1).unwrap().enthalpy = 40000.0;

        nm.get_mut(inlet2).unwrap().temp = 30.0;
        nm.get_mut(inlet2).unwrap().mass_flow_rate = 1.0;
        nm.get_mut(inlet2).unwrap().humidity_ratio = 0.012;
        nm.get_mut(inlet2).unwrap().enthalpy = 60000.0;

        let mixed = mix_nodes(&nm, &[inlet1, inlet2], outlet).unwrap();
        assert!((mixed.temp - 25.0).abs() < 1e-10);
        assert!((mixed.humidity_ratio - 0.010).abs() < 1e-10);
        assert!((mixed.enthalpy - 50000.0).abs() < 1e-10);
        assert!((mixed.mass_flow_rate - 2.0).abs() < 1e-10);
    }

    #[test]
    fn mixer_unequal_flows() {
        let mut nm = NodeManager::new();
        let inlet1 = nm.add(Node::air("in1"));
        let inlet2 = nm.add(Node::air("in2"));
        let outlet = nm.add(Node::air("out"));

        nm.get_mut(inlet1).unwrap().temp = 10.0;
        nm.get_mut(inlet1).unwrap().mass_flow_rate = 3.0;
        nm.get_mut(inlet2).unwrap().temp = 30.0;
        nm.get_mut(inlet2).unwrap().mass_flow_rate = 1.0;

        let mixed = mix_nodes(&nm, &[inlet1, inlet2], outlet).unwrap();
        // 3*10 + 1*30 = 60, /4 = 15
        assert!((mixed.temp - 15.0).abs() < 1e-10);
        assert!((mixed.mass_flow_rate - 4.0).abs() < 1e-10);
    }

    #[test]
    fn splitter_distributes_state() {
        let mut nm = NodeManager::new();
        let inlet = nm.add(Node::air("inlet"));
        let out1 = nm.add(Node::air("out1"));
        let out2 = nm.add(Node::air("out2"));

        nm.get_mut(inlet).unwrap().temp = 15.0;
        nm.get_mut(inlet).unwrap().humidity_ratio = 0.009;
        nm.get_mut(inlet).unwrap().mass_flow_rate = 3.0;

        split_flow(&mut nm, inlet, &[out1, out2], &[2.0, 1.0]);

        assert!((nm.get(out1).unwrap().temp - 15.0).abs() < 1e-10);
        assert!((nm.get(out2).unwrap().temp - 15.0).abs() < 1e-10);
        assert!((nm.get(out1).unwrap().mass_flow_rate - 2.0).abs() < 1e-10);
        assert!((nm.get(out2).unwrap().mass_flow_rate - 1.0).abs() < 1e-10);
    }

    #[test]
    fn oa_mixer() {
        let mut nm = NodeManager::new();
        let mixed = nm.add(Node::air("mixed"));
        let oa = nm.add(Node::air("outside"));
        let ret = nm.add(Node::air("return"));
        let relief = nm.add(Node::air("relief"));

        nm.get_mut(oa).unwrap().temp = 5.0;
        nm.get_mut(oa).unwrap().humidity_ratio = 0.003;
        nm.get_mut(oa).unwrap().enthalpy = 12000.0;
        nm.get_mut(ret).unwrap().temp = 24.0;
        nm.get_mut(ret).unwrap().humidity_ratio = 0.009;
        nm.get_mut(ret).unwrap().enthalpy = 47000.0;

        let oam = OutsideAirMixer {
            name: "OA Mixer".into(),
            mixed_air_node: mixed,
            outside_air_node: oa,
            return_air_node: ret,
            relief_air_node: relief,
        };

        // 30% OA, 70% return
        oam.mix(&mut nm, 0.3, 0.7);
        let m = nm.get(mixed).unwrap();
        // T = 0.3*5 + 0.7*24 = 1.5 + 16.8 = 18.3
        assert!((m.temp - 18.3).abs() < 0.01, "T={}", m.temp);
        assert!((m.mass_flow_rate - 1.0).abs() < 1e-10);
        // Relief should get return conditions at OA flow
        let r = nm.get(relief).unwrap();
        assert!((r.temp - 24.0).abs() < 1e-10);
        assert!((r.mass_flow_rate - 0.3).abs() < 1e-10);
    }

    #[test]
    fn branch_and_component() {
        let mut nm = NodeManager::new();
        let n1 = nm.add(Node::water("branch_in"));
        let n2 = nm.add(Node::water("comp_mid"));
        let n3 = nm.add(Node::water("branch_out"));

        let mut branch = Branch::new("main_branch", n1, n3);
        let comp = BranchComponent::new("boiler1", "Boiler:HotWater", n1, n2);
        branch.add_component(comp);
        assert_eq!(branch.components.len(), 1);
        assert!(!branch.is_bypass);
    }

    #[test]
    fn node_enthalpy_update() {
        let mut n = Node::air("test");
        n.temp = 25.0;
        n.humidity_ratio = 0.010;
        n.update_enthalpy_from_temp();
        // h ≈ 1006*25 + 0.010*(2501000 + 1860*25) = 25150 + 25476 ≈ 50626 J/kg
        assert!(n.enthalpy > 40000.0 && n.enthalpy < 60000.0, "h={}", n.enthalpy);

        // Round-trip
        let _saved_h = n.enthalpy;
        n.temp = 0.0;
        n.update_temp_from_enthalpy();
        assert!((n.temp - 25.0).abs() < 0.1, "T={}", n.temp);
    }

    #[test]
    fn save_all_timesteps() {
        let mut nm = NodeManager::new();
        let n1 = nm.add(Node::air("n1"));
        let n2 = nm.add(Node::air("n2"));
        nm.get_mut(n1).unwrap().temp = 20.0;
        nm.get_mut(n2).unwrap().temp = 30.0;
        nm.save_all_timesteps();
        assert!((nm.get(n1).unwrap().temp_last_timestep - 20.0).abs() < 1e-10);
        assert!((nm.get(n2).unwrap().temp_last_timestep - 30.0).abs() < 1e-10);
    }
}
