//! Air-side controller models.
//!
//! Implements outdoor air (OA) economizer control, proportional controllers
//! for coil leaving air temperature, and demand-controlled ventilation.

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

    /// Create a fixed enthalpy economizer controller.
    pub fn fixed_enthalpy(name: impl Into<String>, min_oa: f64, high_enthalpy: f64) -> Self {
        Self {
            name: name.into(),
            economizer_type: EconomizerType::FixedEnthalpy,
            min_oa_fraction: min_oa,
            max_oa_fraction: 1.0,
            high_temp_limit: 28.0,
            high_enthalpy_limit: high_enthalpy,
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

// ---------------------------------------------------------------------------
// Outdoor Air Controller (full OA system controller)
// ---------------------------------------------------------------------------

/// Outdoor air controller combining economizer, minimum OA, and DCV.
#[derive(Debug, Clone)]
pub struct OutdoorAirController {
    pub name: String,
    /// Economizer controller.
    pub economizer: EconomizerController,
    /// Minimum outdoor air flow rate (m3/s) — design ventilation.
    pub min_oa_flow: f64,
    /// Maximum outdoor air flow rate (m3/s).
    pub max_oa_flow: f64,
    /// Whether demand-controlled ventilation is enabled.
    pub dcv_enabled: bool,
    /// Per-person outdoor air flow rate for DCV (m3/s per person).
    pub oa_per_person: f64,
    /// Per-area outdoor air flow rate for DCV (m3/s per m2).
    pub oa_per_area: f64,
}

/// Result of outdoor air controller calculation.
#[derive(Debug, Clone, Copy)]
pub struct OAControllerResult {
    /// Outdoor air mass flow rate (kg/s).
    pub oa_mass_flow: f64,
    /// Outdoor air fraction (0-1).
    pub oa_fraction: f64,
    /// Whether economizer is active.
    pub economizer_active: bool,
    /// Minimum OA flow used (m3/s) after DCV adjustment.
    pub min_oa_flow_used: f64,
}

impl OutdoorAirController {
    /// Create an OA controller with economizer.
    pub fn new(
        name: impl Into<String>,
        economizer: EconomizerController,
        min_oa_flow: f64,
        max_oa_flow: f64,
    ) -> Self {
        Self {
            name: name.into(),
            economizer,
            min_oa_flow,
            max_oa_flow,
            dcv_enabled: false,
            oa_per_person: 0.0075, // ~7.5 L/s per person (ASHRAE 62.1)
            oa_per_area: 0.0006,   // ~0.6 L/s per m2
        }
    }

    /// Enable demand-controlled ventilation.
    pub fn with_dcv(mut self, oa_per_person: f64, oa_per_area: f64) -> Self {
        self.dcv_enabled = true;
        self.oa_per_person = oa_per_person;
        self.oa_per_area = oa_per_area;
        self
    }

    /// Calculate OA flow.
    ///
    /// `supply_mass_flow` — total supply air mass flow (kg/s).
    /// `oa_temp` / `return_temp` — temperatures (C).
    /// `oa_enthalpy` / `return_enthalpy` — enthalpies (J/kg).
    /// `cooling_needed` — whether cooling load exists.
    /// `occupancy` — number of people (for DCV).
    /// `floor_area` — zone floor area (m2, for DCV).
    /// `air_density` — outdoor air density (kg/m3).
    pub fn calculate(
        &self,
        supply_mass_flow: f64,
        oa_temp: f64,
        return_temp: f64,
        oa_enthalpy: f64,
        return_enthalpy: f64,
        cooling_needed: bool,
        occupancy: f64,
        floor_area: f64,
        air_density: f64,
    ) -> OAControllerResult {
        // Step 1: Determine minimum OA based on DCV or design
        let min_oa_vol = if self.dcv_enabled {
            let people_oa = occupancy * self.oa_per_person;
            let area_oa = floor_area * self.oa_per_area;
            (people_oa + area_oa).max(0.0)
        } else {
            self.min_oa_flow
        };

        let min_oa_mass = min_oa_vol * air_density;

        // Step 2: Calculate economizer OA fraction
        let min_frac = if supply_mass_flow > 1e-10 {
            (min_oa_mass / supply_mass_flow).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Create a temporary economizer with the DCV-adjusted minimum
        let mut econ = self.economizer.clone();
        econ.min_oa_fraction = min_frac;

        let econ_result = econ.calculate(oa_temp, return_temp, oa_enthalpy, return_enthalpy, cooling_needed);

        let oa_mass_flow = (econ_result.oa_fraction * supply_mass_flow)
            .clamp(0.0, self.max_oa_flow * air_density);

        let oa_fraction = if supply_mass_flow > 1e-10 {
            oa_mass_flow / supply_mass_flow
        } else {
            0.0
        };

        OAControllerResult {
            oa_mass_flow,
            oa_fraction,
            economizer_active: econ_result.economizer_active,
            min_oa_flow_used: min_oa_vol,
        }
    }
}

// ---------------------------------------------------------------------------
// PI Controller
// ---------------------------------------------------------------------------

/// PI (Proportional-Integral) controller for tighter control.
#[derive(Debug, Clone)]
pub struct PIController {
    pub name: String,
    pub setpoint: f64,
    pub kp: f64,
    pub ki: f64,
    pub min_output: f64,
    pub max_output: f64,
    pub reverse_acting: bool,
    /// Accumulated integral error.
    integral: f64,
}

impl PIController {
    pub fn new(
        name: impl Into<String>,
        setpoint: f64,
        kp: f64,
        ki: f64,
        reverse_acting: bool,
    ) -> Self {
        Self {
            name: name.into(),
            setpoint,
            kp,
            ki,
            min_output: 0.0,
            max_output: 1.0,
            reverse_acting,
            integral: 0.0,
        }
    }

    /// Calculate control output and update integral state.
    pub fn calculate(&mut self, sensed: f64, dt: f64) -> f64 {
        let error = if self.reverse_acting {
            sensed - self.setpoint
        } else {
            self.setpoint - sensed
        };

        self.integral += error * dt;

        // Anti-windup: clamp integral contribution
        let max_integral = (self.max_output - self.min_output) / self.ki.abs().max(1e-10);
        self.integral = self.integral.clamp(-max_integral, max_integral);

        let output = self.kp * error + self.ki * self.integral;
        output.clamp(self.min_output, self.max_output)
    }

    /// Reset the integral state.
    pub fn reset(&mut self) {
        self.integral = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Economizer tests ---

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
    fn fixed_enthalpy_economizer() {
        let ctrl = EconomizerController::fixed_enthalpy("OA Ctrl", 0.2, 55_000.0);
        // OA enthalpy 40kJ < limit 55kJ: economize
        let result = ctrl.calculate(20.0, 24.0, 40_000.0, 60_000.0, true);
        assert!(result.economizer_active);

        // OA enthalpy 60kJ > limit 55kJ: no economizer
        let result = ctrl.calculate(30.0, 24.0, 60_000.0, 55_000.0, true);
        assert!(!result.economizer_active);
    }

    // --- Proportional controller tests ---

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

    // --- OA Controller tests ---

    #[test]
    fn oa_controller_basic() {
        let econ = EconomizerController::fixed_dry_bulb("Econ", 0.2, 24.0);
        let oa_ctrl = OutdoorAirController::new("OA Ctrl", econ, 0.5, 5.0);

        // OAT=18 < 24: economizer active → max OA
        let result = oa_ctrl.calculate(
            2.0, 18.0, 24.0, 40000.0, 50000.0, true, 10.0, 100.0, 1.2,
        );
        assert!(result.economizer_active);
        assert!(result.oa_fraction > 0.5, "oa_frac={}", result.oa_fraction);
    }

    #[test]
    fn oa_controller_min_oa_only() {
        let econ = EconomizerController::no_economizer("Econ", 0.2);
        let oa_ctrl = OutdoorAirController::new("OA Ctrl", econ, 0.3, 5.0);

        let result = oa_ctrl.calculate(
            2.0, 35.0, 24.0, 60000.0, 50000.0, true, 10.0, 100.0, 1.2,
        );
        assert!(!result.economizer_active);
        // OA fraction based on min_oa_flow / supply_mass_flow
        // min_oa_mass = 0.3 * 1.2 = 0.36, supply = 2.0 → frac = 0.18
        assert!(result.oa_fraction > 0.1 && result.oa_fraction < 0.3,
                "oa_frac={}", result.oa_fraction);
    }

    #[test]
    fn oa_controller_dcv() {
        let econ = EconomizerController::no_economizer("Econ", 0.1);
        let oa_ctrl = OutdoorAirController::new("OA Ctrl", econ, 0.5, 5.0)
            .with_dcv(0.0075, 0.0006); // 7.5 L/s/person, 0.6 L/s/m2

        // Full occupancy: 50 people, 200 m2
        let result_full = oa_ctrl.calculate(
            3.0, 25.0, 24.0, 50000.0, 50000.0, false, 50.0, 200.0, 1.2,
        );
        // Min OA = 50*0.0075 + 200*0.0006 = 0.375 + 0.12 = 0.495 m3/s
        assert!((result_full.min_oa_flow_used - 0.495).abs() < 0.01,
                "min_oa={}", result_full.min_oa_flow_used);

        // Low occupancy: 5 people
        let result_low = oa_ctrl.calculate(
            3.0, 25.0, 24.0, 50000.0, 50000.0, false, 5.0, 200.0, 1.2,
        );
        // Min OA = 5*0.0075 + 200*0.0006 = 0.0375 + 0.12 = 0.1575 m3/s
        assert!((result_low.min_oa_flow_used - 0.1575).abs() < 0.01,
                "min_oa={}", result_low.min_oa_flow_used);

        // DCV should reduce OA at low occupancy
        assert!(result_low.oa_mass_flow < result_full.oa_mass_flow,
                "low={}, full={}", result_low.oa_mass_flow, result_full.oa_mass_flow);
    }

    #[test]
    fn oa_controller_dcv_zero_occupancy() {
        let econ = EconomizerController::no_economizer("Econ", 0.1);
        let oa_ctrl = OutdoorAirController::new("OA Ctrl", econ, 0.5, 5.0)
            .with_dcv(0.0075, 0.0006);

        // Zero occupancy: only area component
        let result = oa_ctrl.calculate(
            3.0, 25.0, 24.0, 50000.0, 50000.0, false, 0.0, 200.0, 1.2,
        );
        assert!((result.min_oa_flow_used - 0.12).abs() < 0.01,
                "min_oa={}", result.min_oa_flow_used);
    }

    // --- PI Controller tests ---

    #[test]
    fn pi_controller_proportional() {
        let mut ctrl = PIController::new("Test", 20.0, 0.5, 0.0, false);
        // Pure P: sensed=18 → error=2 → output = 0.5*2 = 1.0
        let output = ctrl.calculate(18.0, 1.0);
        assert!((output - 1.0).abs() < 0.01, "output={}", output);
    }

    #[test]
    fn pi_controller_integral_accumulation() {
        let mut ctrl = PIController::new("Test", 20.0, 0.1, 0.1, false);
        // Steady error of 2 over multiple timesteps → integral grows
        let out1 = ctrl.calculate(18.0, 1.0);
        let out2 = ctrl.calculate(18.0, 1.0);
        assert!(out2 > out1, "out1={}, out2={}", out1, out2);
    }

    #[test]
    fn pi_controller_reset() {
        let mut ctrl = PIController::new("Test", 20.0, 0.1, 0.5, false);
        ctrl.calculate(18.0, 1.0);
        ctrl.calculate(18.0, 1.0);
        ctrl.reset();
        // After reset, output should be only proportional
        let output = ctrl.calculate(18.0, 0.0);
        let expected = 0.1 * (20.0 - 18.0);
        assert!((output - expected).abs() < 0.01, "output={}", output);
    }

    #[test]
    fn pi_controller_reverse_acting() {
        let mut ctrl = PIController::new("Cooling", 13.0, 0.5, 0.0, true);
        // Reverse acting: sensed=15 > setpoint=13 → output positive
        let output = ctrl.calculate(15.0, 1.0);
        assert!(output > 0.0, "output={}", output);

        // Sensed below setpoint → output 0
        let output = ctrl.calculate(12.0, 1.0);
        assert!(output <= 0.0, "output={}", output);
    }

    #[test]
    fn pi_controller_antiwindup() {
        let mut ctrl = PIController::new("Test", 20.0, 0.1, 1.0, false);
        // Large error over many steps → output should be clamped to max
        for _ in 0..100 {
            ctrl.calculate(10.0, 1.0);
        }
        let output = ctrl.calculate(10.0, 1.0);
        assert!((output - 1.0).abs() < 0.01, "output={}", output);
    }
}
