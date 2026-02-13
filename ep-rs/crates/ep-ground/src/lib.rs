//! Ground heat transfer models for EnergyPlus-rs.
//!
//! Implements ground temperature models and foundation coupling:
//! - Kusuda-Achenbach undisturbed ground temperature model
//! - Monthly ground temperature schedule
//! - Shallow ground temperature based on weather data

use std::f64::consts::PI;

/// Kusuda-Achenbach undisturbed ground temperature model.
///
/// T(z, t) = T_mean - T_amp * exp(-z * sqrt(pi / (365 * alpha)))
///         * cos(2*pi/365 * (t - t_shift - z/2 * sqrt(365 / (pi * alpha))))
///
/// Reference: Kusuda, T. and P.R. Achenbach. 1965.
///
/// # Arguments
/// * `depth` - Depth below surface (m)
/// * `day_of_year` - Day of year (1-365)
/// * `t_mean` - Annual average surface temperature (C)
/// * `t_amplitude` - Annual surface temperature amplitude (C)
/// * `phase_shift_days` - Day of minimum surface temperature
/// * `soil_diffusivity` - Thermal diffusivity of soil (m2/day)
#[derive(Debug, Clone)]
pub struct KusudaAchenbach {
    pub t_mean: f64,
    pub t_amplitude: f64,
    pub phase_shift_days: f64,
    pub soil_diffusivity: f64,
}

impl KusudaAchenbach {
    /// Create a new Kusuda-Achenbach model with typical soil parameters.
    pub fn new(t_mean: f64, t_amplitude: f64, phase_shift_days: f64) -> Self {
        Self {
            t_mean,
            t_amplitude,
            phase_shift_days,
            soil_diffusivity: 0.0023, // Typical soil diffusivity (m2/day)
        }
    }

    /// Calculate undisturbed ground temperature at given depth and time.
    ///
    /// Returns temperature in Celsius.
    pub fn temperature(&self, depth: f64, day_of_year: f64) -> f64 {
        if depth < 0.0 || self.soil_diffusivity <= 0.0 {
            return self.t_mean;
        }

        let omega = 2.0 * PI / 365.0; // Annual frequency (rad/day)
        let sqrt_factor = (PI / (365.0 * self.soil_diffusivity)).sqrt();

        let depth_decay = (-depth * sqrt_factor).exp();
        let phase_delay = depth / 2.0 * (365.0 / (PI * self.soil_diffusivity)).sqrt();

        let cos_term = (omega * (day_of_year - self.phase_shift_days - phase_delay)).cos();

        self.t_mean - self.t_amplitude * depth_decay * cos_term
    }

    /// Calculate ground temperature at surface (z=0).
    pub fn surface_temperature(&self, day_of_year: f64) -> f64 {
        self.temperature(0.0, day_of_year)
    }

    /// Calculate deep ground temperature (converges to annual mean).
    pub fn deep_temperature(&self) -> f64 {
        self.t_mean
    }

    /// Estimate parameters from monthly average temperatures.
    ///
    /// Fits T_mean, T_amplitude, and phase_shift from 12 monthly values.
    pub fn from_monthly_temperatures(monthly_temps: &[f64; 12]) -> Self {
        let t_mean: f64 = monthly_temps.iter().sum::<f64>() / 12.0;
        let t_max = monthly_temps.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let t_min = monthly_temps.iter().cloned().fold(f64::INFINITY, f64::min);
        let t_amplitude = (t_max - t_min) / 2.0;

        // Find month of minimum temperature
        let min_month = monthly_temps
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);

        // Convert month to day of year (mid-month)
        let phase_shift_days = (min_month as f64 + 0.5) * 30.44;

        Self {
            t_mean,
            t_amplitude,
            phase_shift_days,
            soil_diffusivity: 0.0023,
        }
    }
}

/// Monthly ground temperature schedule.
///
/// Simple 12-month schedule for ground surface or building foundation contact.
#[derive(Debug, Clone)]
pub struct MonthlyGroundTemperature {
    /// Monthly temperatures (C), index 0 = January.
    pub temperatures: [f64; 12],
}

impl MonthlyGroundTemperature {
    /// Create from 12 monthly values.
    pub fn new(temps: [f64; 12]) -> Self {
        Self { temperatures: temps }
    }

    /// Create with a constant temperature for all months.
    pub fn constant(temp: f64) -> Self {
        Self { temperatures: [temp; 12] }
    }

    /// Get temperature for a given month (1-12).
    pub fn for_month(&self, month: u8) -> f64 {
        let idx = (month.saturating_sub(1) as usize).min(11);
        self.temperatures[idx]
    }

    /// Get temperature interpolated for a given day of year.
    pub fn for_day(&self, day_of_year: f64) -> f64 {
        // Convert day to fractional month (0-indexed)
        let month_frac = (day_of_year - 1.0) / 30.44;
        let month_idx = month_frac.floor() as usize;
        let frac = month_frac - month_idx as f64;

        let idx0 = month_idx % 12;
        let idx1 = (month_idx + 1) % 12;

        self.temperatures[idx0] * (1.0 - frac) + self.temperatures[idx1] * frac
    }

    /// Annual average.
    pub fn annual_average(&self) -> f64 {
        self.temperatures.iter().sum::<f64>() / 12.0
    }
}

/// Shallow ground temperature model.
///
/// Estimates ground temperature at shallow depths (0-2m) using a simplified
/// correlation based on outdoor air temperature with a time lag.
#[derive(Debug, Clone)]
pub struct ShallowGroundTemperature {
    /// Depth (m).
    pub depth: f64,
    /// Annual average outdoor temperature (C).
    pub t_annual_avg: f64,
    /// Annual outdoor temperature amplitude (C).
    pub t_amplitude: f64,
}

impl ShallowGroundTemperature {
    /// Calculate ground temperature at given day of year.
    ///
    /// Uses simplified Kusuda model with fixed soil diffusivity.
    pub fn temperature(&self, day_of_year: f64, phase_shift: f64) -> f64 {
        let model = KusudaAchenbach {
            t_mean: self.t_annual_avg,
            t_amplitude: self.t_amplitude,
            phase_shift_days: phase_shift,
            soil_diffusivity: 0.0023,
        };
        model.temperature(self.depth, day_of_year)
    }
}

/// Estimate soil thermal diffusivity from soil type.
///
/// Returns diffusivity in m2/day.
pub fn soil_diffusivity(soil_type: SoilType) -> f64 {
    match soil_type {
        SoilType::HeavyDamp => 0.0031,
        SoilType::HeavyDry => 0.0020,
        SoilType::LightDamp => 0.0022,
        SoilType::LightDry => 0.0013,
    }
}

/// Soil type classification for thermal property estimation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoilType {
    HeavyDamp,
    HeavyDry,
    LightDamp,
    LightDry,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kusuda_surface_temperature() {
        let model = KusudaAchenbach::new(12.0, 15.0, 30.0);

        // At surface (z=0), should follow cosine wave
        let t_jan = model.surface_temperature(15.0); // Mid-January
        let t_jul = model.surface_temperature(196.0); // Mid-July

        // January should be near minimum, July near maximum
        assert!(t_jan < t_jul, "Jan={t_jan}, Jul={t_jul}");
        assert!(t_jan < 12.0, "Jan={t_jan} should be below mean");
        assert!(t_jul > 12.0, "Jul={t_jul} should be above mean");
    }

    #[test]
    fn kusuda_deep_temperature() {
        let model = KusudaAchenbach::new(12.0, 15.0, 30.0);

        // At great depth, temperature converges to annual mean
        let t_deep = model.temperature(20.0, 100.0);
        assert!((t_deep - 12.0).abs() < 0.5, "t_deep={t_deep}");
    }

    #[test]
    fn kusuda_phase_lag() {
        let model = KusudaAchenbach::new(12.0, 15.0, 30.0);

        // At depth, the temperature extremes are delayed
        let t_surface_min_day = find_min_day(&model, 0.0);
        let t_1m_min_day = find_min_day(&model, 1.0);
        let _t_3m_min_day = find_min_day(&model, 3.0);

        // Deeper = later minimum
        assert!(t_1m_min_day > t_surface_min_day || t_1m_min_day < t_surface_min_day - 300.0);
    }

    fn find_min_day(model: &KusudaAchenbach, depth: f64) -> f64 {
        let mut min_t = f64::MAX;
        let mut min_day = 0.0;
        for d in 1..=365 {
            let t = model.temperature(depth, d as f64);
            if t < min_t {
                min_t = t;
                min_day = d as f64;
            }
        }
        min_day
    }

    #[test]
    fn kusuda_amplitude_decay() {
        let model = KusudaAchenbach::new(12.0, 15.0, 30.0);

        // Temperature amplitude should decay with depth
        let amp_0 = amplitude(&model, 0.0);
        let amp_1 = amplitude(&model, 1.0);
        let amp_3 = amplitude(&model, 3.0);

        assert!(amp_1 < amp_0, "amp_1={amp_1} < amp_0={amp_0}");
        assert!(amp_3 < amp_1, "amp_3={amp_3} < amp_1={amp_1}");
    }

    fn amplitude(model: &KusudaAchenbach, depth: f64) -> f64 {
        let mut min_t = f64::MAX;
        let mut max_t = f64::MIN;
        for d in 1..=365 {
            let t = model.temperature(depth, d as f64);
            min_t = min_t.min(t);
            max_t = max_t.max(t);
        }
        (max_t - min_t) / 2.0
    }

    #[test]
    fn from_monthly_temperatures() {
        let temps = [-5.0, -3.0, 2.0, 8.0, 15.0, 20.0, 23.0, 22.0, 17.0, 10.0, 3.0, -2.0];
        let model = KusudaAchenbach::from_monthly_temperatures(&temps);

        let expected_mean: f64 = temps.iter().sum::<f64>() / 12.0;
        assert!((model.t_mean - expected_mean).abs() < 0.01);
        assert!(model.t_amplitude > 10.0);
    }

    #[test]
    fn monthly_ground_temp() {
        let temps = [2.0, 2.5, 4.0, 8.0, 13.0, 17.0, 19.0, 18.0, 15.0, 10.0, 6.0, 3.0];
        let mgt = MonthlyGroundTemperature::new(temps);

        assert!((mgt.for_month(1) - 2.0).abs() < 1e-10);
        assert!((mgt.for_month(7) - 19.0).abs() < 1e-10);

        let avg = mgt.annual_average();
        assert!(avg > 8.0 && avg < 12.0, "avg={avg}");
    }

    #[test]
    fn monthly_ground_temp_interpolation() {
        let mgt = MonthlyGroundTemperature::constant(10.0);
        // Constant should give 10 regardless of day
        assert!((mgt.for_day(1.0) - 10.0).abs() < 0.1);
        assert!((mgt.for_day(180.0) - 10.0).abs() < 0.1);
    }

    #[test]
    fn soil_diffusivity_values() {
        let d1 = soil_diffusivity(SoilType::HeavyDamp);
        let d2 = soil_diffusivity(SoilType::LightDry);
        assert!(d1 > d2); // Heavy damp has higher diffusivity
    }
}
