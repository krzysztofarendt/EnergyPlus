//! Zone mixing and cross-mixing models.
//!
//! Zone mixing: one-directional airflow from source zone to receiving zone.
//! Cross-mixing: bidirectional airflow exchange between two zones.

use ep_psychrometrics::{cp_air, rho_air};

/// Zone mixing: air flows from one zone to another.
#[derive(Debug, Clone)]
pub struct ZoneMixing {
    pub name: String,
    /// Source zone index.
    pub from_zone: usize,
    /// Receiving zone index.
    pub to_zone: usize,
    /// Design volume flow rate (m3/s).
    pub design_flow_rate: f64,
    /// Minimum temperature difference to trigger mixing (C).
    /// Mixing occurs when T_from - T_to > delta_temp.
    pub delta_temperature: f64,
    /// Temperature control limits.
    pub min_receiving_temp: Option<f64>,
    pub max_receiving_temp: Option<f64>,
    pub min_source_temp: Option<f64>,
    pub max_source_temp: Option<f64>,
}

impl ZoneMixing {
    pub fn new(name: impl Into<String>, from_zone: usize, to_zone: usize, flow_rate: f64) -> Self {
        Self {
            name: name.into(),
            from_zone,
            to_zone,
            design_flow_rate: flow_rate,
            delta_temperature: 0.0,
            min_receiving_temp: None,
            max_receiving_temp: None,
            min_source_temp: None,
            max_source_temp: None,
        }
    }

    /// Check if mixing is active based on temperature conditions.
    pub fn is_active(&self, t_receiving: f64, t_source: f64) -> bool {
        // Delta temperature check
        if self.delta_temperature > 0.0 {
            if t_source - t_receiving < self.delta_temperature {
                return false;
            }
        }
        if let Some(min) = self.min_receiving_temp {
            if t_receiving < min { return false; }
        }
        if let Some(max) = self.max_receiving_temp {
            if t_receiving > max { return false; }
        }
        if let Some(min) = self.min_source_temp {
            if t_source < min { return false; }
        }
        if let Some(max) = self.max_source_temp {
            if t_source > max { return false; }
        }
        true
    }

    /// Calculate mixing flow and thermal effects.
    pub fn calculate(
        &self,
        schedule: f64,
        t_receiving: f64,
        t_source: f64,
        w_source: f64,
        pressure: f64,
    ) -> MixingOutput {
        if !self.is_active(t_receiving, t_source) || schedule <= 0.0 {
            return MixingOutput::default();
        }

        let v_dot = self.design_flow_rate * schedule;
        let rho = rho_air(pressure, t_source, w_source);
        let mass_flow = v_dot * rho;

        MixingOutput {
            volume_flow_rate: v_dot,
            mass_flow_rate: mass_flow,
            from_zone: self.from_zone,
            to_zone: self.to_zone,
        }
    }
}

/// Cross-mixing: bidirectional airflow between two zones.
#[derive(Debug, Clone)]
pub struct CrossMixing {
    pub name: String,
    /// First zone index.
    pub zone_a: usize,
    /// Second zone index.
    pub zone_b: usize,
    /// Design volume flow rate in each direction (m3/s).
    pub design_flow_rate: f64,
    /// Minimum temperature difference to trigger mixing (C).
    pub delta_temperature: f64,
}

impl CrossMixing {
    pub fn new(name: impl Into<String>, zone_a: usize, zone_b: usize, flow_rate: f64) -> Self {
        Self {
            name: name.into(),
            zone_a,
            zone_b,
            design_flow_rate: flow_rate,
            delta_temperature: 0.0,
        }
    }

    /// Calculate cross-mixing flows and thermal effects.
    ///
    /// Returns two MixingOutputs: A→B and B→A.
    pub fn calculate(
        &self,
        schedule: f64,
        t_a: f64,
        w_a: f64,
        t_b: f64,
        w_b: f64,
        pressure: f64,
    ) -> (MixingOutput, MixingOutput) {
        if schedule <= 0.0 {
            return (MixingOutput::default(), MixingOutput::default());
        }

        if self.delta_temperature > 0.0 {
            if (t_a - t_b).abs() < self.delta_temperature {
                return (MixingOutput::default(), MixingOutput::default());
            }
        }

        let v_dot = self.design_flow_rate * schedule;

        let rho_a = rho_air(pressure, t_a, w_a);
        let rho_b = rho_air(pressure, t_b, w_b);

        let a_to_b = MixingOutput {
            volume_flow_rate: v_dot,
            mass_flow_rate: v_dot * rho_a,
            from_zone: self.zone_a,
            to_zone: self.zone_b,
        };

        let b_to_a = MixingOutput {
            volume_flow_rate: v_dot,
            mass_flow_rate: v_dot * rho_b,
            from_zone: self.zone_b,
            to_zone: self.zone_a,
        };

        (a_to_b, b_to_a)
    }
}

/// Output from a mixing calculation.
#[derive(Debug, Clone, Copy, Default)]
pub struct MixingOutput {
    /// Volume flow rate (m3/s).
    pub volume_flow_rate: f64,
    /// Mass flow rate (kg/s).
    pub mass_flow_rate: f64,
    /// Source zone index.
    pub from_zone: usize,
    /// Receiving zone index.
    pub to_zone: usize,
}

impl MixingOutput {
    /// Calculate sensible heat transfer to the receiving zone (W).
    ///
    /// Positive = heating the receiving zone.
    pub fn sensible_heat(&self, t_receiving: f64, t_source: f64, w_source: f64) -> f64 {
        let cp = cp_air(w_source);
        self.mass_flow_rate * cp * (t_source - t_receiving)
    }

    /// Calculate latent heat transfer to the receiving zone (W).
    pub fn latent_heat(&self, w_receiving: f64, w_source: f64) -> f64 {
        self.mass_flow_rate * 2_501_000.0 * (w_source - w_receiving)
    }

    /// Calculate MCp product for the predictor-corrector (W/K).
    pub fn mcp(&self, w_source: f64) -> f64 {
        self.mass_flow_rate * cp_air(w_source)
    }

    /// Calculate MCpT product for the predictor-corrector (W).
    pub fn mcpt(&self, t_source: f64, w_source: f64) -> f64 {
        self.mass_flow_rate * cp_air(w_source) * t_source
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_mixing_basic() {
        let mix = ZoneMixing::new("Mix", 0, 1, 0.5);
        let output = mix.calculate(1.0, 22.0, 25.0, 0.008, 101325.0);

        assert!((output.volume_flow_rate - 0.5).abs() < 1e-10);
        assert!(output.mass_flow_rate > 0.5);
        assert_eq!(output.from_zone, 0);
        assert_eq!(output.to_zone, 1);
    }

    #[test]
    fn zone_mixing_delta_temp() {
        let mut mix = ZoneMixing::new("Mix", 0, 1, 0.5);
        mix.delta_temperature = 5.0;

        // dT = 25 - 22 = 3 < 5, so mixing should not occur
        assert!(!mix.is_active(22.0, 25.0));
        let output = mix.calculate(1.0, 22.0, 25.0, 0.008, 101325.0);
        assert!((output.volume_flow_rate).abs() < 1e-10);

        // dT = 30 - 22 = 8 > 5, mixing should occur
        assert!(mix.is_active(22.0, 30.0));
    }

    #[test]
    fn cross_mixing_bidirectional() {
        let cross = CrossMixing::new("Cross", 0, 1, 0.3);
        let (a_to_b, b_to_a) = cross.calculate(
            1.0,
            22.0, 0.008,  // Zone A
            25.0, 0.010,  // Zone B
            101325.0,
        );

        // Both directions should have the same volume flow
        assert!((a_to_b.volume_flow_rate - 0.3).abs() < 1e-10);
        assert!((b_to_a.volume_flow_rate - 0.3).abs() < 1e-10);

        // Mass flows may differ due to density differences
        assert!(a_to_b.mass_flow_rate > 0.0);
        assert!(b_to_a.mass_flow_rate > 0.0);
    }

    #[test]
    fn mixing_thermal_effect() {
        let output = MixingOutput {
            volume_flow_rate: 0.5,
            mass_flow_rate: 0.6,
            from_zone: 0,
            to_zone: 1,
        };
        // Source zone at 30C flowing into zone at 22C -> heating
        let q = output.sensible_heat(22.0, 30.0, 0.008);
        assert!(q > 0.0, "Should heat the receiving zone");
        // Q ≈ 0.6 * 1005 * 8 ≈ 4824 W
        assert!((q - 0.6 * cp_air(0.008) * 8.0).abs() < 1.0);
    }

    #[test]
    fn mixing_mcp_product() {
        let output = MixingOutput {
            volume_flow_rate: 0.1,
            mass_flow_rate: 0.12,
            from_zone: 0,
            to_zone: 1,
        };
        let mcp = output.mcp(0.008);
        assert!(mcp > 100.0); // 0.12 * ~1005 ≈ 120.6
        let mcpt = output.mcpt(25.0, 0.008);
        assert!((mcpt - mcp * 25.0).abs() < 1e-10);
    }
}
