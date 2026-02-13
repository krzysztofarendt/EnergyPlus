//! People (occupancy) internal gains model.
//!
//! Metabolic heat is split into sensible and latent components.
//! Sensible heat is further split into radiant and convective fractions.

use crate::GainOutput;

/// People internal gain definition.
#[derive(Debug, Clone)]
pub struct People {
    pub name: String,
    /// Maximum number of people in the space.
    pub number_of_people: f64,
    /// Fraction of sensible gain that is radiant (0-1).
    pub fraction_radiant: f64,
    /// Fraction of total gain that is sensible (if specified, overrides comfort model).
    /// Typical value ~0.6 at light activity.
    pub sensible_heat_fraction: Option<f64>,
    /// CO2 generation rate per watt of total metabolic rate (m3/s-W).
    pub co2_rate_factor: f64,
    /// Whether a comfort model calculates the sensible/latent split.
    pub use_comfort_model: bool,
}

impl People {
    /// Create people with default activity parameters.
    ///
    /// Default: 0.3 radiant fraction, 0.6 sensible fraction, standard CO2 rate.
    pub fn new(name: impl Into<String>, number_of_people: f64) -> Self {
        Self {
            name: name.into(),
            number_of_people,
            fraction_radiant: 0.30,
            sensible_heat_fraction: Some(0.6),
            co2_rate_factor: 3.82e-8, // m3/s per W (ASHRAE standard)
            use_comfort_model: false,
        }
    }

    /// Calculate people internal gains.
    ///
    /// # Arguments
    /// * `occupancy_fraction` - Schedule value (0-1) for fraction of max people present
    /// * `activity_level` - Metabolic rate per person (W/person)
    pub fn calculate(&self, occupancy_fraction: f64, activity_level: f64) -> GainOutput {
        let num_people = self.number_of_people * occupancy_fraction.max(0.0);
        let total_metabolic = num_people * activity_level;

        if total_metabolic <= 0.0 {
            return GainOutput::default();
        }

        // Default sensible fraction (can be overridden by comfort model)
        let sensible_frac = self.sensible_heat_fraction.unwrap_or(0.6);
        let sensible = total_metabolic * sensible_frac;
        let latent = total_metabolic - sensible;

        let radiant = sensible * self.fraction_radiant;
        let convective = sensible * (1.0 - self.fraction_radiant);

        let co2 = total_metabolic * self.co2_rate_factor;

        GainOutput {
            total_power: total_metabolic,
            convective,
            radiant,
            latent,
            co2_generation: co2,
            ..Default::default()
        }
    }
}

/// Standard metabolic rates (W/person) from ASHRAE.
pub mod activity_levels {
    /// Seated, quiet (office work): ~120 W/person.
    pub const SEATED_QUIET: f64 = 120.0;
    /// Standing, light work: ~150 W/person.
    pub const STANDING_LIGHT: f64 = 150.0;
    /// Walking (moderate activity): ~200 W/person.
    pub const WALKING: f64 = 200.0;
    /// Heavy work (exercise): ~300 W/person.
    pub const HEAVY_WORK: f64 = 300.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn people_basic_gain() {
        let people = People::new("Office", 10.0);
        let gain = people.calculate(1.0, activity_levels::SEATED_QUIET);

        // 10 people * 120 W = 1200 W total
        assert!((gain.total_power - 1200.0).abs() < 1e-10);

        // Sensible = 0.6 * 1200 = 720 W
        assert!((gain.sensible() - 720.0).abs() < 1e-10);

        // Latent = 480 W
        assert!((gain.latent - 480.0).abs() < 1e-10);

        // Radiant = 0.3 * 720 = 216 W
        assert!((gain.radiant - 216.0).abs() < 1e-10);

        // Convective = 0.7 * 720 = 504 W
        assert!((gain.convective - 504.0).abs() < 1e-10);

        // Conservation
        let sum = gain.convective + gain.radiant + gain.latent;
        assert!((sum - 1200.0).abs() < 1e-10);
    }

    #[test]
    fn people_partial_occupancy() {
        let people = People::new("Office", 20.0);
        let gain_full = people.calculate(1.0, 120.0);
        let gain_half = people.calculate(0.5, 120.0);
        assert!((gain_half.total_power - gain_full.total_power * 0.5).abs() < 1e-10);
    }

    #[test]
    fn people_co2_generation() {
        let people = People::new("Room", 5.0);
        let gain = people.calculate(1.0, 120.0);
        // CO2 = 600 * 3.82e-8 = 2.292e-5 m3/s
        assert!(gain.co2_generation > 0.0);
        assert!((gain.co2_generation - 600.0 * 3.82e-8).abs() < 1e-12);
    }

    #[test]
    fn people_zero_occupancy() {
        let people = People::new("Empty", 10.0);
        let gain = people.calculate(0.0, 120.0);
        assert!((gain.total_power).abs() < 1e-10);
    }
}
