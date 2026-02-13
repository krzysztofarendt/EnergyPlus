//! Surface geometry, view factors, and building envelope topology for EnergyPlus-rs.
//!
//! Handles surface vertex processing, area/tilt/azimuth calculation,
//! coordinate transforms, view factor computation, and surface adjacency.

use ep_units::*;

pub mod geometry;
pub mod view_factors;

/// Functional surface classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SurfaceClass {
    #[default]
    Wall,
    Floor,
    Roof,
    Ceiling,
    Window,
    Door,
    GlassDoor,
    InternalMass,
    Shading,
    Overhang,
    Fin,
}

impl SurfaceClass {
    /// Whether this surface participates in heat transfer.
    pub fn is_heat_transfer(self) -> bool {
        matches!(
            self,
            Self::Wall | Self::Floor | Self::Roof | Self::Ceiling
                | Self::Window | Self::Door | Self::GlassDoor
                | Self::InternalMass
        )
    }

    /// Whether this is a subsurface (window/door on a base surface).
    pub fn is_subsurface(self) -> bool {
        matches!(self, Self::Window | Self::Door | Self::GlassDoor)
    }

    /// Coarse classification (floor/wall/ceiling).
    pub fn coarse(self) -> CoarseSurfaceClass {
        match self {
            Self::Floor => CoarseSurfaceClass::Floor,
            Self::Roof | Self::Ceiling => CoarseSurfaceClass::Ceiling,
            _ => CoarseSurfaceClass::Wall,
        }
    }
}

/// Coarse surface classification for convection logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoarseSurfaceClass {
    Floor,
    Wall,
    Ceiling,
}

/// Heat transfer algorithm to use for a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeatTransferModel {
    #[default]
    CTF,
    CondFD,
    HAMT,
    Window,
    Kiva,
    AirBoundary,
    None,
}

/// External boundary condition for a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExteriorBoundary {
    /// Exposed to outdoor environment.
    Exterior,
    /// Ground contact.
    Ground,
    /// Adjacent to another surface (interzone).
    Adjacent(usize),
    /// Adiabatic (self-adjacent).
    Adiabatic,
    /// Other-side coefficients.
    OtherSideCoefficients(usize),
    /// Other-side conditions model.
    OtherSideConditionsModel(usize),
}

/// A 3D vertex point.
#[derive(Debug, Clone, Copy, Default)]
pub struct Vertex {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vertex {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Dot product of two vertices (as vectors).
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Cross product of two vertices (as vectors).
    pub fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    /// Magnitude (length) of the vector.
    pub fn magnitude(self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    /// Normalized (unit) vector.
    pub fn normalized(self) -> Self {
        let m = self.magnitude();
        if m > 1e-15 {
            Self { x: self.x / m, y: self.y / m, z: self.z / m }
        } else {
            Self::default()
        }
    }

    /// Subtract another vertex.
    pub fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }

    /// Add another vertex.
    pub fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }

    /// Scale by a scalar.
    pub fn scale(self, s: f64) -> Self {
        Self { x: self.x * s, y: self.y * s, z: self.z * s }
    }
}

/// Surface definition with geometry and thermal properties.
#[derive(Debug, Clone)]
pub struct Surface {
    pub name: String,
    pub class: SurfaceClass,
    pub heat_transfer_model: HeatTransferModel,

    // Geometry
    pub vertices: Vec<Vertex>,
    pub area: f64,               // m2 (net, excluding subsurfaces)
    pub gross_area: f64,         // m2 (including subsurfaces)
    pub tilt: Angle,             // degrees from horizontal (0=horiz, 90=vert, 180=inverted)
    pub azimuth: Angle,          // degrees from north (0=N, 90=E, 180=S, 270=W)
    pub outward_normal: Vertex,  // unit outward normal vector
    pub centroid: Vertex,        // geometric center

    // Precomputed trig
    pub sin_tilt: f64,
    pub cos_tilt: f64,
    pub sin_azimuth: f64,
    pub cos_azimuth: f64,

    // Topology
    pub construction_index: usize,
    pub zone_index: usize,
    pub base_surface_index: Option<usize>,  // Parent for subsurfaces
    pub exterior_boundary: ExteriorBoundary,

    // Exposure
    pub ext_solar: bool,         // Exposed to solar radiation
    pub ext_wind: bool,          // Exposed to wind

    // View factors
    pub view_factor_sky: f64,
    pub view_factor_ground: f64,

    // Enclosures
    pub radiant_enclosure_index: usize,
    pub solar_enclosure_index: usize,

    // Subsurface tracking
    pub subsurface_indices: Vec<usize>,
}

impl Surface {
    /// Create a new surface from vertices and compute geometry.
    pub fn new(
        name: impl Into<String>,
        class: SurfaceClass,
        vertices: Vec<Vertex>,
        construction_index: usize,
        zone_index: usize,
    ) -> Self {
        let (area, normal) = geometry::newell_area_and_normal(&vertices);
        let centroid = geometry::centroid(&vertices);
        let tilt = geometry::tilt_from_normal(&normal);
        let azimuth = geometry::azimuth_from_normal(&normal);

        let tilt_rad = tilt.to_degrees() * std::f64::consts::PI / 180.0;
        let azimuth_rad = azimuth.to_degrees() * std::f64::consts::PI / 180.0;

        // Default view factors based on tilt
        let cos_tilt_val = tilt_rad.cos();
        let view_factor_sky = (1.0 + cos_tilt_val) / 2.0;
        let view_factor_ground = (1.0 - cos_tilt_val) / 2.0;

        // Default exposure based on class
        let is_exterior = !class.is_subsurface() && class.is_heat_transfer();

        Self {
            name: name.into(),
            class,
            heat_transfer_model: if class.is_heat_transfer() {
                HeatTransferModel::CTF
            } else {
                HeatTransferModel::None
            },
            vertices,
            area,
            gross_area: area,
            tilt,
            azimuth,
            outward_normal: normal.normalized(),
            centroid,
            sin_tilt: tilt_rad.sin(),
            cos_tilt: tilt_rad.cos(),
            sin_azimuth: azimuth_rad.sin(),
            cos_azimuth: azimuth_rad.cos(),
            construction_index,
            zone_index,
            base_surface_index: None,
            exterior_boundary: ExteriorBoundary::Exterior,
            ext_solar: is_exterior,
            ext_wind: is_exterior,
            view_factor_sky,
            view_factor_ground,
            radiant_enclosure_index: 0,
            solar_enclosure_index: 0,
            subsurface_indices: Vec::new(),
        }
    }

    /// Is the surface vertical (within 22.5 degrees)?
    pub fn is_vertical(&self) -> bool {
        self.cos_tilt.abs() < 0.3827 // cos(67.5°)
    }

    /// Is the surface horizontal (within 22.5 degrees)?
    pub fn is_horizontal(&self) -> bool {
        self.cos_tilt.abs() >= 0.9239 // cos(22.5°)
    }

    /// Is the surface tilted (between horizontal and vertical)?
    pub fn is_tilted(&self) -> bool {
        !self.is_vertical() && !self.is_horizontal()
    }

    /// Number of vertices.
    pub fn num_vertices(&self) -> usize {
        self.vertices.len()
    }

    /// Is this an exterior surface?
    pub fn is_exterior(&self) -> bool {
        matches!(self.exterior_boundary, ExteriorBoundary::Exterior)
    }

    /// Is this a ground-contact surface?
    pub fn is_ground(&self) -> bool {
        matches!(self.exterior_boundary, ExteriorBoundary::Ground)
    }
}

/// Zone data for heat balance calculations.
#[derive(Debug, Clone)]
pub struct Zone {
    pub name: String,
    pub volume: f64,             // m3
    pub floor_area: f64,         // m2
    pub ceiling_height: f64,     // m
    pub multiplier: u32,         // zone replication
    pub origin: Vertex,          // zone origin in building coordinates

    // Surface indices belonging to this zone
    pub surface_indices: Vec<usize>,
    pub window_indices: Vec<usize>,

    // Enclosure indices
    pub radiant_enclosure_index: usize,
    pub solar_enclosure_index: usize,

    // Geometry bounds
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
    pub min_z: f64,
    pub max_z: f64,

    // Flags
    pub has_floor: bool,
    pub has_roof: bool,
    pub has_window: bool,
}

impl Zone {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            volume: 0.0,
            floor_area: 0.0,
            ceiling_height: 0.0,
            multiplier: 1,
            origin: Vertex::default(),
            surface_indices: Vec::new(),
            window_indices: Vec::new(),
            radiant_enclosure_index: 0,
            solar_enclosure_index: 0,
            min_x: f64::MAX,
            max_x: f64::MIN,
            min_y: f64::MAX,
            max_y: f64::MIN,
            min_z: f64::MAX,
            max_z: f64::MIN,
            has_floor: false,
            has_roof: false,
            has_window: false,
        }
    }

    /// Update bounds from a vertex.
    pub fn extend_bounds(&mut self, v: &Vertex) {
        self.min_x = self.min_x.min(v.x);
        self.max_x = self.max_x.max(v.x);
        self.min_y = self.min_y.min(v.y);
        self.max_y = self.max_y.max(v.y);
        self.min_z = self.min_z.min(v.z);
        self.max_z = self.max_z.max(v.z);
    }
}

/// Surface database holding all surfaces and zones.
#[derive(Debug, Default)]
pub struct SurfaceDatabase {
    pub surfaces: Vec<Surface>,
    pub zones: Vec<Zone>,
}

impl SurfaceDatabase {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a surface and return its index.
    pub fn add_surface(&mut self, surface: Surface) -> usize {
        let idx = self.surfaces.len();
        self.surfaces.push(surface);
        idx
    }

    /// Add a zone and return its index.
    pub fn add_zone(&mut self, zone: Zone) -> usize {
        let idx = self.zones.len();
        self.zones.push(zone);
        idx
    }

    /// Get a surface by index.
    pub fn get_surface(&self, index: usize) -> Option<&Surface> {
        self.surfaces.get(index)
    }

    /// Get a mutable surface by index.
    pub fn get_surface_mut(&mut self, index: usize) -> Option<&mut Surface> {
        self.surfaces.get_mut(index)
    }

    /// Exterior heat transfer surfaces.
    pub fn exterior_surfaces(&self) -> impl Iterator<Item = (usize, &Surface)> {
        self.surfaces.iter().enumerate().filter(|(_, s)| {
            s.class.is_heat_transfer() && s.is_exterior()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_operations() {
        let a = Vertex::new(1.0, 0.0, 0.0);
        let b = Vertex::new(0.0, 1.0, 0.0);

        let c = a.cross(b);
        assert!((c.x).abs() < 1e-10);
        assert!((c.y).abs() < 1e-10);
        assert!((c.z - 1.0).abs() < 1e-10);

        assert!((a.dot(b)).abs() < 1e-10);
        assert!((a.magnitude() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn surface_class_properties() {
        assert!(SurfaceClass::Wall.is_heat_transfer());
        assert!(!SurfaceClass::Shading.is_heat_transfer());
        assert!(SurfaceClass::Window.is_subsurface());
        assert!(!SurfaceClass::Wall.is_subsurface());
    }

    #[test]
    fn vertical_wall_surface() {
        // Vertical south-facing wall
        let verts = vec![
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(10.0, 0.0, 0.0),
            Vertex::new(10.0, 0.0, 3.0),
            Vertex::new(0.0, 0.0, 3.0),
        ];
        let s = Surface::new("SouthWall", SurfaceClass::Wall, verts, 0, 0);

        assert!((s.area - 30.0).abs() < 0.01, "area={}", s.area);
        assert!(s.is_vertical(), "tilt={}", s.tilt.to_degrees());
        assert!(s.is_exterior());
    }

    #[test]
    fn horizontal_floor_surface() {
        // Horizontal floor
        let verts = vec![
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(10.0, 0.0, 0.0),
            Vertex::new(10.0, 10.0, 0.0),
            Vertex::new(0.0, 10.0, 0.0),
        ];
        let s = Surface::new("Floor", SurfaceClass::Floor, verts, 0, 0);

        assert!((s.area - 100.0).abs() < 0.01, "area={}", s.area);
        assert!(s.is_horizontal(), "tilt={}", s.tilt.to_degrees());
    }

    #[test]
    fn zone_bounds() {
        let mut z = Zone::new("TestZone");
        z.extend_bounds(&Vertex::new(0.0, 0.0, 0.0));
        z.extend_bounds(&Vertex::new(10.0, 5.0, 3.0));
        assert_eq!(z.min_x, 0.0);
        assert_eq!(z.max_x, 10.0);
        assert_eq!(z.max_z, 3.0);
    }
}
