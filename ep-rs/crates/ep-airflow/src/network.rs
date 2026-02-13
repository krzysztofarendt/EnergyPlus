//! Airflow network graph: nodes and links.
//!
//! The network represents the building as a graph where nodes are
//! zones, ambient, or duct junctions, and links are airflow paths
//! (cracks, openings, ducts) connecting them.

use crate::components::{AirState, Component};

/// Node type in the airflow network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    /// Internal node with unknown pressure (solved by the network).
    Internal,
    /// External node with known pressure (boundary condition).
    External,
}

/// A node in the airflow network.
#[derive(Debug, Clone)]
pub struct NetworkNode {
    pub name: String,
    pub node_type: NodeType,
    /// Height above reference (m) — for buoyancy pressure.
    pub height: f64,
    /// Associated zone index (for internal/zone nodes).
    pub zone_index: Option<usize>,
    /// Air state at this node.
    pub state: AirState,
    /// Pressure at this node (Pa, gauge above reference).
    pub pressure: f64,
}

impl NetworkNode {
    /// Create an internal (zone) node.
    pub fn internal(name: impl Into<String>, height: f64, zone_index: usize) -> Self {
        Self {
            name: name.into(),
            node_type: NodeType::Internal,
            height,
            zone_index: Some(zone_index),
            state: AirState::standard(),
            pressure: 0.0,
        }
    }

    /// Create an external (outdoor ambient) node.
    pub fn external(name: impl Into<String>, height: f64) -> Self {
        Self {
            name: name.into(),
            node_type: NodeType::External,
            height,
            zone_index: None,
            state: AirState::standard(),
            pressure: 0.0,
        }
    }

    /// Update node air state from zone conditions.
    pub fn update_state(&mut self, temperature: f64, humidity_ratio: f64, barometric_pressure: f64) {
        self.state = AirState::new(temperature, humidity_ratio, barometric_pressure);
    }
}

/// A link connecting two nodes through a component.
#[derive(Debug, Clone)]
pub struct NetworkLink {
    pub name: String,
    /// Index of upstream node.
    pub node_1: usize,
    /// Index of downstream node.
    pub node_2: usize,
    /// Component index in the network's component list.
    pub component_index: usize,
    /// Wind pressure coefficient for this link (Pa).
    pub wind_pressure: f64,
    /// Stack pressure from height and density differences (Pa).
    pub stack_pressure: f64,
    /// Additional fixed pressure source (Pa) — e.g., from fan.
    pub fixed_pressure: f64,
    // === Simulation results ===
    /// Mass flow rate from last solve (kg/s). Positive = node_1 to node_2.
    pub flow: f64,
    /// Reverse flow for bidirectional components (kg/s).
    pub flow_reverse: f64,
    /// Volume flow rate (m3/s).
    pub volume_flow: f64,
    /// Pressure drop across link (Pa).
    pub pressure_drop: f64,
}

impl NetworkLink {
    pub fn new(
        name: impl Into<String>,
        node_1: usize,
        node_2: usize,
        component_index: usize,
    ) -> Self {
        Self {
            name: name.into(),
            node_1,
            node_2,
            component_index,
            wind_pressure: 0.0,
            stack_pressure: 0.0,
            fixed_pressure: 0.0,
            flow: 0.0,
            flow_reverse: 0.0,
            volume_flow: 0.0,
            pressure_drop: 0.0,
        }
    }

    /// Total pressure source across this link (Pa).
    /// Added to the node pressure difference.
    pub fn total_pressure_source(&self) -> f64 {
        self.wind_pressure + self.stack_pressure + self.fixed_pressure
    }
}

/// The complete airflow network.
#[derive(Debug, Clone)]
pub struct AirflowNetwork {
    pub nodes: Vec<NetworkNode>,
    pub links: Vec<NetworkLink>,
    pub components: Vec<Component>,
}

impl AirflowNetwork {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            links: Vec::new(),
            components: Vec::new(),
        }
    }

    /// Add a node and return its index.
    pub fn add_node(&mut self, node: NetworkNode) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(node);
        idx
    }

    /// Add a component and return its index.
    pub fn add_component(&mut self, component: Component) -> usize {
        let idx = self.components.len();
        self.components.push(component);
        idx
    }

    /// Add a link and return its index.
    pub fn add_link(&mut self, link: NetworkLink) -> usize {
        let idx = self.links.len();
        self.links.push(link);
        idx
    }

    /// Number of internal (unknown pressure) nodes.
    pub fn num_internal_nodes(&self) -> usize {
        self.nodes.iter().filter(|n| n.node_type == NodeType::Internal).count()
    }

    /// Indices of internal nodes.
    pub fn internal_node_indices(&self) -> Vec<usize> {
        self.nodes.iter().enumerate()
            .filter(|(_, n)| n.node_type == NodeType::Internal)
            .map(|(i, _)| i)
            .collect()
    }

    /// Calculate stack pressure for a link based on height and density differences.
    pub fn update_stack_pressures(&mut self, reference_pressure: f64) {
        let g = 9.81;
        for link in &mut self.links {
            let n1 = &self.nodes[link.node_1];
            let n2 = &self.nodes[link.node_2];
            let rho_avg = (n1.state.density + n2.state.density) / 2.0;
            let dh = n1.height - n2.height;
            link.stack_pressure = rho_avg * g * dh;
            let _ = reference_pressure; // used for more advanced models
        }
    }

    /// Calculate wind pressure for external links.
    ///
    /// # Arguments
    /// * `wind_speed` - Local wind speed (m/s)
    /// * `wind_direction` - Wind direction (degrees from north, 0=N, 90=E)
    /// * `cp_values` - Wind pressure coefficient for each link (dimensionless)
    pub fn update_wind_pressures(&mut self, wind_speed: f64, _wind_direction: f64, cp_values: &[f64]) {
        let rho = 1.2; // Reference air density
        let dynamic_pressure = 0.5 * rho * wind_speed * wind_speed;

        for (i, link) in self.links.iter_mut().enumerate() {
            let cp = cp_values.get(i).copied().unwrap_or(0.0);
            link.wind_pressure = cp * dynamic_pressure;
        }
    }
}

impl Default for AirflowNetwork {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::SurfaceCrack;

    fn simple_two_zone_network() -> AirflowNetwork {
        let mut net = AirflowNetwork::new();

        // Outdoor node
        let ext = net.add_node(NetworkNode::external("Outdoor", 1.5));
        // Zone 1
        let z1 = net.add_node(NetworkNode::internal("Zone1", 1.5, 0));
        // Zone 2
        let z2 = net.add_node(NetworkNode::internal("Zone2", 1.5, 1));

        // Crack from outdoor to zone 1
        let c1 = net.add_component(Component::Crack(
            SurfaceCrack::new("ExtCrack", 0.01, 0.65),
        ));
        // Crack between zone 1 and zone 2
        let c2 = net.add_component(Component::Crack(
            SurfaceCrack::new("IntCrack", 0.005, 0.65),
        ));
        // Crack from zone 2 to outdoor
        let c3 = net.add_component(Component::Crack(
            SurfaceCrack::new("ExtCrack2", 0.01, 0.65),
        ));

        net.add_link(NetworkLink::new("Ext-Z1", ext, z1, c1));
        net.add_link(NetworkLink::new("Z1-Z2", z1, z2, c2));
        net.add_link(NetworkLink::new("Z2-Ext", z2, ext, c3));

        net
    }

    #[test]
    fn network_structure() {
        let net = simple_two_zone_network();
        assert_eq!(net.nodes.len(), 3);
        assert_eq!(net.links.len(), 3);
        assert_eq!(net.components.len(), 3);
        assert_eq!(net.num_internal_nodes(), 2);
    }

    #[test]
    fn internal_node_indices() {
        let net = simple_two_zone_network();
        let internal = net.internal_node_indices();
        assert_eq!(internal.len(), 2);
        assert_eq!(internal[0], 1); // Zone1
        assert_eq!(internal[1], 2); // Zone2
    }

    #[test]
    fn stack_pressure_calculation() {
        let mut net = AirflowNetwork::new();
        let mut n1 = NetworkNode::external("Bottom", 0.0);
        n1.state = AirState::new(20.0, 0.008, 101325.0);
        let mut n2 = NetworkNode::internal("Top", 3.0, 0);
        n2.state = AirState::new(25.0, 0.008, 101325.0);

        let _i1 = net.add_node(n1);
        let _i2 = net.add_node(n2);
        let c = net.add_component(Component::Crack(SurfaceCrack::new("C", 0.01, 0.65)));
        net.add_link(NetworkLink::new("L", 0, 1, c));

        net.update_stack_pressures(101325.0);
        // Stack pressure = rho_avg * g * dh = ~1.18 * 9.81 * -3 = ~-34.7 Pa
        let sp = net.links[0].stack_pressure;
        assert!(sp < -30.0 && sp > -40.0, "sp={sp}");
    }

    #[test]
    fn wind_pressure_update() {
        let mut net = simple_two_zone_network();
        net.update_wind_pressures(5.0, 180.0, &[0.6, 0.0, -0.3]);

        // Link 0: Cp=0.6, dynamic_pressure = 0.5*1.2*25 = 15, wp = 9.0
        assert!((net.links[0].wind_pressure - 9.0).abs() < 0.1);
        // Link 1: interior, Cp=0
        assert!((net.links[1].wind_pressure).abs() < 1e-10);
        // Link 2: Cp=-0.3, wp = -4.5
        assert!((net.links[2].wind_pressure - (-4.5)).abs() < 0.1);
    }
}
