//! Shadow casting and sunlit fraction calculations.
//!
//! Implements polygon clipping for determining the sunlit area of surfaces,
//! accounting for self-shading and external obstructions.

use ep_surfaces::Vertex;

/// A 2D point for polygon clipping operations.
#[derive(Debug, Clone, Copy)]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

impl Point2D {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Calculate the sunlit fraction of a surface given shadow polygons.
///
/// Returns a value between 0.0 (fully shaded) and 1.0 (fully sunlit).
///
/// # Arguments
/// * `surface_area` - Total surface area (m2)
/// * `shadowed_area` - Area in shadow (m2)
pub fn sunlit_fraction(surface_area: f64, shadowed_area: f64) -> f64 {
    if surface_area <= 0.0 {
        return 0.0;
    }
    (1.0 - shadowed_area / surface_area).clamp(0.0, 1.0)
}

/// Project a 3D polygon onto a plane perpendicular to the sun direction.
///
/// Returns 2D coordinates in the sun's coordinate system.
pub fn project_to_sun_plane(
    vertices: &[Vertex],
    sun_altitude: f64,
    sun_azimuth: f64,
) -> Vec<Point2D> {
    let sin_alt = sun_altitude.sin();
    let cos_alt = sun_altitude.cos();
    let sin_az = sun_azimuth.sin();
    let cos_az = sun_azimuth.cos();

    vertices
        .iter()
        .map(|v| {
            // Rotate to sun coordinate system
            let x_proj = v.x * cos_az - v.y * sin_az;
            let y_proj = v.x * sin_az * sin_alt + v.y * cos_az * sin_alt + v.z * cos_alt;
            Point2D::new(x_proj, y_proj)
        })
        .collect()
}

/// Calculate the area of a 2D polygon using the shoelace formula.
pub fn polygon_area_2d(points: &[Point2D]) -> f64 {
    let n = points.len();
    if n < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        area += points[i].x * points[j].y;
        area -= points[j].x * points[i].y;
    }
    (area / 2.0).abs()
}

/// Sutherland-Hodgman polygon clipping algorithm.
///
/// Clips `subject` polygon against the edges of `clip` polygon.
/// Returns the intersection polygon (empty if no overlap).
pub fn clip_polygon(subject: &[Point2D], clip: &[Point2D]) -> Vec<Point2D> {
    if subject.is_empty() || clip.is_empty() {
        return Vec::new();
    }

    let mut output = subject.to_vec();

    for i in 0..clip.len() {
        if output.is_empty() {
            return Vec::new();
        }

        let input = output.clone();
        output.clear();

        let edge_start = clip[i];
        let edge_end = clip[(i + 1) % clip.len()];

        for j in 0..input.len() {
            let current = input[j];
            let previous = input[(j + input.len() - 1) % input.len()];

            let cur_inside = is_inside(current, edge_start, edge_end);
            let prev_inside = is_inside(previous, edge_start, edge_end);

            if cur_inside {
                if !prev_inside {
                    if let Some(p) = line_intersection(previous, current, edge_start, edge_end) {
                        output.push(p);
                    }
                }
                output.push(current);
            } else if prev_inside {
                if let Some(p) = line_intersection(previous, current, edge_start, edge_end) {
                    output.push(p);
                }
            }
        }
    }

    output
}

/// Check if a point is on the inside (left side) of a directed edge.
fn is_inside(point: Point2D, edge_start: Point2D, edge_end: Point2D) -> bool {
    let cross = (edge_end.x - edge_start.x) * (point.y - edge_start.y)
        - (edge_end.y - edge_start.y) * (point.x - edge_start.x);
    cross >= 0.0
}

/// Find the intersection point of two line segments.
fn line_intersection(
    a1: Point2D,
    a2: Point2D,
    b1: Point2D,
    b2: Point2D,
) -> Option<Point2D> {
    let dx_a = a2.x - a1.x;
    let dy_a = a2.y - a1.y;
    let dx_b = b2.x - b1.x;
    let dy_b = b2.y - b1.y;

    let denom = dx_a * dy_b - dy_a * dx_b;
    if denom.abs() < 1e-15 {
        return None; // Parallel lines
    }

    let t = ((b1.x - a1.x) * dy_b - (b1.y - a1.y) * dx_b) / denom;

    Some(Point2D::new(a1.x + t * dx_a, a1.y + t * dy_a))
}

/// Calculate the shadow cast by an overhang onto a surface.
///
/// Returns the shadowed fraction (0 to 1).
///
/// # Arguments
/// * `overhang_depth` - Depth of the overhang (m)
/// * `overhang_offset` - Vertical distance from top of surface to overhang (m)
/// * `surface_height` - Height of the surface (m)
/// * `solar_altitude` - Solar altitude angle (radians)
/// * `solar_azimuth_relative` - Relative azimuth (sun azimuth - surface azimuth, radians)
pub fn overhang_shadow_fraction(
    overhang_depth: f64,
    overhang_offset: f64,
    surface_height: f64,
    solar_altitude: f64,
    solar_azimuth_relative: f64,
) -> f64 {
    if solar_altitude <= 0.0 || surface_height <= 0.0 || overhang_depth <= 0.0 {
        return 0.0;
    }

    let cos_rel_az = solar_azimuth_relative.cos();
    if cos_rel_az <= 0.0 {
        return 0.0; // Sun behind surface
    }

    // Shadow depth on the wall
    let shadow_depth = overhang_depth * solar_altitude.tan().recip() * cos_rel_az;
    let shadow_on_surface = (shadow_depth - overhang_offset).max(0.0);

    (shadow_on_surface / surface_height).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sunlit_fraction_fully_lit() {
        assert!((sunlit_fraction(100.0, 0.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn sunlit_fraction_half_shaded() {
        assert!((sunlit_fraction(100.0, 50.0) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn polygon_area_triangle() {
        let tri = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(4.0, 0.0),
            Point2D::new(0.0, 3.0),
        ];
        assert!((polygon_area_2d(&tri) - 6.0).abs() < 1e-10);
    }

    #[test]
    fn polygon_area_square() {
        let sq = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(5.0, 0.0),
            Point2D::new(5.0, 5.0),
            Point2D::new(0.0, 5.0),
        ];
        assert!((polygon_area_2d(&sq) - 25.0).abs() < 1e-10);
    }

    #[test]
    fn clip_polygon_full_overlap() {
        // Subject fully inside clip
        let subject = vec![
            Point2D::new(1.0, 1.0),
            Point2D::new(4.0, 1.0),
            Point2D::new(4.0, 4.0),
            Point2D::new(1.0, 4.0),
        ];
        let clip = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(5.0, 0.0),
            Point2D::new(5.0, 5.0),
            Point2D::new(0.0, 5.0),
        ];
        let result = clip_polygon(&subject, &clip);
        assert_eq!(result.len(), 4);
        let area = polygon_area_2d(&result);
        assert!((area - 9.0).abs() < 0.01, "area={area}");
    }

    #[test]
    fn clip_polygon_partial_overlap() {
        // Subject partially outside clip
        let subject = vec![
            Point2D::new(-1.0, -1.0),
            Point2D::new(3.0, -1.0),
            Point2D::new(3.0, 3.0),
            Point2D::new(-1.0, 3.0),
        ];
        let clip = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(5.0, 0.0),
            Point2D::new(5.0, 5.0),
            Point2D::new(0.0, 5.0),
        ];
        let result = clip_polygon(&subject, &clip);
        let area = polygon_area_2d(&result);
        // Intersection is 3x3 = 9
        assert!((area - 9.0).abs() < 0.01, "area={area}");
    }

    #[test]
    fn overhang_shadow_high_sun() {
        // Sun at 60° altitude, directly facing surface
        let frac = overhang_shadow_fraction(
            1.0,   // 1m overhang
            0.0,   // No offset
            2.0,   // 2m surface height
            60.0_f64.to_radians(),
            0.0,   // Sun directly facing surface
        );
        // Shadow depth = 1.0 / tan(60°) ≈ 0.577m
        assert!(frac > 0.2 && frac < 0.35, "frac={frac}");
    }

    #[test]
    fn overhang_no_shadow_sun_behind() {
        let frac = overhang_shadow_fraction(
            1.0,
            0.0,
            2.0,
            45.0_f64.to_radians(),
            std::f64::consts::PI, // Sun behind surface
        );
        assert!((frac).abs() < 1e-10);
    }
}
