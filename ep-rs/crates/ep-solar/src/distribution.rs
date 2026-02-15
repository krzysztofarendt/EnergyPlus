//! Solar distribution to interior surfaces.
//!
//! Distributes transmitted solar radiation through windows onto interior zone
//! surfaces, and computes absorbed solar on exterior opaque surfaces.
//! Follows EnergyPlus conventions for beam and diffuse distribution.

/// Result of solar distribution for a zone.
#[derive(Debug, Clone, Default)]
pub struct SolarDistributionResult {
    /// Solar absorbed per exterior opaque surface (W).
    pub surface_absorbed_exterior: Vec<f64>,
    /// Solar absorbed per interior surface from transmitted beam + diffuse (W).
    pub surface_absorbed_interior: Vec<f64>,
    /// Beam solar transmitted per window (W).
    pub transmitted_beam: Vec<f64>,
    /// Diffuse solar transmitted per window (W).
    pub transmitted_diffuse: Vec<f64>,
    /// Total beam solar entering the zone (W).
    pub zone_beam_solar: f64,
    /// Total diffuse solar entering the zone (W).
    pub zone_diff_solar: f64,
}

/// Properties of a window for solar distribution.
#[derive(Debug, Clone)]
pub struct WindowSolar {
    /// Window area (m2).
    pub area: f64,
    /// Beam transmittance at current angle of incidence.
    pub beam_transmittance: f64,
    /// Diffuse hemispherical transmittance.
    pub diffuse_transmittance: f64,
    /// Beam solar incident on window (W/m2).
    pub incident_beam: f64,
    /// Diffuse solar incident on window (W/m2, sky + ground-reflected).
    pub incident_diffuse: f64,
    /// Sunlit fraction of this window (0 to 1).
    pub sunlit_fraction: f64,
    /// Solar absorbed per glazing layer (W/m2), to be applied as heat source.
    pub absorbed_per_layer: Vec<f64>,
}

/// Properties of an exterior opaque surface for solar absorption.
#[derive(Debug, Clone)]
pub struct OpaqueSurfaceSolar {
    /// Surface area (m2).
    pub area: f64,
    /// Solar absorptance.
    pub absorptance: f64,
    /// Beam solar incident (W/m2).
    pub incident_beam: f64,
    /// Diffuse solar incident (W/m2, sky + ground-reflected).
    pub incident_diffuse: f64,
    /// Sunlit fraction (0 to 1).
    pub sunlit_fraction: f64,
}

/// Properties of an interior surface for receiving transmitted solar.
#[derive(Debug, Clone)]
pub struct InteriorSurfaceSolar {
    /// Surface area (m2).
    pub area: f64,
    /// Solar absorptance.
    pub absorptance: f64,
}

/// Interior solar distribution method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InteriorDistribution {
    /// Uniform: all transmitted solar spread equally over all zone surfaces.
    #[default]
    Uniform,
    /// FullInteriorAndExterior: beam to floor first, remainder diffuse to all.
    FullInteriorAndExterior,
    /// FullExterior: beam to floor, diffuse to all (same as full interior/exterior).
    FullExterior,
}

/// Distribute solar radiation within a zone.
///
/// Computes:
/// 1. Transmitted beam and diffuse solar through each window
/// 2. Absorbed solar on exterior opaque surfaces
/// 3. Distribution of transmitted solar to interior surfaces
///
/// # Arguments
/// * `windows` - Solar properties for each window in the zone
/// * `opaque_exterior` - Solar properties for each exterior opaque surface
/// * `interior_surfaces` - Properties for all interior zone surfaces (for receiving transmitted)
/// * `floor_indices` - Indices into `interior_surfaces` that are floor surfaces
/// * `distribution` - Interior distribution method
pub fn distribute_solar(
    windows: &[WindowSolar],
    opaque_exterior: &[OpaqueSurfaceSolar],
    interior_surfaces: &[InteriorSurfaceSolar],
    floor_indices: &[usize],
    distribution: InteriorDistribution,
) -> SolarDistributionResult {
    let n_opaque = opaque_exterior.len();
    let n_interior = interior_surfaces.len();
    let n_windows = windows.len();

    let mut result = SolarDistributionResult {
        surface_absorbed_exterior: vec![0.0; n_opaque],
        surface_absorbed_interior: vec![0.0; n_interior],
        transmitted_beam: vec![0.0; n_windows],
        transmitted_diffuse: vec![0.0; n_windows],
        zone_beam_solar: 0.0,
        zone_diff_solar: 0.0,
    };

    // 1. Compute transmitted solar through each window
    for (i, w) in windows.iter().enumerate() {
        let beam = w.incident_beam * w.beam_transmittance * w.area * w.sunlit_fraction;
        let diff = w.incident_diffuse * w.diffuse_transmittance * w.area;

        result.transmitted_beam[i] = beam;
        result.transmitted_diffuse[i] = diff;
        result.zone_beam_solar += beam;
        result.zone_diff_solar += diff;
    }

    // 2. Compute absorbed solar on exterior opaque surfaces
    for (i, s) in opaque_exterior.iter().enumerate() {
        let beam_absorbed = s.incident_beam * s.sunlit_fraction * s.absorptance * s.area;
        let diff_absorbed = s.incident_diffuse * s.absorptance * s.area;
        result.surface_absorbed_exterior[i] = beam_absorbed + diff_absorbed;
    }

    // 3. Distribute transmitted solar to interior surfaces
    let total_transmitted = result.zone_beam_solar + result.zone_diff_solar;
    if total_transmitted <= 0.0 || n_interior == 0 {
        return result;
    }

    match distribution {
        InteriorDistribution::Uniform => {
            // Distribute all transmitted solar uniformly by area
            distribute_uniform(
                &mut result.surface_absorbed_interior,
                interior_surfaces,
                total_transmitted,
            );
        }
        InteriorDistribution::FullInteriorAndExterior
        | InteriorDistribution::FullExterior => {
            // Beam goes to floor first, remainder + diffuse spread to all
            distribute_beam_to_floor(
                &mut result.surface_absorbed_interior,
                interior_surfaces,
                floor_indices,
                result.zone_beam_solar,
                result.zone_diff_solar,
            );
        }
    }

    result
}

/// Distribute solar uniformly across all interior surfaces weighted by area.
fn distribute_uniform(
    absorbed: &mut [f64],
    surfaces: &[InteriorSurfaceSolar],
    total_solar: f64,
) {
    let total_area: f64 = surfaces.iter().map(|s| s.area).sum();
    if total_area <= 0.0 {
        return;
    }
    for (i, s) in surfaces.iter().enumerate() {
        let fraction = s.area / total_area;
        absorbed[i] = total_solar * fraction * s.absorptance;
    }
}

/// Distribute beam to floor surfaces first, remainder + diffuse to all surfaces.
fn distribute_beam_to_floor(
    absorbed: &mut [f64],
    surfaces: &[InteriorSurfaceSolar],
    floor_indices: &[usize],
    beam_solar: f64,
    diff_solar: f64,
) {
    // Compute floor area for beam distribution
    let floor_area: f64 = floor_indices
        .iter()
        .filter_map(|&i| surfaces.get(i))
        .map(|s| s.area)
        .sum();

    let mut beam_remaining = beam_solar;

    if floor_area > 0.0 {
        // Distribute beam to floor surfaces by area fraction
        for &fi in floor_indices {
            if let Some(s) = surfaces.get(fi) {
                let fraction = s.area / floor_area;
                let beam_to_floor = beam_solar * fraction;
                let floor_absorbed = beam_to_floor * s.absorptance;
                absorbed[fi] += floor_absorbed;
                beam_remaining -= floor_absorbed;
            }
        }
    }

    // Remaining beam (reflected from floor) + diffuse → distribute uniformly
    let diffuse_total = beam_remaining.max(0.0) + diff_solar;
    if diffuse_total <= 0.0 {
        return;
    }

    let total_area: f64 = surfaces.iter().map(|s| s.area).sum();
    if total_area <= 0.0 {
        return;
    }

    for (i, s) in surfaces.iter().enumerate() {
        let fraction = s.area / total_area;
        absorbed[i] += diffuse_total * fraction * s.absorptance;
    }
}

/// Compute the total zone solar heat gain (W) from all sources.
pub fn total_zone_solar_gain(result: &SolarDistributionResult) -> f64 {
    result.surface_absorbed_interior.iter().sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transmitted_beam_calculation() {
        let windows = vec![WindowSolar {
            area: 2.0,
            beam_transmittance: 0.7,
            diffuse_transmittance: 0.6,
            incident_beam: 800.0,
            incident_diffuse: 100.0,
            sunlit_fraction: 1.0,
            absorbed_per_layer: vec![],
        }];
        let result = distribute_solar(&windows, &[], &[], &[], InteriorDistribution::Uniform);
        // beam = 800 * 0.7 * 2.0 * 1.0 = 1120 W
        assert!((result.transmitted_beam[0] - 1120.0).abs() < 0.01);
        // diff = 100 * 0.6 * 2.0 = 120 W
        assert!((result.transmitted_diffuse[0] - 120.0).abs() < 0.01);
        assert!((result.zone_beam_solar - 1120.0).abs() < 0.01);
        assert!((result.zone_diff_solar - 120.0).abs() < 0.01);
    }

    #[test]
    fn transmitted_beam_partial_sunlit() {
        let windows = vec![WindowSolar {
            area: 2.0,
            beam_transmittance: 0.7,
            diffuse_transmittance: 0.6,
            incident_beam: 800.0,
            incident_diffuse: 100.0,
            sunlit_fraction: 0.5, // half shaded
            absorbed_per_layer: vec![],
        }];
        let result = distribute_solar(&windows, &[], &[], &[], InteriorDistribution::Uniform);
        // beam = 800 * 0.7 * 2.0 * 0.5 = 560 W
        assert!((result.transmitted_beam[0] - 560.0).abs() < 0.01);
        // diffuse is not affected by sunlit fraction
        assert!((result.transmitted_diffuse[0] - 120.0).abs() < 0.01);
    }

    #[test]
    fn exterior_opaque_absorbed() {
        let opaque = vec![OpaqueSurfaceSolar {
            area: 20.0,
            absorptance: 0.6,
            incident_beam: 500.0,
            incident_diffuse: 80.0,
            sunlit_fraction: 0.8,
        }];
        let result = distribute_solar(&[], &opaque, &[], &[], InteriorDistribution::Uniform);
        // beam_abs = 500 * 0.8 * 0.6 * 20 = 4800 W
        // diff_abs = 80 * 0.6 * 20 = 960 W
        let expected = 4800.0 + 960.0;
        assert!(
            (result.surface_absorbed_exterior[0] - expected).abs() < 0.1,
            "absorbed={}",
            result.surface_absorbed_exterior[0]
        );
    }

    #[test]
    fn uniform_distribution() {
        let windows = vec![WindowSolar {
            area: 3.0,
            beam_transmittance: 0.8,
            diffuse_transmittance: 0.7,
            incident_beam: 600.0,
            incident_diffuse: 100.0,
            sunlit_fraction: 1.0,
            absorbed_per_layer: vec![],
        }];
        // Two interior surfaces: 10 m2 and 20 m2
        let interior = vec![
            InteriorSurfaceSolar { area: 10.0, absorptance: 0.5 },
            InteriorSurfaceSolar { area: 20.0, absorptance: 0.5 },
        ];
        let result = distribute_solar(&windows, &[], &interior, &[], InteriorDistribution::Uniform);

        let total = result.zone_beam_solar + result.zone_diff_solar;
        // Surface 0 gets 10/30 fraction, surface 1 gets 20/30
        assert!(
            (result.surface_absorbed_interior[0] - total * (10.0 / 30.0) * 0.5).abs() < 0.1,
            "abs0={}",
            result.surface_absorbed_interior[0]
        );
        assert!(
            (result.surface_absorbed_interior[1] - total * (20.0 / 30.0) * 0.5).abs() < 0.1,
            "abs1={}",
            result.surface_absorbed_interior[1]
        );
    }

    #[test]
    fn beam_to_floor_distribution() {
        let windows = vec![WindowSolar {
            area: 2.0,
            beam_transmittance: 0.8,
            diffuse_transmittance: 0.6,
            incident_beam: 800.0,
            incident_diffuse: 0.0, // no diffuse for simpler check
            sunlit_fraction: 1.0,
            absorbed_per_layer: vec![],
        }];
        // Floor (index 0) and wall (index 1)
        let interior = vec![
            InteriorSurfaceSolar { area: 10.0, absorptance: 0.8 }, // floor
            InteriorSurfaceSolar { area: 30.0, absorptance: 0.5 }, // wall
        ];
        let result = distribute_solar(
            &windows,
            &[],
            &interior,
            &[0],
            InteriorDistribution::FullInteriorAndExterior,
        );

        // Floor should absorb more than wall since beam goes to floor first
        assert!(
            result.surface_absorbed_interior[0] > result.surface_absorbed_interior[1],
            "floor={}, wall={}",
            result.surface_absorbed_interior[0],
            result.surface_absorbed_interior[1]
        );
    }

    #[test]
    fn no_windows_no_solar() {
        let interior = vec![
            InteriorSurfaceSolar { area: 10.0, absorptance: 0.5 },
        ];
        let result = distribute_solar(&[], &[], &interior, &[], InteriorDistribution::Uniform);
        assert!((result.zone_beam_solar).abs() < 1e-10);
        assert!((result.zone_diff_solar).abs() < 1e-10);
        assert!((result.surface_absorbed_interior[0]).abs() < 1e-10);
    }

    #[test]
    fn conservation_check() {
        // All absorbed interior should be <= total transmitted
        let windows = vec![WindowSolar {
            area: 3.0,
            beam_transmittance: 0.7,
            diffuse_transmittance: 0.6,
            incident_beam: 500.0,
            incident_diffuse: 150.0,
            sunlit_fraction: 1.0,
            absorbed_per_layer: vec![],
        }];
        let interior = vec![
            InteriorSurfaceSolar { area: 10.0, absorptance: 1.0 }, // perfect absorber
            InteriorSurfaceSolar { area: 20.0, absorptance: 1.0 },
        ];
        let result = distribute_solar(&windows, &[], &interior, &[], InteriorDistribution::Uniform);
        let total_abs: f64 = result.surface_absorbed_interior.iter().sum();
        let total_trans = result.zone_beam_solar + result.zone_diff_solar;
        // With absorptance=1.0, total absorbed should equal total transmitted
        assert!(
            (total_abs - total_trans).abs() < 0.1,
            "absorbed={total_abs}, transmitted={total_trans}"
        );
    }

    #[test]
    fn multiple_windows() {
        let windows = vec![
            WindowSolar {
                area: 2.0,
                beam_transmittance: 0.7,
                diffuse_transmittance: 0.6,
                incident_beam: 600.0,
                incident_diffuse: 100.0,
                sunlit_fraction: 1.0,
                absorbed_per_layer: vec![],
            },
            WindowSolar {
                area: 1.5,
                beam_transmittance: 0.5,
                diffuse_transmittance: 0.4,
                incident_beam: 400.0,
                incident_diffuse: 80.0,
                sunlit_fraction: 0.6,
                absorbed_per_layer: vec![],
            },
        ];
        let result = distribute_solar(&windows, &[], &[], &[], InteriorDistribution::Uniform);
        // Window 1: beam = 600*0.7*2.0*1.0=840, diff=100*0.6*2.0=120
        // Window 2: beam = 400*0.5*1.5*0.6=180, diff=80*0.4*1.5=48
        assert!((result.transmitted_beam[0] - 840.0).abs() < 0.1);
        assert!((result.transmitted_beam[1] - 180.0).abs() < 0.1);
        assert!((result.zone_beam_solar - 1020.0).abs() < 0.1);
    }

    #[test]
    fn total_zone_solar_gain_fn() {
        let mut result = SolarDistributionResult::default();
        result.surface_absorbed_interior = vec![100.0, 200.0, 50.0];
        assert!((total_zone_solar_gain(&result) - 350.0).abs() < 1e-10);
    }

    #[test]
    fn zero_area_surfaces() {
        let interior = vec![
            InteriorSurfaceSolar { area: 0.0, absorptance: 0.5 },
        ];
        let windows = vec![WindowSolar {
            area: 2.0,
            beam_transmittance: 0.7,
            diffuse_transmittance: 0.6,
            incident_beam: 800.0,
            incident_diffuse: 100.0,
            sunlit_fraction: 1.0,
            absorbed_per_layer: vec![],
        }];
        // Should not panic with zero-area surfaces
        let result = distribute_solar(&windows, &[], &interior, &[], InteriorDistribution::Uniform);
        assert!(result.zone_beam_solar > 0.0);
    }
}
