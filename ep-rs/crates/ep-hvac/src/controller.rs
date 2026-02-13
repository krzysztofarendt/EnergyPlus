//! Air-side controller models.
//!
//! Implements outdoor air (OA) economizer control and simple
//! proportional controllers for coil leaving air temperature.

/// Economizer control type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EconomizerType {
    /// No economizer — minimum outdoor air only.
    #[default]
    NoEconomizer,
    /// Fixed dry-bulb: economize when OAT < high limit.
    FixedDryBulb,
    /// Differential dry-bulb: economize when OAT < return air temp.
    DifferentialDryBulb,
    /// Fixed enthalpy: economize when OA enthalpy < limit.
    FixedEnthalpy,
    /// Differential enthalpy: economize when OA enthalpy < return enthalpy.
    DifferentialEnthalpy,
}

/// Outdoor air economizer controller.
#[derive(Debug, Clone)]
pub struct EconomizerController {
    pub name: String,
    pub economizer_type: EconomizerType,
    /// Minimum outdoor air fraction (0-1).
    pub min_oa_fraction: f64,
    /// Maximum outdoor air fraction (0-1).
    pub max_oa_fraction: f64,
    /// High temperature limit for FixedDryBulb (C).
    pub high_temp_limit: f64,
    /// High enthalpy limit for FixedEnthalpy (J/kg).
    pub high_enthalpy_limit: f64,
}

/// Economizer controller result.
#[derive(Debug, Clone, Copy)]
pub struct EconomizerResult {
    /// Outdoor air fraction (0-1).
    pub oa_fraction: f64,
    /// Whether economizer is active.
    pub economizer_active: bool,
}

impl EconomizerController {
    /// Create a controller with no economizer.
    pub fn no_economizer(name: impl Into<String>, min_oa: f64) -> Self {
        Self {
            name: name.into(),
            economizer_type: EconomizerType::NoEconomizer,
            min_oa_fraction: min_oa,
            max_oa_fraction: 1.0,
            high_temp_limit: 28.0,
            high_enthalpy_limit: 64_000.0,
        }
    }

    /// Create a fixed dry-bulb economizer controller.
    pub fn fixed_dry_bulb(name: impl Into<String>, min_oa: f64, high_temp: f64) -> Self {
        Self {
            name: name.into(),
            economizer_type: EconomizerType::FixedDryBulb,
            min_oa_fraction: min_oa,
            max_oa_fraction: 1.0,
            high_temp_limit: high_temp,
            high_enthalpy_limit: 64_000.0,
        }
    }

    /// Create a differential dry-bulb economizer controller.
    pub fn differential_dry_bulb(name: impl Into<String>, min_oa: f64) -> Self {
        Self {
            name: name.into(),
            economizer_type: EconomizerType::DifferentialDryBulb,
            min_oa_fraction: min_oa,
            max_oa_fraction: 1.0,
            high_temp_limit: 28.0,
            high_enthalpy_limit: 64_000.0,
        }
    }

    /// Determine the outdoor air fraction.
    ///
    /// `oa_temp` — outdoor air dry-bulb temperature (C).
    /// `return_temp` — return air dry-bulb temperature (C).
    /// `oa_enthalpy` — outdoor air enthalpy (J/kg).
    /// `return_enthalpy` — return air enthalpy (J/kg).
    /// `cooling_needed` — whether cooling is needed (true if cooling load exists).
    pub fn calculate(
        &self,
        oa_temp: f64,
        return_temp: f64,
        oa_enthalpy: f64,
        return_enthalpy: f64,
        cooling_needed: bool,
    ) -> EconomizerResult {
        let can_economize = cooling_needed && match self.economizer_type {
            EconomizerType::NoEconomizer => false,
            EconomizerType::FixedDryBulb => oa_temp < self.high_temp_limit,
            EconomizerType::DifferentialDryBulb => oa_temp < return_temp,
            EconomizerType::FixedEnthalpy => oa_enthalpy < self.high_enthalpy_limit,
            EconomizerType::DifferentialEnthalpy => oa_enthalpy < return_enthalpy,
        };

        let oa_fraction = if can_economize {
            self.max_oa_fraction
        } else {
            self.min_oa_fraction
        };

        EconomizerResult {
            oa_fraction,
            economizer_active: can_economize,
        }
    }
}

/// Simple proportional controller for coil leaving air temperature.
///
/// Calculates a control signal (0-1) based on error between setpoint
/// and sensed value, with proportional band.
#[derive(Debug, Clone)]
pub struct ProportionalController {
    pub name: String,
    pub setpoint: f64,
    pub proportional_band: f64,
    pub min_output: f64,
    pub max_output: f64,
    /// Reverse acting: output increases as sensed goes above setpoint.
    pub reverse_acting: bool,
}

impl ProportionalController {
    pub fn new(
        name: impl Into<String>,
        setpoint: f64,
        band: f64,
        reverse_acting: bool,
    ) -> Self {
        Self {
            name: name.into(),
            setpoint,
            proportional_band: band,
            min_output: 0.0,
            max_output: 1.0,
            reverse_acting,
        }
    }

    /// Calculate control output (0-1) from sensed value.
    pub fn calculate(&self, sensed: f64) -> f64 {
        if self.proportional_band.abs() < 1e-10 {
            // On-off control
            return if (self.reverse_acting && sensed > self.setpoint)
                || (!self.reverse_acting && sensed < self.setpoint)
            {
                self.max_output
            } else {
                self.min_output
            };
        }

        let error = if self.reverse_acting {
            sensed - self.setpoint
        } else {
            self.setpoint - sensed
        };

        let output = error / self.proportional_band;
        output.clamp(self.min_output, self.max_output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_economizer() {
        let ctrl = EconomizerController::no_economizer("OA Ctrl", 0.2);
        let result = ctrl.calculate(20.0, 24.0, 40000.0, 50000.0, true);
        assert!(!result.economizer_active);
        assert!((result.oa_fraction - 0.2).abs() < 0.01);
    }

    #[test]
    fn fixed_drybulb_economizer_active() {
        let ctrl = EconomizerController::fixed_dry_bulb("OA Ctrl", 0.2, 24.0);
        // OAT=18 < limit=24: economize
        let result = ctrl.calculate(18.0, 24.0, 40000.0, 50000.0, true);
        assert!(result.economizer_active);
        assert!((result.oa_fraction - 1.0).abs() < 0.01);
    }

    #[test]
    fn fixed_drybulb_economizer_inactive() {
        let ctrl = EconomizerController::fixed_dry_bulb("OA Ctrl", 0.2, 24.0);
        // OAT=30 > limit=24: no economizer
        let result = ctrl.calculate(30.0, 24.0, 40000.0, 50000.0, true);
        assert!(!result.economizer_active);
        assert!((result.oa_fraction - 0.2).abs() < 0.01);
    }

    #[test]
    fn differential_drybulb() {
        let ctrl = EconomizerController::differential_dry_bulb("OA Ctrl", 0.15);
        // OAT=20 < return=24: economize
        let result = ctrl.calculate(20.0, 24.0, 40000.0, 50000.0, true);
        assert!(result.economizer_active);

        // OAT=26 > return=24: no economizer
        let result = ctrl.calculate(26.0, 24.0, 40000.0, 50000.0, true);
        assert!(!result.economizer_active);
    }

    #[test]
    fn economizer_no_cooling_needed() {
        let ctrl = EconomizerController::fixed_dry_bulb("OA Ctrl", 0.2, 24.0);
        // Conditions favor economizer but no cooling needed
        let result = ctrl.calculate(18.0, 24.0, 40000.0, 50000.0, false);
        assert!(!result.economizer_active);
    }

    #[test]
    fn proportional_controller_midpoint() {
        // Cooling coil controller: reverse acting (more cooling as temp rises)
        let ctrl = ProportionalController::new("CW Valve", 12.8, 2.0, true);
        // At setpoint: output = 0
        assert!(ctrl.calculate(12.8).abs() < 0.01);
        // 1C above setpoint with 2C band: 50%
        assert!((ctrl.calculate(13.8) - 0.5).abs() < 0.01);
        // 2C above setpoint: 100%
        assert!((ctrl.calculate(14.8) - 1.0).abs() < 0.01);
    }

    #[test]
    fn proportional_controller_clamped() {
        let ctrl = ProportionalController::new("Test", 12.8, 2.0, true);
        // Way above setpoint
        assert!((ctrl.calculate(20.0) - 1.0).abs() < 0.01);
        // Below setpoint
        assert!(ctrl.calculate(10.0).abs() < 0.01);
    }

    #[test]
    fn proportional_controller_direct_acting() {
        // Heating coil: direct acting (more heating as temp drops)
        let ctrl = ProportionalController::new("HW Valve", 40.0, 5.0, false);
        // Below setpoint by 5C: 100%
        assert!((ctrl.calculate(35.0) - 1.0).abs() < 0.01);
        // At setpoint: 0%
        assert!(ctrl.calculate(40.0).abs() < 0.01);
        // Above setpoint: 0%
        assert!(ctrl.calculate(45.0).abs() < 0.01);
    }
}
