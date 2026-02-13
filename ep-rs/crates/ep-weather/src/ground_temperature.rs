//! Ground temperature models.

use ep_units::*;
use crate::GroundTemperatureModel;

/// Kusuda-Achenbach ground temperature correlation.
///
/// T(z, t) = T_avg - T_amp * exp(-z * sqrt(pi/(365*alpha))) * cos(2*pi/365 * (t - t_shift - z/2 * sqrt(365/(pi*alpha))))
///
/// Reference: Kusuda, T. and Achenbach, P.R. (1965)
#[derive(Debug, Clone)]
pub struct KusudaAchenbach {
    /// Average annual surface temperature (K).
    pub t_avg: Temperature,
    /// Annual surface temperature amplitude (K or deltaK).
    pub t_amplitude: f64,
    /// Phase constant: day of minimum surface temperature.
    pub phase_constant: f64,
    /// Soil thermal diffusivity (m²/s).
    pub soil_diffusivity: f64,
}

impl KusudaAchenbach {
    pub fn new(t_avg_celsius: f64, t_amplitude: f64, phase_constant: f64, soil_diffusivity: f64) -> Self {
        Self {
            t_avg: Temperature::from_celsius(t_avg_celsius),
            t_amplitude,
            phase_constant,
            soil_diffusivity,
        }
    }
}

impl GroundTemperatureModel for KusudaAchenbach {
    fn temperature_at_depth(&self, depth: Length, day_of_year: u16) -> Temperature {
        let z = depth.value(); // meters
        let t = day_of_year as f64;
        let pi = std::f64::consts::PI;

        let period = 365.0 * 24.0 * 3600.0; // seconds in a year
        let alpha = self.soil_diffusivity;

        // Depth attenuation factor
        let depth_factor = (-z * (pi / (period * alpha)).sqrt()).exp();

        // Phase shift due to depth
        let phase_shift = z / 2.0 * (period / (pi * alpha)).sqrt() / (24.0 * 3600.0); // in days

        let t_ground_c = self.t_avg.to_celsius()
            - self.t_amplitude * depth_factor * (2.0 * pi / 365.0 * (t - self.phase_constant - phase_shift)).cos();

        Temperature::from_celsius(t_ground_c)
    }
}

/// Simple monthly ground temperature model.
///
/// Returns a constant temperature for each month (12 values).
#[derive(Debug, Clone)]
pub struct MonthlyGroundTemperature {
    /// Monthly ground temperatures (°C), index 0 = January.
    pub monthly_temps: [f64; 12],
}

impl MonthlyGroundTemperature {
    pub fn new(temps: [f64; 12]) -> Self {
        Self { monthly_temps: temps }
    }
}

impl GroundTemperatureModel for MonthlyGroundTemperature {
    fn temperature_at_depth(&self, _depth: Length, day_of_year: u16) -> Temperature {
        // Convert day of year to approximate month index
        let month = ((day_of_year as f64 / 30.44).floor() as usize).min(11);
        Temperature::from_celsius(self.monthly_temps[month])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kusuda_achenbach_surface() {
        // At surface (z=0), should follow the cosine wave
        let model = KusudaAchenbach::new(12.0, 10.0, 30.0, 1.5e-6);
        let t_summer = model.temperature_at_depth(Length::new(0.0), 200);
        let t_winter = model.temperature_at_depth(Length::new(0.0), 30);
        // Summer should be warmer than winter at surface
        assert!(t_summer.to_celsius() > t_winter.to_celsius());
    }

    #[test]
    fn kusuda_achenbach_deep() {
        // At great depth, temperature should approach the annual average
        let model = KusudaAchenbach::new(12.0, 10.0, 30.0, 1.5e-6);
        let t_deep_summer = model.temperature_at_depth(Length::new(20.0), 200);
        let t_deep_winter = model.temperature_at_depth(Length::new(20.0), 30);
        // At 20m depth, seasonal variation should be negligible
        assert!((t_deep_summer.to_celsius() - t_deep_winter.to_celsius()).abs() < 1.0);
        assert!((t_deep_summer.to_celsius() - 12.0).abs() < 1.0);
    }

    #[test]
    fn monthly_ground_temp() {
        let temps = [5.0, 5.5, 7.0, 10.0, 14.0, 18.0, 21.0, 22.0, 19.0, 15.0, 10.0, 6.0];
        let model = MonthlyGroundTemperature::new(temps);
        // July (day ~200)
        let t = model.temperature_at_depth(Length::new(1.0), 200);
        assert!((t.to_celsius() - 21.0).abs() < 1.0);
    }
}
