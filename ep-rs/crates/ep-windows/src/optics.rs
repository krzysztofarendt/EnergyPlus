//! Glazing optical property calculations.
//!
//! Computes angular-dependent transmittance and reflectance for single
//! and multi-pane glazing systems.

use ep_materials::GlassMaterial;

/// Optical properties of a glazing layer at a specific angle.
#[derive(Debug, Clone, Copy, Default)]
pub struct LayerOptics {
    pub transmittance_solar: f64,
    pub reflectance_solar_front: f64,
    pub reflectance_solar_back: f64,
    pub transmittance_visible: f64,
    pub reflectance_visible_front: f64,
    pub reflectance_visible_back: f64,
    pub absorptance_solar: f64,
}

/// Optical properties of a complete glazing system at a specific angle.
#[derive(Debug, Clone, Default)]
pub struct SystemOptics {
    pub transmittance_solar: f64,
    pub reflectance_solar_front: f64,
    pub reflectance_solar_back: f64,
    pub transmittance_visible: f64,
    pub reflectance_visible_front: f64,
    pub absorptance_solar_per_layer: Vec<f64>,
}

/// Compute angular-dependent optical properties for a single glass layer.
///
/// Uses the empirical polynomial fit from EnergyPlus (TransAndReflAtPhi).
/// At normal incidence, properties are the specified values.
/// At other angles, they degrade based on the cos(theta) relationship.
pub fn layer_optics_at_angle(glass: &GlassMaterial, cos_incidence: f64) -> LayerOptics {
    let cos_i = cos_incidence.clamp(0.0, 1.0);

    if cos_i < 0.001 {
        // Grazing incidence: everything is reflected
        return LayerOptics {
            transmittance_solar: 0.0,
            reflectance_solar_front: 1.0,
            reflectance_solar_back: 1.0,
            transmittance_visible: 0.0,
            reflectance_visible_front: 1.0,
            reflectance_visible_back: 1.0,
            absorptance_solar: 0.0,
        };
    }

    // Angular factor based on empirical fit
    // At normal incidence (cos_i=1): factor=1
    // At grazing angles: factor→0 for transmittance, →1 for reflectance
    let t_factor = angular_transmittance_factor(cos_i);
    let r_factor = angular_reflectance_factor(cos_i, glass.solar_reflectance_front);

    let t_sol = glass.solar_transmittance * t_factor * glass.dirt_correction_factor;
    let r_sol_f = 1.0 - t_sol - (1.0 - glass.solar_reflectance_front - glass.solar_transmittance) * t_factor;
    let r_sol_f = r_sol_f.max(glass.solar_reflectance_front * r_factor);
    let r_sol_b = r_sol_f; // Simplified: same front and back for thin glass

    let t_vis = glass.visible_transmittance * t_factor * glass.dirt_correction_factor;
    let r_vis_f = 1.0 - t_vis - (1.0 - glass.visible_reflectance_front - glass.visible_transmittance) * t_factor;
    let r_vis_f = r_vis_f.max(glass.visible_reflectance_front * r_factor);

    let a_sol = (1.0 - t_sol - r_sol_f).max(0.0);

    LayerOptics {
        transmittance_solar: t_sol,
        reflectance_solar_front: r_sol_f,
        reflectance_solar_back: r_sol_b,
        transmittance_visible: t_vis,
        reflectance_visible_front: r_vis_f,
        reflectance_visible_back: r_vis_f,
        absorptance_solar: a_sol,
    }
}

/// Angular transmittance factor: polynomial fit.
///
/// T(theta) / T(0) ≈ polynomial in cos(theta).
/// Based on generic glass angular behavior.
fn angular_transmittance_factor(cos_i: f64) -> f64 {
    // Polynomial approximation for typical glass
    // At cos_i=1 (normal): factor=1
    // At cos_i=0 (grazing): factor=0
    let x = cos_i;
    let factor = x * (0.0918 + x * (2.7709 + x * (-4.4284 + x * (2.5667))));
    factor.clamp(0.0, 1.0)
}

/// Angular reflectance factor.
fn angular_reflectance_factor(cos_i: f64, r_normal: f64) -> f64 {
    // At normal incidence: factor=1
    // At grazing: factor increases (more reflection)
    let x = cos_i;
    let factor = 1.0 + (1.0 - x) * (1.0 - x) * (1.0 / r_normal.max(0.01) - 1.0) * 0.3;
    factor.clamp(1.0, 1.0 / r_normal.max(0.01))
}

/// Compute system optical properties for a multi-pane glazing system.
///
/// Uses the net radiation method for combining multiple glazing layers,
/// accounting for inter-reflections between panes.
pub fn system_optics(layers: &[LayerOptics]) -> SystemOptics {
    if layers.is_empty() {
        return SystemOptics::default();
    }

    if layers.len() == 1 {
        return SystemOptics {
            transmittance_solar: layers[0].transmittance_solar,
            reflectance_solar_front: layers[0].reflectance_solar_front,
            reflectance_solar_back: layers[0].reflectance_solar_back,
            transmittance_visible: layers[0].transmittance_visible,
            reflectance_visible_front: layers[0].reflectance_visible_front,
            absorptance_solar_per_layer: vec![layers[0].absorptance_solar],
        };
    }

    // Two-pane system using the standard inter-reflection formula
    if layers.len() == 2 {
        return two_pane_system(&layers[0], &layers[1]);
    }

    // For 3+ panes, combine iteratively from outside in
    let mut combined = two_pane_system(&layers[0], &layers[1]);
    let mut abs_layers = combined.absorptance_solar_per_layer.clone();

    for i in 2..layers.len() {
        let outer = LayerOptics {
            transmittance_solar: combined.transmittance_solar,
            reflectance_solar_front: combined.reflectance_solar_front,
            reflectance_solar_back: combined.reflectance_solar_back,
            transmittance_visible: combined.transmittance_visible,
            reflectance_visible_front: combined.reflectance_visible_front,
            reflectance_visible_back: combined.reflectance_visible_front,
            absorptance_solar: 1.0 - combined.transmittance_solar - combined.reflectance_solar_front,
        };
        combined = two_pane_system(&outer, &layers[i]);
        abs_layers.push(combined.absorptance_solar_per_layer.last().copied().unwrap_or(0.0));
    }

    combined.absorptance_solar_per_layer = abs_layers;
    combined
}

/// Combine two glazing layers accounting for inter-reflections.
fn two_pane_system(outer: &LayerOptics, inner: &LayerOptics) -> SystemOptics {
    // Inter-reflection factor
    let denom = 1.0 - outer.reflectance_solar_back * inner.reflectance_solar_front;
    let denom = denom.max(1e-10);

    let t_sol = outer.transmittance_solar * inner.transmittance_solar / denom;
    let r_sol_f = outer.reflectance_solar_front
        + outer.transmittance_solar * outer.transmittance_solar * inner.reflectance_solar_front / denom;

    let r_sol_b = inner.reflectance_solar_back
        + inner.transmittance_solar * inner.transmittance_solar * outer.reflectance_solar_back / denom;

    // Visible
    let denom_vis = 1.0 - outer.reflectance_visible_back * inner.reflectance_visible_front;
    let denom_vis = denom_vis.max(1e-10);
    let t_vis = outer.transmittance_visible * inner.transmittance_visible / denom_vis;
    let r_vis_f = outer.reflectance_visible_front
        + outer.transmittance_visible * outer.transmittance_visible * inner.reflectance_visible_front / denom_vis;

    // Layer absorptances
    let a_outer = outer.absorptance_solar
        + outer.transmittance_solar * inner.reflectance_solar_front * outer.absorptance_solar / denom;
    let a_inner = outer.transmittance_solar * inner.absorptance_solar / denom;

    SystemOptics {
        transmittance_solar: t_sol.max(0.0),
        reflectance_solar_front: r_sol_f.max(0.0),
        reflectance_solar_back: r_sol_b.max(0.0),
        transmittance_visible: t_vis.max(0.0),
        reflectance_visible_front: r_vis_f.max(0.0),
        absorptance_solar_per_layer: vec![a_outer.max(0.0), a_inner.max(0.0)],
    }
}

/// Calculate SHGC (Solar Heat Gain Coefficient) from optical properties.
///
/// SHGC = T_solar + sum(A_i * N_i)
/// where N_i is the inward-flowing fraction of absorbed solar in layer i.
///
/// Simplified: N_i ≈ (R_inside / R_total) for the i-th layer.
pub fn calculate_shgc(
    system: &SystemOptics,
    u_value: f64,
    h_outside: f64,
    h_inside: f64,
) -> f64 {
    if system.absorptance_solar_per_layer.is_empty() {
        return system.transmittance_solar;
    }

    let n_layers = system.absorptance_solar_per_layer.len();
    let r_total = if u_value > 0.0 { 1.0 / u_value } else { 1.0 };
    let r_inside = if h_inside > 0.0 { 1.0 / h_inside } else { 0.0 };
    let r_outside = if h_outside > 0.0 { 1.0 / h_outside } else { 0.0 };

    let mut shgc = system.transmittance_solar;
    for (i, &abs_i) in system.absorptance_solar_per_layer.iter().enumerate() {
        // Inward-flowing fraction: approximate by position in wall
        let fraction_inside = if n_layers == 1 {
            r_inside / r_total
        } else {
            // Linearly interpolate between outside and inside
            let pos = (i as f64 + 0.5) / n_layers as f64;
            let r_to_inside = r_inside + (r_total - r_inside - r_outside) * (1.0 - pos);
            r_to_inside / r_total
        };
        shgc += abs_i * fraction_inside;
    }

    shgc.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_glass() -> GlassMaterial {
        GlassMaterial {
            name: "ClearGlass".into(),
            solar_transmittance: 0.775,
            solar_reflectance_front: 0.071,
            solar_reflectance_back: 0.071,
            visible_transmittance: 0.881,
            visible_reflectance_front: 0.080,
            visible_reflectance_back: 0.080,
            ..Default::default()
        }
    }

    #[test]
    fn normal_incidence_single_pane() {
        let glass = clear_glass();
        let optics = layer_optics_at_angle(&glass, 1.0);

        // At normal incidence, transmittance should be close to nominal
        assert!(
            (optics.transmittance_solar - 0.775).abs() < 0.02,
            "t_sol={}",
            optics.transmittance_solar
        );
        assert!(optics.absorptance_solar > 0.0);
        // Conservation: T + R + A = 1
        let sum = optics.transmittance_solar + optics.reflectance_solar_front + optics.absorptance_solar;
        assert!((sum - 1.0).abs() < 0.01, "T+R+A={sum}");
    }

    #[test]
    fn grazing_incidence() {
        let glass = clear_glass();
        let optics = layer_optics_at_angle(&glass, 0.0);

        assert!(optics.transmittance_solar < 0.01);
        assert!(optics.reflectance_solar_front > 0.9);
    }

    #[test]
    fn oblique_incidence() {
        let glass = clear_glass();
        // 60 degree incidence
        let optics = layer_optics_at_angle(&glass, 0.5);

        // Transmittance should be reduced at oblique angles
        assert!(optics.transmittance_solar < 0.775);
        assert!(optics.transmittance_solar > 0.2);
    }

    #[test]
    fn single_pane_system() {
        let glass = clear_glass();
        let layer = layer_optics_at_angle(&glass, 1.0);
        let sys = system_optics(&[layer]);

        assert!((sys.transmittance_solar - layer.transmittance_solar).abs() < 1e-10);
        assert_eq!(sys.absorptance_solar_per_layer.len(), 1);
    }

    #[test]
    fn double_pane_system() {
        let glass = clear_glass();
        let layer = layer_optics_at_angle(&glass, 1.0);
        let sys = system_optics(&[layer, layer]);

        // Double pane transmittance should be less than single pane
        assert!(sys.transmittance_solar < layer.transmittance_solar);
        assert!(sys.transmittance_solar > 0.4); // But not too low for clear glass
        assert_eq!(sys.absorptance_solar_per_layer.len(), 2);

        // Conservation
        let total = sys.transmittance_solar
            + sys.reflectance_solar_front
            + sys.absorptance_solar_per_layer.iter().sum::<f64>();
        assert!((total - 1.0).abs() < 0.05, "total={total}");
    }

    #[test]
    fn shgc_single_pane() {
        let glass = clear_glass();
        let layer = layer_optics_at_angle(&glass, 1.0);
        let sys = system_optics(&[layer]);

        let shgc = calculate_shgc(&sys, 5.8, 23.0, 8.29);
        // SHGC for clear single pane should be around 0.82-0.86
        assert!(shgc > 0.75 && shgc < 0.95, "SHGC={shgc}");
    }
}
