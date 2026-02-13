//! Surface geometry calculations.
//!
//! Implements Newell's method for area/normal computation,
//! tilt/azimuth extraction, and convexity testing.

use ep_units::Angle;

use crate::Vertex;

/// Calculate surface area and outward normal using Newell's method.
///
/// Returns (area, normal_vector). The normal vector is NOT normalized;
/// its magnitude equals 2x the surface area.
pub fn newell_area_and_normal(vertices: &[Vertex]) -> (f64, Vertex) {
    let n = vertices.len();
    if n < 3 {
        return (0.0, Vertex::default());
    }

    let mut nx = 0.0;
    let mut ny = 0.0;
    let mut nz = 0.0;

    for i in 0..n {
        let j = (i + 1) % n;
        nx += (vertices[i].y - vertices[j].y) * (vertices[i].z + vertices[j].z);
        ny += (vertices[i].z - vertices[j].z) * (vertices[i].x + vertices[j].x);
        nz += (vertices[i].x - vertices[j].x) * (vertices[i].y + vertices[j].y);
    }

    let normal = Vertex::new(nx, ny, nz);
    let area = 0.5 * normal.magnitude();

    (area, normal)
}

/// Calculate the centroid (geometric center) of a polygon.
pub fn centroid(vertices: &[Vertex]) -> Vertex {
    if vertices.is_empty() {
        return Vertex::default();
    }
    let n = vertices.len() as f64;
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut cz = 0.0;
    for v in vertices {
        cx += v.x;
        cy += v.y;
        cz += v.z;
    }
    Vertex::new(cx / n, cy / n, cz / n)
}

/// Calculate tilt angle from the outward normal vector.
///
/// Returns the angle from horizontal:
/// - 0 degrees = facing up (horizontal upward-facing)
/// - 90 degrees = vertical
/// - 180 degrees = facing down (horizontal downward-facing)
pub fn tilt_from_normal(normal: &Vertex) -> Angle {
    let mag = normal.magnitude();
    if mag < 1e-15 {
        return Angle::from_degrees(0.0);
    }
    let cos_tilt = normal.z / mag;
    Angle::from_degrees(cos_tilt.clamp(-1.0, 1.0).acos().to_degrees())
}

/// Calculate azimuth from the outward normal vector.
///
/// Returns compass direction of outward normal:
/// - 0 degrees = North (+Y direction)
/// - 90 degrees = East (+X direction)
/// - 180 degrees = South (-Y direction)
/// - 270 degrees = West (-X direction)
pub fn azimuth_from_normal(normal: &Vertex) -> Angle {
    let horizontal_mag = (normal.x * normal.x + normal.y * normal.y).sqrt();
    if horizontal_mag < 1e-15 {
        return Angle::from_degrees(0.0);
    }

    let mut azimuth = normal.x.atan2(normal.y).to_degrees();
    if azimuth < 0.0 {
        azimuth += 360.0;
    }
    Angle::from_degrees(azimuth)
}

/// Test whether a polygon (given as 3D vertices) is convex.
///
/// Projects to the dominant plane (using the Newell normal) and checks
/// that all cross products have the same sign.
pub fn is_convex(vertices: &[Vertex]) -> bool {
    let n = vertices.len();
    if n < 3 {
        return false;
    }
    if n == 3 {
        return true; // Triangles are always convex
    }

    let (_, normal) = newell_area_and_normal(vertices);
    let mut positive = false;
    let mut negative = false;

    for i in 0..n {
        let j = (i + 1) % n;
        let k = (i + 2) % n;
        let edge1 = vertices[j].sub(vertices[i]);
        let edge2 = vertices[k].sub(vertices[j]);
        let cross = edge1.cross(edge2);
        let dot = cross.dot(normal);

        if dot > 1e-10 {
            positive = true;
        } else if dot < -1e-10 {
            negative = true;
        }

        if positive && negative {
            return false;
        }
    }

    true
}

/// Calculate the perimeter of a polygon.
pub fn perimeter(vertices: &[Vertex]) -> f64 {
    let n = vertices.len();
    if n < 2 {
        return 0.0;
    }
    let mut p = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        p += vertices[j].sub(vertices[i]).magnitude();
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newell_rectangle() {
        // 10x3 rectangle in the XZ plane (south wall)
        let verts = vec![
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(10.0, 0.0, 0.0),
            Vertex::new(10.0, 0.0, 3.0),
            Vertex::new(0.0, 0.0, 3.0),
        ];
        let (area, normal) = newell_area_and_normal(&verts);
        assert!((area - 30.0).abs() < 1e-10, "area={area}");
        // Normal should point in -Y direction (south wall outward normal)
        let n = normal.normalized();
        assert!(n.y < -0.99, "ny={}", n.y);
    }

    #[test]
    fn newell_horizontal_floor() {
        let verts = vec![
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(10.0, 0.0, 0.0),
            Vertex::new(10.0, 10.0, 0.0),
            Vertex::new(0.0, 10.0, 0.0),
        ];
        let (area, _normal) = newell_area_and_normal(&verts);
        assert!((area - 100.0).abs() < 1e-10, "area={area}");
    }

    #[test]
    fn tilt_vertical_wall() {
        let normal = Vertex::new(0.0, -1.0, 0.0); // South wall
        let tilt = tilt_from_normal(&normal);
        assert!((tilt.to_degrees() - 90.0).abs() < 0.01, "tilt={}", tilt.to_degrees());
    }

    #[test]
    fn tilt_horizontal_floor() {
        let normal = Vertex::new(0.0, 0.0, -1.0); // Floor, normal pointing down
        let tilt = tilt_from_normal(&normal);
        assert!((tilt.to_degrees() - 180.0).abs() < 0.01, "tilt={}", tilt.to_degrees());
    }

    #[test]
    fn tilt_horizontal_roof() {
        let normal = Vertex::new(0.0, 0.0, 1.0); // Roof, normal pointing up
        let tilt = tilt_from_normal(&normal);
        assert!((tilt.to_degrees()).abs() < 0.01, "tilt={}", tilt.to_degrees());
    }

    #[test]
    fn azimuth_south_wall() {
        let normal = Vertex::new(0.0, -1.0, 0.0);
        let az = azimuth_from_normal(&normal);
        assert!((az.to_degrees() - 180.0).abs() < 0.01, "az={}", az.to_degrees());
    }

    #[test]
    fn azimuth_east_wall() {
        let normal = Vertex::new(1.0, 0.0, 0.0);
        let az = azimuth_from_normal(&normal);
        assert!((az.to_degrees() - 90.0).abs() < 0.01, "az={}", az.to_degrees());
    }

    #[test]
    fn convexity_rectangle() {
        let verts = vec![
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(10.0, 0.0, 0.0),
            Vertex::new(10.0, 10.0, 0.0),
            Vertex::new(0.0, 10.0, 0.0),
        ];
        assert!(is_convex(&verts));
    }

    #[test]
    fn convexity_l_shape() {
        // L-shaped polygon (non-convex)
        let verts = vec![
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(10.0, 0.0, 0.0),
            Vertex::new(10.0, 5.0, 0.0),
            Vertex::new(5.0, 5.0, 0.0),
            Vertex::new(5.0, 10.0, 0.0),
            Vertex::new(0.0, 10.0, 0.0),
        ];
        assert!(!is_convex(&verts));
    }

    #[test]
    fn perimeter_square() {
        let verts = vec![
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(5.0, 0.0, 0.0),
            Vertex::new(5.0, 5.0, 0.0),
            Vertex::new(0.0, 5.0, 0.0),
        ];
        assert!((perimeter(&verts) - 20.0).abs() < 1e-10);
    }

    #[test]
    fn centroid_square() {
        let verts = vec![
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(10.0, 0.0, 0.0),
            Vertex::new(10.0, 10.0, 0.0),
            Vertex::new(0.0, 10.0, 0.0),
        ];
        let c = centroid(&verts);
        assert!((c.x - 5.0).abs() < 1e-10);
        assert!((c.y - 5.0).abs() < 1e-10);
    }
}
