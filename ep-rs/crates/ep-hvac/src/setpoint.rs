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
    /// Single zone reheat: supply temp based on single zone load.
    SingleZoneReheat,
    /// Mixed air: track mixed air node conditions.
    MixedAir,
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
    /// Minimum supply air temp (for Warmest/SingleZoneReheat).
    pub min_setpoint: f64,
    /// Maximum supply air temp (for Coldest/SingleZoneReheat).
    pub max_setpoint: f64,
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
            min_setpoint: 10.0,
            max_setpoint: 50.0,
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
            min_setpoint: high_setpoint.min(low_setpoint),
            max_setpoint: high_setpoint.max(low_setpoint),
        }
    }

    /// Create a warmest-zone setpoint manager.
    ///
    /// Sets supply temperature based on the warmest zone temperature.
    /// As the warmest zone gets warmer, supply temp decreases (more cooling).
    pub fn warmest(name: impl Into<String>, node: usize, min_sp: f64, max_sp: f64) -> Self {
        Self {
            name: name.into(),
            setpoint_type: SetpointType::Warmest,
            variable: SetpointVariable::Temperature,
            setpoint_node: node,
            scheduled_value: max_sp,
            oa_reset_low_setpoint: 0.0,
            oa_reset_low_outdoor: 0.0,
            oa_reset_high_setpoint: 0.0,
            oa_reset_high_outdoor: 0.0,
            min_setpoint: min_sp,
            max_setpoint: max_sp,
        }
    }

    /// Create a coldest-zone setpoint manager.
    ///
    /// Sets supply temperature based on the coldest zone temperature.
    /// As the coldest zone gets colder, supply temp increases (more heating).
    pub fn coldest(name: impl Into<String>, node: usize, min_sp: f64, max_sp: f64) -> Self {
        Self {
            name: name.into(),
            setpoint_type: SetpointType::Coldest,
            variable: SetpointVariable::Temperature,
            setpoint_node: node,
            scheduled_value: min_sp,
            oa_reset_low_setpoint: 0.0,
            oa_reset_low_outdoor: 0.0,
            oa_reset_high_setpoint: 0.0,
            oa_reset_high_outdoor: 0.0,
            min_setpoint: min_sp,
            max_setpoint: max_sp,
        }
    }

    /// Create a single-zone reheat setpoint manager.
    ///
    /// For single-zone systems, supply temp tracks the zone load directly.
    pub fn single_zone_reheat(name: impl Into<String>, node: usize, min_sp: f64, max_sp: f64) -> Self {
        Self {
            name: name.into(),
            setpoint_type: SetpointType::SingleZoneReheat,
            variable: SetpointVariable::Temperature,
            setpoint_node: node,
            scheduled_value: (min_sp + max_sp) / 2.0,
            oa_reset_low_setpoint: 0.0,
            oa_reset_low_outdoor: 0.0,
            oa_reset_high_setpoint: 0.0,
            oa_reset_high_outdoor: 0.0,
            min_setpoint: min_sp,
            max_setpoint: max_sp,
        }
    }

    /// Create a mixed air setpoint manager.
    ///
    /// Sets mixed air node setpoint to maintain supply temp through downstream equipment.
    pub fn mixed_air(name: impl Into<String>, node: usize, reference_setpoint: f64) -> Self {
        Self {
            name: name.into(),
            setpoint_type: SetpointType::MixedAir,
            variable: SetpointVariable::Temperature,
            setpoint_node: node,
            scheduled_value: reference_setpoint,
            oa_reset_low_setpoint: 0.0,
            oa_reset_low_outdoor: 0.0,
            oa_reset_high_setpoint: 0.0,
            oa_reset_high_outdoor: 0.0,
            min_setpoint: -50.0,
            max_setpoint: 100.0,
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

            // Warmest/Coldest/SingleZoneReheat require zone data — use calculate_from_zones
            SetpointType::Warmest | SetpointType::Coldest | SetpointType::SingleZoneReheat => {
                self.scheduled_value
            }

            SetpointType::MixedAir => self.scheduled_value,
        }
    }

    /// Calculate setpoint from zone temperatures (for Warmest/Coldest/SingleZoneReheat).
    ///
    /// `zone_temps` — current zone air temperatures (C).
    /// `zone_setpoints` — zone cooling/heating setpoints (C).
    /// `outdoor_temp` — outdoor air temp (C).
    pub fn calculate_from_zones(
        &self,
        zone_temps: &[f64],
        zone_cooling_setpoints: &[f64],
        zone_heating_setpoints: &[f64],
        outdoor_temp: f64,
    ) -> f64 {
        if zone_temps.is_empty() {
            return self.calculate(outdoor_temp);
        }

        match self.setpoint_type {
            SetpointType::Warmest => {
                // Find the warmest zone relative to its cooling setpoint
                // Supply temp decreases as zones get warmer
                let max_deviation = zone_temps.iter()
                    .zip(zone_cooling_setpoints.iter())
                    .map(|(t, sp)| t - sp)
                    .fold(f64::NEG_INFINITY, f64::max);

                // Map deviation to supply temp: more deviation → lower supply temp
                // At 0 deviation → max_setpoint, at large deviation → min_setpoint
                let band = 3.0; // degrees of zone deviation to go from max to min
                let fraction = (max_deviation / band).clamp(0.0, 1.0);
                let sp = self.max_setpoint - fraction * (self.max_setpoint - self.min_setpoint);
                sp.clamp(self.min_setpoint, self.max_setpoint)
            }

            SetpointType::Coldest => {
                // Find the coldest zone relative to its heating setpoint
                let min_deviation = zone_temps.iter()
                    .zip(zone_heating_setpoints.iter())
                    .map(|(t, sp)| t - sp)
                    .fold(f64::INFINITY, f64::min);

                // More negative deviation → higher supply temp (more heating)
                let band = 3.0;
                let fraction = (-min_deviation / band).clamp(0.0, 1.0);
                let sp = self.min_setpoint + fraction * (self.max_setpoint - self.min_setpoint);
                sp.clamp(self.min_setpoint, self.max_setpoint)
            }

            SetpointType::SingleZoneReheat => {
                // Single zone: supply temp tracks the zone load directly
                let zone_temp = zone_temps[0];
                let cool_sp = zone_cooling_setpoints[0];
                let heat_sp = zone_heating_setpoints[0];

                if zone_temp > cool_sp {
                    // Cooling needed — low supply temp
                    self.min_setpoint
                } else if zone_temp < heat_sp {
                    // Heating needed — high supply temp
                    self.max_setpoint
                } else {
                    // Deadband — moderate supply temp
                    (self.min_setpoint + self.max_setpoint) / 2.0
                }
            }

            _ => self.calculate(outdoor_temp),
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

    // --- Warmest zone tests ---

    #[test]
    fn warmest_zone_at_setpoint() {
        let spm = SetpointManager::warmest("Warmest", 10, 12.0, 18.0);
        // Zone at cooling setpoint: deviation=0 → max supply temp (least cooling)
        let sp = spm.calculate_from_zones(&[24.0], &[24.0], &[20.0], 30.0);
        assert!((sp - 18.0).abs() < 0.01, "sp={}", sp);
    }

    #[test]
    fn warmest_zone_above_setpoint() {
        let spm = SetpointManager::warmest("Warmest", 10, 12.0, 18.0);
        // Zone 3C above cooling setpoint → fully at min supply temp
        let sp = spm.calculate_from_zones(&[27.0], &[24.0], &[20.0], 30.0);
        assert!((sp - 12.0).abs() < 0.01, "sp={}", sp);
    }

    #[test]
    fn warmest_zone_midpoint() {
        let spm = SetpointManager::warmest("Warmest", 10, 12.0, 18.0);
        // Zone 1.5C above cooling setpoint → midpoint supply temp
        let sp = spm.calculate_from_zones(&[25.5], &[24.0], &[20.0], 30.0);
        assert!((sp - 15.0).abs() < 0.1, "sp={}", sp);
    }

    #[test]
    fn warmest_zone_multi_zone() {
        let spm = SetpointManager::warmest("Warmest", 10, 12.0, 18.0);
        // Two zones: one at setpoint, one above — supply follows warmest
        let sp = spm.calculate_from_zones(
            &[24.0, 27.0], &[24.0, 24.0], &[20.0, 20.0], 30.0,
        );
        assert!((sp - 12.0).abs() < 0.01, "sp={}", sp);
    }

    // --- Coldest zone tests ---

    #[test]
    fn coldest_zone_at_setpoint() {
        let spm = SetpointManager::coldest("Coldest", 10, 20.0, 40.0);
        // Zone at heating setpoint: deviation=0 → min supply temp
        let sp = spm.calculate_from_zones(&[21.0], &[24.0], &[21.0], -5.0);
        assert!((sp - 20.0).abs() < 0.01, "sp={}", sp);
    }

    #[test]
    fn coldest_zone_below_setpoint() {
        let spm = SetpointManager::coldest("Coldest", 10, 20.0, 40.0);
        // Zone 3C below heating setpoint → max supply temp
        let sp = spm.calculate_from_zones(&[18.0], &[24.0], &[21.0], -5.0);
        assert!((sp - 40.0).abs() < 0.01, "sp={}", sp);
    }

    // --- Single zone reheat tests ---

    #[test]
    fn single_zone_reheat_cooling() {
        let spm = SetpointManager::single_zone_reheat("SZR", 10, 12.0, 40.0);
        // Zone above cooling setpoint → min supply temp
        let sp = spm.calculate_from_zones(&[26.0], &[24.0], &[20.0], 30.0);
        assert!((sp - 12.0).abs() < 0.01, "sp={}", sp);
    }

    #[test]
    fn single_zone_reheat_heating() {
        let spm = SetpointManager::single_zone_reheat("SZR", 10, 12.0, 40.0);
        // Zone below heating setpoint → max supply temp
        let sp = spm.calculate_from_zones(&[18.0], &[24.0], &[20.0], -5.0);
        assert!((sp - 40.0).abs() < 0.01, "sp={}", sp);
    }

    #[test]
    fn single_zone_reheat_deadband() {
        let spm = SetpointManager::single_zone_reheat("SZR", 10, 12.0, 40.0);
        // Zone in deadband (between heating and cooling setpoints)
        let sp = spm.calculate_from_zones(&[22.0], &[24.0], &[20.0], 15.0);
        assert!((sp - 26.0).abs() < 0.01, "sp={}", sp); // (12+40)/2 = 26
    }

    // --- Mixed air setpoint tests ---

    #[test]
    fn mixed_air_setpoint() {
        let spm = SetpointManager::mixed_air("MA SPM", 5, 13.0);
        assert!((spm.calculate(25.0) - 13.0).abs() < 0.01);
    }
}
