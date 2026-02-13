//! View factor calculations for radiant exchange.
//!
//! Computes geometric view factors between surfaces in an enclosure
//! for interior longwave radiation exchange.

/// A radiant enclosure containing surfaces that exchange longwave radiation.
#[derive(Debug, Clone)]
pub struct RadiantEnclosure {
    /// Indices of surfaces in this enclosure.
    pub surface_indices: Vec<usize>,
    /// View factor matrix: view_factors[i][j] = F(i->j).
    pub view_factors: Vec<Vec<f64>>,
    /// Script-F matrix for radiosity calculations (includes emissivity).
    pub script_f: Vec<Vec<f64>>,
}

impl RadiantEnclosure {
    /// Create a new enclosure with the given surface indices.
    pub fn new(surface_indices: Vec<usize>) -> Self {
        let n = surface_indices.len();
        Self {
            surface_indices,
            view_factors: vec![vec![0.0; n]; n],
            script_f: vec![vec![0.0; n]; n],
        }
    }

    /// Number of surfaces in the enclosure.
    pub fn num_surfaces(&self) -> usize {
        self.surface_indices.len()
    }

    /// Compute approximate view factors using the area-weighted method.
    ///
    /// This is the simplest model: F(i->j) = A(j) / (total_area - A(i))
    /// Each surface "sees" other surfaces proportional to their area.
    pub fn compute_area_weighted_view_factors(&mut self, areas: &[f64]) {
        let n = self.num_surfaces();
        let total_area: f64 = areas.iter().sum();

        for i in 0..n {
            let denom = total_area - areas[i];
            if denom > 1e-10 {
                for j in 0..n {
                    if i != j {
                        self.view_factors[i][j] = areas[j] / denom;
                    } else {
                        self.view_factors[i][j] = 0.0;
                    }
                }
            }
        }
    }

    /// Compute the script-F matrix from view factors and emissivities.
    ///
    /// The script-F method accounts for the emissivity of surfaces
    /// in the enclosure. For gray diffuse surfaces:
    ///
    /// ScriptF(i,j) = F(i,j) * eps(j) / (1 - (1-eps(j)) * F(j,j_effective))
    ///
    /// Simplified version using Hottel's script-F for gray surfaces.
    pub fn compute_script_f(&mut self, emissivities: &[f64]) {
        let n = self.num_surfaces();

        // For the simplified (non-iterative) version, ScriptF = F * eps
        // This is exact for black body enclosures and approximate for gray
        for i in 0..n {
            for j in 0..n {
                self.script_f[i][j] = self.view_factors[i][j] * emissivities[j];
            }
        }
    }
}

/// Compute view factor between two finite parallel rectangles.
///
/// Uses the exact closed-form solution for aligned parallel rectangles
/// of dimensions (a x b) separated by distance c.
///
/// Reference: Modest, "Radiative Heat Transfer", 3rd Ed., Table D.2
pub fn view_factor_parallel_rectangles(a: f64, b: f64, c: f64) -> f64 {
    if a <= 0.0 || b <= 0.0 || c <= 0.0 {
        return 0.0;
    }

    let x = a / c;
    let y = b / c;

    let x2 = x * x;
    let y2 = y * y;

    let term1 = ((1.0 + x2) * (1.0 + y2) / (1.0 + x2 + y2)).ln();
    let term2 = x * (1.0 + y2).sqrt() * (x / (1.0 + y2).sqrt()).atan();
    let term3 = y * (1.0 + x2).sqrt() * (y / (1.0 + x2).sqrt()).atan();
    let term4 = x * x.atan();
    let term5 = y * y.atan();

    (2.0 / (std::f64::consts::PI * x * y)) * (term1 / 2.0 + term2 + term3 - term4 - term5)
}

/// Compute view factor between two perpendicular rectangles sharing a common edge.
///
/// Rectangle 1: width a, height c (common edge = c)
/// Rectangle 2: width b, height c (common edge = c)
///
/// Reference: Modest, "Radiative Heat Transfer", 3rd Ed., Eq. D.42
pub fn view_factor_perpendicular_rectangles(a: f64, b: f64, c: f64) -> f64 {
    if a <= 0.0 || b <= 0.0 || c <= 0.0 {
        return 0.0;
    }

    let h = a / c;
    let w = b / c;

    let h2 = h * h;
    let w2 = w * w;

    let a_val = (1.0 + h2) * (1.0 + w2) / (1.0 + h2 + w2);
    let b_val = ((w2 * (1.0 + h2 + w2)) / ((1.0 + w2) * (h2 + w2))).powf(w2);
    let c_val = ((h2 * (1.0 + h2 + w2)) / ((1.0 + h2) * (h2 + w2))).powf(h2);

    let term1 = h * (w / (1.0 + h2).sqrt()).atan();
    let term2 = w * (h / (1.0 + w2).sqrt()).atan();
    let term3 = (h2 + w2).sqrt() * ((1.0 / (h2 + w2).sqrt()).atan());
    let term4 = 0.25 * (a_val * b_val * c_val).ln();

    (1.0 / (std::f64::consts::PI * h)) * (term1 + term2 - term3 + term4)
}

/// Compute view factor from a small surface to a large surface using
/// the area ratio approximation.
///
/// For surface i looking at surface j: F(i,j) approx A(j) / sum(A_visible)
pub fn view_factor_area_ratio(area_j: f64, total_visible_area: f64) -> f64 {
    if total_visible_area > 0.0 {
        area_j / total_visible_area
    } else {
        0.0
    }
}

/// Apply view factor reciprocity: A_i * F(i,j) = A_j * F(j,i)
pub fn apply_reciprocity(f_ij: f64, area_i: f64, area_j: f64) -> f64 {
    if area_j > 0.0 {
        f_ij * area_i / area_j
    } else {
        0.0
    }
}

/// Estimate view factor from a tilted surface to the sky.
///
/// For a tilted surface: F_sky = (1 + cos(tilt)) / 2
pub fn view_factor_to_sky(cos_tilt: f64) -> f64 {
    ((1.0 + cos_tilt) / 2.0).max(0.0)
}

/// Estimate view factor from a tilted surface to the ground.
///
/// For a tilted surface: F_ground = (1 - cos(tilt)) / 2
pub fn view_factor_to_ground(cos_tilt: f64) -> f64 {
    ((1.0 - cos_tilt) / 2.0).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_rectangles_equal() {
        // Two 1x1 squares separated by distance 1
        let f = view_factor_parallel_rectangles(1.0, 1.0, 1.0);
        // Known value ~0.1998
        assert!((f - 0.1998).abs() < 0.001, "F={f}");
    }

    #[test]
    fn sky_ground_view_factors() {
        // Vertical wall: tilt = 90, cos(90) = 0
        assert!((view_factor_to_sky(0.0) - 0.5).abs() < 1e-10);
        assert!((view_factor_to_ground(0.0) - 0.5).abs() < 1e-10);

        // Horizontal upward: tilt = 0, cos(0) = 1
        assert!((view_factor_to_sky(1.0) - 1.0).abs() < 1e-10);
        assert!((view_factor_to_ground(1.0)).abs() < 1e-10);
    }

    #[test]
    fn area_weighted_view_factors() {
        let mut enclosure = RadiantEnclosure::new(vec![0, 1, 2, 3]);
        let areas = vec![10.0, 20.0, 10.0, 20.0]; // Total = 60
        enclosure.compute_area_weighted_view_factors(&areas);

        // F(0->1) = 20 / (60 - 10) = 0.4
        assert!((enclosure.view_factors[0][1] - 0.4).abs() < 1e-10);
        // F(0->0) = 0 (no self-view)
        assert!((enclosure.view_factors[0][0]).abs() < 1e-10);

        // Row sum should be ~1.0
        let sum: f64 = enclosure.view_factors[0].iter().sum();
        assert!((sum - 1.0).abs() < 1e-10, "sum={sum}");
    }

    #[test]
    fn reciprocity() {
        let f_ij = 0.4;
        let a_i = 10.0;
        let a_j = 20.0;
        let f_ji = apply_reciprocity(f_ij, a_i, a_j);
        // A_i * F_ij = A_j * F_ji => F_ji = F_ij * A_i / A_j = 0.2
        assert!((f_ji - 0.2).abs() < 1e-10);
    }
}
