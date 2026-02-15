//! Shadow casting and sunlit fraction calculations.
//!
//! Implements polygon clipping for determining the sunlit area of surfaces,
//! accounting for self-shading, overhangs, fins, and external obstructions.
//! Uses Sutherland-Hodgman polygon clipping to compute sunlit fractions.

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

/// Sun position for shadow calculations.
#[derive(Debug, Clone, Copy)]
pub struct SunPosition {
    /// Solar altitude angle (radians, 0 = horizon, PI/2 = zenith).
    pub altitude: f64,
    /// Solar azimuth angle (radians, from south, positive west).
    pub azimuth: f64,
}

impl SunPosition {
    pub fn new(altitude: f64, azimuth: f64) -> Self {
        Self { altitude, azimuth }
    }

    /// Sun direction vector (unit vector pointing toward the sun).
    pub fn direction(&self) -> Vertex {
        let cos_alt = self.altitude.cos();
        Vertex::new(
            -cos_alt * self.azimuth.sin(), // x: east-west (positive east)
            -cos_alt * self.azimuth.cos(), // y: north-south (positive north, negative=south)
            self.altitude.sin(),            // z: vertical
        )
    }

    /// Whether the sun is above the horizon.
    pub fn is_up(&self) -> bool {
        self.altitude > 0.0
    }
}

/// A shading surface (overhang, fin, detached obstruction).
#[derive(Debug, Clone)]
pub struct ShadingSurface {
    pub name: String,
    pub vertices: Vec<Vertex>,
    pub shading_type: ShadingType,
}

/// Type of shading surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadingType {
    /// Detached shading (trees, neighboring buildings).
    Detached,
    /// Zone-attached overhang.
    Overhang,
    /// Zone-attached vertical fin.
    Fin,
    /// Building surface acting as self-shading.
    Building,
}

/// Result of sunlit fraction computation for all receiving surfaces.
#[derive(Debug, Clone)]
pub struct SunlitResult {
    /// Sunlit fraction per receiving surface [0.0, 1.0].
    pub fractions: Vec<f64>,
}

/// Calculate the sunlit fraction of a surface given shadow polygons.
///
/// Returns a value between 0.0 (fully shaded) and 1.0 (fully sunlit).
pub fn sunlit_fraction(surface_area: f64, shadowed_area: f64) -> f64 {
    if surface_area <= 0.0 {
        return 0.0;
    }
    (1.0 - shadowed_area / surface_area).clamp(0.0, 1.0)
}

/// Determine whether a shading surface can potentially shade a receiving surface.
///
/// Quick geometric pre-filter based on surface normals and relative position.
/// Returns false if the shading surface is behind the receiving surface
/// relative to the sun direction.
pub fn can_shade(
    receiving_normal: &Vertex,
    receiving_centroid: &Vertex,
    shading_centroid: &Vertex,
    sun: &SunPosition,
) -> bool {
    if !sun.is_up() {
        return false;
    }

    let sun_dir = sun.direction();

    // Receiving surface must face the sun (sun dot normal > 0)
    let face_sun = sun_dir.dot(*receiving_normal);
    if face_sun <= 0.0 {
        return false;
    }

    // Vector from receiving to shading surface
    let to_shade = shading_centroid.sub(*receiving_centroid);

    // Shading surface must be on the sun-side of the receiving surface
    // (the shadow caster must be between the sun and the receiver)
    let shade_along_sun = to_shade.dot(sun_dir);
    shade_along_sun > 0.0
}

/// Compute sunlit fractions for multiple receiving surfaces given shading surfaces.
///
/// For each receiving surface, projects all relevant shading surfaces onto the
/// sun plane, clips the shadow polygons against the receiving polygon, and
/// computes the shadowed area fraction.
pub fn compute_sunlit_fractions(
    receiving_vertices: &[Vec<Vertex>],
    receiving_areas: &[f64],
    receiving_normals: &[Vertex],
    receiving_centroids: &[Vertex],
    shading_surfaces: &[ShadingSurface],
    sun: &SunPosition,
) -> SunlitResult {
    let n = receiving_vertices.len();
    let mut fractions = vec![1.0; n];

    if !sun.is_up() {
        return SunlitResult {
            fractions: vec![0.0; n],
        };
    }

    for i in 0..n {
        if receiving_areas[i] <= 0.0 {
            fractions[i] = 0.0;
            continue;
        }

        // Project receiving surface to sun plane
        let recv_proj = project_to_sun_plane(&receiving_vertices[i], sun.altitude, sun.azimuth);
        let recv_area_proj = polygon_area_2d(&recv_proj);
        if recv_area_proj < 1e-10 {
            fractions[i] = 0.0;
            continue;
        }

        let mut total_shadow_area = 0.0;

        for shade in shading_surfaces {
            if !can_shade(
                &receiving_normals[i],
                &receiving_centroids[i],
                &centroid_3d(&shade.vertices),
                sun,
            ) {
                continue;
            }

            // Project shading surface shadow onto the receiving surface plane
            let shade_proj =
                project_to_sun_plane(&shade.vertices, sun.altitude, sun.azimuth);

            // Clip shadow polygon against receiving polygon
            let overlap = clip_polygon(&shade_proj, &recv_proj);
            if overlap.len() >= 3 {
                total_shadow_area += polygon_area_2d(&overlap);
            }
        }

        fractions[i] = sunlit_fraction(recv_area_proj, total_shadow_area);
    }

    SunlitResult { fractions }
}

/// Compute the centroid of a 3D polygon.
fn centroid_3d(vertices: &[Vertex]) -> Vertex {
    if vertices.is_empty() {
        return Vertex::default();
    }
    let n = vertices.len() as f64;
    let sum = vertices.iter().fold(Vertex::default(), |acc, v| acc.add(*v));
    sum.scale(1.0 / n)
}

/// Calculate the shadow cast by a vertical fin onto a surface.
///
/// Returns the shadowed fraction (0 to 1).
///
/// # Arguments
/// * `fin_depth` - Depth of the fin projection from the wall (m)
/// * `fin_offset` - Horizontal distance from fin to nearest edge of surface (m)
/// * `surface_width` - Width of the surface (m)
/// * `solar_altitude` - Solar altitude angle (radians)
/// * `solar_azimuth_relative` - Relative azimuth (sun azimuth - surface azimuth, radians)
pub fn fin_shadow_fraction(
    fin_depth: f64,
    fin_offset: f64,
    surface_width: f64,
    solar_altitude: f64,
    solar_azimuth_relative: f64,
) -> f64 {
    if solar_altitude <= 0.0 || surface_width <= 0.0 || fin_depth <= 0.0 {
        return 0.0;
    }

    let cos_rel_az = solar_azimuth_relative.cos();
    if cos_rel_az <= 0.0 {
        return 0.0; // Sun behind surface
    }

    let sin_rel_az = solar_azimuth_relative.sin();
    // Shadow width depends on the horizontal profile angle
    // tan(horizontal_profile) = fin_depth * |sin(relative_azimuth)| / cos(relative_azimuth)
    let shadow_width = fin_depth * sin_rel_az.abs() / cos_rel_az;
    let shadow_on_surface = (shadow_width - fin_offset).max(0.0);

    (shadow_on_surface / surface_width).min(1.0)
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
    use std::f64::consts::PI;

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
        assert!((area - 9.0).abs() < 0.01, "area={area}");
    }

    #[test]
    fn clip_polygon_no_overlap() {
        let subject = vec![
            Point2D::new(10.0, 10.0),
            Point2D::new(12.0, 10.0),
            Point2D::new(12.0, 12.0),
            Point2D::new(10.0, 12.0),
        ];
        let clip = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(5.0, 0.0),
            Point2D::new(5.0, 5.0),
            Point2D::new(0.0, 5.0),
        ];
        let result = clip_polygon(&subject, &clip);
        assert!(result.is_empty() || polygon_area_2d(&result) < 1e-10);
    }

    #[test]
    fn overhang_shadow_high_sun() {
        let frac = overhang_shadow_fraction(
            1.0,
            0.0,
            2.0,
            60.0_f64.to_radians(),
            0.0,
        );
        // Shadow depth = 1.0 / tan(60°) ≈ 0.577m on 2m surface
        assert!(frac > 0.2 && frac < 0.35, "frac={frac}");
    }

    #[test]
    fn overhang_no_shadow_sun_behind() {
        let frac = overhang_shadow_fraction(
            1.0, 0.0, 2.0,
            45.0_f64.to_radians(),
            PI,
        );
        assert!(frac.abs() < 1e-10);
    }

    #[test]
    fn overhang_shadow_with_offset() {
        // Overhang 0.5m above window top, 0.8m deep, 1.5m surface
        let frac = overhang_shadow_fraction(
            0.8, 0.5, 1.5,
            45.0_f64.to_radians(),
            0.0,
        );
        // Shadow depth = 0.8 / tan(45°) = 0.8m, minus 0.5m offset = 0.3m on 1.5m
        assert!((frac - 0.2).abs() < 0.05, "frac={frac}");
    }

    #[test]
    fn fin_shadow_sun_directly_facing() {
        // Sun directly facing surface (relative_azimuth = 0) → no fin shadow
        let frac = fin_shadow_fraction(
            1.0, 0.0, 3.0,
            45.0_f64.to_radians(),
            0.0,
        );
        assert!(frac.abs() < 1e-10, "frac={frac}");
    }

    #[test]
    fn fin_shadow_oblique_sun() {
        // Sun 30° off normal, 1m deep fin, 3m wide surface
        let frac = fin_shadow_fraction(
            1.0, 0.0, 3.0,
            45.0_f64.to_radians(),
            30.0_f64.to_radians(),
        );
        // shadow_width = 1.0 * sin(30°) / cos(30°) = tan(30°) ≈ 0.577m on 3m
        assert!(frac > 0.15 && frac < 0.25, "frac={frac}");
    }

    #[test]
    fn fin_shadow_sun_behind() {
        let frac = fin_shadow_fraction(
            1.0, 0.0, 3.0,
            45.0_f64.to_radians(),
            PI,
        );
        assert!(frac.abs() < 1e-10);
    }

    #[test]
    fn sun_position_direction_vector() {
        // Sun at zenith: direction should be (0, 0, 1)
        let sun = SunPosition::new(PI / 2.0, 0.0);
        let dir = sun.direction();
        assert!(dir.z > 0.99, "z={}", dir.z);
        assert!(dir.x.abs() < 0.01);
        assert!(dir.y.abs() < 0.01);
    }

    #[test]
    fn sun_position_south_facing() {
        // Sun from due south at 45° altitude: azimuth=0 (from south)
        let sun = SunPosition::new(45.0_f64.to_radians(), 0.0);
        let dir = sun.direction();
        // Should point toward south (negative y) and up
        assert!(dir.y < 0.0, "y={}", dir.y);
        assert!(dir.z > 0.0, "z={}", dir.z);
    }

    #[test]
    fn can_shade_sun_behind_receiver() {
        // Sun behind receiving surface → cannot shade
        let normal = Vertex::new(0.0, -1.0, 0.0); // facing south
        let recv_centroid = Vertex::new(0.0, 0.0, 1.5);
        let shade_centroid = Vertex::new(0.0, 5.0, 3.0); // north of receiver
        let sun = SunPosition::new(45.0_f64.to_radians(), PI); // sun from north

        assert!(!can_shade(&normal, &recv_centroid, &shade_centroid, &sun));
    }

    #[test]
    fn can_shade_valid_configuration() {
        // Overhang above a south-facing wall, sun from south
        let normal = Vertex::new(0.0, -1.0, 0.0); // facing south
        let recv_centroid = Vertex::new(0.0, 0.0, 1.5);
        let shade_centroid = Vertex::new(0.0, -0.5, 3.1); // above and slightly south
        let sun = SunPosition::new(60.0_f64.to_radians(), 0.0); // sun from south, high

        assert!(can_shade(&normal, &recv_centroid, &shade_centroid, &sun));
    }

    #[test]
    fn compute_sunlit_fractions_no_shading() {
        // Single surface, no shading surfaces → fully sunlit
        let verts = vec![vec![
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(3.0, 0.0, 0.0),
            Vertex::new(3.0, 0.0, 2.0),
            Vertex::new(0.0, 0.0, 2.0),
        ]];
        let areas = vec![6.0];
        let normals = vec![Vertex::new(0.0, -1.0, 0.0)];
        let centroids = vec![Vertex::new(1.5, 0.0, 1.0)];
        let sun = SunPosition::new(45.0_f64.to_radians(), 0.0);

        let result = compute_sunlit_fractions(&verts, &areas, &normals, &centroids, &[], &sun);
        assert!((result.fractions[0] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn compute_sunlit_fractions_night() {
        // Sun below horizon → all surfaces have sunlit fraction = 0
        let verts = vec![vec![
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(3.0, 0.0, 0.0),
            Vertex::new(3.0, 0.0, 2.0),
            Vertex::new(0.0, 0.0, 2.0),
        ]];
        let areas = vec![6.0];
        let normals = vec![Vertex::new(0.0, -1.0, 0.0)];
        let centroids = vec![Vertex::new(1.5, 0.0, 1.0)];
        let sun = SunPosition::new(-0.1, 0.0);

        let result = compute_sunlit_fractions(&verts, &areas, &normals, &centroids, &[], &sun);
        assert!((result.fractions[0]).abs() < 1e-10);
    }

    #[test]
    fn centroid_3d_simple() {
        let verts = vec![
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(4.0, 0.0, 0.0),
            Vertex::new(4.0, 4.0, 0.0),
            Vertex::new(0.0, 4.0, 0.0),
        ];
        let c = centroid_3d(&verts);
        assert!((c.x - 2.0).abs() < 1e-10);
        assert!((c.y - 2.0).abs() < 1e-10);
        assert!((c.z).abs() < 1e-10);
    }
}
