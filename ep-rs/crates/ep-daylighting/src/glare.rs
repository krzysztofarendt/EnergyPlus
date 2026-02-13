//! Glare calculation using Cornell/BRS large-source formula.
//!
//! Computes discomfort glare index (DGI) from window luminances,
//! solid angles, and background luminance.

use std::f64::consts::PI;

/// Glare source (window or portion of window).
#[derive(Debug, Clone, Copy)]
pub struct GlareSource {
    /// Source luminance (cd/m2).
    pub luminance: f64,
    /// Solid angle subtended by source at the eye (sr).
    pub solid_angle: f64,
    /// Position factor based on angular displacement from line of sight.
    pub position_factor: f64,
}

/// Calculate discomfort glare index using Cornell/BRS formula.
///
/// DGI = 10 * log10(sum_i(0.4794 * L_s^1.6 * omega_s^0.8 / (L_b + 0.07*sqrt(omega)*L_s)))
///
/// # Arguments
/// * `sources` - All glare sources (windows)
/// * `background_luminance` - Background luminance at the eye (cd/m2)
///
/// # Returns
/// Discomfort Glare Index (0-40+)
pub fn glare_index(sources: &[GlareSource], background_luminance: f64) -> f64 {
    if sources.is_empty() || background_luminance <= 0.0 {
        return 0.0;
    }

    let mut g_sum = 0.0;

    for src in sources {
        if src.luminance <= 0.0 || src.solid_angle <= 0.0 || src.position_factor <= 0.0 {
            continue;
        }

        let l = src.luminance;
        let omega = src.solid_angle;
        let p = src.position_factor;

        // Weighted solid angle (accounts for position in visual field)
        let omega_w = omega * p;

        let numerator = 0.4794 * l.powf(1.6) * omega_w.powf(0.8);
        let denominator = background_luminance + 0.07 * omega.sqrt() * l;

        if denominator > 1e-10 {
            g_sum += numerator / denominator;
        }
    }

    if g_sum > 1e-6 {
        10.0 * (g_sum + 1e-6).log10()
    } else {
        0.0
    }
}

/// Glare comfort categories based on DGI value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlareCategory {
    /// DGI < 18: Imperceptible.
    Imperceptible,
    /// 18 <= DGI < 22: Perceptible.
    Perceptible,
    /// 22 <= DGI < 28: Disturbing.
    Disturbing,
    /// DGI >= 28: Intolerable.
    Intolerable,
}

/// Classify glare based on DGI value.
pub fn classify_glare(dgi: f64) -> GlareCategory {
    if dgi < 18.0 {
        GlareCategory::Imperceptible
    } else if dgi < 22.0 {
        GlareCategory::Perceptible
    } else if dgi < 28.0 {
        GlareCategory::Disturbing
    } else {
        GlareCategory::Intolerable
    }
}

/// Position factor based on angular displacement from line of sight.
///
/// Implements a 2D lookup based on lateral (x) and vertical (y) displacement.
/// Returns value between 0 and 1.
///
/// # Arguments
/// * `horizontal_angle` - Horizontal angle from line of sight (radians, 0-pi/2)
/// * `vertical_angle` - Vertical angle from line of sight (radians, 0-pi/2)
pub fn position_factor(horizontal_angle: f64, vertical_angle: f64) -> f64 {
    // Simplified Guth position index
    let x = horizontal_angle.abs().min(PI / 2.0);
    let y = vertical_angle.abs().min(PI / 2.0);

    // Combined angle from line of sight
    let total_angle = (x * x + y * y).sqrt();

    if total_angle > PI / 2.0 {
        0.0
    } else {
        // Position factor decreases with angle from line of sight
        // Peripheral vision is less sensitive to glare
        let factor = (PI / 2.0 - total_angle) / (PI / 2.0);
        factor * factor // quadratic falloff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glare_index_typical_window() {
        let sources = vec![GlareSource {
            luminance: 5000.0,  // bright window
            solid_angle: 0.1,   // moderate size
            position_factor: 0.5,
        }];
        let dgi = glare_index(&sources, 200.0); // typical interior background
        // Should be in a reasonable range
        assert!(dgi > 10.0 && dgi < 40.0, "DGI={dgi}");
    }

    #[test]
    fn glare_increases_with_luminance() {
        let bg = 200.0;
        let dim = glare_index(&[GlareSource {
            luminance: 1000.0, solid_angle: 0.1, position_factor: 0.5,
        }], bg);
        let bright = glare_index(&[GlareSource {
            luminance: 10000.0, solid_angle: 0.1, position_factor: 0.5,
        }], bg);
        assert!(bright > dim, "bright={bright}, dim={dim}");
    }

    #[test]
    fn glare_zero_no_sources() {
        assert!((glare_index(&[], 200.0)).abs() < 1e-10);
    }

    #[test]
    fn glare_classification() {
        assert_eq!(classify_glare(10.0), GlareCategory::Imperceptible);
        assert_eq!(classify_glare(20.0), GlareCategory::Perceptible);
        assert_eq!(classify_glare(25.0), GlareCategory::Disturbing);
        assert_eq!(classify_glare(32.0), GlareCategory::Intolerable);
    }

    #[test]
    fn position_factor_on_axis() {
        let p = position_factor(0.0, 0.0);
        assert!((p - 1.0).abs() < 1e-10, "p={p}");
    }

    #[test]
    fn position_factor_peripheral() {
        let p_center = position_factor(0.0, 0.0);
        let p_side = position_factor(PI / 4.0, 0.0);
        assert!(p_center > p_side, "center={p_center}, side={p_side}");
    }
}
