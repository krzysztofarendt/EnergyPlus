//! Enhanced ground heat transfer models for building foundations.
//!
//! Implements:
//! - 2D finite-difference foundation model (simplified Kiva cross-section solver)
//! - Slab-on-grade with F-factor perimeter method
//! - Basement wall coupling with depth-varying ground temperatures
//! - Soil moisture effects on thermal conductivity (Johansen method)

use std::f64::consts::PI;

// ─── Kiva 2D Foundation Model ───────────────────────────────────────────────

/// Configuration for the 2D foundation finite-difference model.
///
/// Defines slab, wall, insulation, and soil geometry/properties for a
/// simplified cross-section analysis.
#[derive(Debug, Clone)]
pub struct FoundationConfig {
    /// Slab thickness (m).
    pub slab_thickness: f64,
    /// Slab thermal conductivity (W/m-K).
    pub slab_conductivity: f64,
    /// Slab density (kg/m3).
    pub slab_density: f64,
    /// Slab specific heat (J/kg-K).
    pub slab_cp: f64,
    /// Foundation wall thickness (m).
    pub wall_thickness: f64,
    /// Foundation wall depth below grade (m).
    pub wall_depth: f64,
    /// Foundation wall thermal conductivity (W/m-K).
    pub wall_conductivity: f64,
    /// Perimeter insulation depth below grade (m), 0 = none.
    pub perimeter_insulation_depth: f64,
    /// Perimeter insulation R-value (m2-K/W).
    pub perimeter_insulation_r_value: f64,
    /// Soil thermal conductivity (W/m-K).
    pub soil_conductivity: f64,
    /// Soil density (kg/m3).
    pub soil_density: f64,
    /// Soil specific heat (J/kg-K).
    pub soil_cp: f64,
    /// Indoor air temperature (C).
    pub indoor_temp: f64,
    /// Far-field width from foundation wall (m).
    pub far_field_width: f64,
}

impl Default for FoundationConfig {
    fn default() -> Self {
        Self {
            slab_thickness: 0.1,
            slab_conductivity: 1.4,
            slab_density: 2300.0,
            slab_cp: 880.0,
            wall_thickness: 0.2,
            wall_depth: 0.3,
            wall_conductivity: 1.4,
            perimeter_insulation_depth: 0.0,
            perimeter_insulation_r_value: 0.0,
            soil_conductivity: 1.5,
            soil_density: 1800.0,
            soil_cp: 1000.0,
            indoor_temp: 20.0,
            far_field_width: 15.0,
        }
    }
}

/// Result of a 2D foundation heat transfer calculation.
#[derive(Debug, Clone, Copy)]
pub struct FoundationResult {
    /// Heat flux through the slab (W/m of cross-section, positive = heat loss from interior).
    pub slab_heat_flux: f64,
    /// Heat flux through the foundation wall (W/m of cross-section, positive = heat loss).
    pub wall_heat_flux: f64,
    /// Average slab inside surface temperature (C).
    pub slab_inside_temp: f64,
    /// Average wall inside surface temperature (C).
    pub wall_inside_temp: f64,
}

/// 2D finite-difference foundation heat transfer model.
///
/// Solves a simplified cross-section of the building foundation using
/// implicit finite difference. The domain extends from the foundation
/// wall outward to the far field, and from grade level downward past
/// the slab/footing.
///
/// Grid layout (looking at a cross-section):
/// - x-axis: from foundation wall (x=0) to far field (x = far_field_width)
/// - z-axis: from grade surface (z=0) downward
/// - The slab occupies nodes near the top, inside the wall boundary
/// - The wall occupies nodes along x=0 from grade to wall_depth
#[derive(Debug, Clone)]
pub struct Foundation2D {
    /// Temperature grid [z][x] in Celsius.
    pub temperatures: Vec<Vec<f64>>,
    /// Grid spacing in x-direction (m).
    pub dx: f64,
    /// Grid spacing in z-direction (m).
    pub dz: f64,
    /// Number of nodes in x-direction.
    pub num_x: usize,
    /// Number of nodes in z-direction.
    pub num_z: usize,
    /// Stored configuration.
    config: FoundationConfig,
    /// Number of x-nodes inside the foundation (slab region).
    slab_x_nodes: usize,
    /// Number of z-nodes for the wall region.
    wall_z_nodes: usize,
    /// Number of z-nodes for the slab thickness.
    slab_z_nodes: usize,
}

impl Foundation2D {
    /// Create a new 2D foundation model from configuration.
    ///
    /// Grid resolution is approximately 0.25m in each direction.
    pub fn new(config: FoundationConfig) -> Self {
        let target_spacing = 0.25;

        // Domain extends from wall to far field in x, and from grade
        // down to at least wall_depth + slab_thickness + some soil below.
        let domain_x = config.far_field_width;
        let domain_z = (config.wall_depth + config.slab_thickness + 5.0).max(8.0);

        let num_x = ((domain_x / target_spacing).ceil() as usize).max(4);
        let num_z = ((domain_z / target_spacing).ceil() as usize).max(4);
        let dx = domain_x / (num_x - 1) as f64;
        let dz = domain_z / (num_z - 1) as f64;

        // How many x-nodes represent the slab (indoor) region:
        // Slab extends inward from the wall. In the cross-section, we model
        // a representative 3m slab interior width.
        let slab_interior_width = 3.0_f64.min(domain_x * 0.3);
        let slab_x_nodes = ((slab_interior_width / dx).ceil() as usize).max(2).min(num_x - 1);

        // Wall z-nodes
        let wall_z_nodes = ((config.wall_depth / dz).ceil() as usize).max(1).min(num_z - 1);

        // Slab thickness z-nodes
        let slab_z_nodes = ((config.slab_thickness / dz).ceil() as usize).max(1).min(num_z - 1);

        // Initialize all temperatures to a blend of indoor and ground
        let init_temp = (config.indoor_temp + 10.0) / 2.0;
        let temperatures = vec![vec![init_temp; num_x]; num_z];

        Self {
            temperatures,
            dx,
            dz,
            num_x,
            num_z,
            config,
            slab_x_nodes,
            wall_z_nodes,
            slab_z_nodes,
        }
    }

    /// Solve for steady-state foundation temperatures.
    ///
    /// Uses iterative Gauss-Seidel relaxation on the 2D grid.
    ///
    /// `outdoor_temp`: outdoor air temperature at grade surface (C)
    /// `ground_temp`: undisturbed deep ground temperature (C)
    pub fn solve_steady_state(&mut self, outdoor_temp: f64, ground_temp: f64) -> FoundationResult {
        let max_iter = 5000;
        let tol = 1e-6;

        // Initialize boundary temperatures
        self.apply_boundary_conditions(outdoor_temp, ground_temp);

        for _iter in 0..max_iter {
            let max_change = self.gauss_seidel_sweep(outdoor_temp, ground_temp);
            if max_change < tol {
                break;
            }
        }

        self.compute_result()
    }

    /// Solve one transient timestep using implicit finite difference.
    ///
    /// `outdoor_temp`: outdoor air temperature at grade surface (C)
    /// `ground_temp`: undisturbed deep ground temperature (C)
    /// `dt`: timestep duration (s)
    pub fn solve_timestep(
        &mut self,
        outdoor_temp: f64,
        ground_temp: f64,
        dt: f64,
    ) -> FoundationResult {
        let max_iter = 2000;
        let tol = 1e-5;

        // Save old temperatures for implicit scheme
        let old_temps = self.temperatures.clone();

        self.apply_boundary_conditions(outdoor_temp, ground_temp);

        for _iter in 0..max_iter {
            let max_change =
                self.gauss_seidel_sweep_transient(outdoor_temp, ground_temp, &old_temps, dt);
            if max_change < tol {
                break;
            }
        }

        self.compute_result()
    }

    /// Apply boundary conditions to the grid edges.
    ///
    /// The domain represents a vertical cross-section from the building
    /// center (x=0) outward to far field, and from grade (z=0) downward.
    ///
    /// Indoor region: all nodes above the slab and inside the wall
    /// (j < wall_z_nodes, i < slab_x_nodes) are fixed at indoor_temp.
    /// The slab top surface (j = wall_z_nodes, i < slab_x_nodes) is also
    /// fixed at indoor_temp (convective surface approximation).
    fn apply_boundary_conditions(&mut self, outdoor_temp: f64, ground_temp: f64) {
        let indoor = self.config.indoor_temp;

        for j in 0..self.num_z {
            // Right edge (far field): ground temperature
            self.temperatures[j][self.num_x - 1] = ground_temp;
        }

        for i in 0..self.num_x {
            // Bottom edge: deep ground temperature
            self.temperatures[self.num_z - 1][i] = ground_temp;
        }

        // Grade surface outside the building: outdoor temp
        for i in self.slab_x_nodes..self.num_x {
            self.temperatures[0][i] = outdoor_temp;
        }

        // Indoor air space above the slab: entire region at indoor_temp
        for j in 0..self.wall_z_nodes.min(self.num_z) {
            for i in 0..self.slab_x_nodes.min(self.num_x) {
                self.temperatures[j][i] = indoor;
            }
        }

        // Slab top surface: indoor_temp (convective BC)
        let slab_top_z = self.wall_z_nodes;
        if slab_top_z < self.num_z {
            for i in 0..self.slab_x_nodes.min(self.num_x) {
                self.temperatures[slab_top_z][i] = indoor;
            }
        }
    }

    /// One Gauss-Seidel relaxation sweep for steady-state.
    /// Returns maximum temperature change.
    fn gauss_seidel_sweep(&mut self, outdoor_temp: f64, ground_temp: f64) -> f64 {
        let mut max_change: f64 = 0.0;

        for j in 1..self.num_z - 1 {
            for i in 1..self.num_x - 1 {
                if self.is_boundary_node(i, j) {
                    continue;
                }

                let k = self.effective_conductivity(i, j);
                let old_t = self.temperatures[j][i];

                // Standard 2D Laplacian with possibly non-square cells:
                // k/dx^2 * (T_{i-1,j} + T_{i+1,j}) + k/dz^2 * (T_{i,j-1} + T_{i,j+1})
                // divided by 2*k*(1/dx^2 + 1/dz^2)
                let inv_dx2 = 1.0 / (self.dx * self.dx);
                let inv_dz2 = 1.0 / (self.dz * self.dz);

                let sum_x =
                    k * inv_dx2 * (self.temperatures[j][i - 1] + self.temperatures[j][i + 1]);
                let sum_z =
                    k * inv_dz2 * (self.temperatures[j - 1][i] + self.temperatures[j + 1][i]);

                let denom = 2.0 * k * (inv_dx2 + inv_dz2);
                if denom > 0.0 {
                    let new_t = (sum_x + sum_z) / denom;
                    self.temperatures[j][i] = new_t;
                    max_change = max_change.max((new_t - old_t).abs());
                }
            }
        }

        // Re-apply boundary conditions after sweep
        self.apply_boundary_conditions(outdoor_temp, ground_temp);
        max_change
    }

    /// One Gauss-Seidel sweep for transient (implicit) solution.
    fn gauss_seidel_sweep_transient(
        &mut self,
        outdoor_temp: f64,
        ground_temp: f64,
        old_temps: &[Vec<f64>],
        dt: f64,
    ) -> f64 {
        let mut max_change: f64 = 0.0;

        for j in 1..self.num_z - 1 {
            for i in 1..self.num_x - 1 {
                if self.is_boundary_node(i, j) {
                    continue;
                }

                let k = self.effective_conductivity(i, j);
                let (rho, cp) = self.effective_thermal_mass(i, j);
                let old_t = self.temperatures[j][i];
                let t_prev = old_temps[j][i];

                let inv_dx2 = 1.0 / (self.dx * self.dx);
                let inv_dz2 = 1.0 / (self.dz * self.dz);

                // Implicit: rho*cp/dt * T^{n+1} = rho*cp/dt * T^n + k * Laplacian(T^{n+1})
                let thermal_mass_term = rho * cp / dt;

                let sum_x =
                    k * inv_dx2 * (self.temperatures[j][i - 1] + self.temperatures[j][i + 1]);
                let sum_z =
                    k * inv_dz2 * (self.temperatures[j - 1][i] + self.temperatures[j + 1][i]);

                let denom = thermal_mass_term + 2.0 * k * (inv_dx2 + inv_dz2);
                if denom > 0.0 {
                    let new_t = (thermal_mass_term * t_prev + sum_x + sum_z) / denom;
                    self.temperatures[j][i] = new_t;
                    max_change = max_change.max((new_t - old_t).abs());
                }
            }
        }

        self.apply_boundary_conditions(outdoor_temp, ground_temp);
        max_change
    }

    /// Check if a node is a fixed boundary node (not to be updated).
    fn is_boundary_node(&self, i: usize, j: usize) -> bool {
        // Indoor air space above slab (entire region at indoor_temp)
        if i < self.slab_x_nodes && j < self.wall_z_nodes {
            return true;
        }
        // Indoor slab top surface
        if i < self.slab_x_nodes && j == self.wall_z_nodes {
            return true;
        }
        // Outdoor grade surface
        if j == 0 && i >= self.slab_x_nodes {
            return true;
        }
        false
    }

    /// Get effective thermal conductivity at a grid node.
    ///
    /// Accounts for slab, wall, insulation, and soil regions.
    fn effective_conductivity(&self, i: usize, j: usize) -> f64 {
        let z = j as f64 * self.dz;

        // Slab region
        if i < self.slab_x_nodes
            && j >= self.wall_z_nodes
            && j < self.wall_z_nodes + self.slab_z_nodes
        {
            return self.config.slab_conductivity;
        }

        // Wall region (vertical strip at x ~ 0)
        if i == 0 && j < self.wall_z_nodes {
            return self.config.wall_conductivity;
        }

        // Check for perimeter insulation zone.
        // The insulation is placed vertically along the outside of the foundation wall,
        // extending from grade down to perimeter_insulation_depth.
        // In the cross-section, the wall/slab boundary is at i = slab_x_nodes.
        // Insulation occupies the first few soil nodes outside the wall.
        if self.config.perimeter_insulation_depth > 0.0
            && self.config.perimeter_insulation_r_value > 0.0
        {
            let insulation_x_cells = 3;
            let just_outside_wall =
                i >= self.slab_x_nodes && i < self.slab_x_nodes + insulation_x_cells;
            let in_insulation_depth = z <= self.config.perimeter_insulation_depth;
            if just_outside_wall && in_insulation_depth {
                // Effective conductivity from R-value: k = thickness / R
                let insulation_thickness = self.dx;
                return insulation_thickness / self.config.perimeter_insulation_r_value.max(0.01);
            }
        }

        // Default: soil
        self.config.soil_conductivity
    }

    /// Get effective density and specific heat at a grid node.
    fn effective_thermal_mass(&self, i: usize, j: usize) -> (f64, f64) {
        // Slab region
        if i < self.slab_x_nodes
            && j >= self.wall_z_nodes
            && j < self.wall_z_nodes + self.slab_z_nodes
        {
            return (self.config.slab_density, self.config.slab_cp);
        }

        // Default: soil
        (self.config.soil_density, self.config.soil_cp)
    }

    /// Compute heat fluxes and surface temperatures from the solved grid.
    ///
    /// Total heat loss is computed by summing the heat flux from every indoor
    /// boundary node to its adjacent non-boundary neighbor. This captures all
    /// heat paths: through the slab downward and through the wall horizontally.
    ///
    /// Slab flux: sum of downward heat flux from all slab surface nodes
    /// (j = wall_z_nodes, i < slab_x_nodes) to the node below.
    ///
    /// Wall flux: sum of horizontal heat flux from all indoor boundary nodes
    /// adjacent to the exterior (j < wall_z_nodes, i = slab_x_nodes - 1)
    /// to the first exterior node, PLUS the heat flux from the slab-edge
    /// node downward at the wall/slab junction.
    fn compute_result(&self) -> FoundationResult {
        let indoor = self.config.indoor_temp;

        // ── Slab heat flux ──
        // Heat flowing downward from the indoor slab surface into the slab material.
        // At j = wall_z_nodes (slab top, fixed at indoor), flux to j+1.
        let slab_top_j = self.wall_z_nodes;
        let slab_below_j = (slab_top_j + 1).min(self.num_z - 1);

        let mut slab_flux_total = 0.0;
        let mut slab_inside_temp_sum = 0.0;
        let mut slab_count = 0;

        for i in 0..self.slab_x_nodes.min(self.num_x) {
            if slab_below_j > slab_top_j {
                let t_top = self.temperatures[slab_top_j][i];
                let t_below = self.temperatures[slab_below_j][i];
                let k = self.config.slab_conductivity;
                // Heat flux density downward (W/m2)
                let flux_density = k * (t_top - t_below) / self.dz;
                // Multiply by node width to get W/m (per unit length of cross-section)
                slab_flux_total += flux_density * self.dx;
                slab_inside_temp_sum += t_top;
                slab_count += 1;
            }
        }

        let slab_inside_temp = if slab_count > 0 {
            slab_inside_temp_sum / slab_count as f64
        } else {
            indoor
        };

        // ── Wall heat flux ──
        // Heat flowing horizontally from the indoor space through the wall.
        // The indoor boundary edge facing the exterior is at
        // i = slab_x_nodes - 1, j = 0..wall_z_nodes (all fixed at indoor_temp).
        // The first exterior column is at i = slab_x_nodes.
        let wall_i_in = (self.slab_x_nodes).max(1) - 1;
        let wall_i_out = self.slab_x_nodes.min(self.num_x - 1);

        let mut wall_flux_total = 0.0;
        let mut wall_inside_temp_sum = 0.0;
        let mut wall_count = 0;

        for j in 0..self.wall_z_nodes.min(self.num_z) {
            if wall_i_out > wall_i_in {
                let t_in = self.temperatures[j][wall_i_in];
                let t_out = self.temperatures[j][wall_i_out];
                let k = self.config.wall_conductivity;
                let flux_density = k * (t_in - t_out) / self.dx;
                wall_flux_total += flux_density * self.dz;
                wall_inside_temp_sum += t_in;
                wall_count += 1;
            }
        }

        let wall_inside_temp = if wall_count > 0 {
            wall_inside_temp_sum / wall_count as f64
        } else {
            indoor
        };

        FoundationResult {
            slab_heat_flux: slab_flux_total,
            wall_heat_flux: wall_flux_total,
            slab_inside_temp,
            wall_inside_temp,
        }
    }
}

// ─── Slab-on-Grade ──────────────────────────────────────────────────────────

/// Slab-on-grade heat loss model using the F-factor perimeter method.
///
/// Combines perimeter-driven heat loss (F-factor method) with center
/// area-based conduction. The F-factor captures the 2D edge effect
/// where most slab heat loss occurs near the exposed perimeter.
///
/// Reference: ASHRAE Handbook - Fundamentals, Chapter 18.
#[derive(Debug, Clone)]
pub struct SlabOnGrade {
    /// Slab floor area (m2).
    pub slab_area: f64,
    /// Slab perimeter length exposed to outdoor conditions (m).
    pub slab_perimeter: f64,
    /// Slab center U-value for conduction to ground (W/m2-K).
    pub slab_u_value: f64,
    /// Perimeter insulation R-value (m2-K/W), 0 = uninsulated.
    pub perimeter_insulation_r: f64,
    /// Perimeter insulation depth (m).
    pub perimeter_insulation_depth: f64,
}

impl SlabOnGrade {
    /// Calculate total slab heat loss (W).
    ///
    /// Q = F * P * (T_indoor - T_outdoor) + U * A_center * (T_indoor - T_ground)
    ///
    /// The F-factor is reduced by perimeter insulation.
    /// Positive result = heat loss from interior.
    pub fn calc_slab_heat_loss(
        &self,
        indoor_temp: f64,
        ground_temp: f64,
        outdoor_temp: f64,
    ) -> f64 {
        // F-factor (W/m-K) for the perimeter heat loss path.
        // Uninsulated: ~1.17 W/m-K (ASHRAE typical for 100mm slab)
        // Insulated: reduced depending on R-value and depth.
        let f_uninsulated = 1.17;
        let f_factor = if self.perimeter_insulation_r > 0.0 && self.perimeter_insulation_depth > 0.0
        {
            // Reduction factor based on insulation effectiveness
            // Approximate: F_insulated = F_uninsulated / (1 + F_uninsulated * R_ins * depth_factor)
            let depth_factor = (self.perimeter_insulation_depth / 1.0).min(1.0);
            f_uninsulated / (1.0 + f_uninsulated * self.perimeter_insulation_r * depth_factor)
        } else {
            f_uninsulated
        };

        // Perimeter loss: driven by outdoor temperature
        let q_perimeter = f_factor * self.slab_perimeter * (indoor_temp - outdoor_temp);

        // Center loss: driven by ground temperature
        // Use a reduced area for center (exclude a ~1m perimeter strip)
        let perimeter_strip_width = 1.0;
        let center_area = if self.slab_perimeter > 0.0 {
            let equiv_width = self.slab_area / (self.slab_perimeter / 4.0).max(1.0);
            let center_width = (equiv_width - 2.0 * perimeter_strip_width).max(0.0);
            let center_fraction = (center_width / equiv_width.max(0.01)).powi(2);
            self.slab_area * center_fraction
        } else {
            self.slab_area
        };
        let q_center = self.slab_u_value * center_area * (indoor_temp - ground_temp);

        q_perimeter + q_center
    }
}

// ─── Basement Wall ──────────────────────────────────────────────────────────

/// Basement wall heat transfer model.
///
/// Divides the below-grade wall into horizontal strips, each at a
/// different ground temperature (varying with depth). Total heat loss
/// is the sum of strip-by-strip conduction.
#[derive(Debug, Clone)]
pub struct BasementWall {
    /// Wall depth below grade (m).
    pub wall_depth: f64,
    /// Wall overall U-value (W/m2-K).
    pub wall_u_value: f64,
    /// Soil thermal conductivity (W/m-K), used for soil resistance.
    pub soil_conductivity: f64,
}

impl BasementWall {
    /// Calculate basement wall heat loss per unit length of wall (W/m).
    ///
    /// `indoor_temp`: indoor air temperature (C)
    /// `ground_temps_by_depth`: ground temperatures at evenly spaced depths
    ///   from grade to wall bottom. If empty, returns 0.
    ///
    /// Each element corresponds to a horizontal strip of wall. The strip
    /// height is wall_depth / len. Heat loss is positive when indoor > ground.
    pub fn calc_basement_wall_loss(
        &self,
        indoor_temp: f64,
        ground_temps_by_depth: &[f64],
    ) -> f64 {
        if ground_temps_by_depth.is_empty() || self.wall_depth <= 0.0 {
            return 0.0;
        }

        let num_strips = ground_temps_by_depth.len();
        let strip_height = self.wall_depth / num_strips as f64;

        let mut total_loss = 0.0;
        for (i, &t_ground) in ground_temps_by_depth.iter().enumerate() {
            // Depth to center of this strip
            let depth = (i as f64 + 0.5) * strip_height;

            // Effective U-value includes wall U and soil path resistance.
            // Soil resistance increases with depth (longer path to surface).
            // R_soil ~ depth / (pi * k_soil) (approximate for semi-infinite medium)
            let r_soil = if self.soil_conductivity > 0.0 {
                depth / (PI * self.soil_conductivity)
            } else {
                0.0
            };
            let r_wall = if self.wall_u_value > 0.0 {
                1.0 / self.wall_u_value
            } else {
                f64::MAX
            };
            let r_total = r_wall + r_soil;

            let u_eff = if r_total > 0.0 { 1.0 / r_total } else { 0.0 };
            total_loss += u_eff * strip_height * (indoor_temp - t_ground);
        }

        total_loss
    }

    /// Calculate the average ground temperature along the wall depth.
    ///
    /// Useful for simplified single-temperature wall loss calculations.
    pub fn average_ground_temp(ground_temps_by_depth: &[f64]) -> f64 {
        if ground_temps_by_depth.is_empty() {
            return 0.0;
        }
        ground_temps_by_depth.iter().sum::<f64>() / ground_temps_by_depth.len() as f64
    }
}

// ─── Soil Moisture ──────────────────────────────────────────────────────────

/// Soil moisture model for effective thermal conductivity.
///
/// Uses the Johansen method to interpolate between dry and saturated
/// soil thermal conductivity based on degree of saturation.
///
/// Reference: Johansen, O. 1975. Thermal conductivity of soils.
#[derive(Debug, Clone)]
pub struct SoilMoistureModel {
    /// Dry soil thermal conductivity (W/m-K).
    pub dry_conductivity: f64,
    /// Saturated soil thermal conductivity (W/m-K).
    pub saturated_conductivity: f64,
    /// Degree of saturation (0 to 1).
    pub saturation: f64,
}

impl SoilMoistureModel {
    /// Calculate effective thermal conductivity using the Johansen method.
    ///
    /// k_eff = k_dry + (k_sat - k_dry) * Ke
    ///
    /// where Ke (Kersten number) = log10(saturation) + 1 for saturation > 0.05
    /// and Ke = 0 for very dry conditions.
    pub fn effective_conductivity(&self) -> f64 {
        effective_conductivity(
            self.dry_conductivity,
            self.saturated_conductivity,
            self.saturation,
        )
    }
}

/// Calculate effective soil thermal conductivity using the Johansen method.
///
/// `dry_k`: dry soil conductivity (W/m-K)
/// `sat_k`: saturated soil conductivity (W/m-K)
/// `saturation`: degree of saturation (0 to 1)
///
/// Returns effective conductivity (W/m-K).
pub fn effective_conductivity(dry_k: f64, sat_k: f64, saturation: f64) -> f64 {
    let s = saturation.clamp(0.0, 1.0);

    if s < 0.05 {
        // Very dry: use dry conductivity (log10 undefined near 0)
        return dry_k;
    }

    // Kersten number: Ke = log10(S) + 1
    let ke = (s.log10() + 1.0).clamp(0.0, 1.0);

    dry_k + (sat_k - dry_k) * ke
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Foundation2D ───

    #[test]
    fn foundation_2d_grid_creation() {
        let config = FoundationConfig::default();
        let f = Foundation2D::new(config);

        assert!(f.num_x >= 4, "num_x={}", f.num_x);
        assert!(f.num_z >= 4, "num_z={}", f.num_z);
        assert!(f.dx > 0.0, "dx={}", f.dx);
        assert!(f.dz > 0.0, "dz={}", f.dz);
        assert_eq!(f.temperatures.len(), f.num_z);
        assert_eq!(f.temperatures[0].len(), f.num_x);
    }

    #[test]
    fn foundation_2d_steady_state_heat_loss_direction() {
        // Indoor (20C) warmer than ground (10C) and outdoor (5C) → heat loss
        let config = FoundationConfig {
            indoor_temp: 20.0,
            ..Default::default()
        };
        let mut f = Foundation2D::new(config);
        let result = f.solve_steady_state(5.0, 10.0);

        assert!(
            result.slab_heat_flux > 0.0,
            "slab flux={}, expected positive (heat loss)",
            result.slab_heat_flux
        );
        assert!(
            result.wall_heat_flux > 0.0,
            "wall flux={}, expected positive (heat loss)",
            result.wall_heat_flux
        );
    }

    #[test]
    fn foundation_2d_cold_indoor_reverses_flux() {
        // Indoor (5C) colder than ground (15C) → heat gain (negative flux)
        let config = FoundationConfig {
            indoor_temp: 5.0,
            ..Default::default()
        };
        let mut f = Foundation2D::new(config);
        let result = f.solve_steady_state(5.0, 15.0);

        // Slab: indoor colder than ground below → heat flows up into space
        // The slab inside temp is fixed at indoor_temp, and ground below is warmer,
        // so the gradient drives heat upward → slab_heat_flux should be negative
        // (heat entering the indoor space from below).
        assert!(
            result.slab_heat_flux <= 0.0,
            "slab flux={}, expected non-positive (heat gain)",
            result.slab_heat_flux
        );
    }

    #[test]
    fn foundation_2d_insulation_reduces_flux() {
        // Use a deep wall (2m) so insulation covers many z-nodes and its
        // effect on the wall heat flux is clearly visible in the solver.
        let config_no_ins = FoundationConfig {
            indoor_temp: 20.0,
            wall_depth: 2.0,
            perimeter_insulation_depth: 0.0,
            perimeter_insulation_r_value: 0.0,
            ..Default::default()
        };
        let mut f_no = Foundation2D::new(config_no_ins);
        let r_no = f_no.solve_steady_state(0.0, 10.0);

        let config_ins = FoundationConfig {
            indoor_temp: 20.0,
            wall_depth: 2.0,
            perimeter_insulation_depth: 2.0,
            perimeter_insulation_r_value: 3.0,
            ..Default::default()
        };
        let mut f_ins = Foundation2D::new(config_ins);
        let r_ins = f_ins.solve_steady_state(0.0, 10.0);

        // Insulation should reduce wall heat flux
        assert!(
            r_ins.wall_heat_flux < r_no.wall_heat_flux,
            "insulated wall flux={} should be < uninsulated wall flux={}",
            r_ins.wall_heat_flux,
            r_no.wall_heat_flux
        );
    }

    #[test]
    fn foundation_2d_timestep_produces_result() {
        let config = FoundationConfig {
            indoor_temp: 20.0,
            ..Default::default()
        };
        let mut f = Foundation2D::new(config);
        let result = f.solve_timestep(5.0, 10.0, 3600.0);

        // Should produce finite results
        assert!(result.slab_heat_flux.is_finite());
        assert!(result.wall_heat_flux.is_finite());
        assert!(result.slab_inside_temp.is_finite());
        assert!(result.wall_inside_temp.is_finite());
    }

    #[test]
    fn foundation_2d_inside_temps_near_indoor() {
        // With Dirichlet BCs, inside surface temps should equal indoor
        let config = FoundationConfig {
            indoor_temp: 22.0,
            ..Default::default()
        };
        let mut f = Foundation2D::new(config);
        let result = f.solve_steady_state(0.0, 10.0);

        assert!(
            (result.slab_inside_temp - 22.0).abs() < 0.1,
            "slab inside temp={}, expected ~22.0",
            result.slab_inside_temp
        );
        assert!(
            (result.wall_inside_temp - 22.0).abs() < 0.1,
            "wall inside temp={}, expected ~22.0",
            result.wall_inside_temp
        );
    }

    // ─── SlabOnGrade ───

    #[test]
    fn slab_on_grade_f_factor_basic() {
        let slab = SlabOnGrade {
            slab_area: 100.0,
            slab_perimeter: 40.0,
            slab_u_value: 0.5,
            perimeter_insulation_r: 0.0,
            perimeter_insulation_depth: 0.0,
        };

        let q = slab.calc_slab_heat_loss(20.0, 10.0, 0.0);
        // Indoor warmer → should lose heat (positive)
        assert!(q > 0.0, "q={}, expected positive heat loss", q);
    }

    #[test]
    fn slab_heat_loss_increases_with_perimeter() {
        let slab_small = SlabOnGrade {
            slab_area: 100.0,
            slab_perimeter: 20.0,
            slab_u_value: 0.5,
            perimeter_insulation_r: 0.0,
            perimeter_insulation_depth: 0.0,
        };
        let slab_large = SlabOnGrade {
            slab_area: 100.0,
            slab_perimeter: 60.0,
            slab_u_value: 0.5,
            perimeter_insulation_r: 0.0,
            perimeter_insulation_depth: 0.0,
        };

        let q_small = slab_small.calc_slab_heat_loss(20.0, 10.0, 0.0);
        let q_large = slab_large.calc_slab_heat_loss(20.0, 10.0, 0.0);

        assert!(
            q_large > q_small,
            "larger perimeter q={} should > smaller q={}",
            q_large,
            q_small
        );
    }

    #[test]
    fn slab_perimeter_insulation_reduces_loss() {
        let slab_no_ins = SlabOnGrade {
            slab_area: 100.0,
            slab_perimeter: 40.0,
            slab_u_value: 0.5,
            perimeter_insulation_r: 0.0,
            perimeter_insulation_depth: 0.0,
        };
        let slab_ins = SlabOnGrade {
            slab_area: 100.0,
            slab_perimeter: 40.0,
            slab_u_value: 0.5,
            perimeter_insulation_r: 2.0,
            perimeter_insulation_depth: 1.0,
        };

        let q_no = slab_no_ins.calc_slab_heat_loss(20.0, 10.0, 0.0);
        let q_ins = slab_ins.calc_slab_heat_loss(20.0, 10.0, 0.0);

        assert!(
            q_ins < q_no,
            "insulated q={} should be < uninsulated q={}",
            q_ins,
            q_no
        );
    }

    #[test]
    fn slab_zero_delta_t_zero_loss() {
        let slab = SlabOnGrade {
            slab_area: 100.0,
            slab_perimeter: 40.0,
            slab_u_value: 0.5,
            perimeter_insulation_r: 0.0,
            perimeter_insulation_depth: 0.0,
        };

        // Indoor = outdoor = ground → no heat loss
        let q = slab.calc_slab_heat_loss(15.0, 15.0, 15.0);
        assert!(
            q.abs() < 1e-10,
            "equal temps should give zero loss, got {}",
            q
        );
    }

    // ─── BasementWall ───

    #[test]
    fn basement_wall_loss_positive_when_indoor_warmer() {
        let wall = BasementWall {
            wall_depth: 2.0,
            wall_u_value: 1.0,
            soil_conductivity: 1.5,
        };

        // Ground at 10C from top to bottom, indoor at 20C
        let ground_temps = vec![10.0; 10];
        let q = wall.calc_basement_wall_loss(20.0, &ground_temps);

        assert!(
            q > 0.0,
            "q={}, expected positive (heat loss from indoor)",
            q
        );
    }

    #[test]
    fn basement_wall_loss_with_depth_varying_temps() {
        let wall = BasementWall {
            wall_depth: 3.0,
            wall_u_value: 1.5,
            soil_conductivity: 1.5,
        };

        // Ground gets warmer with depth (winter scenario near surface)
        let ground_temps = vec![2.0, 4.0, 6.0, 8.0, 10.0, 12.0];
        let q = wall.calc_basement_wall_loss(20.0, &ground_temps);

        // All ground temps below indoor → positive heat loss
        assert!(q > 0.0, "q={}, expected positive", q);
    }

    #[test]
    fn basement_wall_deeper_has_more_loss() {
        // Deeper wall should have more heat loss area (more strips)
        let wall_shallow = BasementWall {
            wall_depth: 1.0,
            wall_u_value: 1.0,
            soil_conductivity: 1.5,
        };
        let wall_deep = BasementWall {
            wall_depth: 3.0,
            wall_u_value: 1.0,
            soil_conductivity: 1.5,
        };

        let ground_temps = vec![10.0; 10];
        let q_shallow = wall_shallow.calc_basement_wall_loss(20.0, &ground_temps);
        let q_deep = wall_deep.calc_basement_wall_loss(20.0, &ground_temps);

        assert!(
            q_deep > q_shallow,
            "deep q={} should > shallow q={}",
            q_deep,
            q_shallow
        );
    }

    #[test]
    fn basement_wall_average_ground_temp() {
        let temps = vec![5.0, 10.0, 15.0];
        let avg = BasementWall::average_ground_temp(&temps);
        assert!((avg - 10.0).abs() < 1e-10, "avg={}, expected 10.0", avg);
    }

    #[test]
    fn basement_wall_empty_temps() {
        let wall = BasementWall {
            wall_depth: 2.0,
            wall_u_value: 1.0,
            soil_conductivity: 1.5,
        };
        let q = wall.calc_basement_wall_loss(20.0, &[]);
        assert!(q.abs() < 1e-10, "empty temps should give 0, got {}", q);

        let avg = BasementWall::average_ground_temp(&[]);
        assert!(avg.abs() < 1e-10, "empty avg should be 0, got {}", avg);
    }

    // ─── SoilMoistureModel ───

    #[test]
    fn soil_moisture_dry_gives_dry_k() {
        let k = effective_conductivity(0.25, 2.0, 0.0);
        assert!(
            (k - 0.25).abs() < 1e-10,
            "saturation=0 should give dry_k=0.25, got {}",
            k
        );
    }

    #[test]
    fn soil_moisture_saturated_gives_sat_k() {
        let k = effective_conductivity(0.25, 2.0, 1.0);
        // At S=1.0: Ke = log10(1) + 1 = 0 + 1 = 1
        // k_eff = 0.25 + (2.0 - 0.25) * 1.0 = 2.0
        assert!(
            (k - 2.0).abs() < 1e-10,
            "saturation=1 should give sat_k=2.0, got {}",
            k
        );
    }

    #[test]
    fn soil_moisture_intermediate() {
        // At S=0.5: Ke = log10(0.5) + 1 ≈ -0.301 + 1 = 0.699
        let k = effective_conductivity(0.25, 2.0, 0.5);
        let expected_ke = 0.5_f64.log10() + 1.0;
        let expected_k = 0.25 + (2.0 - 0.25) * expected_ke;
        assert!(
            (k - expected_k).abs() < 1e-6,
            "k={}, expected {}",
            k,
            expected_k
        );
        // Should be between dry and saturated
        assert!(k > 0.25 && k < 2.0, "k={} should be in (0.25, 2.0)", k);
    }

    #[test]
    fn soil_moisture_model_struct() {
        let model = SoilMoistureModel {
            dry_conductivity: 0.3,
            saturated_conductivity: 2.5,
            saturation: 0.8,
        };
        let k = model.effective_conductivity();
        // Ke = log10(0.8) + 1 ≈ -0.097 + 1 = 0.903
        let expected_ke = (0.8_f64.log10() + 1.0).clamp(0.0, 1.0);
        let expected_k = 0.3 + (2.5 - 0.3) * expected_ke;
        assert!(
            (k - expected_k).abs() < 1e-6,
            "k={}, expected {}",
            k,
            expected_k
        );
    }

    #[test]
    fn soil_moisture_clamped_above_1() {
        // Saturation > 1 should be clamped to 1
        let k = effective_conductivity(0.25, 2.0, 1.5);
        assert!(
            (k - 2.0).abs() < 1e-10,
            "saturation>1 should clamp to sat_k, got {}",
            k
        );
    }

    #[test]
    fn soil_moisture_very_low_returns_dry() {
        // Saturation < 0.05 returns dry conductivity (avoids log10(0) issue)
        let k = effective_conductivity(0.25, 2.0, 0.01);
        assert!(
            (k - 0.25).abs() < 1e-10,
            "very low saturation should give dry_k, got {}",
            k
        );
    }
}
