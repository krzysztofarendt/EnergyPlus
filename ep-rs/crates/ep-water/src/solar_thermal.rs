//! Solar thermal collector models.
//!
//! Flat-plate and evacuated-tube collectors based on ASHRAE/ISO
//! efficiency equations: eta = FR * (tau_alpha) - FR * UL * (Ti - Ta) / G

/// Collector type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectorType {
    FlatPlate,
    EvacuatedTube,
    IntegralCollectorStorage,
}

/// Solar thermal collector.
#[derive(Debug, Clone)]
pub struct SolarCollector {
    pub name: String,
    pub collector_type: CollectorType,
    /// Gross collector area (m²).
    pub gross_area: f64,
    /// FR * (tau*alpha) — optical efficiency (dimensionless).
    pub fr_tau_alpha: f64,
    /// FR * UL — first-order heat loss coefficient (W/m²·K).
    pub fr_ul: f64,
    /// Second-order heat loss coefficient (W/m²·K²).
    pub fr_ul2: f64,
    /// Incident angle modifier coefficient (b0 in Souka-Safwat).
    pub iam_coefficient: f64,
    /// Test flow rate per unit area (kg/s·m²).
    pub test_flow_per_area: f64,
    /// Maximum flow rate (kg/s).
    pub max_flow_rate: f64,
    /// Fluid specific heat (J/kg·K).
    pub fluid_cp: f64,
}

/// Collector calculation result.
#[derive(Debug, Clone, Copy, Default)]
pub struct CollectorResult {
    /// Useful heat gain (W).
    pub useful_gain: f64,
    /// Collector efficiency.
    pub efficiency: f64,
    /// Outlet fluid temperature (C).
    pub outlet_temp: f64,
    /// Inlet fluid temperature (C).
    pub inlet_temp: f64,
    /// Incident solar on collector (W).
    pub incident_solar: f64,
    /// Heat loss (W).
    pub heat_loss: f64,
}

impl SolarCollector {
    pub fn flat_plate(name: impl Into<String>, gross_area: f64) -> Self {
        Self {
            name: name.into(),
            collector_type: CollectorType::FlatPlate,
            gross_area,
            fr_tau_alpha: 0.75,
            fr_ul: 4.5,
            fr_ul2: 0.0,
            iam_coefficient: 0.1,
            test_flow_per_area: 0.02,
            max_flow_rate: gross_area * 0.02,
            fluid_cp: 4186.0,
        }
    }

    pub fn evacuated_tube(name: impl Into<String>, gross_area: f64) -> Self {
        Self {
            name: name.into(),
            collector_type: CollectorType::EvacuatedTube,
            gross_area,
            fr_tau_alpha: 0.65,
            fr_ul: 1.5,
            fr_ul2: 0.005,
            iam_coefficient: 0.05,
            test_flow_per_area: 0.02,
            max_flow_rate: gross_area * 0.02,
            fluid_cp: 4186.0,
        }
    }

    /// Calculate collector performance.
    ///
    /// `irradiance` — total incident solar irradiance on collector plane (W/m²).
    /// `inlet_temp` — fluid inlet temperature (C).
    /// `ambient_temp` — outdoor air temperature (C).
    /// `flow_rate` — fluid mass flow rate (kg/s).
    /// `incidence_angle` — solar incidence angle (degrees).
    pub fn calculate(
        &self,
        irradiance: f64,
        inlet_temp: f64,
        ambient_temp: f64,
        flow_rate: f64,
        incidence_angle: f64,
    ) -> CollectorResult {
        let incident_solar = irradiance * self.gross_area;

        if irradiance < 1.0 || flow_rate <= 0.0 {
            return CollectorResult {
                incident_solar,
                inlet_temp,
                outlet_temp: inlet_temp,
                ..Default::default()
            };
        }

        // Incidence angle modifier (IAM)
        let cos_theta = incidence_angle.to_radians().cos().max(0.0);
        let iam = if cos_theta > 0.01 {
            1.0 - self.iam_coefficient * (1.0 / cos_theta - 1.0)
        } else {
            0.0
        };
        let iam = iam.max(0.0).min(1.0);

        // Temperature difference
        let dt = inlet_temp - ambient_temp;

        // Collector efficiency: eta = FR(τα)·IAM - FR·UL·ΔT/G - FR·UL2·ΔT²/G
        let eta = self.fr_tau_alpha * iam
            - self.fr_ul * dt / irradiance
            - self.fr_ul2 * dt * dt / irradiance;
        let eta = eta.max(0.0);

        // Useful heat gain
        let useful_gain = eta * irradiance * self.gross_area;
        let heat_loss = incident_solar - useful_gain;

        // Outlet temperature
        let outlet_temp = if flow_rate * self.fluid_cp > 0.0 {
            inlet_temp + useful_gain / (flow_rate * self.fluid_cp)
        } else {
            inlet_temp
        };

        CollectorResult {
            useful_gain,
            efficiency: eta,
            outlet_temp,
            inlet_temp,
            incident_solar,
            heat_loss,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_plate_rated() {
        let col = SolarCollector::flat_plate("FP-1", 4.0);
        let result = col.calculate(800.0, 30.0, 25.0, 0.05, 0.0);

        assert!(result.efficiency > 0.0 && result.efficiency < 1.0);
        assert!(result.useful_gain > 0.0);
        assert!(result.outlet_temp > 30.0);
    }

    #[test]
    fn flat_plate_high_inlet_temp() {
        let col = SolarCollector::flat_plate("FP-1", 4.0);
        let low_inlet = col.calculate(800.0, 30.0, 25.0, 0.05, 0.0);
        let high_inlet = col.calculate(800.0, 80.0, 25.0, 0.05, 0.0);

        // Higher inlet temp → more losses → lower efficiency
        assert!(high_inlet.efficiency < low_inlet.efficiency);
    }

    #[test]
    fn flat_plate_no_sun() {
        let col = SolarCollector::flat_plate("FP-1", 4.0);
        let result = col.calculate(0.0, 30.0, 25.0, 0.05, 0.0);

        assert!(result.useful_gain.abs() < 1e-10);
        assert!((result.outlet_temp - 30.0).abs() < 0.01);
    }

    #[test]
    fn evacuated_tube_vs_flat_plate() {
        let fp = SolarCollector::flat_plate("FP", 4.0);
        let et = SolarCollector::evacuated_tube("ET", 4.0);

        // At high temperature difference, evacuated tube should be better
        let fp_result = fp.calculate(800.0, 70.0, 20.0, 0.05, 0.0);
        let et_result = et.calculate(800.0, 70.0, 20.0, 0.05, 0.0);

        assert!(et_result.efficiency > fp_result.efficiency,
                "ET eff={} > FP eff={}", et_result.efficiency, fp_result.efficiency);
    }

    #[test]
    fn collector_incidence_angle() {
        let col = SolarCollector::flat_plate("FP-1", 4.0);
        let normal = col.calculate(800.0, 30.0, 25.0, 0.05, 0.0);
        let angled = col.calculate(800.0, 30.0, 25.0, 0.05, 50.0);

        // Higher incidence angle → lower IAM → lower efficiency
        assert!(angled.efficiency < normal.efficiency);
    }

    #[test]
    fn collector_stagnation() {
        let col = SolarCollector::flat_plate("FP-1", 4.0);
        // Very high inlet temp, moderate sun → efficiency should drop to 0
        let result = col.calculate(400.0, 120.0, 25.0, 0.05, 0.0);
        assert!(result.efficiency < 0.01);
    }
}
