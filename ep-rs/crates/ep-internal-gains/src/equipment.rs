//! Equipment internal gains: electric, gas, hot water, steam, and IT equipment.
//!
//! All equipment types split heat into four fractions:
//! latent, radiant, lost (to outside), and convective (remainder).

use crate::GainOutput;

/// Equipment fuel/energy type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquipmentType {
    Electric,
    Gas,
    HotWater,
    Steam,
    Other,
}

/// Equipment internal gain definition.
#[derive(Debug, Clone)]
pub struct Equipment {
    pub name: String,
    pub equipment_type: EquipmentType,
    /// Design power level (W).
    pub design_level: f64,
    /// Fraction of heat that is latent (moisture) (0-1).
    pub fraction_latent: f64,
    /// Fraction of heat that is radiant (0-1).
    pub fraction_radiant: f64,
    /// Fraction of heat lost / converted to work (0-1).
    pub fraction_lost: f64,
    /// CO2 generation rate per watt (m3/s-W). Typically only for gas equipment.
    pub co2_rate_factor: f64,
}

impl Equipment {
    /// Create electric equipment with typical office fractions.
    ///
    /// Default: 0.0 latent, 0.3 radiant, 0.0 lost, 0.7 convective.
    pub fn electric(name: impl Into<String>, design_level: f64) -> Self {
        Self {
            name: name.into(),
            equipment_type: EquipmentType::Electric,
            design_level,
            fraction_latent: 0.0,
            fraction_radiant: 0.30,
            fraction_lost: 0.0,
            co2_rate_factor: 0.0,
        }
    }

    /// Create gas equipment with typical fractions.
    ///
    /// Default: 0.0 latent, 0.3 radiant, 0.0 lost, 0.7 convective.
    pub fn gas(name: impl Into<String>, design_level: f64) -> Self {
        Self {
            name: name.into(),
            equipment_type: EquipmentType::Gas,
            design_level,
            fraction_latent: 0.0,
            fraction_radiant: 0.30,
            fraction_lost: 0.0,
            co2_rate_factor: 0.0,
        }
    }

    /// Fraction convective (remainder after latent + radiant + lost).
    pub fn fraction_convective(&self) -> f64 {
        (1.0 - self.fraction_latent - self.fraction_radiant - self.fraction_lost).max(0.0)
    }

    /// Calculate equipment internal gains.
    ///
    /// # Arguments
    /// * `schedule_value` - Schedule multiplier (0-1)
    pub fn calculate(&self, schedule_value: f64) -> GainOutput {
        let power = self.design_level * schedule_value.max(0.0);

        if power <= 0.0 {
            return GainOutput::default();
        }

        let f_conv = self.fraction_convective();
        let lost = power * self.fraction_lost;

        GainOutput {
            total_power: power,
            convective: power * f_conv,
            radiant: power * self.fraction_radiant,
            latent: power * self.fraction_latent,
            lost,
            co2_generation: power * self.co2_rate_factor,
            ..Default::default()
        }
    }
}

/// IT Equipment model with CPU, fan, and UPS components.
#[derive(Debug, Clone)]
pub struct ITEquipment {
    pub name: String,
    /// Design power (W) for the IT load (CPU).
    pub design_power: f64,
    /// Fan power fraction of IT load (0-1).
    pub fan_power_fraction: f64,
    /// UPS efficiency (0-1). Losses = design_power * (1/efficiency - 1).
    pub ups_efficiency: f64,
    /// Fraction of CPU heat that is radiant.
    pub fraction_radiant: f64,
    /// Supply air temperature setpoint (C) for IT cooling.
    pub supply_air_setpoint: f64,
}

impl ITEquipment {
    pub fn new(name: impl Into<String>, design_power: f64) -> Self {
        Self {
            name: name.into(),
            design_power,
            fan_power_fraction: 0.15,
            ups_efficiency: 0.95,
            fraction_radiant: 0.20,
            supply_air_setpoint: 20.0,
        }
    }

    /// Calculate IT equipment gains.
    ///
    /// Returns gains for CPU + fan heat to zone, plus UPS losses.
    pub fn calculate(&self, schedule_value: f64) -> GainOutput {
        let cpu_power = self.design_power * schedule_value.max(0.0);
        if cpu_power <= 0.0 {
            return GainOutput::default();
        }

        let fan_power = cpu_power * self.fan_power_fraction;
        let ups_loss = cpu_power * (1.0 / self.ups_efficiency.max(0.01) - 1.0);
        let total = cpu_power + fan_power + ups_loss;

        let radiant = total * self.fraction_radiant;
        let convective = total - radiant;

        GainOutput {
            total_power: total,
            convective,
            radiant,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electric_equipment_basic() {
        let equip = Equipment::electric("Computers", 500.0);
        let gain = equip.calculate(1.0);

        assert!((gain.total_power - 500.0).abs() < 1e-10);
        assert!((gain.radiant - 150.0).abs() < 1e-10);
        assert!((gain.convective - 350.0).abs() < 1e-10);

        // Conservation
        let sum = gain.convective + gain.radiant + gain.latent + gain.lost;
        assert!((sum - 500.0).abs() < 1e-10);
    }

    #[test]
    fn gas_equipment_with_latent() {
        let mut equip = Equipment::gas("GasRange", 3000.0);
        equip.fraction_latent = 0.20;
        equip.fraction_radiant = 0.30;
        equip.fraction_lost = 0.10;

        let gain = equip.calculate(0.5);
        let power = 1500.0;

        assert!((gain.total_power - power).abs() < 1e-10);
        assert!((gain.latent - power * 0.20).abs() < 1e-10);
        assert!((gain.radiant - power * 0.30).abs() < 1e-10);
        assert!((gain.lost - power * 0.10).abs() < 1e-10);
        assert!((gain.convective - power * 0.40).abs() < 1e-10);
    }

    #[test]
    fn equipment_fraction_conservation() {
        let equip = Equipment {
            name: "Test".into(),
            equipment_type: EquipmentType::Electric,
            design_level: 1000.0,
            fraction_latent: 0.15,
            fraction_radiant: 0.35,
            fraction_lost: 0.10,
            co2_rate_factor: 0.0,
        };
        let f_sum = equip.fraction_latent
            + equip.fraction_radiant
            + equip.fraction_lost
            + equip.fraction_convective();
        assert!((f_sum - 1.0).abs() < 1e-10);
    }

    #[test]
    fn it_equipment_with_ups() {
        let it = ITEquipment::new("DataCenter", 10000.0);
        let gain = it.calculate(1.0);

        // CPU = 10000, Fan = 1500, UPS loss = 10000*(1/0.95 - 1) ≈ 526.3
        let expected_total = 10000.0 + 1500.0 + 10000.0 * (1.0 / 0.95 - 1.0);
        assert!((gain.total_power - expected_total).abs() < 1.0, "total={}", gain.total_power);

        // Radiant = 20% of total
        assert!((gain.radiant - expected_total * 0.20).abs() < 1.0);
    }

    #[test]
    fn equipment_schedule_off() {
        let equip = Equipment::electric("Eq", 1000.0);
        let gain = equip.calculate(0.0);
        assert!((gain.total_power).abs() < 1e-10);
    }
}
