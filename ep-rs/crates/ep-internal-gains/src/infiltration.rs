//! Infiltration models for zone air leakage.
//!
//! Three models:
//! - DesignFlowRate: simple with wind/temperature coefficients
//! - ShermanGrimsrud: effective leakage area
//! - AIM2: advanced Walker & Wilson model

use ep_psychrometrics::{cp_air, rho_air};

/// Infiltration model type.
#[derive(Debug, Clone)]
pub enum InfiltrationModel {
    /// Simple design flow rate with wind/temperature adjustments.
    DesignFlowRate(DesignFlowRateInfiltration),
    /// Sherman-Grimsrud effective leakage area model.
    ShermanGrimsrud(ShermanGrimsrudInfiltration),
    /// AIM-2 (Walker & Wilson) advanced model.
    AIM2(AIM2Infiltration),
}

/// Result of infiltration calculation.
#[derive(Debug, Clone, Copy, Default)]
pub struct InfiltrationOutput {
    /// Volume flow rate (m3/s).
    pub volume_flow_rate: f64,
    /// Mass flow rate (kg/s).
    pub mass_flow_rate: f64,
    /// Sensible heat gain/loss (W). Positive = heat into zone.
    pub sensible_heat: f64,
    /// Latent heat gain/loss (W). Positive = moisture into zone.
    pub latent_heat: f64,
    /// Air changes per hour.
    pub air_changes_per_hour: f64,
}

/// Design flow rate infiltration model.
///
/// V_dot = V_design * schedule * (A + B*|dT| + C*V_wind + D*V_wind^2)
#[derive(Debug, Clone)]
pub struct DesignFlowRateInfiltration {
    pub name: String,
    /// Design volume flow rate (m3/s).
    pub design_flow_rate: f64,
    /// Constant term coefficient.
    pub constant_coef: f64,
    /// Temperature term coefficient.
    pub temperature_coef: f64,
    /// Wind speed term coefficient.
    pub velocity_coef: f64,
    /// Wind speed squared term coefficient.
    pub velocity_sq_coef: f64,
}

impl DesignFlowRateInfiltration {
    /// Create with constant infiltration (A=1, B=C=D=0).
    pub fn constant(name: impl Into<String>, flow_rate: f64) -> Self {
        Self {
            name: name.into(),
            design_flow_rate: flow_rate,
            constant_coef: 1.0,
            temperature_coef: 0.0,
            velocity_coef: 0.0,
            velocity_sq_coef: 0.0,
        }
    }

    /// Calculate infiltration rate.
    pub fn calculate(
        &self,
        schedule: f64,
        t_zone: f64,
        t_outdoor: f64,
        wind_speed: f64,
    ) -> f64 {
        let dt = (t_zone - t_outdoor).abs();
        let factor = self.constant_coef
            + self.temperature_coef * dt
            + self.velocity_coef * wind_speed
            + self.velocity_sq_coef * wind_speed * wind_speed;

        self.design_flow_rate * schedule.max(0.0) * factor.max(0.0)
    }
}

/// Sherman-Grimsrud effective leakage area model.
///
/// V_dot = AL * sqrt(Cs * |dT| + Cw * V_wind^2)
#[derive(Debug, Clone)]
pub struct ShermanGrimsrudInfiltration {
    pub name: String,
    /// Effective air leakage area (m2).
    pub leakage_area: f64,
    /// Stack coefficient (m3/(s*m2*K)).
    pub stack_coefficient: f64,
    /// Wind coefficient (m3/(s*m2*(m/s)^2)).
    pub wind_coefficient: f64,
}

impl ShermanGrimsrudInfiltration {
    pub fn new(name: impl Into<String>, leakage_area: f64) -> Self {
        Self {
            name: name.into(),
            leakage_area,
            stack_coefficient: 0.000145,
            wind_coefficient: 0.000174,
        }
    }

    /// Calculate infiltration volume flow rate (m3/s).
    pub fn calculate(
        &self,
        _schedule: f64,
        t_zone: f64,
        t_outdoor: f64,
        wind_speed: f64,
    ) -> f64 {
        let dt = (t_zone - t_outdoor).abs();
        let under_sqrt = self.stack_coefficient * dt + self.wind_coefficient * wind_speed * wind_speed;
        self.leakage_area * under_sqrt.max(0.0).sqrt()
    }
}

/// AIM-2 (Walker & Wilson) advanced infiltration model.
///
/// V_dot = c * sqrt((Cs*|dT|)^n + (Cw*(s*V_wind))^(2*n))^(1/n)
#[derive(Debug, Clone)]
pub struct AIM2Infiltration {
    pub name: String,
    /// Flow coefficient (m3/s at 1 Pa).
    pub flow_coefficient: f64,
    /// Stack coefficient.
    pub stack_coefficient: f64,
    /// Wind coefficient.
    pub wind_coefficient: f64,
    /// Pressure exponent (typically 0.65-0.67).
    pub pressure_exponent: f64,
    /// Shelter factor (0-1).
    pub shelter_factor: f64,
}

impl AIM2Infiltration {
    pub fn new(name: impl Into<String>, flow_coefficient: f64) -> Self {
        Self {
            name: name.into(),
            flow_coefficient,
            stack_coefficient: 0.000145,
            wind_coefficient: 0.000174,
            pressure_exponent: 0.65,
            shelter_factor: 0.5,
        }
    }

    /// Calculate infiltration volume flow rate (m3/s).
    pub fn calculate(
        &self,
        _schedule: f64,
        t_zone: f64,
        t_outdoor: f64,
        wind_speed: f64,
    ) -> f64 {
        let dt = (t_zone - t_outdoor).abs();
        let n = self.pressure_exponent;
        let s_wind = self.shelter_factor * wind_speed;

        let stack_term = (self.stack_coefficient * dt).powf(n);
        let wind_term = (self.wind_coefficient * s_wind * s_wind).powf(n);

        self.flow_coefficient * (stack_term + wind_term).powf(1.0 / n)
    }
}

/// Calculate infiltration thermal effects from a volume flow rate.
pub fn infiltration_thermal_output(
    volume_flow_rate: f64,
    t_zone: f64,
    t_outdoor: f64,
    w_zone: f64,
    w_outdoor: f64,
    pressure: f64,
    zone_volume: f64,
) -> InfiltrationOutput {
    if volume_flow_rate <= 0.0 {
        return InfiltrationOutput::default();
    }

    let rho = rho_air(pressure, t_outdoor, w_outdoor);
    let mass_flow = volume_flow_rate * rho;
    let cp = cp_air(w_outdoor);

    let sensible = mass_flow * cp * (t_outdoor - t_zone);
    let latent = mass_flow * 2_501_000.0 * (w_outdoor - w_zone);

    let ach = if zone_volume > 0.0 {
        volume_flow_rate * 3600.0 / zone_volume
    } else {
        0.0
    };

    InfiltrationOutput {
        volume_flow_rate,
        mass_flow_rate: mass_flow,
        sensible_heat: sensible,
        latent_heat: latent,
        air_changes_per_hour: ach,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_flow_rate_constant() {
        let infil = DesignFlowRateInfiltration::constant("Test", 0.05);
        let v = infil.calculate(1.0, 22.0, 0.0, 5.0);
        // Constant: V = 0.05 * 1.0 * 1.0 = 0.05
        assert!((v - 0.05).abs() < 1e-10);
    }

    #[test]
    fn design_flow_rate_with_wind() {
        let infil = DesignFlowRateInfiltration {
            name: "WindTest".into(),
            design_flow_rate: 0.1,
            constant_coef: 0.606,
            temperature_coef: 0.03636,
            velocity_coef: 0.1177,
            velocity_sq_coef: 0.0,
        };
        let v = infil.calculate(1.0, 22.0, 0.0, 5.0);
        // factor = 0.606 + 0.03636*22 + 0.1177*5 = 0.606 + 0.7999 + 0.5885 = 1.9944
        let expected = 0.1 * 1.9944;
        assert!((v - expected).abs() < 0.001, "v={v}, expected={expected}");
    }

    #[test]
    fn sherman_grimsrud_basic() {
        let infil = ShermanGrimsrudInfiltration::new("SG", 0.01);
        let v = infil.calculate(1.0, 22.0, 0.0, 5.0);
        // V = 0.01 * sqrt(0.000145 * 22 + 0.000174 * 25)
        let expected = 0.01 * (0.000145 * 22.0 + 0.000174 * 25.0_f64).sqrt();
        assert!((v - expected).abs() < 1e-8, "v={v}, expected={expected}");
    }

    #[test]
    fn aim2_basic() {
        let infil = AIM2Infiltration::new("AIM2", 0.05);
        let v = infil.calculate(1.0, 22.0, 0.0, 5.0);
        assert!(v > 0.0);
        assert!(v < 1.0); // Reasonable for a building
    }

    #[test]
    fn infiltration_thermal_heating() {
        // Cold outdoor air infiltrating a warm zone
        let output = infiltration_thermal_output(
            0.05,  // 50 L/s
            22.0,  // zone at 22C
            -5.0,  // outdoor at -5C
            0.008, // zone humidity
            0.002, // outdoor humidity
            101325.0,
            300.0, // 300 m3 zone
        );
        assert!(output.sensible_heat < 0.0, "Should be heat loss: {}", output.sensible_heat);
        assert!(output.mass_flow_rate > 0.0);
        assert!(output.air_changes_per_hour > 0.0);
        // ACH = 0.05 * 3600 / 300 = 0.6
        assert!((output.air_changes_per_hour - 0.6).abs() < 0.01);
    }

    #[test]
    fn infiltration_zero_flow() {
        let output = infiltration_thermal_output(0.0, 22.0, 0.0, 0.008, 0.002, 101325.0, 300.0);
        assert!((output.sensible_heat).abs() < 1e-10);
        assert!((output.mass_flow_rate).abs() < 1e-10);
    }
}
