//! Setpoint manager models.
//!
//! Setpoint managers determine supply air and loop temperature setpoints
//! based on outdoor conditions, schedules, or zone feedback.

/// Setpoint manager type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetpointType {
    /// Fixed scheduled setpoint.
    Scheduled,
    /// Outdoor air reset: setpoint varies linearly with outdoor temp.
    OutdoorAirReset,
    /// Warmest zone: setpoint tracks the warmest zone.
    Warmest,
    /// Coldest zone: setpoint tracks the coldest zone.
    Coldest,
    /// Follow outdoor air temperature.
    FollowOutdoorAir,
}

/// Setpoint variable being controlled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SetpointVariable {
    #[default]
    Temperature,
    HumidityRatioMax,
    HumidityRatioMin,
}

/// A setpoint manager that calculates setpoint for a node.
#[derive(Debug, Clone)]
pub struct SetpointManager {
    pub name: String,
    pub setpoint_type: SetpointType,
    pub variable: SetpointVariable,
    pub setpoint_node: usize,
    /// Fixed setpoint value (for Scheduled type).
    pub scheduled_value: f64,
    /// OA reset: setpoint at low outdoor temp.
    pub oa_reset_low_setpoint: f64,
    /// OA reset: outdoor temp at low setpoint.
    pub oa_reset_low_outdoor: f64,
    /// OA reset: setpoint at high outdoor temp.
    pub oa_reset_high_setpoint: f64,
    /// OA reset: outdoor temp at high setpoint.
    pub oa_reset_high_outdoor: f64,
}

impl SetpointManager {
    /// Create a scheduled (fixed) setpoint manager.
    pub fn scheduled(name: impl Into<String>, node: usize, value: f64) -> Self {
        Self {
            name: name.into(),
            setpoint_type: SetpointType::Scheduled,
            variable: SetpointVariable::Temperature,
            setpoint_node: node,
            scheduled_value: value,
            oa_reset_low_setpoint: 0.0,
            oa_reset_low_outdoor: 0.0,
            oa_reset_high_setpoint: 0.0,
            oa_reset_high_outdoor: 0.0,
        }
    }

    /// Create an outdoor air reset setpoint manager.
    ///
    /// The setpoint varies linearly between low and high outdoor temperatures.
    /// Typical use: supply air temp = 16C at OAT=10C, 12C at OAT=30C.
    pub fn outdoor_air_reset(
        name: impl Into<String>,
        node: usize,
        low_outdoor: f64,
        low_setpoint: f64,
        high_outdoor: f64,
        high_setpoint: f64,
    ) -> Self {
        Self {
            name: name.into(),
            setpoint_type: SetpointType::OutdoorAirReset,
            variable: SetpointVariable::Temperature,
            setpoint_node: node,
            scheduled_value: 0.0,
            oa_reset_low_setpoint: low_setpoint,
            oa_reset_low_outdoor: low_outdoor,
            oa_reset_high_setpoint: high_setpoint,
            oa_reset_high_outdoor: high_outdoor,
        }
    }

    /// Calculate the setpoint value.
    pub fn calculate(&self, outdoor_temp: f64) -> f64 {
        match self.setpoint_type {
            SetpointType::Scheduled => self.scheduled_value,

            SetpointType::OutdoorAirReset => {
                let range = self.oa_reset_high_outdoor - self.oa_reset_low_outdoor;
                if range.abs() < 0.1 {
                    return self.oa_reset_low_setpoint;
                }
                let fraction = (outdoor_temp - self.oa_reset_low_outdoor) / range;
                let fraction = fraction.clamp(0.0, 1.0);
                self.oa_reset_low_setpoint
                    + fraction * (self.oa_reset_high_setpoint - self.oa_reset_low_setpoint)
            }

            SetpointType::FollowOutdoorAir => outdoor_temp,

            // Warmest/Coldest require zone data — return scheduled_value as fallback
            SetpointType::Warmest | SetpointType::Coldest => self.scheduled_value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_setpoint() {
        let spm = SetpointManager::scheduled("Fixed SAT", 10, 12.8);
        assert!((spm.calculate(35.0) - 12.8).abs() < 1e-10);
        assert!((spm.calculate(-10.0) - 12.8).abs() < 1e-10);
    }

    #[test]
    fn oa_reset_at_endpoints() {
        // At OAT=10C → SAT=16C, at OAT=30C → SAT=12C
        let spm = SetpointManager::outdoor_air_reset("OA Reset", 10, 10.0, 16.0, 30.0, 12.0);
        assert!((spm.calculate(10.0) - 16.0).abs() < 0.01, "sp={}", spm.calculate(10.0));
        assert!((spm.calculate(30.0) - 12.0).abs() < 0.01, "sp={}", spm.calculate(30.0));
    }

    #[test]
    fn oa_reset_midpoint() {
        let spm = SetpointManager::outdoor_air_reset("OA Reset", 10, 10.0, 16.0, 30.0, 12.0);
        // At OAT=20C (midpoint): SAT = 16 + 0.5*(12-16) = 14
        assert!((spm.calculate(20.0) - 14.0).abs() < 0.01, "sp={}", spm.calculate(20.0));
    }

    #[test]
    fn oa_reset_clamped() {
        let spm = SetpointManager::outdoor_air_reset("OA Reset", 10, 10.0, 16.0, 30.0, 12.0);
        // Below low outdoor: clamped to low setpoint
        assert!((spm.calculate(-5.0) - 16.0).abs() < 0.01);
        // Above high outdoor: clamped to high setpoint
        assert!((spm.calculate(40.0) - 12.0).abs() < 0.01);
    }

    #[test]
    fn follow_outdoor_air() {
        let mut spm = SetpointManager::scheduled("Follow", 10, 0.0);
        spm.setpoint_type = SetpointType::FollowOutdoorAir;
        assert!((spm.calculate(22.5) - 22.5).abs() < 1e-10);
    }
}
