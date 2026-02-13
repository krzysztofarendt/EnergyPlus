//! Material and construction definitions for EnergyPlus-rs.
//!
//! Defines material thermal/optical properties and construction layer assemblies.
//! Supports opaque materials, glazing layers, gas fills, and shade/screen/blind layers.

use ep_units::*;

pub mod gas;

/// Maximum number of layers in a construction.
pub const MAX_LAYERS: usize = 11;

/// Maximum number of CTF terms.
pub const MAX_CTF_TERMS: usize = 19;

/// Surface roughness classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SurfaceRoughness {
    VeryRough = 0,
    Rough = 1,
    #[default]
    MediumRough = 2,
    MediumSmooth = 3,
    Smooth = 4,
    VerySmooth = 5,
}

impl SurfaceRoughness {
    /// ASHRAE simple exterior convection coefficients D, E, F.
    /// h = D + E * V_wind + F * V_wind^2
    pub fn ashrae_ext_conv_coeffs(self) -> (f64, f64, f64) {
        match self {
            Self::VeryRough => (11.58, 5.894, 0.0),
            Self::Rough => (12.49, 4.065, 0.028),
            Self::MediumRough => (10.79, 4.192, 0.0),
            Self::MediumSmooth => (8.23, 4.00, -0.057),
            Self::Smooth => (10.22, 3.100, 0.0),
            Self::VerySmooth => (8.23, 3.33, -0.036),
        }
    }

    /// DOE-2 roughness multiplier for exterior convection.
    pub fn doe2_roughness_multiplier(self) -> f64 {
        match self {
            Self::VeryRough => 2.17,
            Self::Rough => 1.67,
            Self::MediumRough => 1.52,
            Self::MediumSmooth => 1.13,
            Self::Smooth => 1.11,
            Self::VerySmooth => 1.0,
        }
    }
}

/// Material definition -- the base for all material types.
#[derive(Debug, Clone)]
pub enum Material {
    /// Regular opaque material with thermal properties.
    Opaque(OpaqueMaterial),
    /// Resistance-only material (no mass, just R-value).
    ResistanceOnly(ResistanceOnlyMaterial),
    /// Air gap with thermal resistance.
    AirGap(AirGapMaterial),
    /// Glass layer for windows.
    Glass(GlassMaterial),
    /// Simple window (specified by U-factor and SHGC).
    SimpleWindow(SimpleWindowMaterial),
    /// Gas fill between glazing layers.
    Gas(GasMaterial),
    /// Gas mixture fill.
    GasMixture(GasMixtureMaterial),
}

impl Material {
    /// Get the material name.
    pub fn name(&self) -> &str {
        match self {
            Self::Opaque(m) => &m.name,
            Self::ResistanceOnly(m) => &m.name,
            Self::AirGap(m) => &m.name,
            Self::Glass(m) => &m.name,
            Self::SimpleWindow(m) => &m.name,
            Self::Gas(m) => &m.name,
            Self::GasMixture(m) => &m.name,
        }
    }

    /// Get the thickness, if the material has one.
    pub fn thickness(&self) -> Option<Length> {
        match self {
            Self::Opaque(m) => Some(m.thickness),
            Self::Glass(m) => Some(m.thickness),
            Self::Gas(m) => Some(m.thickness),
            Self::GasMixture(m) => Some(m.thickness),
            _ => None,
        }
    }

    /// Get the thermal resistance (R-value, m2-K/W).
    pub fn resistance(&self) -> f64 {
        match self {
            Self::Opaque(m) => {
                if m.conductivity > 0.0 {
                    m.thickness.value() / m.conductivity
                } else {
                    0.0
                }
            }
            Self::ResistanceOnly(m) => m.resistance,
            Self::AirGap(m) => m.resistance,
            Self::Glass(m) => {
                if m.conductivity > 0.0 {
                    m.thickness.value() / m.conductivity
                } else {
                    0.0
                }
            }
            Self::Gas(_) | Self::GasMixture(_) => 0.0, // Calculated from gas properties
            Self::SimpleWindow(_) => 0.0,
        }
    }
}

/// Regular opaque material with full thermal properties.
#[derive(Debug, Clone)]
pub struct OpaqueMaterial {
    pub name: String,
    pub roughness: SurfaceRoughness,
    pub thickness: Length,          // m
    pub conductivity: f64,          // W/(m-K)
    pub density: f64,               // kg/m3
    pub specific_heat: f64,         // J/(kg-K)
    pub absorptance_thermal: f64,   // Long-wave (IR) absorptance
    pub absorptance_solar: f64,     // Solar absorptance
    pub absorptance_visible: f64,   // Visible absorptance
}

impl Default for OpaqueMaterial {
    fn default() -> Self {
        Self {
            name: String::new(),
            roughness: SurfaceRoughness::MediumRough,
            thickness: Length::new(0.1),
            conductivity: 1.0,
            density: 2000.0,
            specific_heat: 1000.0,
            absorptance_thermal: 0.9,
            absorptance_solar: 0.7,
            absorptance_visible: 0.7,
        }
    }
}

impl OpaqueMaterial {
    /// Thermal diffusivity (m2/s).
    pub fn thermal_diffusivity(&self) -> f64 {
        if self.density > 0.0 && self.specific_heat > 0.0 {
            self.conductivity / (self.density * self.specific_heat)
        } else {
            0.0
        }
    }
}

/// Resistance-only material (no mass).
#[derive(Debug, Clone)]
pub struct ResistanceOnlyMaterial {
    pub name: String,
    pub resistance: f64, // m2-K/W
}

/// Air gap with specified thermal resistance.
#[derive(Debug, Clone)]
pub struct AirGapMaterial {
    pub name: String,
    pub resistance: f64, // m2-K/W
}

/// Glass material for fenestration.
#[derive(Debug, Clone)]
pub struct GlassMaterial {
    pub name: String,
    pub thickness: Length,
    pub conductivity: f64,          // W/(m-K)
    pub solar_transmittance: f64,   // Normal incidence
    pub solar_reflectance_front: f64,
    pub solar_reflectance_back: f64,
    pub visible_transmittance: f64,
    pub visible_reflectance_front: f64,
    pub visible_reflectance_back: f64,
    pub ir_transmittance: f64,
    pub emissivity_front: f64,      // IR emissivity
    pub emissivity_back: f64,
    pub dirt_correction_factor: f64,
    pub is_solar_diffusing: bool,
}

impl Default for GlassMaterial {
    fn default() -> Self {
        Self {
            name: String::new(),
            thickness: Length::new(0.006),
            conductivity: 0.9,
            solar_transmittance: 0.775,
            solar_reflectance_front: 0.071,
            solar_reflectance_back: 0.071,
            visible_transmittance: 0.881,
            visible_reflectance_front: 0.080,
            visible_reflectance_back: 0.080,
            ir_transmittance: 0.0,
            emissivity_front: 0.84,
            emissivity_back: 0.84,
            dirt_correction_factor: 1.0,
            is_solar_diffusing: false,
        }
    }
}

/// Simple window defined by U-factor and SHGC.
#[derive(Debug, Clone)]
pub struct SimpleWindowMaterial {
    pub name: String,
    pub u_factor: f64,           // W/(m2-K)
    pub shgc: f64,               // Solar heat gain coefficient
    pub visible_transmittance: Option<f64>,
}

/// Gas type for window gap fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GasType {
    #[default]
    Air,
    Argon,
    Krypton,
    Xenon,
    Custom,
}

/// Gas fill material.
#[derive(Debug, Clone)]
pub struct GasMaterial {
    pub name: String,
    pub gas_type: GasType,
    pub thickness: Length,
}

/// Gas mixture material (up to 5 gases).
#[derive(Debug, Clone)]
pub struct GasMixtureMaterial {
    pub name: String,
    pub thickness: Length,
    pub gases: Vec<(GasType, f64)>, // (gas_type, fraction)
}

// ─── Construction ─────────────────────────────────────────────────────

/// A construction is an ordered list of material layers (outside to inside).
#[derive(Debug, Clone)]
pub struct Construction {
    pub name: String,
    /// Material indices, outside layer first.
    pub layers: Vec<usize>,
    /// Is this a window construction?
    pub is_window: bool,
    /// Outside surface roughness (from outermost opaque layer).
    pub outside_roughness: SurfaceRoughness,
    /// Thermal absorptance of inside surface.
    pub inside_absorptance_thermal: f64,
    /// Thermal absorptance of outside surface.
    pub outside_absorptance_thermal: f64,
    /// Solar absorptance of inside surface.
    pub inside_absorptance_solar: f64,
    /// Solar absorptance of outside surface.
    pub outside_absorptance_solar: f64,
    /// Visible absorptance of inside surface.
    pub inside_absorptance_visible: f64,
    /// Visible absorptance of outside surface.
    pub outside_absorptance_visible: f64,
    /// CTF coefficients (computed from material properties).
    pub ctf: Option<CtfCoefficients>,
    /// Overall U-value (W/(m2-K)).
    pub u_value: f64,
}

impl Construction {
    /// Create a new empty construction.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            layers: Vec::new(),
            is_window: false,
            outside_roughness: SurfaceRoughness::MediumRough,
            inside_absorptance_thermal: 0.9,
            outside_absorptance_thermal: 0.9,
            inside_absorptance_solar: 0.7,
            outside_absorptance_solar: 0.7,
            inside_absorptance_visible: 0.7,
            outside_absorptance_visible: 0.7,
            ctf: None,
            u_value: 0.0,
        }
    }

    /// Total number of layers.
    pub fn num_layers(&self) -> usize {
        self.layers.len()
    }

    /// Calculate total thickness from material database.
    pub fn total_thickness(&self, materials: &MaterialDatabase) -> Length {
        let mut total = 0.0;
        for &layer_idx in &self.layers {
            if let Some(mat) = materials.get(layer_idx) {
                if let Some(t) = mat.thickness() {
                    total += t.value();
                }
            }
        }
        Length::new(total)
    }

    /// Calculate total R-value (excluding air films) from material database.
    pub fn total_resistance(&self, materials: &MaterialDatabase) -> f64 {
        let mut r_total = 0.0;
        for &layer_idx in &self.layers {
            if let Some(mat) = materials.get(layer_idx) {
                r_total += mat.resistance();
            }
        }
        r_total
    }

    /// Set surface absorptances from outermost and innermost opaque materials.
    pub fn set_absorptances_from_materials(&mut self, materials: &MaterialDatabase) {
        // Outside (first layer)
        if let Some(&first) = self.layers.first() {
            if let Some(Material::Opaque(m)) = materials.get(first) {
                self.outside_roughness = m.roughness;
                self.outside_absorptance_thermal = m.absorptance_thermal;
                self.outside_absorptance_solar = m.absorptance_solar;
                self.outside_absorptance_visible = m.absorptance_visible;
            }
        }
        // Inside (last layer)
        if let Some(&last) = self.layers.last() {
            if let Some(Material::Opaque(m)) = materials.get(last) {
                self.inside_absorptance_thermal = m.absorptance_thermal;
                self.inside_absorptance_solar = m.absorptance_solar;
                self.inside_absorptance_visible = m.absorptance_visible;
            }
        }
    }
}

/// CTF (Conduction Transfer Function) coefficients for a construction.
///
/// These are precomputed during setup from the material layer properties
/// using the state-space method.
#[derive(Debug, Clone)]
pub struct CtfCoefficients {
    /// Outside surface response factors (X series).
    pub outside: Vec<f64>,
    /// Cross response factors (Y series).
    pub cross: Vec<f64>,
    /// Inside surface response factors (Z series).
    pub inside: Vec<f64>,
    /// Flux history coefficients (Phi series).
    pub flux: Vec<f64>,
    /// Number of history terms.
    pub num_terms: usize,
    /// Number of timestep histories needed.
    pub num_histories: usize,
    /// CTF time step (seconds). May be >= simulation time step.
    pub time_step: f64,
}

impl CtfCoefficients {
    /// Evaluate outside surface heat flux using CTF.
    ///
    /// q_outside = Sum(X[j] * T_outside[t-j]) + Sum(Y[j] * T_inside[t-j])
    ///           + Sum(Phi[j] * q_outside[t-j])
    pub fn outside_flux(
        &self,
        t_outside_history: &[f64],
        t_inside_history: &[f64],
        flux_history: &[f64],
    ) -> f64 {
        let n = self.num_terms;
        let mut q = 0.0;
        for j in 0..=n {
            if j < t_outside_history.len() {
                q += self.outside[j] * t_outside_history[j];
            }
            if j < t_inside_history.len() {
                q += self.cross[j] * t_inside_history[j];
            }
        }
        for j in 1..=n {
            if j < flux_history.len() + 1 {
                q += self.flux[j] * flux_history[j - 1];
            }
        }
        q
    }

    /// Evaluate inside surface heat flux using CTF.
    ///
    /// q_inside = Sum(Y[j] * T_outside[t-j]) + Sum(Z[j] * T_inside[t-j])
    ///          + Sum(Phi[j] * q_inside[t-j])
    pub fn inside_flux(
        &self,
        t_outside_history: &[f64],
        t_inside_history: &[f64],
        flux_history: &[f64],
    ) -> f64 {
        let n = self.num_terms;
        let mut q = 0.0;
        for j in 0..=n {
            if j < t_outside_history.len() {
                q += self.cross[j] * t_outside_history[j];
            }
            if j < t_inside_history.len() {
                q += self.inside[j] * t_inside_history[j];
            }
        }
        for j in 1..=n {
            if j < flux_history.len() + 1 {
                q += self.flux[j] * flux_history[j - 1];
            }
        }
        q
    }
}

// ─── Material Database ────────────────────────────────────────────────

/// Database holding all materials and constructions.
#[derive(Debug, Default)]
pub struct MaterialDatabase {
    pub materials: Vec<Material>,
    pub constructions: Vec<Construction>,
}

impl MaterialDatabase {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a material and return its index.
    pub fn add_material(&mut self, material: Material) -> usize {
        let idx = self.materials.len();
        self.materials.push(material);
        idx
    }

    /// Get a material by index.
    pub fn get(&self, index: usize) -> Option<&Material> {
        self.materials.get(index)
    }

    /// Add a construction and return its index.
    pub fn add_construction(&mut self, construction: Construction) -> usize {
        let idx = self.constructions.len();
        self.constructions.push(construction);
        idx
    }

    /// Get a construction by index.
    pub fn get_construction(&self, index: usize) -> Option<&Construction> {
        self.constructions.get(index)
    }

    /// Find a material by name.
    pub fn find_material(&self, name: &str) -> Option<usize> {
        self.materials.iter().position(|m| m.name() == name)
    }

    /// Find a construction by name.
    pub fn find_construction(&self, name: &str) -> Option<usize> {
        self.constructions.iter().position(|c| c.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_material_defaults() {
        let m = OpaqueMaterial::default();
        assert!(m.conductivity > 0.0);
        assert!(m.density > 0.0);
        assert!(m.specific_heat > 0.0);
        assert!(m.thermal_diffusivity() > 0.0);
    }

    #[test]
    fn opaque_material_resistance() {
        let m = Material::Opaque(OpaqueMaterial {
            name: "Concrete".into(),
            thickness: Length::new(0.2),
            conductivity: 1.4,
            ..Default::default()
        });
        let r = m.resistance();
        assert!((r - 0.2 / 1.4).abs() < 1e-10);
    }

    #[test]
    fn construction_total_resistance() {
        let mut db = MaterialDatabase::new();

        let concrete_idx = db.add_material(Material::Opaque(OpaqueMaterial {
            name: "Concrete".into(),
            thickness: Length::new(0.2),
            conductivity: 1.4,
            ..Default::default()
        }));
        let insulation_idx = db.add_material(Material::Opaque(OpaqueMaterial {
            name: "Insulation".into(),
            thickness: Length::new(0.05),
            conductivity: 0.04,
            ..Default::default()
        }));
        let gypsum_idx = db.add_material(Material::Opaque(OpaqueMaterial {
            name: "Gypsum".into(),
            thickness: Length::new(0.013),
            conductivity: 0.16,
            ..Default::default()
        }));

        let mut constr = Construction::new("Wall");
        constr.layers = vec![concrete_idx, insulation_idx, gypsum_idx];

        let r = constr.total_resistance(&db);
        let expected = 0.2 / 1.4 + 0.05 / 0.04 + 0.013 / 0.16;
        assert!((r - expected).abs() < 1e-10);

        let t = constr.total_thickness(&db);
        assert!((t.value() - 0.263).abs() < 1e-10);
    }

    #[test]
    fn glass_material_defaults() {
        let g = GlassMaterial::default();
        assert!(g.solar_transmittance > 0.0);
        assert!(g.visible_transmittance > 0.0);
        assert!(g.emissivity_front > 0.0);
    }

    #[test]
    fn roughness_coefficients() {
        let (d, e, f) = SurfaceRoughness::VerySmooth.ashrae_ext_conv_coeffs();
        assert!((d - 8.23).abs() < 1e-10);
        assert!((e - 3.33).abs() < 1e-10);
        assert!((f - (-0.036)).abs() < 1e-10);
    }

    #[test]
    fn ctf_simple_evaluation() {
        // Simple test: single-term CTF
        let ctf = CtfCoefficients {
            outside: vec![2.0, -1.0],
            cross: vec![1.0, -0.5],
            inside: vec![3.0, -1.5],
            flux: vec![0.0, 0.2],
            num_terms: 1,
            num_histories: 1,
            time_step: 3600.0,
        };

        let t_out = &[20.0, 18.0];
        let t_in = &[22.0, 21.0];
        let q_hist = &[5.0];

        let q_out = ctf.outside_flux(t_out, t_in, q_hist);
        // 2.0*20 + (-1.0)*18 + 1.0*22 + (-0.5)*21 + 0.2*5 = 40 - 18 + 22 - 10.5 + 1 = 34.5
        assert!((q_out - 34.5).abs() < 1e-10);
    }

    #[test]
    fn material_database() {
        let mut db = MaterialDatabase::new();
        let idx = db.add_material(Material::Opaque(OpaqueMaterial {
            name: "Test".into(),
            ..Default::default()
        }));
        assert_eq!(db.find_material("Test"), Some(idx));
        assert_eq!(db.find_material("NotFound"), None);
    }
}
