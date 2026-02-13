//! Newton-Raphson pressure-flow solver for the airflow network.
//!
//! Solves the system of nonlinear mass conservation equations:
//! For each internal node i: sum(F_in) - sum(F_out) = 0
//!
//! Uses Newton-Raphson with direct LU factorization of the Jacobian.
//! The Jacobian is sparse and assembled from individual link contributions.

use crate::components::AirflowComponent;
use crate::network::AirflowNetwork;

/// Solver configuration.
#[derive(Debug, Clone)]
pub struct SolverConfig {
    /// Maximum iterations for Newton-Raphson.
    pub max_iterations: usize,
    /// Absolute convergence tolerance on node flow balance (kg/s).
    pub absolute_tolerance: f64,
    /// Relative convergence tolerance (flow imbalance / total flow).
    pub relative_tolerance: f64,
    /// Maximum pressure correction per iteration (Pa).
    pub max_pressure_correction: f64,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            max_iterations: 500,
            absolute_tolerance: 1e-6,
            relative_tolerance: 1e-4,
            max_pressure_correction: 500.0,
        }
    }
}

/// Result of the solver.
#[derive(Debug, Clone)]
pub struct SolverResult {
    /// Whether the solver converged.
    pub converged: bool,
    /// Number of iterations used.
    pub iterations: usize,
    /// Maximum residual (flow imbalance) at convergence.
    pub max_residual: f64,
}

/// Solve the airflow network for node pressures and link flows.
///
/// Returns the solver result. Node pressures and link flows
/// are updated in-place on the network.
pub fn solve_network(network: &mut AirflowNetwork, config: &SolverConfig) -> SolverResult {
    let internal_indices = network.internal_node_indices();
    let n = internal_indices.len();

    if n == 0 {
        return SolverResult {
            converged: true,
            iterations: 0,
            max_residual: 0.0,
        };
    }

    // Map from node index to equation index (only internal nodes)
    let mut node_to_eq = vec![usize::MAX; network.nodes.len()];
    for (eq, &ni) in internal_indices.iter().enumerate() {
        node_to_eq[ni] = eq;
    }

    // Initial linear solve (laminar approximation)
    solve_iteration(network, &internal_indices, &node_to_eq, n, true, config);

    // Newton-Raphson iterations
    for iter in 0..config.max_iterations {
        let (residual, max_abs_flow) = solve_iteration(
            network, &internal_indices, &node_to_eq, n, false, config,
        );

        // Check convergence
        let max_rel = if max_abs_flow > 1e-10 {
            residual / max_abs_flow
        } else {
            residual
        };

        if residual < config.absolute_tolerance || max_rel < config.relative_tolerance {
            // Update link results
            update_link_results(network);
            return SolverResult {
                converged: true,
                iterations: iter + 1,
                max_residual: residual,
            };
        }
    }

    update_link_results(network);
    SolverResult {
        converged: false,
        iterations: config.max_iterations,
        max_residual: f64::MAX,
    }
}

/// Perform one Newton-Raphson iteration.
/// Returns (max_residual, max_absolute_flow).
fn solve_iteration(
    network: &mut AirflowNetwork,
    internal_indices: &[usize],
    node_to_eq: &[usize],
    n: usize,
    laminar: bool,
    config: &SolverConfig,
) -> (f64, f64) {
    // Assemble Jacobian and residual
    let mut jacobian = vec![vec![0.0; n]; n];
    let mut residual = vec![0.0; n];
    let mut sum_abs_flow = vec![0.0; n];

    for link in network.links.iter() {
        let n1 = link.node_1;
        let n2 = link.node_2;

        // Pressure drop: P1 - P2 + pressure sources
        let dp = network.nodes[n1].pressure - network.nodes[n2].pressure
            + link.total_pressure_source();

        let s1 = &network.nodes[n1].state;
        let s2 = &network.nodes[n2].state;
        let comp = &network.components[link.component_index];
        let result = comp.calculate(dp, s1, s2, laminar);

        let flow = result.flow;
        let df = result.df_dp;

        let eq1 = node_to_eq[n1];
        let eq2 = node_to_eq[n2];

        // Node 1: flow leaves (subtract)
        if eq1 != usize::MAX {
            residual[eq1] -= flow;
            sum_abs_flow[eq1] += flow.abs();
            jacobian[eq1][eq1] -= df;
            if eq2 != usize::MAX {
                jacobian[eq1][eq2] += df;
            }
        }

        // Node 2: flow enters (add)
        if eq2 != usize::MAX {
            residual[eq2] += flow;
            sum_abs_flow[eq2] += flow.abs();
            jacobian[eq2][eq2] -= df;
            if eq1 != usize::MAX {
                jacobian[eq2][eq1] += df;
            }
        }
    }

    // Solve J * dP = -residual for pressure corrections
    let corrections = solve_linear_system(&jacobian, &residual, n);

    // Apply corrections with damping
    for (eq, &ni) in internal_indices.iter().enumerate() {
        let mut dp = corrections[eq];
        dp = dp.clamp(-config.max_pressure_correction, config.max_pressure_correction);
        network.nodes[ni].pressure -= dp;
    }

    // Return convergence metrics
    let max_residual = residual.iter().map(|r| r.abs()).fold(0.0_f64, |a, b| a.max(b));
    let max_abs = sum_abs_flow.iter().fold(0.0_f64, |a, &b| a.max(b));
    (max_residual, max_abs)
}

/// Solve dense linear system using Gaussian elimination with partial pivoting.
fn solve_linear_system(a: &[Vec<f64>], b: &[f64], n: usize) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }

    // Copy to working arrays
    let mut aug: Vec<Vec<f64>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = a[i].clone();
        row.push(b[i]);
        aug.push(row);
    }

    // Forward elimination with partial pivoting
    for col in 0..n {
        // Find pivot
        let mut max_val = aug[col][col].abs();
        let mut max_row = col;
        for row in (col + 1)..n {
            if aug[row][col].abs() > max_val {
                max_val = aug[row][col].abs();
                max_row = row;
            }
        }

        if max_val < 1e-30 {
            continue; // Singular or near-singular
        }

        if max_row != col {
            aug.swap(col, max_row);
        }

        let pivot = aug[col][col];
        for row in (col + 1)..n {
            let factor = aug[row][col] / pivot;
            for j in col..=n {
                aug[row][j] -= factor * aug[col][j];
            }
        }
    }

    // Back substitution
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut sum = aug[i][n];
        for j in (i + 1)..n {
            sum -= aug[i][j] * x[j];
        }
        if aug[i][i].abs() > 1e-30 {
            x[i] = sum / aug[i][i];
        }
    }

    x
}

/// Update link flow results after solver converges.
fn update_link_results(network: &mut AirflowNetwork) {
    for link_idx in 0..network.links.len() {
        let n1 = network.links[link_idx].node_1;
        let n2 = network.links[link_idx].node_2;

        let dp = network.nodes[n1].pressure - network.nodes[n2].pressure
            + network.links[link_idx].total_pressure_source();

        let s1 = network.nodes[n1].state.clone();
        let s2 = network.nodes[n2].state.clone();
        let comp_idx = network.links[link_idx].component_index;
        let result = network.components[comp_idx].calculate(dp, &s1, &s2, false);

        network.links[link_idx].flow = result.flow;
        network.links[link_idx].flow_reverse = result.flow_reverse;
        network.links[link_idx].pressure_drop = dp;

        let rho = if result.flow >= 0.0 { s1.density } else { s2.density };
        if rho > 0.0 {
            network.links[link_idx].volume_flow = result.flow / rho;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Component, SurfaceCrack, SimpleOpening, AirState};
    use crate::network::{NetworkNode, NetworkLink};

    fn two_zone_wind_driven() -> AirflowNetwork {
        let mut net = AirflowNetwork::new();

        // External node (windward, positive pressure)
        let mut ext1 = NetworkNode::external("Windward", 1.5);
        ext1.pressure = 15.0; // Wind pressure from Cp=0.6, v=5m/s
        ext1.state = AirState::new(10.0, 0.005, 101325.0);

        // Zone node
        let z1 = NetworkNode::internal("Zone1", 1.5, 0);

        // External node (leeward, negative pressure)
        let mut ext2 = NetworkNode::external("Leeward", 1.5);
        ext2.pressure = -7.5; // Wind pressure from Cp=-0.3
        ext2.state = AirState::new(10.0, 0.005, 101325.0);

        let _i0 = net.add_node(ext1);
        let _i1 = net.add_node(z1);
        let _i2 = net.add_node(ext2);

        let c1 = net.add_component(Component::Crack(SurfaceCrack::new("WindwardCrack", 0.01, 0.65)));
        let c2 = net.add_component(Component::Crack(SurfaceCrack::new("LeewardCrack", 0.01, 0.65)));

        net.add_link(NetworkLink::new("Wind-Zone", 0, 1, c1));
        net.add_link(NetworkLink::new("Zone-Lee", 1, 2, c2));

        net
    }

    #[test]
    fn solve_single_zone() {
        let mut net = two_zone_wind_driven();
        let config = SolverConfig::default();
        let result = solve_network(&mut net, &config);

        assert!(result.converged, "Did not converge in {} iterations", result.iterations);

        // Wind drives flow from windward to leeward
        assert!(net.links[0].flow > 0.0, "Windward flow should be positive");
        assert!(net.links[1].flow > 0.0, "Leeward flow should be positive");

        // Mass conservation: flow in ≈ flow out
        let flow_in = net.links[0].flow;
        let flow_out = net.links[1].flow;
        assert!((flow_in - flow_out).abs() < 0.001,
                "Mass balance: in={flow_in}, out={flow_out}");
    }

    #[test]
    fn solve_two_zone_series() {
        let mut net = AirflowNetwork::new();

        let mut ext1 = NetworkNode::external("Ext1", 1.5);
        ext1.pressure = 10.0;
        let z1 = NetworkNode::internal("Z1", 1.5, 0);
        let z2 = NetworkNode::internal("Z2", 1.5, 1);
        let mut ext2 = NetworkNode::external("Ext2", 1.5);
        ext2.pressure = -5.0;

        net.add_node(ext1);
        net.add_node(z1);
        net.add_node(z2);
        net.add_node(ext2);

        let c1 = net.add_component(Component::Crack(SurfaceCrack::new("C1", 0.01, 0.65)));
        let c2 = net.add_component(Component::Crack(SurfaceCrack::new("C2", 0.005, 0.65)));
        let c3 = net.add_component(Component::Crack(SurfaceCrack::new("C3", 0.01, 0.65)));

        net.add_link(NetworkLink::new("E1-Z1", 0, 1, c1));
        net.add_link(NetworkLink::new("Z1-Z2", 1, 2, c2));
        net.add_link(NetworkLink::new("Z2-E2", 2, 3, c3));

        let result = solve_network(&mut net, &SolverConfig::default());
        assert!(result.converged);

        // All flows should be positive (from high to low pressure)
        for link in &net.links {
            assert!(link.flow > 0.0, "Link {} flow={}", link.name, link.flow);
        }

        // Mass conservation at each internal node
        let flow_01 = net.links[0].flow;
        let flow_12 = net.links[1].flow;
        let flow_23 = net.links[2].flow;
        assert!((flow_01 - flow_12).abs() < 0.001, "Z1: in={flow_01}, out={flow_12}");
        assert!((flow_12 - flow_23).abs() < 0.001, "Z2: in={flow_12}, out={flow_23}");

        // Zone 1 pressure should be between ext pressures, higher than zone 2
        assert!(net.nodes[1].pressure > net.nodes[2].pressure,
                "P_z1={} should be > P_z2={}", net.nodes[1].pressure, net.nodes[2].pressure);
    }

    #[test]
    fn solve_with_opening() {
        let mut net = AirflowNetwork::new();

        let mut ext = NetworkNode::external("Ext", 1.5);
        ext.pressure = 20.0;
        let z = NetworkNode::internal("Zone", 1.5, 0);
        let mut ext2 = NetworkNode::external("Ext2", 1.5);
        ext2.pressure = 0.0;

        net.add_node(ext);
        net.add_node(z);
        net.add_node(ext2);

        let c1 = net.add_component(Component::Opening(
            SimpleOpening::new("Window", 0.5),
        ));
        let c2 = net.add_component(Component::Crack(SurfaceCrack::new("Leak", 0.01, 0.65)));

        net.add_link(NetworkLink::new("Win", 0, 1, c1));
        net.add_link(NetworkLink::new("Leak", 1, 2, c2));

        let result = solve_network(&mut net, &SolverConfig::default());
        assert!(result.converged);

        // Series circuit: flows must be equal (mass conservation)
        assert!((net.links[0].flow - net.links[1].flow).abs() < 0.001,
                "Window flow={}, crack flow={}", net.links[0].flow, net.links[1].flow);
        // Most pressure drop across the crack (higher resistance), less across window
        // Zone pressure should be closer to ext (high P) side since window is low-resistance
        assert!(net.nodes[1].pressure > 5.0,
                "Zone pressure={} should be high (window is low resistance)", net.nodes[1].pressure);
    }

    #[test]
    fn linear_solver_basic() {
        // 2x2 system: 2x + y = 5, x + 3y = 7
        let a = vec![vec![2.0, 1.0], vec![1.0, 3.0]];
        let b = vec![5.0, 7.0];
        let x = solve_linear_system(&a, &b, 2);
        assert!((x[0] - 1.6).abs() < 1e-10);
        assert!((x[1] - 1.8).abs() < 1e-10);
    }

    #[test]
    fn empty_network() {
        let mut net = AirflowNetwork::new();
        let result = solve_network(&mut net, &SolverConfig::default());
        assert!(result.converged);
        assert_eq!(result.iterations, 0);
    }
}
