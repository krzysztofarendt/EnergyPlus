//! Ventilation models for outdoor air supply to zones.
//!
//! Two models:
//! - DesignFlowRate: fixed outdoor air with temperature/wind adjustments
//! - WindAndStack: natural ventilation through openings

use ep_psychrometrics::{cp_air, rho_air};

/// Ventilation type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VentilationType {
    /// Natural ventilation through openings.
    Natural,
    /// Mechanical intake (supply only).
    Intake,
    /// Mechanical exhaust (removal only).
    Exhaust,
    /// Balanced (equal supply and exhaust).
    Balanced,
}

/// Design flow rate ventilation.
///
/// Similar to infiltration but represents intentional outdoor air.
/// V_dot = V_design * schedule * (A + B*|dT| + C*V_wind + D*V_wind^2)
#[derive(Debug, Clone)]
pub struct DesignFlowRateVentilation {
    pub name: String,
    pub ventilation_type: VentilationType,
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
    /// Fan pressure rise (Pa) for mechanical ventilation.
    pub fan_pressure: f64,
    /// Fan efficiency (0-1).
    pub fan_efficiency: f64,
    /// Temperature control limits.
    pub min_indoor_temp: Option<f64>,
    pub max_indoor_temp: Option<f64>,
    pub min_outdoor_temp: Option<f64>,
    pub max_outdoor_temp: Option<f64>,
    /// Maximum wind speed for operation (m/s).
    pub max_wind_speed: Option<f64>,
}

impl DesignFlowRateVentilation {
    /// Create simple constant ventilation.
    pub fn constant(name: impl Into<String>, flow_rate: f64) -> Self {
        Self {
            name: name.into(),
            ventilation_type: VentilationType::Natural,
            design_flow_rate: flow_rate,
            constant_coef: 1.0,
            temperature_coef: 0.0,
            velocity_coef: 0.0,
            velocity_sq_coef: 0.0,
            fan_pressure: 0.0,
            fan_efficiency: 1.0,
            min_indoor_temp: None,
            max_indoor_temp: None,
            min_outdoor_temp: None,
            max_outdoor_temp: None,
            max_wind_speed: None,
        }
    }

    /// Check if ventilation is available given current conditions.
    pub fn is_available(&self, t_zone: f64, t_outdoor: f64, wind_speed: f64) -> bool {
        if let Some(min) = self.min_indoor_temp {
            if t_zone < min { return false; }
        }
        if let Some(max) = self.max_indoor_temp {
            if t_zone > max { return false; }
        }
        if let Some(min) = self.min_outdoor_temp {
            if t_outdoor < min { return false; }
        }
        if let Some(max) = self.max_outdoor_temp {
            if t_outdoor > max { return false; }
        }
        if let Some(max_ws) = self.max_wind_speed {
            if wind_speed > max_ws { return false; }
        }
        true
    }

    /// Calculate ventilation volume flow rate (m3/s).
    pub fn calculate(
        &self,
        schedule: f64,
        t_zone: f64,
        t_outdoor: f64,
        wind_speed: f64,
    ) -> f64 {
        if !self.is_available(t_zone, t_outdoor, wind_speed) {
            return 0.0;
        }
        let dt = (t_zone - t_outdoor).abs();
        let factor = self.constant_coef
            + self.temperature_coef * dt
            + self.velocity_coef * wind_speed
            + self.velocity_sq_coef * wind_speed * wind_speed;

        self.design_flow_rate * schedule.max(0.0) * factor.max(0.0)
    }

    /// Calculate fan power consumption (W).
    pub fn fan_power(&self, volume_flow_rate: f64) -> f64 {
        if self.fan_efficiency > 0.0 && self.fan_pressure > 0.0 {
            volume_flow_rate * self.fan_pressure / self.fan_efficiency
        } else {
            0.0
        }
    }
}

/// Wind-and-stack natural ventilation through openings.
///
/// Combines buoyancy-driven and wind-driven flows.
#[derive(Debug, Clone)]
pub struct WindAndStackVentilation {
    pub name: String,
    /// Opening area (m2).
    pub opening_area: f64,
    /// Opening effectiveness (dimensionless, typically 0.5-0.65).
    pub opening_effectiveness: f64,
    /// Effective angle of opening relative to wind (degrees).
    pub effective_angle: f64,
    /// Height difference for stack effect (m).
    pub height_difference: f64,
    /// Discharge coefficient (typically 0.65).
    pub discharge_coefficient: f64,
}

impl WindAndStackVentilation {
    pub fn new(name: impl Into<String>, opening_area: f64, height_diff: f64) -> Self {
        Self {
            name: name.into(),
            opening_area,
            opening_effectiveness: 0.55,
            effective_angle: 0.0,
            height_difference: height_diff,
            discharge_coefficient: 0.65,
        }
    }

    /// Calculate natural ventilation volume flow rate (m3/s).
    ///
    /// V = Cd * A * sqrt(dT * g * h / T_avg + Cw * v_wind^2)
    pub fn calculate(
        &self,
        schedule: f64,
        t_zone: f64,
        t_outdoor: f64,
        wind_speed: f64,
    ) -> f64 {
        let a = self.opening_area * schedule.max(0.0);
        if a <= 0.0 {
            return 0.0;
        }

        let dt = (t_zone - t_outdoor).abs();
        let t_avg_k = ((t_zone + t_outdoor) / 2.0 + 273.15).max(250.0);
        let g = 9.81;

        // Stack-driven component
        let stack = dt * g * self.height_difference.abs() / t_avg_k;

        // Wind-driven component
        let wind = self.opening_effectiveness * wind_speed * wind_speed;

        self.discharge_coefficient * a * (stack + wind).max(0.0).sqrt()
    }
}

/// Ventilation thermal output.
#[derive(Debug, Clone, Copy, Default)]
pub struct VentilationOutput {
    /// Volume flow rate (m3/s).
    pub volume_flow_rate: f64,
    /// Mass flow rate (kg/s).
    pub mass_flow_rate: f64,
    /// Sensible heat to zone (W). Positive = heating zone.
    pub sensible_heat: f64,
    /// Latent heat to zone (W).
    pub latent_heat: f64,
    /// Fan power consumption (W).
    pub fan_power: f64,
}

/// Calculate ventilation thermal effects from a volume flow rate.
pub fn ventilation_thermal_output(
    volume_flow_rate: f64,
    t_zone: f64,
    t_outdoor: f64,
    w_zone: f64,
    w_outdoor: f64,
    pressure: f64,
    fan_power: f64,
) -> VentilationOutput {
    if volume_flow_rate <= 0.0 {
        return VentilationOutput::default();
    }

    let rho = rho_air(pressure, t_outdoor, w_outdoor);
    let mass_flow = volume_flow_rate * rho;
    let cp = cp_air(w_outdoor);

    let sensible = mass_flow * cp * (t_outdoor - t_zone);
    let latent = mass_flow * 2_501_000.0 * (w_outdoor - w_zone);

    VentilationOutput {
        volume_flow_rate,
        mass_flow_rate: mass_flow,
        sensible_heat: sensible,
        latent_heat: latent,
        fan_power,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_ventilation() {
        let vent = DesignFlowRateVentilation::constant("OA", 0.1);
        let v = vent.calculate(1.0, 22.0, 10.0, 3.0);
        assert!((v - 0.1).abs() < 1e-10);
    }

    #[test]
    fn ventilation_temperature_limits() {
        let mut vent = DesignFlowRateVentilation::constant("OA", 0.1);
        vent.min_outdoor_temp = Some(5.0);

        // Should operate when outdoor > 5C
        assert!(vent.is_available(22.0, 10.0, 0.0));
        // Should not operate when outdoor < 5C
        assert!(!vent.is_available(22.0, 0.0, 0.0));

        let v = vent.calculate(1.0, 22.0, 0.0, 0.0);
        assert!((v).abs() < 1e-10);
    }

    #[test]
    fn wind_and_stack_basic() {
        let vent = WindAndStackVentilation::new("NatVent", 2.0, 3.0);
        let v = vent.calculate(1.0, 25.0, 15.0, 2.0);
        assert!(v > 0.0);
        // Reasonable range for a 2 m2 opening
        assert!(v < 5.0, "v={v}");
    }

    #[test]
    fn wind_and_stack_zero_dt() {
        let vent = WindAndStackVentilation::new("NatVent", 2.0, 3.0);
        // Even with no temperature difference, wind still drives flow
        let v = vent.calculate(1.0, 20.0, 20.0, 5.0);
        assert!(v > 0.0);
    }

    #[test]
    fn ventilation_thermal_cooling() {
        let output = ventilation_thermal_output(
            0.1,    // 100 L/s
            25.0,   // zone at 25C
            15.0,   // outdoor at 15C (cooler)
            0.010,  // zone humidity
            0.006,  // outdoor humidity
            101325.0,
            0.0,    // no fan
        );
        assert!(output.sensible_heat < 0.0, "Should cool the zone: {}", output.sensible_heat);
        assert!(output.mass_flow_rate > 0.0);
    }

    #[test]
    fn fan_power_calculation() {
        let mut vent = DesignFlowRateVentilation::constant("Mech", 0.5);
        vent.ventilation_type = VentilationType::Intake;
        vent.fan_pressure = 500.0; // Pa
        vent.fan_efficiency = 0.7;

        let power = vent.fan_power(0.5);
        // P = 0.5 * 500 / 0.7 ≈ 357 W
        assert!((power - 0.5 * 500.0 / 0.7).abs() < 1.0);
    }
}
