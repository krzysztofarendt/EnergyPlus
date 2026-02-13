//! Lights internal gains model.
//!
//! Lighting heat is split into four fractions:
//! radiant, visible (short-wave), return air, and convective (remainder).

use crate::GainOutput;

/// Lights internal gain definition.
#[derive(Debug, Clone)]
pub struct Lights {
    pub name: String,
    /// Design lighting power (W).
    pub design_level: f64,
    /// Fraction of heat to return air plenum (0-1).
    pub fraction_return_air: f64,
    /// Fraction of heat that is long-wave radiant (0-1).
    pub fraction_radiant: f64,
    /// Fraction of heat that is short-wave visible (0-1).
    pub fraction_visible: f64,
    /// Whether return air fraction is calculated dynamically based on plenum temperature.
    pub return_air_is_calculated: bool,
    /// Coefficients for dynamic return air fraction: f_ra = c0 - c1 * T_plenum.
    pub return_air_coefficients: Option<(f64, f64)>,
}

impl Lights {
    /// Create lights with typical office lighting fractions.
    ///
    /// Default: 0.72 radiant, 0.18 visible, 0.0 return air, 0.10 convective.
    pub fn new(name: impl Into<String>, design_level: f64) -> Self {
        Self {
            name: name.into(),
            design_level,
            fraction_return_air: 0.0,
            fraction_radiant: 0.72,
            fraction_visible: 0.18,
            return_air_is_calculated: false,
            return_air_coefficients: None,
        }
    }

    /// Fraction convective (remainder after radiant + visible + return air).
    pub fn fraction_convective(&self) -> f64 {
        (1.0 - self.fraction_return_air - self.fraction_radiant - self.fraction_visible).max(0.0)
    }

    /// Calculate lighting internal gains.
    ///
    /// # Arguments
    /// * `schedule_value` - Schedule multiplier (0-1)
    /// * `daylighting_reduction` - Power reduction from daylighting controls (0-1, 1=no reduction)
    /// * `plenum_temp` - Return air plenum temperature (C), used if return_air_is_calculated
    pub fn calculate(
        &self,
        schedule_value: f64,
        daylighting_reduction: f64,
        plenum_temp: Option<f64>,
    ) -> GainOutput {
        let power = self.design_level * schedule_value.max(0.0) * daylighting_reduction.clamp(0.0, 1.0);

        if power <= 0.0 {
            return GainOutput::default();
        }

        // Dynamic return air fraction
        let f_return = if self.return_air_is_calculated {
            if let (Some((c0, c1)), Some(t_plenum)) = (self.return_air_coefficients, plenum_temp) {
                (c0 - c1 * t_plenum).clamp(0.0, 1.0)
            } else {
                self.fraction_return_air
            }
        } else {
            self.fraction_return_air
        };

        let f_conv = (1.0 - f_return - self.fraction_radiant - self.fraction_visible).max(0.0);

        GainOutput {
            total_power: power,
            convective: power * f_conv,
            radiant: power * self.fraction_radiant,
            visible: power * self.fraction_visible,
            return_air: power * f_return,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lights_basic_gain() {
        let lights = Lights::new("OfficeLights", 1000.0);
        let gain = lights.calculate(1.0, 1.0, None);

        assert!((gain.total_power - 1000.0).abs() < 1e-10);
        assert!((gain.radiant - 720.0).abs() < 1e-10);
        assert!((gain.visible - 180.0).abs() < 1e-10);
        assert!((gain.convective - 100.0).abs() < 1e-10);
        assert!((gain.return_air).abs() < 1e-10);

        // Conservation: conv + rad + vis + return = total
        let sum = gain.convective + gain.radiant + gain.visible + gain.return_air;
        assert!((sum - 1000.0).abs() < 1e-10);
    }

    #[test]
    fn lights_with_return_air() {
        let mut lights = Lights::new("PlenumLights", 500.0);
        lights.fraction_return_air = 0.20;
        lights.fraction_radiant = 0.50;
        lights.fraction_visible = 0.10;

        let gain = lights.calculate(1.0, 1.0, None);
        assert!((gain.return_air - 100.0).abs() < 1e-10);
        assert!((gain.radiant - 250.0).abs() < 1e-10);
        assert!((gain.visible - 50.0).abs() < 1e-10);
        assert!((gain.convective - 100.0).abs() < 1e-10);
    }

    #[test]
    fn lights_daylighting_reduction() {
        let lights = Lights::new("DaylitLights", 1000.0);
        let gain_full = lights.calculate(1.0, 1.0, None);
        let gain_dimmed = lights.calculate(1.0, 0.5, None);

        assert!((gain_dimmed.total_power - 500.0).abs() < 1e-10);
        assert!((gain_dimmed.total_power - gain_full.total_power * 0.5).abs() < 1e-10);
    }

    #[test]
    fn lights_schedule_off() {
        let lights = Lights::new("Lights", 1000.0);
        let gain = lights.calculate(0.0, 1.0, None);
        assert!((gain.total_power).abs() < 1e-10);
    }
}
