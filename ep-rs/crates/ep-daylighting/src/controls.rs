//! Daylighting-responsive lighting controls.
//!
//! Three control types:
//! - Continuous dimming: linear power reduction
//! - Stepped: discrete power steps
//! - Continuous-off: like continuous but turns off when daylight is sufficient

/// Lighting control type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LightingControlType {
    /// Continuous dimming from full to minimum power.
    #[default]
    Continuous,
    /// Discrete steps between full and off.
    Stepped,
    /// Continuous dimming with full off capability.
    ContinuousOff,
}

/// Daylighting lighting control parameters.
#[derive(Debug, Clone)]
pub struct DaylightControl {
    /// Control type.
    pub control_type: LightingControlType,
    /// Number of steps for stepped control (1 = on/off).
    pub num_steps: u32,
    /// Minimum power fraction for continuous dimming (0-1).
    pub min_power_fraction: f64,
    /// Minimum light output fraction for continuous dimming (0-1).
    pub min_light_fraction: f64,
}

impl Default for DaylightControl {
    fn default() -> Self {
        Self {
            control_type: LightingControlType::Continuous,
            num_steps: 3,
            min_power_fraction: 0.3,
            min_light_fraction: 0.2,
        }
    }
}

impl DaylightControl {
    /// Calculate electric lighting power fraction given daylight illuminance.
    ///
    /// # Arguments
    /// * `daylight_illuminance` - Current daylight illuminance at reference point (lux)
    /// * `setpoint` - Target illuminance setpoint (lux)
    ///
    /// # Returns
    /// Power fraction (0-1) where 1.0 = full power, 0.0 = lights off
    pub fn power_fraction(&self, daylight_illuminance: f64, setpoint: f64) -> f64 {
        if setpoint <= 0.0 {
            return 1.0;
        }

        match self.control_type {
            LightingControlType::Continuous => {
                self.continuous_power(daylight_illuminance, setpoint)
            }
            LightingControlType::Stepped => {
                self.stepped_power(daylight_illuminance, setpoint)
            }
            LightingControlType::ContinuousOff => {
                self.continuous_off_power(daylight_illuminance, setpoint)
            }
        }
    }

    /// Continuous dimming control.
    ///
    /// Linear relationship between daylight fraction and power:
    /// - At 0 daylight: power = 1.0
    /// - At setpoint daylight: power = min_power_fraction
    /// - Above setpoint: power = min_power_fraction (but lights never fully off)
    fn continuous_power(&self, daylight: f64, setpoint: f64) -> f64 {
        if daylight >= setpoint {
            self.min_power_fraction
        } else if daylight <= setpoint * self.min_light_fraction {
            // Not enough daylight for any reduction
            1.0
        } else {
            // Linear interpolation
            let fl = daylight / setpoint; // fraction of setpoint met by daylight
            // Power needed = base power * (1 - (daylight contribution above minimum))
            let power = 1.0 - (fl - self.min_light_fraction) / (1.0 - self.min_light_fraction)
                * (1.0 - self.min_power_fraction);
            power.clamp(self.min_power_fraction, 1.0)
        }
    }

    /// Stepped control.
    ///
    /// Discrete power levels: 1/N, 2/N, ..., N/N of full power.
    fn stepped_power(&self, daylight: f64, setpoint: f64) -> f64 {
        if self.num_steps == 0 {
            return 1.0;
        }

        let fl = (daylight / setpoint).clamp(0.0, 1.0);
        // Number of steps that daylight covers
        let daylight_steps = (fl * self.num_steps as f64).floor() as u32;
        let remaining_steps = self.num_steps.saturating_sub(daylight_steps);

        remaining_steps as f64 / self.num_steps as f64
    }

    /// Continuous-off control.
    ///
    /// Like continuous, but lights can turn fully off when daylight
    /// exceeds the minimum light output threshold.
    fn continuous_off_power(&self, daylight: f64, setpoint: f64) -> f64 {
        if daylight >= setpoint {
            0.0 // Fully off when daylight meets setpoint
        } else if daylight <= setpoint * self.min_light_fraction {
            1.0
        } else {
            // Linear dimming from 1.0 to 0.0
            let fl = daylight / setpoint;
            let power = 1.0 - (fl - self.min_light_fraction) / (1.0 - self.min_light_fraction);
            power.clamp(0.0, 1.0)
        }
    }
}

/// Calculate total zone lighting power reduction from multiple reference points.
///
/// Each reference point controls a fraction of the zone.
/// Total reduction = sum(fraction_i * power_fraction_i) + (1 - sum(fraction_i)) * 1.0
pub fn zone_power_reduction(
    ref_point_fractions: &[f64],
    power_fractions: &[f64],
) -> f64 {
    let mut total = 0.0;
    let mut controlled_fraction = 0.0;

    for (frac, power) in ref_point_fractions.iter().zip(power_fractions.iter()) {
        total += frac * power;
        controlled_fraction += frac;
    }

    // Uncontrolled portion at full power
    total += (1.0 - controlled_fraction).max(0.0);

    total.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_full_daylight() {
        let ctrl = DaylightControl::default();
        let pf = ctrl.power_fraction(500.0, 500.0);
        // At full setpoint: should be at minimum power
        assert!((pf - ctrl.min_power_fraction).abs() < 0.01, "pf={pf}");
    }

    #[test]
    fn continuous_no_daylight() {
        let ctrl = DaylightControl::default();
        let pf = ctrl.power_fraction(0.0, 500.0);
        assert!((pf - 1.0).abs() < 0.01, "pf={pf}");
    }

    #[test]
    fn continuous_half_daylight() {
        let ctrl = DaylightControl {
            control_type: LightingControlType::Continuous,
            min_power_fraction: 0.0,
            min_light_fraction: 0.0,
            ..Default::default()
        };
        let pf = ctrl.power_fraction(250.0, 500.0);
        // With min_power=0 and min_light=0: linear from 1.0 to 0.0
        assert!((pf - 0.5).abs() < 0.01, "pf={pf}");
    }

    #[test]
    fn stepped_control() {
        let ctrl = DaylightControl {
            control_type: LightingControlType::Stepped,
            num_steps: 3,
            ..Default::default()
        };

        // No daylight: full power
        assert!((ctrl.power_fraction(0.0, 500.0) - 1.0).abs() < 0.01);
        // 1/3 daylight: 2/3 power
        let pf = ctrl.power_fraction(200.0, 500.0);
        assert!((pf - 2.0 / 3.0).abs() < 0.01, "pf={pf}");
        // Full daylight: lights off
        assert!((ctrl.power_fraction(500.0, 500.0)).abs() < 0.01);
    }

    #[test]
    fn continuous_off_turns_off() {
        let ctrl = DaylightControl {
            control_type: LightingControlType::ContinuousOff,
            min_power_fraction: 0.3,
            min_light_fraction: 0.2,
            ..Default::default()
        };

        // At full setpoint: should be off (unlike continuous which stays at min)
        assert!((ctrl.power_fraction(500.0, 500.0)).abs() < 0.01);
        // Below minimum: full power
        assert!((ctrl.power_fraction(50.0, 500.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn zone_power_reduction_single_ref() {
        // Single reference point controlling 50% of zone
        let reduction = zone_power_reduction(&[0.5], &[0.3]);
        // 0.5 * 0.3 + 0.5 * 1.0 = 0.15 + 0.5 = 0.65
        assert!((reduction - 0.65).abs() < 1e-10);
    }

    #[test]
    fn zone_power_reduction_two_refs() {
        // Two reference points: 40% and 40% of zone
        let reduction = zone_power_reduction(&[0.4, 0.4], &[0.5, 0.3]);
        // 0.4*0.5 + 0.4*0.3 + 0.2*1.0 = 0.2 + 0.12 + 0.2 = 0.52
        assert!((reduction - 0.52).abs() < 1e-10);
    }

    #[test]
    fn zone_power_reduction_full_control() {
        // Full zone controlled, all lights off
        let reduction = zone_power_reduction(&[1.0], &[0.0]);
        assert!((reduction).abs() < 1e-10);
    }
}
