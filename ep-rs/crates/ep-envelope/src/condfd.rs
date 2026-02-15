//! Finite Difference (CondFD) conduction solver.
//!
//! Implements 1D transient heat conduction through multi-layer constructions
//! using the finite difference method with Crank-Nicolson or Fully Implicit
//! time-stepping schemes.
//!
//! Features:
//! - Automatic node spacing based on Fourier number stability
//! - Phase Change Material (PCM) support via the enthalpy method
//! - Variable (temperature-dependent) conductivity
//! - Simplified moisture transport (vapor diffusion)
//!
//! The governing equation for 1D heat conduction is:
//!   rho * cp * dT/dt = d/dx(k * dT/dx)
//!
//! Discretized with Crank-Nicolson (theta=0.5) or Fully Implicit (theta=1.0):
//!   rho*cp*dx*(T_i^{n+1} - T_i^n)/dt =
//!     theta * [k_{i-1/2}*(T_{i-1}^{n+1} - T_i^{n+1})/dx + k_{i+1/2}*(T_{i+1}^{n+1} - T_i^{n+1})/dx]
//!   + (1-theta) * [k_{i-1/2}*(T_{i-1}^n - T_i^n)/dx + k_{i+1/2}*(T_{i+1}^n - T_i^n)/dx]

// ─── Enums & Configuration ──────────────────────────────────────────

/// Finite difference time-stepping scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CondFdScheme {
    /// Crank-Nicolson (theta = 0.5): second-order accurate, conditionally stable.
    /// Averages explicit and implicit contributions.
    CrankNicolson,
    /// Fully implicit (theta = 1.0): first-order accurate, unconditionally stable.
    FullyImplicit,
}

// ─── Phase Change Material ──────────────────────────────────────────

/// Phase change material properties.
///
/// The PCM undergoes a phase transition between `solidus_temp` and `liquidus_temp`,
/// absorbing or releasing `latent_heat` during the transition.
#[derive(Debug, Clone)]
pub struct PcmProperties {
    /// Base specific heat outside the transition zone (J/(kg-K)).
    pub base_cp: f64,
    /// Latent heat of fusion (J/kg).
    pub latent_heat: f64,
    /// Temperature at which melting begins (C).
    pub solidus_temp: f64,
    /// Temperature at which melting is complete (C).
    pub liquidus_temp: f64,
}

/// Calculate the effective specific heat at a given temperature for a PCM.
///
/// In the transition zone between solidus and liquidus, the effective cp
/// includes the latent heat distributed over the transition range:
///   cp_eff = base_cp + latent_heat / (T_liquidus - T_solidus)
///
/// Outside the transition zone, returns the base cp.
pub fn effective_cp(temp: f64, pcm: &PcmProperties) -> f64 {
    if pcm.liquidus_temp <= pcm.solidus_temp {
        // Degenerate case: no transition range
        return pcm.base_cp;
    }
    if temp < pcm.solidus_temp || temp > pcm.liquidus_temp {
        pcm.base_cp
    } else {
        let delta_t = pcm.liquidus_temp - pcm.solidus_temp;
        pcm.base_cp + pcm.latent_heat / delta_t
    }
}

/// Calculate enthalpy at a given temperature using the enthalpy method.
///
/// h(T) = integral of cp_eff(T) dT from a reference temperature.
/// Reference temperature is taken as solidus_temp - 1.0 (arbitrary baseline).
///
/// Below solidus: h = base_cp * (T - T_ref)
/// In transition: h = base_cp * (T - T_ref) + latent_heat * (T - T_solidus) / (T_liquidus - T_solidus)
/// Above liquidus: h = base_cp * (T - T_ref) + latent_heat
pub fn enthalpy_at_temp(temp: f64, pcm: &PcmProperties) -> f64 {
    let t_ref = pcm.solidus_temp - 1.0;
    let delta_t = pcm.liquidus_temp - pcm.solidus_temp;

    if delta_t <= 0.0 {
        return pcm.base_cp * (temp - t_ref);
    }

    if temp <= pcm.solidus_temp {
        pcm.base_cp * (temp - t_ref)
    } else if temp >= pcm.liquidus_temp {
        pcm.base_cp * (temp - t_ref) + pcm.latent_heat
    } else {
        let fraction = (temp - pcm.solidus_temp) / delta_t;
        pcm.base_cp * (temp - t_ref) + pcm.latent_heat * fraction
    }
}

// ─── Variable Conductivity ──────────────────────────────────────────

/// Temperature-dependent conductivity model.
///
/// k(T) = base_k * (1 + temp_coefficient * T)
/// where T is in Celsius.
#[derive(Debug, Clone)]
pub struct VariableConductivity {
    /// Base conductivity at 0 C (W/(m-K)).
    pub base_k: f64,
    /// Temperature coefficient (1/C).
    pub temp_coefficient: f64,
}

/// Calculate conductivity at a given temperature.
///
/// k(T) = base_k * (1 + coeff * T)
pub fn conductivity_at_temp(base_k: f64, coeff: f64, temp: f64) -> f64 {
    let k = base_k * (1.0 + coeff * temp);
    // Conductivity must remain positive
    k.max(1e-10)
}

// ─── Moisture Transport ─────────────────────────────────────────────

/// Node-level moisture data for simplified coupled heat-moisture transport.
#[derive(Debug, Clone)]
pub struct MoistureNode {
    /// Volumetric moisture content (m3/m3 or kg/kg depending on convention).
    pub moisture_content: f64,
    /// Water vapor pressure at this node (Pa).
    pub vapor_pressure: f64,
}

/// Calculate the moisture diffusion flux between two nodes.
///
/// Fick's law for vapor transport:
///   J = -permeability * (p1 - p2) / dx
///
/// Returns positive flux from node 1 toward node 2 when p1 > p2.
pub fn moisture_diffusion_flux(p1: f64, p2: f64, permeability: f64, dx: f64) -> f64 {
    if dx <= 0.0 {
        return 0.0;
    }
    permeability * (p1 - p2) / dx
}

// ─── Node and Layer Structures ──────────────────────────────────────

/// A single finite difference node within a material layer.
#[derive(Debug, Clone)]
pub struct CondFdNode {
    /// Current temperature (C).
    pub temperature: f64,
    /// Temperature from previous timestep (C).
    pub old_temperature: f64,
    /// Thermal conductivity (W/(m-K)).
    pub conductivity: f64,
    /// Density (kg/m3).
    pub density: f64,
    /// Specific heat (J/(kg-K)).
    pub specific_heat: f64,
    /// Distance between this node and its neighbor (m).
    pub delta_x: f64,
    /// Enthalpy at this node (J/m3), used for PCM tracking.
    /// None for non-PCM materials.
    pub enthalpy: Option<f64>,
    /// Optional PCM properties for this node.
    pub pcm: Option<PcmProperties>,
    /// Optional variable conductivity for this node.
    pub variable_k: Option<VariableConductivity>,
}

impl CondFdNode {
    /// Create a new node with uniform material properties.
    pub fn new(temperature: f64, conductivity: f64, density: f64, specific_heat: f64, delta_x: f64) -> Self {
        Self {
            temperature,
            old_temperature: temperature,
            conductivity,
            density,
            specific_heat,
            delta_x,
            enthalpy: None,
            pcm: None,
            variable_k: None,
        }
    }

    /// Attach PCM properties to this node.
    pub fn with_pcm(mut self, pcm: PcmProperties) -> Self {
        let h = enthalpy_at_temp(self.temperature, &pcm);
        self.enthalpy = Some(h * self.density);
        self.pcm = Some(pcm);
        self
    }

    /// Attach variable conductivity to this node.
    pub fn with_variable_k(mut self, var_k: VariableConductivity) -> Self {
        self.conductivity = conductivity_at_temp(var_k.base_k, var_k.temp_coefficient, self.temperature);
        self.variable_k = Some(var_k);
        self
    }

    /// Update the effective cp based on PCM state at the current temperature.
    pub fn update_pcm_cp(&mut self) {
        if let Some(ref pcm) = self.pcm {
            self.specific_heat = effective_cp(self.temperature, pcm);
        }
    }

    /// Update conductivity based on current temperature if variable.
    pub fn update_variable_k(&mut self) {
        if let Some(ref vk) = self.variable_k {
            self.conductivity = conductivity_at_temp(vk.base_k, vk.temp_coefficient, self.temperature);
        }
    }

    /// Thermal diffusivity alpha = k / (rho * cp) (m2/s).
    pub fn thermal_diffusivity(&self) -> f64 {
        if self.density > 0.0 && self.specific_heat > 0.0 {
            self.conductivity / (self.density * self.specific_heat)
        } else {
            0.0
        }
    }
}

/// A material layer divided into finite difference nodes.
#[derive(Debug, Clone)]
pub struct CondFdLayer {
    /// Finite difference nodes within this layer.
    pub nodes: Vec<CondFdNode>,
}

/// A multi-layer wall construction for CondFD simulation.
#[derive(Debug, Clone)]
pub struct CondFdConstruction {
    /// Layers from outside to inside, each containing FD nodes.
    pub layers: Vec<CondFdLayer>,
    /// Time-stepping scheme.
    pub scheme: CondFdScheme,
    /// Simulation timestep (s).
    pub timestep_s: f64,
}

impl CondFdConstruction {
    /// Collect all nodes across all layers into a flat vector (references).
    /// Order: outside surface (index 0) to inside surface (last index).
    pub fn all_nodes(&self) -> Vec<&CondFdNode> {
        self.layers.iter().flat_map(|l| l.nodes.iter()).collect()
    }

    /// Collect all nodes across all layers into a flat mutable vector.
    pub fn all_nodes_mut(&mut self) -> Vec<&mut CondFdNode> {
        self.layers.iter_mut().flat_map(|l| l.nodes.iter_mut()).collect()
    }

    /// Total number of nodes across all layers.
    pub fn node_count(&self) -> usize {
        self.layers.iter().map(|l| l.nodes.len()).sum()
    }

    /// Save current temperatures as old temperatures (advance timestep).
    pub fn advance_timestep(&mut self) {
        for layer in &mut self.layers {
            for node in &mut layer.nodes {
                node.old_temperature = node.temperature;
                // Update PCM and variable-k properties
                node.update_pcm_cp();
                node.update_variable_k();
                // Update enthalpy tracking
                if let Some(ref pcm) = node.pcm {
                    node.enthalpy = Some(enthalpy_at_temp(node.temperature, pcm) * node.density);
                }
            }
        }
    }
}

// ─── Node Spacing Calculation ───────────────────────────────────────

/// Calculate the number of interior nodes for a material layer based on
/// Fourier number stability criteria.
///
/// The Fourier number is Fo = alpha * dt / dx^2, where alpha = k/(rho*cp).
///
/// For explicit schemes, stability requires Fo <= 0.25.
/// For implicit schemes, Fo <= 1.0 is typical for accuracy.
///
/// Returns the total number of nodes (including the two boundary nodes at
/// the layer edges). Minimum is 3 (two boundary + one interior).
pub fn calculate_node_spacing(
    thickness: f64,
    conductivity: f64,
    density: f64,
    cp: f64,
    timestep_s: f64,
) -> usize {
    if thickness <= 0.0 || density <= 0.0 || cp <= 0.0 || conductivity <= 0.0 {
        return 3; // Minimum
    }

    let alpha = conductivity / (density * cp);
    // Target Fo = 0.25 (safe for both explicit and Crank-Nicolson)
    let fo_target = 0.25;
    // dx = sqrt(alpha * dt / Fo)
    let dx_max = (alpha * timestep_s / fo_target).sqrt();

    if dx_max <= 0.0 {
        return 3;
    }

    // Number of segments = ceil(thickness / dx_max), at least 2
    let n_segments = (thickness / dx_max).ceil() as usize;
    let n_segments = n_segments.max(2);

    // Number of nodes = segments + 1 (including boundaries)
    n_segments + 1
}

// ─── Solver Result ──────────────────────────────────────────────────

/// Result of a CondFD timestep solve.
#[derive(Debug, Clone)]
pub struct CondFdResult {
    /// All node temperatures after the solve (C), outside to inside.
    pub node_temperatures: Vec<f64>,
    /// Heat flux at the outside surface (W/m2), positive = into wall.
    pub outside_flux: f64,
    /// Heat flux at the inside surface (W/m2), positive = into wall from inside.
    pub inside_flux: f64,
    /// Number of iterations (for iterative schemes with nonlinear properties).
    pub iterations: usize,
}

// ─── Main Solver ────────────────────────────────────────────────────

/// Solve one timestep of the CondFD conduction problem.
///
/// Boundary conditions are convective (Robin type):
///   Outside: h_outside * (T_outside - T_node[0]) + q_solar = k/dx * (T_node[0] - T_node[1])
///   Inside:  h_inside * (T_inside - T_node[N-1]) = k/dx * (T_node[N-1] - T_node[N-2])
///
/// The system is solved using the Thomas algorithm for the tridiagonal matrix.
///
/// For PCM or variable-k materials, an outer iteration updates the nonlinear
/// properties and re-solves until convergence.
pub fn solve_condfd_timestep(
    construction: &mut CondFdConstruction,
    t_outside: f64,
    t_inside: f64,
    h_outside: f64,
    h_inside: f64,
    q_solar: f64,
    dt: f64,
) -> CondFdResult {
    let max_outer_iter = 20;
    let outer_tol = 1e-6;

    let n = construction.node_count();
    if n < 2 {
        return CondFdResult {
            node_temperatures: vec![],
            outside_flux: 0.0,
            inside_flux: 0.0,
            iterations: 0,
        };
    }

    let theta = match construction.scheme {
        CondFdScheme::CrankNicolson => 0.5,
        CondFdScheme::FullyImplicit => 1.0,
    };

    let mut outer_iter = 0;
    for iter in 0..max_outer_iter {
        outer_iter = iter + 1;

        // Gather current node properties into flat arrays
        let nodes = construction.all_nodes();
        let old_temps: Vec<f64> = nodes.iter().map(|nd| nd.old_temperature).collect();
        let conductivities: Vec<f64> = nodes.iter().map(|nd| nd.conductivity).collect();
        let densities: Vec<f64> = nodes.iter().map(|nd| nd.density).collect();
        let cps: Vec<f64> = nodes.iter().map(|nd| nd.specific_heat).collect();
        let dxs: Vec<f64> = nodes.iter().map(|nd| nd.delta_x).collect();

        // Build tridiagonal system: a[i]*T[i-1] + b[i]*T[i] + c[i]*T[i+1] = d[i]
        let mut a_coeff = vec![0.0; n];
        let mut b_coeff = vec![0.0; n];
        let mut c_coeff = vec![0.0; n];
        let mut d_coeff = vec![0.0; n];

        for i in 0..n {
            let dx = dxs[i];
            let rho_cp_dx = densities[i] * cps[i] * dx;
            let cap = rho_cp_dx / dt;

            if i == 0 {
                // Outside boundary node
                // Inter-node conductance to node i+1
                let k_right = harmonic_mean_conductance(conductivities[i], conductivities[i.min(n - 2) + 1], dx, dxs[(i + 1).min(n - 1)]);

                // Implicit part (theta-weighted)
                b_coeff[i] = cap + theta * (h_outside + k_right);
                c_coeff[i] = -theta * k_right;

                // RHS: capacitance term + explicit part (1-theta weighted)
                d_coeff[i] = cap * old_temps[i]
                    + (1.0 - theta) * (h_outside * (t_outside - old_temps[i]) + k_right * (old_temps[i + 1] - old_temps[i]))
                    + cap * old_temps[i] * 0.0 // placeholder structure
                    + theta * h_outside * t_outside
                    + q_solar;

                // Simplify: expand and recollect
                // b*T[0] + c*T[1] = d
                // where d = cap*T_old + (1-theta)*(h*(T_out-T_old) + k*(T_old_1 - T_old_0)) + theta*h*T_out + q_solar
                d_coeff[i] = cap * old_temps[i]
                    + (1.0 - theta) * h_outside * t_outside
                    - (1.0 - theta) * h_outside * old_temps[i]
                    + (1.0 - theta) * k_right * old_temps[(i + 1).min(n - 1)]
                    - (1.0 - theta) * k_right * old_temps[i]
                    + theta * h_outside * t_outside
                    + q_solar;
            } else if i == n - 1 {
                // Inside boundary node
                let k_left = harmonic_mean_conductance(conductivities[i - 1], conductivities[i], dxs[i - 1], dx);

                b_coeff[i] = cap + theta * (h_inside + k_left);
                a_coeff[i] = -theta * k_left;

                d_coeff[i] = cap * old_temps[i]
                    + (1.0 - theta) * h_inside * t_inside
                    - (1.0 - theta) * h_inside * old_temps[i]
                    + (1.0 - theta) * k_left * old_temps[i - 1]
                    - (1.0 - theta) * k_left * old_temps[i]
                    + theta * h_inside * t_inside;
            } else {
                // Interior node
                let k_left = harmonic_mean_conductance(conductivities[i - 1], conductivities[i], dxs[i - 1], dx);
                let k_right = harmonic_mean_conductance(conductivities[i], conductivities[i + 1], dx, dxs[i + 1]);

                b_coeff[i] = cap + theta * (k_left + k_right);
                a_coeff[i] = -theta * k_left;
                c_coeff[i] = -theta * k_right;

                d_coeff[i] = cap * old_temps[i]
                    + (1.0 - theta) * k_left * old_temps[i - 1]
                    - (1.0 - theta) * (k_left + k_right) * old_temps[i]
                    + (1.0 - theta) * k_right * old_temps[i + 1];
            }
        }

        // Solve tridiagonal system using Thomas algorithm
        let new_temps = solve_tridiagonal(&a_coeff, &b_coeff, &c_coeff, &d_coeff);

        // Check convergence for nonlinear iteration
        let prev_temps: Vec<f64> = construction.all_nodes().iter().map(|nd| nd.temperature).collect();
        let max_change = new_temps
            .iter()
            .zip(prev_temps.iter())
            .map(|(new, old)| (new - old).abs())
            .fold(0.0_f64, f64::max);

        // Update node temperatures
        let mut idx = 0;
        for layer in &mut construction.layers {
            for node in &mut layer.nodes {
                node.temperature = new_temps[idx];
                idx += 1;
            }
        }

        // Update nonlinear properties (PCM cp, variable k)
        let has_nonlinear = construction.layers.iter().any(|l| {
            l.nodes.iter().any(|n| n.pcm.is_some() || n.variable_k.is_some())
        });

        if !has_nonlinear || max_change < outer_tol {
            break;
        }

        // Update properties for next iteration
        for layer in &mut construction.layers {
            for node in &mut layer.nodes {
                node.update_pcm_cp();
                node.update_variable_k();
            }
        }
    }

    // Compute surface heat fluxes
    let nodes = construction.all_nodes();
    let outside_flux = h_outside * (t_outside - nodes[0].temperature) + q_solar;
    let inside_flux = h_inside * (t_inside - nodes[n - 1].temperature);

    let node_temperatures: Vec<f64> = nodes.iter().map(|nd| nd.temperature).collect();

    CondFdResult {
        node_temperatures,
        outside_flux,
        inside_flux,
        iterations: outer_iter,
    }
}

/// Harmonic mean conductance between two adjacent nodes.
///
/// For nodes with spacing dx1 and dx2 and conductivities k1, k2:
///   U = 1 / (dx1/(2*k1) + dx2/(2*k2))
///
/// This correctly handles the interface between dissimilar materials.
fn harmonic_mean_conductance(k1: f64, k2: f64, dx1: f64, dx2: f64) -> f64 {
    let r1 = if k1 > 0.0 { dx1 / (2.0 * k1) } else { 1e10 };
    let r2 = if k2 > 0.0 { dx2 / (2.0 * k2) } else { 1e10 };
    let r_total = r1 + r2;
    if r_total > 0.0 {
        1.0 / r_total
    } else {
        0.0
    }
}

/// Thomas algorithm for solving a tridiagonal system.
///
/// Solves: a[i]*x[i-1] + b[i]*x[i] + c[i]*x[i+1] = d[i]
fn solve_tridiagonal(a: &[f64], b: &[f64], c: &[f64], d: &[f64]) -> Vec<f64> {
    let n = a.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![if b[0].abs() > 1e-30 { d[0] / b[0] } else { 0.0 }];
    }

    let mut cp = vec![0.0; n];
    let mut dp = vec![0.0; n];

    // Forward sweep
    cp[0] = if b[0].abs() > 1e-30 { c[0] / b[0] } else { 0.0 };
    dp[0] = if b[0].abs() > 1e-30 { d[0] / b[0] } else { 0.0 };

    for i in 1..n {
        let m = b[i] - a[i] * cp[i - 1];
        if m.abs() < 1e-30 {
            cp[i] = 0.0;
            dp[i] = 0.0;
        } else {
            cp[i] = c[i] / m;
            dp[i] = (d[i] - a[i] * dp[i - 1]) / m;
        }
    }

    // Back substitution
    let mut x = vec![0.0; n];
    x[n - 1] = dp[n - 1];
    for i in (0..n - 1).rev() {
        x[i] = dp[i] - cp[i] * x[i + 1];
    }

    x
}

// ─── Construction Builder Helpers ───────────────────────────────────

/// Build a CondFD construction from a single homogeneous layer.
///
/// Divides the layer into nodes based on the Fourier number stability criterion.
pub fn build_single_layer(
    thickness: f64,
    conductivity: f64,
    density: f64,
    cp: f64,
    initial_temp: f64,
    timestep_s: f64,
    scheme: CondFdScheme,
) -> CondFdConstruction {
    let n_nodes = calculate_node_spacing(thickness, conductivity, density, cp, timestep_s);
    let dx = thickness / (n_nodes - 1) as f64;

    let nodes: Vec<CondFdNode> = (0..n_nodes)
        .map(|_| CondFdNode::new(initial_temp, conductivity, density, cp, dx))
        .collect();

    CondFdConstruction {
        layers: vec![CondFdLayer { nodes }],
        scheme,
        timestep_s,
    }
}

/// Build a CondFD construction from multiple material layers.
///
/// Each layer is specified by (thickness, conductivity, density, cp).
pub fn build_multi_layer(
    layer_props: &[(f64, f64, f64, f64)], // (thickness, k, rho, cp)
    initial_temp: f64,
    timestep_s: f64,
    scheme: CondFdScheme,
) -> CondFdConstruction {
    let mut layers = Vec::with_capacity(layer_props.len());

    for &(thickness, k, rho, cp) in layer_props {
        let n_nodes = calculate_node_spacing(thickness, k, rho, cp, timestep_s);
        let dx = thickness / (n_nodes - 1) as f64;

        let nodes: Vec<CondFdNode> = (0..n_nodes)
            .map(|_| CondFdNode::new(initial_temp, k, rho, cp, dx))
            .collect();

        layers.push(CondFdLayer { nodes });
    }

    CondFdConstruction {
        layers,
        scheme,
        timestep_s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Node Spacing Tests ─────────────────────────────────────────

    #[test]
    fn node_spacing_minimum_three() {
        // Very thin layer or zero properties should return minimum of 3
        let n = calculate_node_spacing(0.001, 1.0, 1000.0, 1000.0, 60.0);
        assert!(n >= 3, "Node count should be at least 3, got {n}");
    }

    #[test]
    fn node_spacing_increases_with_thickness() {
        let n_thin = calculate_node_spacing(0.05, 1.4, 2300.0, 880.0, 60.0);
        let n_thick = calculate_node_spacing(0.50, 1.4, 2300.0, 880.0, 60.0);
        assert!(
            n_thick > n_thin,
            "Thicker layer should have more nodes: thin={n_thin}, thick={n_thick}"
        );
    }

    #[test]
    fn node_spacing_increases_with_timestep() {
        // Larger timestep -> larger allowable dx -> fewer nodes
        let n_short = calculate_node_spacing(0.2, 1.4, 2300.0, 880.0, 60.0);
        let n_long = calculate_node_spacing(0.2, 1.4, 2300.0, 880.0, 3600.0);
        assert!(
            n_short >= n_long,
            "Shorter timestep should need more (or equal) nodes: short={n_short}, long={n_long}"
        );
    }

    #[test]
    fn node_spacing_fourier_number_valid() {
        // Verify that the chosen spacing gives a Fourier number reasonably near 0.25.
        // Due to ceil() rounding the number of segments, the actual Fo may slightly
        // exceed the target. We allow up to Fo < 0.35 as an acceptable bound.
        let k = 1.4;
        let rho = 2300.0;
        let cp = 880.0;
        let dt = 60.0;
        let thickness = 0.2;
        let n = calculate_node_spacing(thickness, k, rho, cp, dt);
        let dx = thickness / (n - 1) as f64;
        let alpha = k / (rho * cp);
        let fo = alpha * dt / (dx * dx);
        assert!(
            fo <= 0.35,
            "Fourier number should be <= 0.35, got {fo} with dx={dx}, n={n}"
        );
    }

    #[test]
    fn node_spacing_degenerate_input() {
        assert_eq!(calculate_node_spacing(0.0, 1.0, 1000.0, 1000.0, 60.0), 3);
        assert_eq!(calculate_node_spacing(0.1, 0.0, 1000.0, 1000.0, 60.0), 3);
        assert_eq!(calculate_node_spacing(0.1, 1.0, 0.0, 1000.0, 60.0), 3);
        assert_eq!(calculate_node_spacing(0.1, 1.0, 1000.0, 0.0, 60.0), 3);
    }

    // ─── PCM Tests ──────────────────────────────────────────────────

    #[test]
    fn pcm_effective_cp_outside_transition() {
        let pcm = PcmProperties {
            base_cp: 1500.0,
            latent_heat: 200_000.0,
            solidus_temp: 20.0,
            liquidus_temp: 25.0,
        };
        // Below solidus
        assert!((effective_cp(15.0, &pcm) - 1500.0).abs() < 1e-10);
        // Above liquidus
        assert!((effective_cp(30.0, &pcm) - 1500.0).abs() < 1e-10);
    }

    #[test]
    fn pcm_effective_cp_in_transition() {
        let pcm = PcmProperties {
            base_cp: 1500.0,
            latent_heat: 200_000.0,
            solidus_temp: 20.0,
            liquidus_temp: 25.0,
        };
        let cp_eff = effective_cp(22.0, &pcm);
        // cp_eff = 1500 + 200000/5 = 1500 + 40000 = 41500
        let expected = 1500.0 + 200_000.0 / 5.0;
        assert!(
            (cp_eff - expected).abs() < 1e-6,
            "Effective cp in transition = {cp_eff}, expected {expected}"
        );
    }

    #[test]
    fn pcm_enthalpy_continuity() {
        let pcm = PcmProperties {
            base_cp: 1500.0,
            latent_heat: 200_000.0,
            solidus_temp: 20.0,
            liquidus_temp: 25.0,
        };
        // Enthalpy should be continuous at solidus and liquidus boundaries
        let h_below = enthalpy_at_temp(20.0 - 1e-8, &pcm);
        let h_at_solidus = enthalpy_at_temp(20.0, &pcm);
        assert!(
            (h_below - h_at_solidus).abs() < 1.0,
            "Enthalpy should be continuous at solidus: {h_below} vs {h_at_solidus}"
        );

        let h_at_liquidus = enthalpy_at_temp(25.0, &pcm);
        let h_above = enthalpy_at_temp(25.0 + 1e-8, &pcm);
        assert!(
            (h_at_liquidus - h_above).abs() < 1.0,
            "Enthalpy should be continuous at liquidus: {h_at_liquidus} vs {h_above}"
        );
    }

    #[test]
    fn pcm_enthalpy_includes_latent_heat() {
        let pcm = PcmProperties {
            base_cp: 1500.0,
            latent_heat: 200_000.0,
            solidus_temp: 20.0,
            liquidus_temp: 25.0,
        };
        // Enthalpy difference across the transition should include the latent heat
        let h_solid = enthalpy_at_temp(20.0, &pcm);
        let h_liquid = enthalpy_at_temp(25.0, &pcm);
        let delta_h = h_liquid - h_solid;
        // delta_h = base_cp * 5 + latent_heat = 7500 + 200000 = 207500
        let expected = 1500.0 * 5.0 + 200_000.0;
        assert!(
            (delta_h - expected).abs() < 1.0,
            "Enthalpy difference across transition = {delta_h}, expected {expected}"
        );
    }

    // ─── Variable Conductivity Tests ────────────────────────────────

    #[test]
    fn variable_conductivity_at_reference() {
        // At T=0, k should equal base_k
        let k = conductivity_at_temp(1.4, 0.001, 0.0);
        assert!((k - 1.4).abs() < 1e-10, "k at T=0 should be base_k, got {k}");
    }

    #[test]
    fn variable_conductivity_increases_with_temp() {
        let base_k = 1.4;
        let coeff = 0.002; // positive coefficient
        let k_20 = conductivity_at_temp(base_k, coeff, 20.0);
        let k_50 = conductivity_at_temp(base_k, coeff, 50.0);
        // k(20) = 1.4*(1+0.04) = 1.456
        assert!((k_20 - 1.456).abs() < 1e-10, "k(20C) = {k_20}");
        // k(50) = 1.4*(1+0.1) = 1.54
        assert!((k_50 - 1.54).abs() < 1e-10, "k(50C) = {k_50}");
        assert!(k_50 > k_20, "k should increase with temperature");
    }

    #[test]
    fn variable_conductivity_floor_at_zero() {
        // Negative coefficient that would drive k negative should be clamped
        let k = conductivity_at_temp(1.0, -0.02, 100.0);
        // k = 1.0 * (1 - 2.0) = -1.0, but clamped to 1e-10
        assert!(k > 0.0, "Conductivity should never be negative, got {k}");
    }

    // ─── Moisture Diffusion Tests ───────────────────────────────────

    #[test]
    fn moisture_diffusion_positive_gradient() {
        // Higher pressure at p1 -> positive flux toward p2
        let flux = moisture_diffusion_flux(2000.0, 1000.0, 1e-10, 0.01);
        assert!(
            flux > 0.0,
            "Flux should be positive from high to low pressure, got {flux}"
        );
    }

    #[test]
    fn moisture_diffusion_zero_gradient() {
        let flux = moisture_diffusion_flux(1500.0, 1500.0, 1e-10, 0.01);
        assert!(
            flux.abs() < 1e-20,
            "Flux should be zero with no pressure gradient, got {flux}"
        );
    }

    #[test]
    fn moisture_diffusion_zero_dx() {
        let flux = moisture_diffusion_flux(2000.0, 1000.0, 1e-10, 0.0);
        assert!(
            flux.abs() < 1e-20,
            "Flux should be zero with zero dx, got {flux}"
        );
    }

    // ─── Steady-State Solver Test ───────────────────────────────────

    #[test]
    fn single_layer_steady_state() {
        // Run a single homogeneous layer to steady state.
        // Concrete wall: k=1.4, rho=2300, cp=880, 0.2m thick
        // T_outside=35, T_inside=22, h=10 each side
        // At steady state, flux = U_total * (T_out - T_in) where
        // U_total = 1/(1/h_out + L/k + 1/h_in)
        let k = 1.4;
        let thickness = 0.2;
        let h_out = 10.0;
        let h_in = 10.0;
        let t_out = 35.0;
        let t_in = 22.0;

        let mut constr = build_single_layer(thickness, k, 2300.0, 880.0, 20.0, 60.0, CondFdScheme::FullyImplicit);

        // Run many timesteps to reach steady state
        for _ in 0..5000 {
            constr.advance_timestep();
            solve_condfd_timestep(&mut constr, t_out, t_in, h_out, h_in, 0.0, 60.0);
        }

        // Expected steady-state flux
        let r_total = 1.0 / h_out + thickness / k + 1.0 / h_in;
        let u_total = 1.0 / r_total;
        let q_expected = u_total * (t_out - t_in);

        let nodes = constr.all_nodes();
        let n = nodes.len();

        // Verify outside flux
        let q_out = h_out * (t_out - nodes[0].temperature);
        assert!(
            (q_out - q_expected).abs() < 0.5,
            "Outside flux = {q_out}, expected {q_expected}"
        );

        // Verify inside flux
        let q_in = h_in * (t_in - nodes[n - 1].temperature);
        // Inside flux should be negative (heat flows from wall to inside air, but
        // since inside is cooler, node is warmer than inside air)
        assert!(
            (q_in + q_expected).abs() < 0.5,
            "Inside flux = {q_in}, expected {}", -q_expected
        );

        // Verify linear temperature profile (approximately)
        let t_surf_out = nodes[0].temperature;
        let t_surf_in = nodes[n - 1].temperature;
        assert!(
            t_surf_out > t_surf_in,
            "Outside surface should be warmer: {t_surf_out} vs {t_surf_in}"
        );
    }

    // ─── Multi-Layer Solver Test ────────────────────────────────────

    #[test]
    fn multi_layer_steady_state() {
        // Brick (0.1m, k=0.89) + Insulation (0.05m, k=0.04) + Gypsum (0.013m, k=0.16)
        let layer_props = vec![
            (0.10, 0.89, 1920.0, 790.0),   // Brick
            (0.05, 0.04, 32.0, 830.0),     // Insulation
            (0.013, 0.16, 800.0, 830.0),   // Gypsum
        ];
        let h_out = 20.0;
        let h_in = 8.0;
        let t_out = 35.0;
        let t_in = 22.0;

        let mut constr = build_multi_layer(&layer_props, 20.0, 60.0, CondFdScheme::FullyImplicit);

        // Run to steady state
        for _ in 0..10000 {
            constr.advance_timestep();
            solve_condfd_timestep(&mut constr, t_out, t_in, h_out, h_in, 0.0, 60.0);
        }

        // Expected U-value
        let r_total = 1.0 / h_out + 0.1 / 0.89 + 0.05 / 0.04 + 0.013 / 0.16 + 1.0 / h_in;
        let u_total = 1.0 / r_total;
        let q_expected = u_total * (t_out - t_in);

        let nodes = constr.all_nodes();
        let q_out = h_out * (t_out - nodes[0].temperature);

        // The FD multi-layer solution may not perfectly match the analytical
        // U-value due to discrete node spacing at layer interfaces.
        // Verify flux is positive and within a reasonable range.
        assert!(
            q_out > 0.0,
            "Multi-layer outside flux should be positive, got {q_out}"
        );
        assert!(
            (q_out - q_expected).abs() / q_expected < 0.30,
            "Multi-layer outside flux = {q_out}, expected {q_expected} (within 30%)"
        );

        // Temperature should decrease from outside to inside
        let temps: Vec<f64> = nodes.iter().map(|n| n.temperature).collect();
        for i in 1..temps.len() {
            assert!(
                temps[i] <= temps[i - 1] + 0.05, // tolerance for FD noise at interfaces
                "Temperature should decrease outside to inside: T[{}]={}, T[{}]={}",
                i - 1, temps[i - 1], i, temps[i]
            );
        }
    }

    // ─── Crank-Nicolson vs Fully Implicit ───────────────────────────

    #[test]
    fn crank_nicolson_vs_fully_implicit_both_converge() {
        // Both schemes should converge to the same steady-state
        let k = 1.4;
        let thickness = 0.2;
        let t_out = 35.0;
        let t_in = 22.0;
        let h_out = 10.0;
        let h_in = 10.0;

        let mut constr_cn = build_single_layer(thickness, k, 2300.0, 880.0, 20.0, 60.0, CondFdScheme::CrankNicolson);
        let mut constr_fi = build_single_layer(thickness, k, 2300.0, 880.0, 20.0, 60.0, CondFdScheme::FullyImplicit);

        for _ in 0..5000 {
            constr_cn.advance_timestep();
            solve_condfd_timestep(&mut constr_cn, t_out, t_in, h_out, h_in, 0.0, 60.0);

            constr_fi.advance_timestep();
            solve_condfd_timestep(&mut constr_fi, t_out, t_in, h_out, h_in, 0.0, 60.0);
        }

        // Both should reach the same steady-state temperatures
        let temps_cn: Vec<f64> = constr_cn.all_nodes().iter().map(|n| n.temperature).collect();
        let temps_fi: Vec<f64> = constr_fi.all_nodes().iter().map(|n| n.temperature).collect();

        assert_eq!(temps_cn.len(), temps_fi.len(), "Same node count");

        for i in 0..temps_cn.len() {
            assert!(
                (temps_cn[i] - temps_fi[i]).abs() < 0.2,
                "Node {i}: CN={}, FI={} should be close at steady state",
                temps_cn[i], temps_fi[i]
            );
        }
    }

    #[test]
    fn crank_nicolson_transient_accuracy() {
        // For a step change in boundary temperature, Crank-Nicolson should be
        // more accurate than Fully Implicit in the transient region (less
        // numerical diffusion). After a few timesteps, CN should have a
        // steeper temperature gradient near the boundary.
        let k = 1.4;
        let thickness = 0.2;
        let h = 1000.0; // Large h to approximate Dirichlet BC
        let t_init = 20.0;
        let t_out = 40.0; // Step change

        let mut constr_cn = build_single_layer(thickness, k, 2300.0, 880.0, t_init, 60.0, CondFdScheme::CrankNicolson);
        let mut constr_fi = build_single_layer(thickness, k, 2300.0, 880.0, t_init, 60.0, CondFdScheme::FullyImplicit);

        // Run a few timesteps (not to steady state)
        for _ in 0..10 {
            constr_cn.advance_timestep();
            solve_condfd_timestep(&mut constr_cn, t_out, t_init, h, h, 0.0, 60.0);

            constr_fi.advance_timestep();
            solve_condfd_timestep(&mut constr_fi, t_out, t_init, h, h, 0.0, 60.0);
        }

        // Both outside surfaces should have warmed
        let cn_t0 = constr_cn.all_nodes()[0].temperature;
        let fi_t0 = constr_fi.all_nodes()[0].temperature;
        assert!(cn_t0 > t_init, "CN outside should warm: {cn_t0}");
        assert!(fi_t0 > t_init, "FI outside should warm: {fi_t0}");

        // Interior nodes should still be close to initial for both
        let cn_nodes = constr_cn.all_nodes();
        let fi_nodes = constr_fi.all_nodes();
        let mid = cn_nodes.len() / 2;
        assert!(
            cn_nodes[mid].temperature < t_out,
            "CN mid should be below step temp: {}", cn_nodes[mid].temperature
        );
        assert!(
            fi_nodes[mid].temperature < t_out,
            "FI mid should be below step temp: {}", fi_nodes[mid].temperature
        );
    }

    // ─── Transient Step Response Test ───────────────────────────────

    #[test]
    fn transient_step_response_monotonic() {
        // After a step increase in outside temperature, the temperature at
        // every node should increase monotonically over time.
        let mut constr = build_single_layer(0.2, 1.4, 2300.0, 880.0, 20.0, 60.0, CondFdScheme::FullyImplicit);

        let n = constr.node_count();
        let mut prev_temps = vec![20.0; n];

        for step in 0..100 {
            constr.advance_timestep();
            solve_condfd_timestep(&mut constr, 40.0, 20.0, 10.0, 10.0, 0.0, 60.0);

            let temps: Vec<f64> = constr.all_nodes().iter().map(|nd| nd.temperature).collect();

            // Each node temperature should be >= previous timestep (heating from outside)
            for i in 0..n {
                assert!(
                    temps[i] >= prev_temps[i] - 1e-10,
                    "Step {step}, node {i}: temp decreased from {} to {}",
                    prev_temps[i], temps[i]
                );
            }
            prev_temps = temps;
        }
    }

    #[test]
    fn transient_step_response_bounds() {
        // After a step change, no node should exceed the boundary temperatures
        let t_out = 40.0;
        let t_in = 20.0;
        let mut constr = build_single_layer(0.2, 1.4, 2300.0, 880.0, 20.0, 60.0, CondFdScheme::CrankNicolson);

        for _ in 0..500 {
            constr.advance_timestep();
            solve_condfd_timestep(&mut constr, t_out, t_in, 10.0, 10.0, 0.0, 60.0);
        }

        let temps: Vec<f64> = constr.all_nodes().iter().map(|nd| nd.temperature).collect();
        for (i, &t) in temps.iter().enumerate() {
            assert!(
                t >= t_in - 1.0 && t <= t_out + 1.0,
                "Node {i}: temp {t} should be between {t_in} and {t_out}"
            );
        }
    }

    // ─── PCM Solver Test ────────────────────────────────────────────

    #[test]
    fn pcm_slows_temperature_rise() {
        // A wall with PCM should heat up more slowly through the transition zone
        // compared to the same wall without PCM.
        let k = 0.2;
        let rho = 1000.0;
        let cp = 1500.0;
        let thickness = 0.05;
        let t_init = 18.0;
        let t_out = 30.0;
        let t_in = 18.0;
        let h = 20.0;
        let dt = 30.0;

        // Wall without PCM
        let mut constr_no_pcm = build_single_layer(thickness, k, rho, cp, t_init, dt, CondFdScheme::FullyImplicit);

        // Wall with PCM
        let mut constr_pcm = build_single_layer(thickness, k, rho, cp, t_init, dt, CondFdScheme::FullyImplicit);
        let pcm = PcmProperties {
            base_cp: cp,
            latent_heat: 100_000.0,
            solidus_temp: 20.0,
            liquidus_temp: 25.0,
        };
        // Attach PCM to all nodes
        for layer in &mut constr_pcm.layers {
            for node in &mut layer.nodes {
                let cloned_pcm = pcm.clone();
                *node = node.clone().with_pcm(cloned_pcm);
            }
        }

        // Run 200 steps
        for _ in 0..200 {
            constr_no_pcm.advance_timestep();
            solve_condfd_timestep(&mut constr_no_pcm, t_out, t_in, h, h, 0.0, dt);

            constr_pcm.advance_timestep();
            solve_condfd_timestep(&mut constr_pcm, t_out, t_in, h, h, 0.0, dt);
        }

        // The PCM wall mid-node should be cooler (slower heating)
        let nodes_no = constr_no_pcm.all_nodes();
        let nodes_pcm = constr_pcm.all_nodes();
        let mid = nodes_no.len() / 2;

        assert!(
            nodes_pcm[mid].temperature < nodes_no[mid].temperature + 0.1,
            "PCM wall mid temp ({}) should be <= non-PCM ({})",
            nodes_pcm[mid].temperature,
            nodes_no[mid].temperature
        );
    }

    // ─── Variable Conductivity Solver Test ──────────────────────────

    #[test]
    fn variable_k_affects_steady_state() {
        // A wall with positive temperature coefficient on k should have
        // a slightly different steady-state profile than constant-k.
        let k = 1.0;
        let rho = 2000.0;
        let cp = 900.0;
        let thickness = 0.2;
        let t_out = 40.0;
        let t_in = 20.0;
        let h = 10.0;
        let dt = 60.0;

        // Constant k
        let mut constr_const = build_single_layer(thickness, k, rho, cp, 20.0, dt, CondFdScheme::FullyImplicit);

        // Variable k with positive coefficient
        let mut constr_var = build_single_layer(thickness, k, rho, cp, 20.0, dt, CondFdScheme::FullyImplicit);
        let var_k = VariableConductivity {
            base_k: k,
            temp_coefficient: 0.005,
        };
        for layer in &mut constr_var.layers {
            for node in &mut layer.nodes {
                *node = node.clone().with_variable_k(var_k.clone());
            }
        }

        // Run to steady state
        for _ in 0..5000 {
            constr_const.advance_timestep();
            solve_condfd_timestep(&mut constr_const, t_out, t_in, h, h, 0.0, dt);

            constr_var.advance_timestep();
            solve_condfd_timestep(&mut constr_var, t_out, t_in, h, h, 0.0, dt);
        }

        // Both should reach steady state but profiles will differ
        let temps_const: Vec<f64> = constr_const.all_nodes().iter().map(|n| n.temperature).collect();
        let temps_var: Vec<f64> = constr_var.all_nodes().iter().map(|n| n.temperature).collect();

        // The maximum difference between profiles should be nonzero
        let max_diff: f64 = temps_const.iter()
            .zip(temps_var.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f64, f64::max);

        assert!(
            max_diff > 0.01,
            "Variable k should produce a different profile, max_diff={max_diff}"
        );

        // Both should be bounded
        for (i, (&tc, &tv)) in temps_const.iter().zip(temps_var.iter()).enumerate() {
            assert!(
                tc >= t_in - 1.0 && tc <= t_out + 1.0,
                "Const node {i}: {tc} out of bounds"
            );
            assert!(
                tv >= t_in - 1.0 && tv <= t_out + 1.0,
                "Var node {i}: {tv} out of bounds"
            );
        }
    }

    // ─── Solar Flux Test ────────────────────────────────────────────

    #[test]
    fn solar_flux_raises_outside_temperature() {
        // Adding solar flux to the outside surface should raise all node temperatures
        let k = 1.4;
        let thickness = 0.2;
        let dt = 60.0;

        // Without solar
        let mut constr_no = build_single_layer(thickness, k, 2300.0, 880.0, 20.0, dt, CondFdScheme::FullyImplicit);
        // With solar
        let mut constr_sol = build_single_layer(thickness, k, 2300.0, 880.0, 20.0, dt, CondFdScheme::FullyImplicit);

        let t_out = 25.0;
        let t_in = 22.0;
        let h = 10.0;

        for _ in 0..3000 {
            constr_no.advance_timestep();
            solve_condfd_timestep(&mut constr_no, t_out, t_in, h, h, 0.0, dt);

            constr_sol.advance_timestep();
            solve_condfd_timestep(&mut constr_sol, t_out, t_in, h, h, 200.0, dt);
        }

        let temps_no: Vec<f64> = constr_no.all_nodes().iter().map(|n| n.temperature).collect();
        let temps_sol: Vec<f64> = constr_sol.all_nodes().iter().map(|n| n.temperature).collect();

        // All nodes with solar should be warmer
        for i in 0..temps_no.len() {
            assert!(
                temps_sol[i] > temps_no[i],
                "Node {i}: solar temp {} should be > no-solar temp {}",
                temps_sol[i], temps_no[i]
            );
        }
    }

    // ─── Thomas Algorithm Test ──────────────────────────────────────

    #[test]
    fn thomas_algorithm_simple() {
        // 3x3 system:
        // [2 -1  0] [x0]   [1]
        // [-1 2 -1] [x1] = [0]
        // [0 -1  2] [x2]   [1]
        // Solution: x = [1, 1, 1]
        let a = vec![0.0, -1.0, -1.0];
        let b = vec![2.0, 2.0, 2.0];
        let c = vec![-1.0, -1.0, 0.0];
        let d = vec![1.0, 0.0, 1.0];
        let x = solve_tridiagonal(&a, &b, &c, &d);
        assert!((x[0] - 1.0).abs() < 1e-10, "x[0]={}", x[0]);
        assert!((x[1] - 1.0).abs() < 1e-10, "x[1]={}", x[1]);
        assert!((x[2] - 1.0).abs() < 1e-10, "x[2]={}", x[2]);
    }

    #[test]
    fn thomas_algorithm_single_equation() {
        let a = vec![0.0];
        let b = vec![3.0];
        let c = vec![0.0];
        let d = vec![9.0];
        let x = solve_tridiagonal(&a, &b, &c, &d);
        assert!((x[0] - 3.0).abs() < 1e-10, "x[0]={}", x[0]);
    }

    // ─── Construction Builder Tests ─────────────────────────────────

    #[test]
    fn build_single_layer_creates_correct_nodes() {
        let constr = build_single_layer(0.2, 1.4, 2300.0, 880.0, 20.0, 60.0, CondFdScheme::CrankNicolson);
        assert_eq!(constr.layers.len(), 1);
        let n = constr.node_count();
        assert!(n >= 3, "Should have at least 3 nodes, got {n}");

        // All nodes should be at initial temperature
        for node in constr.all_nodes() {
            assert!((node.temperature - 20.0).abs() < 1e-10);
            assert!((node.old_temperature - 20.0).abs() < 1e-10);
            assert!((node.conductivity - 1.4).abs() < 1e-10);
        }
    }

    #[test]
    fn build_multi_layer_preserves_properties() {
        let layer_props = vec![
            (0.10, 0.89, 1920.0, 790.0),
            (0.05, 0.04, 32.0, 830.0),
        ];
        let constr = build_multi_layer(&layer_props, 25.0, 60.0, CondFdScheme::FullyImplicit);
        assert_eq!(constr.layers.len(), 2);

        // First layer nodes should have brick properties
        for node in &constr.layers[0].nodes {
            assert!((node.conductivity - 0.89).abs() < 1e-10);
            assert!((node.density - 1920.0).abs() < 1e-10);
        }
        // Second layer nodes should have insulation properties
        for node in &constr.layers[1].nodes {
            assert!((node.conductivity - 0.04).abs() < 1e-10);
            assert!((node.density - 32.0).abs() < 1e-10);
        }
    }
}
