//! Solar and shading calculations for EnergyPlus-rs.
//!
//! Calculates incident solar radiation on tilted surfaces, shadow casting
//! via polygon clipping, and solar distribution through windows.

use ep_units::*;

pub mod distribution;
pub mod incident;
pub mod shading;

/// Incident solar radiation decomposed into components.
#[derive(Debug, Clone, Default)]
pub struct SurfaceSolarIncident {
    /// Direct (beam) component on the surface (W/m2).
    pub beam: Irradiance,
    /// Diffuse sky component on the surface (W/m2).
    pub diffuse_sky: Irradiance,
    /// Ground-reflected component (W/m2).
    pub diffuse_ground: Irradiance,
    /// Total incident solar (W/m2).
    pub total: Irradiance,
    /// Cosine of the angle of incidence.
    pub cos_incidence: f64,
    /// Sunlit fraction (0 to 1).
    pub sunlit_fraction: f64,
}

/// Sky diffuse irradiance model selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkyDiffuseModel {
    /// Isotropic: uniform sky dome radiation.
    #[default]
    Isotropic,
    /// Perez anisotropic: circumsolar + horizon brightening.
    Perez,
    /// Hay-Davies-Klucher-Reindl model.
    HDKR,
}

/// Ground reflectance (albedo) properties.
#[derive(Debug, Clone)]
pub struct GroundReflectance {
    /// Monthly ground reflectance values (12 months).
    pub monthly: [f64; 12],
}

impl Default for GroundReflectance {
    fn default() -> Self {
        Self {
            monthly: [0.2; 12], // Default 0.2 for all months
        }
    }
}

impl GroundReflectance {
    /// Get reflectance for a given month (1-12).
    pub fn for_month(&self, month: u8) -> f64 {
        let idx = (month.saturating_sub(1) as usize).min(11);
        self.monthly[idx]
    }
}

/// Solar calculator for computing incident solar on surfaces.
#[derive(Debug, Clone)]
pub struct SolarCalculator {
    pub sky_model: SkyDiffuseModel,
    pub ground_reflectance: GroundReflectance,
}

impl Default for SolarCalculator {
    fn default() -> Self {
        Self {
            sky_model: SkyDiffuseModel::Isotropic,
            ground_reflectance: GroundReflectance::default(),
        }
    }
}

impl SolarCalculator {
    /// Calculate incident solar radiation on a tilted surface.
    pub fn incident_solar(
        &self,
        dni: Irradiance,           // Direct normal irradiance
        dhi: Irradiance,           // Diffuse horizontal irradiance
        ghi: Irradiance,           // Global horizontal irradiance
        cos_incidence: f64,        // Cosine of angle of incidence
        surface_tilt: Angle,       // Surface tilt from horizontal
        albedo: f64,               // Ground reflectance
        sunlit_fraction: f64,      // Fraction of surface in sunlight
    ) -> SurfaceSolarIncident {
        // Beam component
        let cos_inc = cos_incidence.max(0.0);
        let beam = Irradiance::new(dni.value() * cos_inc * sunlit_fraction);

        // Diffuse sky component (isotropic model)
        let cos_tilt = surface_tilt.cos();
        let diffuse_sky = incident::isotropic_diffuse(dhi, cos_tilt);

        // Ground-reflected component
        let diffuse_ground = incident::ground_reflected(ghi, cos_tilt, albedo);

        let total = Irradiance::new(beam.value() + diffuse_sky.value() + diffuse_ground.value());

        SurfaceSolarIncident {
            beam,
            diffuse_sky,
            diffuse_ground,
            total,
            cos_incidence: cos_inc,
            sunlit_fraction,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incident_solar_south_wall_noon() {
        let calc = SolarCalculator::default();
        let result = calc.incident_solar(
            Irradiance::new(800.0),   // DNI
            Irradiance::new(100.0),   // DHI
            Irradiance::new(700.0),   // GHI
            0.866,                     // cos(30°) - good angle
            Angle::from_degrees(90.0), // Vertical wall
            0.2,                       // Albedo
            1.0,                       // Fully sunlit
        );

        assert!(result.beam.value() > 600.0, "beam={}", result.beam.value());
        assert!(result.diffuse_sky.value() > 40.0, "dif_sky={}", result.diffuse_sky.value());
        assert!(result.diffuse_ground.value() > 50.0, "dif_gnd={}", result.diffuse_ground.value());
        assert!(result.total.value() > result.beam.value());
    }

    #[test]
    fn incident_solar_nighttime() {
        let calc = SolarCalculator::default();
        let result = calc.incident_solar(
            Irradiance::new(0.0),
            Irradiance::new(0.0),
            Irradiance::new(0.0),
            -0.5,
            Angle::from_degrees(90.0),
            0.2,
            0.0,
        );

        assert!(result.total.value().abs() < 1e-10);
    }

    #[test]
    fn ground_reflectance_monthly() {
        let gr = GroundReflectance {
            monthly: [0.6, 0.6, 0.5, 0.3, 0.2, 0.2, 0.2, 0.2, 0.2, 0.3, 0.5, 0.6],
        };
        assert_eq!(gr.for_month(1), 0.6);
        assert_eq!(gr.for_month(6), 0.2);
        assert_eq!(gr.for_month(12), 0.6);
    }
}
