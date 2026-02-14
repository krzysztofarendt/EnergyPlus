//! Refrigerated display case model.
//!
//! Models open and closed display cases with rated capacity, fan power,
//! lighting heat, anti-sweat heater, defrost, and restocking schedules.

/// Defrost type for display cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefrostType {
    /// No defrost (for medium-temp cases).
    None,
    /// Off-cycle defrost — compressor off, fan continues.
    OffCycle,
    /// Electric heater defrost.
    Electric,
    /// Hot gas defrost.
    HotGas,
}

/// Anti-sweat heater control type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntiSweatControl {
    /// Constant power.
    Constant,
    /// Linear with humidity.
    Linear,
    /// Dew-point based.
    DewPoint,
    /// Heat balance method.
    HeatBalance,
}

/// Refrigerated display case.
#[derive(Debug, Clone)]
pub struct DisplayCase {
    pub name: String,
    /// Rated total cooling capacity per unit length (W/m).
    pub rated_capacity_per_length: f64,
    /// Case length (m).
    pub length: f64,
    /// Operating temperature (C).
    pub operating_temp: f64,
    /// Rated ambient temperature (C) — typically 23.9°C (75°F).
    pub rated_ambient_temp: f64,
    /// Rated ambient relative humidity (fraction).
    pub rated_ambient_rh: f64,
    /// Rated latent heat ratio (fraction of total cooling that is latent).
    pub rated_lhr: f64,
    /// Fan power per unit length (W/m).
    pub fan_power_per_length: f64,
    /// Lighting power per unit length (W/m).
    pub lighting_power_per_length: f64,
    /// Fraction of lighting heat to case.
    pub lighting_to_case_fraction: f64,
    /// Anti-sweat heater power per unit length (W/m).
    pub anti_sweat_power_per_length: f64,
    /// Anti-sweat heater control.
    pub anti_sweat_control: AntiSweatControl,
    /// Defrost type.
    pub defrost_type: DefrostType,
    /// Defrost power per unit length (W/m).
    pub defrost_power_per_length: f64,
    /// Defrost schedule fraction (0-1, fraction of hour in defrost).
    pub defrost_schedule_fraction: f64,
    /// Restocking schedule (W of added load).
    pub restocking_load: f64,
    /// Case credit fraction — fraction of case cooling that offsets zone HVAC.
    pub case_credit_fraction: f64,
}

/// Display case calculation results.
#[derive(Debug, Clone, Copy, Default)]
pub struct CaseResult {
    /// Total cooling load on the refrigeration system (W).
    pub total_cooling_load: f64,
    /// Sensible case credit to zone (W, negative = cooling).
    pub sensible_case_credit: f64,
    /// Latent case credit to zone (W).
    pub latent_case_credit: f64,
    /// Fan electric power (W).
    pub fan_power: f64,
    /// Lighting electric power (W).
    pub lighting_power: f64,
    /// Anti-sweat heater electric power (W).
    pub anti_sweat_power: f64,
    /// Defrost electric power (W).
    pub defrost_power: f64,
}

impl DisplayCase {
    pub fn new(name: impl Into<String>, length: f64, operating_temp: f64) -> Self {
        Self {
            name: name.into(),
            rated_capacity_per_length: 375.0, // W/m typical
            length,
            operating_temp,
            rated_ambient_temp: 23.9,
            rated_ambient_rh: 0.55,
            rated_lhr: 0.3,
            fan_power_per_length: 55.0,
            lighting_power_per_length: 33.0,
            lighting_to_case_fraction: 0.1,
            anti_sweat_power_per_length: 0.0,
            anti_sweat_control: AntiSweatControl::Constant,
            defrost_type: DefrostType::OffCycle,
            defrost_power_per_length: 0.0,
            defrost_schedule_fraction: 0.0,
            restocking_load: 0.0,
            case_credit_fraction: 0.2,
        }
    }

    /// Calculate case loads for the current timestep.
    ///
    /// `zone_temp` — zone dry-bulb temperature (C).
    /// `zone_rh` — zone relative humidity (fraction).
    pub fn calculate(&self, zone_temp: f64, zone_rh: f64) -> CaseResult {
        // Capacity correction for ambient conditions
        let temp_factor = (zone_temp - self.operating_temp)
            / (self.rated_ambient_temp - self.operating_temp);
        let temp_factor = temp_factor.max(0.0);

        // Humidity correction for latent load
        let rh_factor = (zone_rh / self.rated_ambient_rh).max(0.0).min(2.0);

        // Base capacity
        let rated_capacity = self.rated_capacity_per_length * self.length;
        let sensible_capacity = rated_capacity * (1.0 - self.rated_lhr) * temp_factor;
        let latent_capacity = rated_capacity * self.rated_lhr * rh_factor * temp_factor;

        // Fan power (always on)
        let fan_power = self.fan_power_per_length * self.length;

        // Lighting power
        let lighting_power = self.lighting_power_per_length * self.length;
        let lighting_to_case = lighting_power * self.lighting_to_case_fraction;

        // Anti-sweat heater
        let anti_sweat_power = match self.anti_sweat_control {
            AntiSweatControl::Constant => self.anti_sweat_power_per_length * self.length,
            AntiSweatControl::Linear => {
                // Reduce linearly as humidity drops below rated
                let factor = (zone_rh / self.rated_ambient_rh).min(1.0);
                self.anti_sweat_power_per_length * self.length * factor
            }
            _ => self.anti_sweat_power_per_length * self.length,
        };

        // Defrost load
        let defrost_power = match self.defrost_type {
            DefrostType::Electric | DefrostType::HotGas => {
                self.defrost_power_per_length * self.length * self.defrost_schedule_fraction
            }
            _ => 0.0,
        };

        // Total cooling load = case capacity + internal heat sources
        let total_cooling_load = sensible_capacity + latent_capacity
            + fan_power
            + lighting_to_case
            + anti_sweat_power
            + defrost_power
            + self.restocking_load;

        // Case credits to zone (cooling effect from case)
        let sensible_credit = -total_cooling_load * self.case_credit_fraction;
        let latent_credit = -latent_capacity * self.case_credit_fraction;

        CaseResult {
            total_cooling_load,
            sensible_case_credit: sensible_credit,
            latent_case_credit: latent_credit,
            fan_power,
            lighting_power,
            anti_sweat_power,
            defrost_power,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_case_rated_conditions() {
        let case = DisplayCase::new("Case-1", 3.0, -5.0);
        let result = case.calculate(23.9, 0.55);

        assert!(result.total_cooling_load > 0.0);
        assert!(result.fan_power > 0.0);
        assert!(result.lighting_power > 0.0);
        assert!(result.sensible_case_credit < 0.0); // Cooling to zone
    }

    #[test]
    fn display_case_low_ambient() {
        let case = DisplayCase::new("Case-1", 3.0, -5.0);
        let rated = case.calculate(23.9, 0.55);
        let low = case.calculate(18.0, 0.40);

        // Lower ambient should reduce capacity
        assert!(low.total_cooling_load < rated.total_cooling_load);
    }

    #[test]
    fn display_case_at_operating_temp() {
        let case = DisplayCase::new("Case-1", 3.0, -5.0);
        let result = case.calculate(-5.0, 0.55);

        // At operating temp, capacity factor is 0 but fan/lighting still run
        assert!(result.fan_power > 0.0);
        assert!(result.lighting_power > 0.0);
    }

    #[test]
    fn display_case_electric_defrost() {
        let mut case = DisplayCase::new("Case-1", 3.0, -23.0);
        case.defrost_type = DefrostType::Electric;
        case.defrost_power_per_length = 350.0;
        case.defrost_schedule_fraction = 0.1;

        let result = case.calculate(23.9, 0.55);
        assert!((result.defrost_power - 105.0).abs() < 1.0); // 350 * 3 * 0.1
    }

    #[test]
    fn display_case_length_scaling() {
        let short = DisplayCase::new("Short", 2.0, -5.0);
        let long = DisplayCase::new("Long", 4.0, -5.0);

        let r_short = short.calculate(23.9, 0.55);
        let r_long = long.calculate(23.9, 0.55);

        assert!((r_long.total_cooling_load / r_short.total_cooling_load - 2.0).abs() < 0.01);
    }
}
