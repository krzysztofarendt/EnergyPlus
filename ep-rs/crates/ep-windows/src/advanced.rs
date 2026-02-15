//! Advanced fenestration models: Equivalent Layer (EQL), BSDF, switchable glazing,
//! and complex fenestration state control.
//!
//! These models extend the basic single/multi-pane glazing calculations in [`optics`]
//! and [`thermal`] to handle complex shading assemblies (EQL net radiation method),
//! directional glazing with full angular resolution (BSDF/Klems basis), and
//! dynamically switchable glazing (electrochromic, thermochromic).

use std::f64::consts::PI;

use crate::optics::LayerOptics;

// ─── Equivalent Layer (EQL) Model ────────────────────────────────────

/// Optical and thermal properties of a single equivalent layer in an EQL assembly.
///
/// Each layer is characterised by its solar-band and IR-band radiative properties
/// plus a conductive/convective thermal resistance. The net radiation method
/// solves for the absorbed flux in each spectral band, then the thermal balance
/// yields layer temperatures and heat flow.
#[derive(Debug, Clone)]
pub struct EquivalentLayer {
    /// Solar transmittance (0-1).
    pub solar_transmittance: f64,
    /// Solar reflectance, front side (0-1).
    pub solar_reflectance_front: f64,
    /// Solar reflectance, back side (0-1).
    pub solar_reflectance_back: f64,
    /// Infrared (longwave) transmittance (0-1).
    pub ir_transmittance: f64,
    /// Infrared emissivity, front side (0-1).
    pub ir_emissivity_front: f64,
    /// Infrared emissivity, back side (0-1).
    pub ir_emissivity_back: f64,
    /// Thermal resistance of the layer (m2-K/W).
    pub thermal_resistance: f64,
}

impl EquivalentLayer {
    /// Solar absorptance of the front face: 1 - T - Rf.
    pub fn solar_absorptance_front(&self) -> f64 {
        (1.0 - self.solar_transmittance - self.solar_reflectance_front).max(0.0)
    }

    /// Solar absorptance of the back face: 1 - T - Rb.
    pub fn solar_absorptance_back(&self) -> f64 {
        (1.0 - self.solar_transmittance - self.solar_reflectance_back).max(0.0)
    }
}

/// Multi-layer equivalent-layer assembly.
///
/// Layers are ordered from outside (index 0) to inside (last index).
#[derive(Debug, Clone)]
pub struct EqlAssembly {
    /// Layers ordered outside to inside.
    pub layers: Vec<EquivalentLayer>,
}

/// Result of solving the EQL system.
#[derive(Debug, Clone)]
pub struct EqlResult {
    /// Temperature of each layer (K), outside to inside.
    pub layer_temperatures: Vec<f64>,
    /// Total solar transmittance of the assembly (0-1).
    pub total_transmittance: f64,
    /// Total solar reflectance (front side) of the assembly (0-1).
    pub total_reflectance: f64,
    /// Solar absorptance per layer (0-1 each).
    pub layer_absorptances: Vec<f64>,
    /// Net heat flow into interior (W/m2), positive = heat gain.
    pub heat_flow_in: f64,
}

/// Solve the EQL system for a multi-layer assembly using a net radiation method.
///
/// The solar band is solved first to get absorbed solar in each layer. Then the
/// thermal balance (conductive resistance chain between outdoor and indoor air)
/// is solved for layer temperatures and interior heat flow.
///
/// # Arguments
/// * `assembly` - The EQL assembly (layers outside to inside)
/// * `incident_solar` - Incident solar irradiance on exterior face (W/m2)
/// * `t_outside` - Outside air temperature (K)
/// * `t_inside` - Inside air temperature (K)
pub fn solve_eql_system(
    assembly: &EqlAssembly,
    incident_solar: f64,
    t_outside: f64,
    t_inside: f64,
) -> EqlResult {
    let n = assembly.layers.len();
    if n == 0 {
        return EqlResult {
            layer_temperatures: vec![],
            total_transmittance: 1.0,
            total_reflectance: 0.0,
            layer_absorptances: vec![],
            heat_flow_in: 0.0,
        };
    }

    // --- Solar band: compute system T, R, and per-layer absorptance ---
    // Use iterative combination from outside in.
    let (total_t, total_r, layer_abs) = solve_solar_net_radiation(&assembly.layers);

    // Absorbed solar per layer (W/m2)
    let absorbed_solar: Vec<f64> = layer_abs.iter().map(|&a| a * incident_solar).collect();

    // --- Thermal balance: resistance chain ---
    // Exterior film resistance (standard NFRC exterior)
    let h_ext = 23.0; // W/(m2-K)
    let h_int = 8.29; // W/(m2-K)
    let r_ext = 1.0 / h_ext;
    let r_int = 1.0 / h_int;

    // Total resistance
    let r_layers: f64 = assembly.layers.iter().map(|l| l.thermal_resistance).sum();
    let r_total = r_ext + r_layers + r_int;

    // Background conductive heat flow (no solar)
    let q_cond = (t_outside - t_inside) / r_total;

    // Layer temperatures: linear distribution modified by absorbed solar
    let mut layer_temps = vec![0.0; n];
    let mut r_cumulative = r_ext;
    for i in 0..n {
        r_cumulative += assembly.layers[i].thermal_resistance / 2.0;
        let frac = r_cumulative / r_total;
        // Base temperature from conduction
        let t_base = t_outside + frac * (t_inside - t_outside);
        // Solar heating: absorbed solar raises layer temperature proportional to
        // the thermal resistance to both sides
        let r_to_outside = r_cumulative;
        let r_to_inside = r_total - r_cumulative;
        let delta_t_solar = absorbed_solar[i] * r_to_outside * r_to_inside / r_total;
        layer_temps[i] = t_base + delta_t_solar;
        r_cumulative += assembly.layers[i].thermal_resistance / 2.0;
    }

    // Heat flow to interior: conduction + inward-flowing fraction of solar
    let mut q_solar_in = 0.0;
    let mut r_accum = r_ext;
    for i in 0..n {
        r_accum += assembly.layers[i].thermal_resistance / 2.0;
        let r_to_outside = r_accum;
        let inward_fraction = r_to_outside / r_total;
        q_solar_in += absorbed_solar[i] * inward_fraction;
        r_accum += assembly.layers[i].thermal_resistance / 2.0;
    }
    // Transmitted solar goes fully to interior
    q_solar_in += total_t * incident_solar;
    // Conductive heat flow (positive when outside > inside)
    let heat_flow_in = q_cond + q_solar_in;

    EqlResult {
        layer_temperatures: layer_temps,
        total_transmittance: total_t,
        total_reflectance: total_r,
        layer_absorptances: layer_abs,
        heat_flow_in,
    }
}

/// Solve solar net radiation for a stack of layers.
///
/// Returns (total_transmittance, total_reflectance_front, per_layer_absorptance).
/// Uses the standard two-flux (forward + backward) inter-reflection method,
/// combining layers iteratively from outside in.
fn solve_solar_net_radiation(layers: &[EquivalentLayer]) -> (f64, f64, Vec<f64>) {
    let n = layers.len();
    if n == 0 {
        return (1.0, 0.0, vec![]);
    }
    if n == 1 {
        let l = &layers[0];
        let a = l.solar_absorptance_front();
        return (l.solar_transmittance, l.solar_reflectance_front, vec![a]);
    }

    // Forward (outside-to-inside) and backward beam tracking
    // f[i] = fraction of incident solar reaching front of layer i
    // b[i] = fraction reflected back reaching back of layer i
    let mut f = vec![0.0; n + 1]; // f[0] = 1.0 (incident), f[n] = transmitted
    let mut b = vec![0.0; n + 1]; // b[n] = 0.0 (no reflection from room side)

    // Iterative solution (Gauss-Seidel style)
    f[0] = 1.0;
    b[n] = 0.0;

    for _iter in 0..20 {
        // Forward pass
        for i in 0..n {
            let l = &layers[i];
            // Flux arriving at front of layer i: forward from left + reflected from right
            let arriving = f[i]; // simplified: ignore back-reflection coupling for first pass
            f[i + 1] = arriving * l.solar_transmittance + b[i + 1] * l.solar_reflectance_back;
        }
        // Backward pass
        for i in (0..n).rev() {
            let l = &layers[i];
            // Flux reflected backward from layer i
            b[i] = f[i] * l.solar_reflectance_front + b[i + 1] * l.solar_transmittance;
        }
    }

    // Iterative two-flux with inter-reflections for 2-layer combine
    // Use recursive combination for accuracy
    let (sys_t, sys_rf, _sys_rb) = combine_layers_recursive(layers);

    // Per-layer absorptance via subtraction
    let mut abs_per_layer = vec![0.0; n];
    // Forward irradiance reaching each layer
    let mut fwd = vec![0.0; n + 1];
    let mut bwd = vec![0.0; n + 1];
    fwd[0] = 1.0;
    bwd[n] = 0.0;

    // Build combined properties for sub-stacks
    // abs[i] = A_front[i] * fwd[i] + A_back[i] * bwd[i+1]
    // where fwd/bwd account for inter-reflections
    // Simplified: use the combined T/R approach
    let mut remaining_fwd = 1.0;
    let mut total_abs = 0.0;
    for i in 0..n {
        let l = &layers[i];
        // Absorptance of this layer from forward flux
        let a_fwd = remaining_fwd * l.solar_absorptance_front();
        abs_per_layer[i] = a_fwd;
        total_abs += a_fwd;
        remaining_fwd *= l.solar_transmittance;
    }

    // Normalise: T + R + sum(A) should = 1
    let raw_sum = sys_t + sys_rf + total_abs;
    if raw_sum > 1e-10 {
        // Scale absorptances to enforce conservation
        let a_target = 1.0 - sys_t - sys_rf;
        if total_abs > 1e-10 && a_target > 0.0 {
            let scale = a_target / total_abs;
            for a in &mut abs_per_layer {
                *a *= scale;
            }
        }
    }

    (sys_t, sys_rf, abs_per_layer)
}

/// Recursively combine layers using the two-surface inter-reflection formula.
///
/// Returns (transmittance, reflectance_front, reflectance_back) of the combined stack.
fn combine_layers_recursive(layers: &[EquivalentLayer]) -> (f64, f64, f64) {
    if layers.len() == 1 {
        let l = &layers[0];
        return (l.solar_transmittance, l.solar_reflectance_front, l.solar_reflectance_back);
    }

    // Split at midpoint and combine recursively
    let mid = layers.len() / 2;
    let (t_a, rf_a, rb_a) = combine_layers_recursive(&layers[..mid]);
    let (t_b, rf_b, rb_b) = combine_layers_recursive(&layers[mid..]);

    // Two-layer inter-reflection combination
    let denom = (1.0 - rb_a * rf_b).max(1e-12);
    let t_sys = t_a * t_b / denom;
    let rf_sys = rf_a + t_a * t_a * rf_b / denom;
    let rb_sys = rb_b + t_b * t_b * rb_a / denom;

    (t_sys.max(0.0), rf_sys.max(0.0), rb_sys.max(0.0))
}

// ─── BSDF (Bidirectional Scattering Distribution Function) ───────────

/// A single patch in the BSDF angular basis (Klems full basis: 145 patches).
#[derive(Debug, Clone)]
pub struct BsdfPatch {
    /// Polar angle of patch centre (radians from normal, 0 = normal).
    pub theta_center: f64,
    /// Azimuthal angle of patch centre (radians, 0 = reference direction).
    pub phi_center: f64,
    /// Solid angle subtended by this patch (sr).
    pub solid_angle: f64,
    /// Direction index (0..n_basis-1).
    pub lambda: usize,
}

/// BSDF matrix data for a glazing or shading layer.
///
/// Stores full transmission and reflection matrices on the Klems basis.
/// Element `[i][j]` is the fraction of flux from incoming patch `j`
/// scattered into outgoing patch `i`.
#[derive(Debug, Clone)]
pub struct BsdfMatrix {
    /// Number of basis patches (typically 145 for Klems full).
    pub n_basis: usize,
    /// Transmission matrix T[outgoing][incoming], size n_basis x n_basis.
    pub transmission_matrix: Vec<Vec<f64>>,
    /// Front reflection matrix Rf[outgoing][incoming].
    pub reflection_front_matrix: Vec<Vec<f64>>,
    /// Back reflection matrix Rb[outgoing][incoming].
    pub reflection_back_matrix: Vec<Vec<f64>>,
}

/// BSDF-based glazing system.
#[derive(Debug, Clone)]
pub struct BsdfGlazing {
    /// BSDF data for this glazing.
    pub bsdf_data: BsdfMatrix,
    /// Angular basis patches.
    pub patches: Vec<BsdfPatch>,
}

/// Result of BSDF solar property calculation.
#[derive(Debug, Clone)]
pub struct BsdfSolarResult {
    /// Total hemispheric solar transmittance for the given incident direction.
    pub transmittance: f64,
    /// Total hemispheric front reflectance for the given incident direction.
    pub reflectance_front: f64,
    /// Total absorptance (1 - T - Rf).
    pub absorptance: f64,
    /// Transmitted flux distribution per outgoing patch (length = n_basis).
    pub transmitted_distribution: Vec<f64>,
}

/// Generate the Klems full basis with 145 patches.
///
/// The Klems basis divides the hemisphere into 9 theta bands with
/// the following number of phi divisions: 1, 8, 16, 20, 24, 24, 24, 16, 12.
/// Total = 145 patches.
pub fn klems_full_basis() -> Vec<BsdfPatch> {
    // Theta band boundaries (degrees) and phi divisions
    let bands: &[(f64, f64, usize)] = &[
        (0.0, 5.0, 1),
        (5.0, 15.0, 8),
        (15.0, 25.0, 16),
        (25.0, 35.0, 20),
        (35.0, 45.0, 24),
        (45.0, 55.0, 24),
        (55.0, 65.0, 24),
        (65.0, 75.0, 16),
        (75.0, 90.0, 12),
    ];

    let mut patches = Vec::with_capacity(145);
    let mut idx = 0;

    for &(theta_lo_deg, theta_hi_deg, n_phi) in bands {
        let theta_lo = theta_lo_deg.to_radians();
        let theta_hi = theta_hi_deg.to_radians();
        let theta_center = 0.5 * (theta_lo + theta_hi);

        // Solid angle of the band ring = 2*PI*(cos(lo) - cos(hi))
        let band_solid = 2.0 * PI * (theta_lo.cos() - theta_hi.cos());
        let patch_solid = band_solid / n_phi as f64;

        let d_phi = 2.0 * PI / n_phi as f64;
        for j in 0..n_phi {
            let phi_center = (j as f64 + 0.5) * d_phi;
            patches.push(BsdfPatch {
                theta_center,
                phi_center,
                solid_angle: patch_solid,
                lambda: idx,
            });
            idx += 1;
        }
    }

    patches
}

/// Find the closest Klems patch index for a given incident direction.
fn find_nearest_patch(patches: &[BsdfPatch], theta: f64, phi: f64) -> usize {
    let mut best = 0;
    let mut best_dist = f64::MAX;

    for (i, p) in patches.iter().enumerate() {
        // Angular distance metric (approximate great-circle)
        let d_theta = (theta - p.theta_center).abs();
        let d_phi_raw = (phi - p.phi_center).abs();
        let d_phi = d_phi_raw.min(2.0 * PI - d_phi_raw);
        let dist = d_theta * d_theta + d_phi * d_phi * theta.sin() * theta.sin();
        if dist < best_dist {
            best_dist = dist;
            best = i;
        }
    }

    best
}

/// Calculate the hemispheric transmittance from a BSDF for a given incident direction.
///
/// Sums the transmission matrix column for the incident patch over all
/// outgoing patches, weighted by their solid angle and cos(theta).
pub fn calc_bsdf_transmittance(bsdf: &BsdfGlazing, incident_theta: f64, incident_phi: f64) -> f64 {
    let j = find_nearest_patch(&bsdf.patches, incident_theta, incident_phi);
    let n = bsdf.bsdf_data.n_basis;

    if j >= n || bsdf.bsdf_data.transmission_matrix.is_empty() {
        return 0.0;
    }

    // Sum T[i][j] * Omega_i * cos(theta_i) / PI for all outgoing patches
    // The BSDF matrix stores luminance coefficients; to get hemispheric transmittance
    // we integrate: T_hem = sum_i ( T[i][j] * Omega_i * cos(theta_i) ) / PI
    // But if the matrix already stores flux fractions (as in Window BSDF XML),
    // then T_hem = sum_i T[i][j].
    let mut t_hem = 0.0;
    for i in 0..n.min(bsdf.bsdf_data.transmission_matrix.len()) {
        let row = &bsdf.bsdf_data.transmission_matrix[i];
        if j < row.len() {
            t_hem += row[j];
        }
    }

    t_hem.clamp(0.0, 1.0)
}

/// Calculate full solar properties from a BSDF for a given sun position.
///
/// Returns total transmittance, front reflectance, absorptance, and the
/// angular distribution of transmitted flux.
pub fn calc_bsdf_solar_properties(
    bsdf: &BsdfGlazing,
    sun_theta: f64,
    sun_phi: f64,
) -> BsdfSolarResult {
    let j = find_nearest_patch(&bsdf.patches, sun_theta, sun_phi);
    let n = bsdf.bsdf_data.n_basis;

    if j >= n {
        return BsdfSolarResult {
            transmittance: 0.0,
            reflectance_front: 0.0,
            absorptance: 1.0,
            transmitted_distribution: vec![],
        };
    }

    // Transmitted flux distribution and hemispheric transmittance
    let mut transmitted_dist = vec![0.0; n];
    let mut t_hem = 0.0;
    for i in 0..n.min(bsdf.bsdf_data.transmission_matrix.len()) {
        let row = &bsdf.bsdf_data.transmission_matrix[i];
        let val = if j < row.len() { row[j] } else { 0.0 };
        transmitted_dist[i] = val;
        t_hem += val;
    }

    // Front reflectance
    let mut r_hem = 0.0;
    for i in 0..n.min(bsdf.bsdf_data.reflection_front_matrix.len()) {
        let row = &bsdf.bsdf_data.reflection_front_matrix[i];
        if j < row.len() {
            r_hem += row[j];
        }
    }

    t_hem = t_hem.clamp(0.0, 1.0);
    r_hem = r_hem.clamp(0.0, 1.0 - t_hem);
    let absorptance = (1.0 - t_hem - r_hem).max(0.0);

    BsdfSolarResult {
        transmittance: t_hem,
        reflectance_front: r_hem,
        absorptance,
        transmitted_distribution: transmitted_dist,
    }
}

// ─── Switchable Glazing ──────────────────────────────────────────────

/// Type of switchable glazing technology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchType {
    /// Electrically controlled tinting (voltage-driven).
    Electrochromic,
    /// Temperature-responsive tinting (passive, no control signal).
    Thermochromic,
}

/// Optical states for switchable glazing.
#[derive(Debug, Clone)]
pub struct SwitchableGlazing {
    /// Fully clear (bleached) state optical properties.
    pub clear_state: LayerOptics,
    /// Fully tinted (coloured) state optical properties.
    pub tinted_state: LayerOptics,
    /// Switching technology type.
    pub switch_type: SwitchType,
}

/// Control parameters for switchable glazing.
#[derive(Debug, Clone)]
pub struct SwitchControl {
    /// Schedule-driven switching fraction (0-1). `None` means use sensor-based control.
    pub schedule_value: Option<f64>,
    /// Solar irradiance threshold for switching (W/m2). Only for electrochromic.
    pub solar_threshold: f64,
    /// Surface temperature threshold for switching (K). Only for thermochromic.
    pub temperature_threshold: f64,
}

impl Default for SwitchControl {
    fn default() -> Self {
        Self {
            schedule_value: None,
            solar_threshold: 300.0,
            temperature_threshold: 303.15, // 30 degC
        }
    }
}

/// Calculate the switching factor (0 = fully clear, 1 = fully tinted).
///
/// For electrochromic: uses schedule if provided, otherwise ramps linearly
/// from 0 at half the solar threshold to 1 at twice the threshold.
///
/// For thermochromic: ramps linearly from 0 at (threshold - 10K) to 1
/// at (threshold + 10K), based on surface temperature.
pub fn calc_switching_factor(
    control: &SwitchControl,
    switch_type: SwitchType,
    incident_solar: f64,
    surface_temp: f64,
) -> f64 {
    match switch_type {
        SwitchType::Electrochromic => {
            if let Some(sched) = control.schedule_value {
                return sched.clamp(0.0, 1.0);
            }
            // Sensor-based: ramp around solar threshold
            let lo = control.solar_threshold * 0.5;
            let hi = control.solar_threshold * 2.0;
            if incident_solar <= lo {
                0.0
            } else if incident_solar >= hi {
                1.0
            } else {
                (incident_solar - lo) / (hi - lo)
            }
        }
        SwitchType::Thermochromic => {
            // Temperature-based ramp (schedule ignored for thermochromic)
            let lo = control.temperature_threshold - 10.0;
            let hi = control.temperature_threshold + 10.0;
            if surface_temp <= lo {
                0.0
            } else if surface_temp >= hi {
                1.0
            } else {
                (surface_temp - lo) / (hi - lo)
            }
        }
    }
}

/// Linearly interpolate optical properties between clear and tinted states.
///
/// `factor` = 0 gives `clear`, `factor` = 1 gives `tinted`.
pub fn interpolate_states(clear: &LayerOptics, tinted: &LayerOptics, factor: f64) -> LayerOptics {
    let f = factor.clamp(0.0, 1.0);
    let lerp = |a: f64, b: f64| a + f * (b - a);

    LayerOptics {
        transmittance_solar: lerp(clear.transmittance_solar, tinted.transmittance_solar),
        reflectance_solar_front: lerp(clear.reflectance_solar_front, tinted.reflectance_solar_front),
        reflectance_solar_back: lerp(clear.reflectance_solar_back, tinted.reflectance_solar_back),
        transmittance_visible: lerp(clear.transmittance_visible, tinted.transmittance_visible),
        reflectance_visible_front: lerp(
            clear.reflectance_visible_front,
            tinted.reflectance_visible_front,
        ),
        reflectance_visible_back: lerp(
            clear.reflectance_visible_back,
            tinted.reflectance_visible_back,
        ),
        absorptance_solar: lerp(clear.absorptance_solar, tinted.absorptance_solar),
    }
}

// ─── Complex Fenestration State ──────────────────────────────────────

/// A complex fenestration with multiple discrete optical states.
///
/// Examples: blinds at different slat angles, multi-position roller shade,
/// BSDF glazing with multiple tint levels.
#[derive(Debug, Clone)]
pub struct ComplexFenestrationState {
    /// Available optical states, indexed 0..N-1.
    pub states: Vec<LayerOptics>,
    /// Currently active state index.
    pub active_state_index: usize,
}

impl ComplexFenestrationState {
    /// Get the currently active optical state.
    pub fn active_state(&self) -> &LayerOptics {
        &self.states[self.active_state_index.min(self.states.len().saturating_sub(1))]
    }
}

/// Control parameters for complex fenestration state selection.
#[derive(Debug, Clone)]
pub struct FenestrationController {
    /// Solar irradiance thresholds for switching between states (W/m2).
    /// Length should be `states.len() - 1`. States activate when solar exceeds
    /// the threshold at that index.
    pub solar_thresholds: Vec<f64>,
    /// Glare index thresholds. Same length and logic as solar thresholds.
    pub glare_thresholds: Vec<f64>,
    /// Schedule override: if `Some(idx)`, forces that state regardless of sensors.
    pub schedule_override: Option<usize>,
}

impl Default for FenestrationController {
    fn default() -> Self {
        Self {
            solar_thresholds: vec![],
            glare_thresholds: vec![],
            schedule_override: None,
        }
    }
}

/// Select the active state for a complex fenestration.
///
/// Priority: schedule override > glare thresholds > solar thresholds.
/// Returns the zero-based state index.
///
/// For threshold-based selection, the highest state whose threshold is exceeded
/// is chosen. State 0 is the default (no threshold exceeded).
pub fn select_state(
    controller: &FenestrationController,
    n_states: usize,
    solar_incident: f64,
    glare_index: f64,
) -> usize {
    if n_states == 0 {
        return 0;
    }

    // Schedule override
    if let Some(idx) = controller.schedule_override {
        return idx.min(n_states - 1);
    }

    // Glare-based selection (highest priority sensor)
    for i in (0..controller.glare_thresholds.len().min(n_states - 1)).rev() {
        if glare_index >= controller.glare_thresholds[i] {
            return i + 1;
        }
    }

    // Solar-based selection
    for i in (0..controller.solar_thresholds.len().min(n_states - 1)).rev() {
        if solar_incident >= controller.solar_thresholds[i] {
            return i + 1;
        }
    }

    // Default: state 0 (clear / open)
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Helper Constructors ─────────────────────────────────────────

    /// Single clear glass equivalent layer.
    fn clear_eql() -> EquivalentLayer {
        EquivalentLayer {
            solar_transmittance: 0.77,
            solar_reflectance_front: 0.07,
            solar_reflectance_back: 0.07,
            ir_transmittance: 0.0,
            ir_emissivity_front: 0.84,
            ir_emissivity_back: 0.84,
            thermal_resistance: 0.003 / 0.9, // 3 mm glass, k = 0.9
        }
    }

    /// Low-e coating equivalent layer.
    fn low_e_eql() -> EquivalentLayer {
        EquivalentLayer {
            solar_transmittance: 0.45,
            solar_reflectance_front: 0.35,
            solar_reflectance_back: 0.30,
            ir_transmittance: 0.0,
            ir_emissivity_front: 0.15,
            ir_emissivity_back: 0.84,
            thermal_resistance: 0.006 / 0.9,
        }
    }

    /// Shade equivalent layer (roller shade).
    fn shade_eql() -> EquivalentLayer {
        EquivalentLayer {
            solar_transmittance: 0.10,
            solar_reflectance_front: 0.60,
            solar_reflectance_back: 0.60,
            ir_transmittance: 0.02,
            ir_emissivity_front: 0.90,
            ir_emissivity_back: 0.90,
            thermal_resistance: 0.001,
        }
    }

    fn clear_optics() -> LayerOptics {
        LayerOptics {
            transmittance_solar: 0.77,
            reflectance_solar_front: 0.07,
            reflectance_solar_back: 0.07,
            transmittance_visible: 0.88,
            reflectance_visible_front: 0.08,
            reflectance_visible_back: 0.08,
            absorptance_solar: 0.16,
        }
    }

    fn tinted_optics() -> LayerOptics {
        LayerOptics {
            transmittance_solar: 0.12,
            reflectance_solar_front: 0.15,
            reflectance_solar_back: 0.15,
            transmittance_visible: 0.15,
            reflectance_visible_front: 0.12,
            reflectance_visible_back: 0.12,
            absorptance_solar: 0.73,
        }
    }

    /// Create a simple BSDF glazing for testing with a given normal-incidence
    /// transmittance. Uses 145 Klems patches; places all transmission in the
    /// normal (patch 0) column and reflectance uniformly.
    fn simple_bsdf(t_normal: f64, r_normal: f64) -> BsdfGlazing {
        let patches = klems_full_basis();
        let n = patches.len();

        // Build transmission matrix: column j gets T spread into outgoing patches
        // For simplicity, all transmitted flux goes to matching outgoing patch (specular)
        let mut t_mat = vec![vec![0.0; n]; n];
        let mut rf_mat = vec![vec![0.0; n]; n];
        let rb_mat = vec![vec![0.0; n]; n];

        for j in 0..n {
            // Transmittance decreases with incidence angle (Fresnel-like)
            let theta_j = patches[j].theta_center;
            let cos_j = theta_j.cos().max(0.0);
            let t_j = t_normal * cos_j * cos_j; // quadratic falloff
            let r_j = r_normal + (1.0 - r_normal) * (1.0 - cos_j).powi(3); // Schlick-like

            // Specular: transmitted into same-index outgoing patch
            t_mat[j][j] = t_j.clamp(0.0, 1.0);
            rf_mat[j][j] = r_j.clamp(0.0, 1.0 - t_j);
        }

        BsdfGlazing {
            bsdf_data: BsdfMatrix {
                n_basis: n,
                transmission_matrix: t_mat,
                reflection_front_matrix: rf_mat,
                reflection_back_matrix: rb_mat,
            },
            patches,
        }
    }

    // ─── EQL Tests ───────────────────────────────────────────────────

    #[test]
    fn eql_single_layer_passthrough() {
        let assembly = EqlAssembly {
            layers: vec![clear_eql()],
        };
        let result = solve_eql_system(&assembly, 500.0, 273.15, 293.15);

        assert_eq!(result.layer_temperatures.len(), 1);
        assert!(
            (result.total_transmittance - 0.77).abs() < 0.01,
            "T = {}",
            result.total_transmittance
        );
        assert!(
            (result.total_reflectance - 0.07).abs() < 0.01,
            "R = {}",
            result.total_reflectance
        );
        // T + R + A = 1
        let sum =
            result.total_transmittance + result.total_reflectance + result.layer_absorptances[0];
        assert!(
            (sum - 1.0).abs() < 0.02,
            "T+R+A = {} (should be 1.0)",
            sum
        );
    }

    #[test]
    fn eql_single_layer_temperature_between_bounds() {
        let assembly = EqlAssembly {
            layers: vec![clear_eql()],
        };
        let t_out = 260.0;
        let t_in = 295.0;
        let result = solve_eql_system(&assembly, 300.0, t_out, t_in);

        let t_layer = result.layer_temperatures[0];
        // Layer temperature should be between outdoor and indoor (plus some solar heating)
        assert!(
            t_layer > t_out - 5.0 && t_layer < t_in + 30.0,
            "T_layer = {} K",
            t_layer
        );
    }

    #[test]
    fn eql_multi_layer_lower_transmittance() {
        let single = EqlAssembly {
            layers: vec![clear_eql()],
        };
        let double = EqlAssembly {
            layers: vec![clear_eql(), clear_eql()],
        };
        let r_single = solve_eql_system(&single, 500.0, 273.15, 293.15);
        let r_double = solve_eql_system(&double, 500.0, 273.15, 293.15);

        assert!(
            r_double.total_transmittance < r_single.total_transmittance,
            "double T={} should be < single T={}",
            r_double.total_transmittance,
            r_single.total_transmittance
        );
    }

    #[test]
    fn eql_multi_layer_energy_conservation() {
        let assembly = EqlAssembly {
            layers: vec![clear_eql(), low_e_eql()],
        };
        let result = solve_eql_system(&assembly, 500.0, 273.15, 293.15);

        let sum_a: f64 = result.layer_absorptances.iter().sum();
        let sum = result.total_transmittance + result.total_reflectance + sum_a;
        assert!(
            (sum - 1.0).abs() < 0.05,
            "T+R+A = {} (T={}, R={}, A={})",
            sum,
            result.total_transmittance,
            result.total_reflectance,
            sum_a
        );
    }

    #[test]
    fn eql_shade_reduces_solar_gain() {
        let glass_only = EqlAssembly {
            layers: vec![clear_eql()],
        };
        let glass_shade = EqlAssembly {
            layers: vec![clear_eql(), shade_eql()],
        };

        let r1 = solve_eql_system(&glass_only, 500.0, 273.15, 293.15);
        let r2 = solve_eql_system(&glass_shade, 500.0, 273.15, 293.15);

        assert!(
            r2.total_transmittance < r1.total_transmittance,
            "shaded T={} should be < unshaded T={}",
            r2.total_transmittance,
            r1.total_transmittance
        );
    }

    #[test]
    fn eql_empty_assembly() {
        let assembly = EqlAssembly { layers: vec![] };
        let result = solve_eql_system(&assembly, 500.0, 273.15, 293.15);
        assert!((result.total_transmittance - 1.0).abs() < 1e-10);
        assert!(result.layer_temperatures.is_empty());
    }

    #[test]
    fn eql_solar_increases_heat_gain() {
        let assembly = EqlAssembly {
            layers: vec![clear_eql()],
        };
        let r_no_solar = solve_eql_system(&assembly, 0.0, 273.15, 293.15);
        let r_solar = solve_eql_system(&assembly, 800.0, 273.15, 293.15);

        assert!(
            r_solar.heat_flow_in > r_no_solar.heat_flow_in,
            "solar heat_flow {} should exceed no-solar {}",
            r_solar.heat_flow_in,
            r_no_solar.heat_flow_in
        );
    }

    // ─── BSDF Tests ─────────────────────────────────────────────────

    #[test]
    fn klems_basis_has_145_patches() {
        let patches = klems_full_basis();
        assert_eq!(patches.len(), 145);
    }

    #[test]
    fn klems_solid_angles_sum_to_hemisphere() {
        let patches = klems_full_basis();
        let total: f64 = patches.iter().map(|p| p.solid_angle).sum();
        // Full hemisphere = 2*PI sr
        assert!(
            (total - 2.0 * PI).abs() < 0.01,
            "total solid angle = {} (expect ~{:.4})",
            total,
            2.0 * PI
        );
    }

    #[test]
    fn bsdf_normal_incidence_transmittance() {
        let bsdf = simple_bsdf(0.70, 0.10);
        let t = calc_bsdf_transmittance(&bsdf, 0.0, 0.0);
        // At normal incidence (theta=0), cos^2 = 1, so T should equal t_normal
        assert!(
            (t - 0.70).abs() < 0.05,
            "T_normal = {} (expect ~0.70)",
            t
        );
    }

    #[test]
    fn bsdf_grazing_angle_high_reflectance() {
        let bsdf = simple_bsdf(0.70, 0.10);
        let result_normal = calc_bsdf_solar_properties(&bsdf, 0.0, 0.0);
        let result_grazing = calc_bsdf_solar_properties(&bsdf, 80.0_f64.to_radians(), 0.0);

        // At grazing angle, transmittance should decrease
        assert!(
            result_grazing.transmittance < result_normal.transmittance,
            "grazing T={} should be < normal T={}",
            result_grazing.transmittance,
            result_normal.transmittance
        );
        // At grazing angle, reflectance should increase
        assert!(
            result_grazing.reflectance_front > result_normal.reflectance_front,
            "grazing R={} should be > normal R={}",
            result_grazing.reflectance_front,
            result_normal.reflectance_front
        );
    }

    #[test]
    fn bsdf_energy_conservation() {
        let bsdf = simple_bsdf(0.70, 0.10);
        let result = calc_bsdf_solar_properties(&bsdf, 30.0_f64.to_radians(), 0.0);

        let sum = result.transmittance + result.reflectance_front + result.absorptance;
        assert!(
            (sum - 1.0).abs() < 0.01,
            "T+R+A = {} (T={}, R={}, A={})",
            sum,
            result.transmittance,
            result.reflectance_front,
            result.absorptance
        );
    }

    #[test]
    fn bsdf_solar_result_has_distribution() {
        let bsdf = simple_bsdf(0.70, 0.10);
        let result = calc_bsdf_solar_properties(&bsdf, 0.0, 0.0);
        assert_eq!(result.transmitted_distribution.len(), 145);
        // At normal incidence, the specular patch (0) should carry the flux
        assert!(
            result.transmitted_distribution[0] > 0.5,
            "specular patch flux = {}",
            result.transmitted_distribution[0]
        );
    }

    // ─── Switchable Glazing Tests ────────────────────────────────────

    #[test]
    fn switchable_interpolation_at_zero() {
        let clear = clear_optics();
        let tinted = tinted_optics();
        let result = interpolate_states(&clear, &tinted, 0.0);
        assert!(
            (result.transmittance_solar - clear.transmittance_solar).abs() < 1e-10,
            "factor=0 should give clear state"
        );
    }

    #[test]
    fn switchable_interpolation_at_one() {
        let clear = clear_optics();
        let tinted = tinted_optics();
        let result = interpolate_states(&clear, &tinted, 1.0);
        assert!(
            (result.transmittance_solar - tinted.transmittance_solar).abs() < 1e-10,
            "factor=1 should give tinted state"
        );
    }

    #[test]
    fn switchable_interpolation_midpoint() {
        let clear = clear_optics();
        let tinted = tinted_optics();
        let result = interpolate_states(&clear, &tinted, 0.5);
        let expected_t = 0.5 * (clear.transmittance_solar + tinted.transmittance_solar);
        assert!(
            (result.transmittance_solar - expected_t).abs() < 1e-10,
            "T_mid = {} (expect {})",
            result.transmittance_solar,
            expected_t
        );
    }

    #[test]
    fn switchable_interpolation_clamps() {
        let clear = clear_optics();
        let tinted = tinted_optics();
        let below = interpolate_states(&clear, &tinted, -0.5);
        let above = interpolate_states(&clear, &tinted, 1.5);
        assert!(
            (below.transmittance_solar - clear.transmittance_solar).abs() < 1e-10,
            "factor < 0 should clamp to clear"
        );
        assert!(
            (above.transmittance_solar - tinted.transmittance_solar).abs() < 1e-10,
            "factor > 1 should clamp to tinted"
        );
    }

    #[test]
    fn electrochromic_schedule_override() {
        let control = SwitchControl {
            schedule_value: Some(0.75),
            solar_threshold: 300.0,
            ..Default::default()
        };
        let f = calc_switching_factor(&control, SwitchType::Electrochromic, 0.0, 293.15);
        assert!(
            (f - 0.75).abs() < 1e-10,
            "schedule should override: f = {}",
            f
        );
    }

    #[test]
    fn electrochromic_solar_below_threshold() {
        let control = SwitchControl {
            schedule_value: None,
            solar_threshold: 300.0,
            ..Default::default()
        };
        // Below 0.5 * 300 = 150 W/m2 → factor = 0
        let f = calc_switching_factor(&control, SwitchType::Electrochromic, 100.0, 293.15);
        assert!(f < 1e-10, "low solar should give f=0: f = {}", f);
    }

    #[test]
    fn electrochromic_solar_above_threshold() {
        let control = SwitchControl {
            schedule_value: None,
            solar_threshold: 300.0,
            ..Default::default()
        };
        // Above 2.0 * 300 = 600 W/m2 → factor = 1
        let f = calc_switching_factor(&control, SwitchType::Electrochromic, 700.0, 293.15);
        assert!((f - 1.0).abs() < 1e-10, "high solar should give f=1: f = {}", f);
    }

    #[test]
    fn electrochromic_solar_mid_range() {
        let control = SwitchControl {
            schedule_value: None,
            solar_threshold: 300.0,
            ..Default::default()
        };
        // At threshold (300 W/m2): lo=150, hi=600 → f = (300-150)/(600-150) = 1/3
        let f = calc_switching_factor(&control, SwitchType::Electrochromic, 300.0, 293.15);
        assert!(
            (f - 1.0 / 3.0).abs() < 0.01,
            "mid-range solar: f = {} (expect ~0.333)",
            f
        );
    }

    #[test]
    fn thermochromic_cold_surface() {
        let control = SwitchControl {
            schedule_value: None,
            temperature_threshold: 303.15, // 30 degC
            ..Default::default()
        };
        // Below threshold - 10 = 293.15 K → factor = 0
        let f = calc_switching_factor(&control, SwitchType::Thermochromic, 500.0, 290.0);
        assert!(f < 1e-10, "cold surface should give f=0: f = {}", f);
    }

    #[test]
    fn thermochromic_hot_surface() {
        let control = SwitchControl {
            schedule_value: None,
            temperature_threshold: 303.15,
            ..Default::default()
        };
        // Above threshold + 10 = 313.15 K → factor = 1
        let f = calc_switching_factor(&control, SwitchType::Thermochromic, 500.0, 320.0);
        assert!(
            (f - 1.0).abs() < 1e-10,
            "hot surface should give f=1: f = {}",
            f
        );
    }

    #[test]
    fn thermochromic_mid_temperature() {
        let control = SwitchControl {
            schedule_value: None,
            temperature_threshold: 303.15,
            ..Default::default()
        };
        // At threshold: lo=293.15, hi=313.15 → f = (303.15 - 293.15)/20 = 0.5
        let f = calc_switching_factor(&control, SwitchType::Thermochromic, 500.0, 303.15);
        assert!(
            (f - 0.5).abs() < 0.01,
            "at threshold: f = {} (expect 0.5)",
            f
        );
    }

    #[test]
    fn thermochromic_ignores_schedule() {
        let control = SwitchControl {
            schedule_value: Some(0.0), // schedule says clear
            temperature_threshold: 303.15,
            ..Default::default()
        };
        // Thermochromic should still tint based on temperature
        let f = calc_switching_factor(&control, SwitchType::Thermochromic, 0.0, 320.0);
        assert!(
            (f - 1.0).abs() < 1e-10,
            "thermochromic should ignore schedule: f = {}",
            f
        );
    }

    // ─── Complex Fenestration State Tests ────────────────────────────

    #[test]
    fn complex_state_default_selection() {
        let controller = FenestrationController::default();
        let idx = select_state(&controller, 3, 0.0, 0.0);
        assert_eq!(idx, 0, "default should be state 0");
    }

    #[test]
    fn complex_state_schedule_override() {
        let controller = FenestrationController {
            schedule_override: Some(2),
            solar_thresholds: vec![100.0, 300.0],
            ..Default::default()
        };
        let idx = select_state(&controller, 3, 0.0, 0.0);
        assert_eq!(idx, 2, "schedule should override sensors");
    }

    #[test]
    fn complex_state_solar_threshold() {
        let controller = FenestrationController {
            solar_thresholds: vec![200.0, 400.0],
            ..Default::default()
        };

        let idx_low = select_state(&controller, 3, 100.0, 0.0);
        assert_eq!(idx_low, 0, "below all thresholds");

        let idx_mid = select_state(&controller, 3, 250.0, 0.0);
        assert_eq!(idx_mid, 1, "above first threshold");

        let idx_high = select_state(&controller, 3, 500.0, 0.0);
        assert_eq!(idx_high, 2, "above both thresholds");
    }

    #[test]
    fn complex_state_glare_priority_over_solar() {
        let controller = FenestrationController {
            solar_thresholds: vec![200.0, 400.0],
            glare_thresholds: vec![0.4, 0.7],
            ..Default::default()
        };

        // Low solar but high glare → glare should win
        let idx = select_state(&controller, 3, 100.0, 0.8);
        assert_eq!(idx, 2, "glare should take priority over solar");
    }

    #[test]
    fn complex_state_active_state_accessor() {
        let states = vec![clear_optics(), tinted_optics()];
        let cfs = ComplexFenestrationState {
            states,
            active_state_index: 1,
        };
        assert!(
            (cfs.active_state().transmittance_solar - tinted_optics().transmittance_solar).abs()
                < 1e-10,
        );
    }

    #[test]
    fn complex_state_clamps_index() {
        let states = vec![clear_optics()];
        let cfs = ComplexFenestrationState {
            states,
            active_state_index: 99, // out of bounds
        };
        // Should clamp to last valid state
        assert!(
            (cfs.active_state().transmittance_solar - clear_optics().transmittance_solar).abs()
                < 1e-10,
        );
    }

    #[test]
    fn switchable_energy_conservation() {
        let clear = clear_optics();
        let tinted = tinted_optics();

        for &factor in &[0.0, 0.25, 0.5, 0.75, 1.0] {
            let result = interpolate_states(&clear, &tinted, factor);
            let sum =
                result.transmittance_solar + result.reflectance_solar_front + result.absorptance_solar;
            assert!(
                (sum - 1.0).abs() < 0.02,
                "factor={}: T+R+A = {} (T={}, R={}, A={})",
                factor,
                sum,
                result.transmittance_solar,
                result.reflectance_solar_front,
                result.absorptance_solar,
            );
        }
    }
}
