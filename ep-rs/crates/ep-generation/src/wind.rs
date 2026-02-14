//! Wind turbine model.
//!
//! Horizontal-axis wind turbine (HAWT) with power coefficient curve.
//! Implements cut-in, rated, and cut-out speed behavior.

/// Wind turbine rotor type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotorType {
    HorizontalAxis,
    VerticalAxis,
}

/// Wind turbine specification.
#[derive(Debug, Clone)]
pub struct WindTurbine {
    pub name: String,
    pub rotor_type: RotorType,
    /// Rotor diameter (m).
    pub rotor_diameter: f64,
    /// Hub height (m).
    pub hub_height: f64,
    /// Rated power output (W).
    pub rated_power: f64,
    /// Rated wind speed (m/s).
    pub rated_wind_speed: f64,
    /// Cut-in wind speed (m/s).
    pub cut_in_speed: f64,
    /// Cut-out wind speed (m/s).
    pub cut_out_speed: f64,
    /// Maximum power coefficient (Betz limit ≈ 0.59).
    pub max_power_coeff: f64,
    /// Overall system efficiency (generator, gearbox, etc.).
    pub system_efficiency: f64,
    /// Power coefficient polynomial: Cp(lambda) = c0 + c1*lambda + ... + c5*lambda^5.
    pub power_coeffs: [f64; 6],
}

/// Wind turbine calculation result.
#[derive(Debug, Clone, Copy)]
pub struct WindResult {
    /// Electrical power output (W).
    pub power: f64,
    /// Power coefficient at operating point.
    pub power_coeff: f64,
    /// Tip speed ratio.
    pub tip_speed_ratio: f64,
    /// Local wind speed at hub height (m/s).
    pub hub_wind_speed: f64,
}

impl WindTurbine {
    /// Create a horizontal-axis wind turbine.
    pub fn hawt(
        name: impl Into<String>,
        rotor_diameter: f64,
        hub_height: f64,
        rated_power: f64,
        rated_wind_speed: f64,
    ) -> Self {
        Self {
            name: name.into(),
            rotor_type: RotorType::HorizontalAxis,
            rotor_diameter,
            hub_height,
            rated_power,
            rated_wind_speed,
            cut_in_speed: 3.5,
            cut_out_speed: 25.0,
            max_power_coeff: 0.45,
            system_efficiency: 0.85,
            power_coeffs: [0.0; 6], // Will use max_power_coeff if all zero
        }
    }

    /// Correct wind speed from measurement height to hub height.
    /// Uses power-law profile: V_hub = V_ref * (h_hub / h_ref)^alpha.
    pub fn correct_wind_speed(&self, wind_speed_ref: f64, ref_height: f64, terrain_exponent: f64) -> f64 {
        if ref_height <= 0.0 || self.hub_height <= 0.0 {
            return wind_speed_ref;
        }
        wind_speed_ref * (self.hub_height / ref_height).powf(terrain_exponent)
    }

    /// Calculate power coefficient from tip speed ratio using polynomial.
    fn power_coefficient(&self, tip_speed_ratio: f64) -> f64 {
        let has_coeffs = self.power_coeffs.iter().any(|&c| c.abs() > 1e-15);
        if !has_coeffs {
            return self.max_power_coeff;
        }
        let lambda = tip_speed_ratio;
        let cp = self.power_coeffs[0]
            + self.power_coeffs[1] * lambda
            + self.power_coeffs[2] * lambda * lambda
            + self.power_coeffs[3] * lambda.powi(3)
            + self.power_coeffs[4] * lambda.powi(4)
            + self.power_coeffs[5] * lambda.powi(5);
        cp.clamp(0.0, 0.593) // Betz limit
    }

    /// Calculate wind turbine output.
    ///
    /// `wind_speed` — wind speed at hub height (m/s).
    /// `air_density` — air density (kg/m3).
    pub fn calculate(&self, wind_speed: f64, air_density: f64) -> WindResult {
        let zero_result = WindResult {
            power: 0.0, power_coeff: 0.0, tip_speed_ratio: 0.0,
            hub_wind_speed: wind_speed,
        };

        // Check operating range
        if wind_speed < self.cut_in_speed || wind_speed > self.cut_out_speed {
            return zero_result;
        }

        let swept_area = std::f64::consts::PI * (self.rotor_diameter / 2.0).powi(2);
        let available_power = 0.5 * air_density * swept_area * wind_speed.powi(3);

        // Tip speed ratio at rated conditions
        let omega_rated = self.rated_wind_speed * 7.0 / self.rotor_diameter; // Typical TSR ~ 7
        let tsr = omega_rated * (self.rotor_diameter / 2.0) / wind_speed;

        let cp = self.power_coefficient(tsr);
        let mut power = available_power * cp * self.system_efficiency;

        // Limit to rated power
        if power > self.rated_power {
            power = self.rated_power;
        }

        WindResult {
            power: power.max(0.0),
            power_coeff: cp,
            tip_speed_ratio: tsr,
            hub_wind_speed: wind_speed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_turbine() -> WindTurbine {
        WindTurbine::hawt("WT-1", 80.0, 80.0, 2_000_000.0, 12.0)
    }

    #[test]
    fn wind_below_cutin() {
        let wt = test_turbine();
        let result = wt.calculate(2.0, 1.225);
        assert!(result.power.abs() < 1e-10);
    }

    #[test]
    fn wind_above_cutout() {
        let wt = test_turbine();
        let result = wt.calculate(30.0, 1.225);
        assert!(result.power.abs() < 1e-10);
    }

    #[test]
    fn wind_at_rated() {
        let wt = test_turbine();
        let result = wt.calculate(12.0, 1.225);
        assert!(result.power > 0.0);
        assert!(result.power <= wt.rated_power * 1.01,
                "power={}W, rated={}W", result.power, wt.rated_power);
    }

    #[test]
    fn wind_power_increases_with_speed() {
        let wt = test_turbine();
        let r5 = wt.calculate(5.0, 1.225);
        let r8 = wt.calculate(8.0, 1.225);
        assert!(r8.power > r5.power, "P(8)={}, P(5)={}", r8.power, r5.power);
    }

    #[test]
    fn wind_speed_correction() {
        let wt = test_turbine();
        let v_hub = wt.correct_wind_speed(5.0, 10.0, 0.14);
        // 5 * (80/10)^0.14 = 5 * 8^0.14 ≈ 5 * 1.328 = 6.64
        assert!(v_hub > 5.0 && v_hub < 8.0, "v_hub={}", v_hub);
    }

    #[test]
    fn wind_density_effect() {
        let wt = test_turbine();
        let r_low = wt.calculate(8.0, 1.0);
        let r_high = wt.calculate(8.0, 1.4);
        assert!(r_high.power > r_low.power);
    }
}
