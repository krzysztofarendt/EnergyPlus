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

    #[test]
    fn kusuda_negative_depth() {
        // depth < 0 should return t_mean (early return in temperature())
        let model = KusudaAchenbach::new(12.0, 15.0, 30.0);
        for day in [1.0, 90.0, 180.0, 270.0, 365.0] {
            let t = model.temperature(-1.0, day);
            assert!(
                (t - 12.0).abs() < 1e-10,
                "negative depth should return t_mean, got {t} on day {day}"
            );
        }
    }

    #[test]
    fn kusuda_zero_diffusivity() {
        // diffusivity <= 0 should return t_mean for any depth/day
        let model = KusudaAchenbach {
            t_mean: 10.0,
            t_amplitude: 20.0,
            phase_shift_days: 30.0,
            soil_diffusivity: 0.0,
        };
        for depth in [0.0, 1.0, 5.0, 20.0] {
            for day in [1.0, 100.0, 200.0, 365.0] {
                let t = model.temperature(depth, day);
                assert!(
                    (t - 10.0).abs() < 1e-10,
                    "zero diffusivity at depth={depth}, day={day} should return t_mean, got {t}"
                );
            }
        }
    }

    #[test]
    fn kusuda_symmetry() {
        // At the surface, half-year offset from phase_shift should give temperatures
        // symmetrically above/below t_mean. Specifically:
        //   at day = phase_shift, cos(0) = 1 → T = t_mean - t_amplitude (minimum)
        //   at day = phase_shift + 182.5, cos(pi) = -1 → T = t_mean + t_amplitude (maximum)
        // Average of these two should be t_mean.
        let model = KusudaAchenbach::new(15.0, 10.0, 45.0);
        let t_at_shift = model.surface_temperature(45.0);
        let t_half_year = model.surface_temperature(45.0 + 182.5);

        // t_at_shift ≈ t_mean - t_amplitude = 5.0
        // t_half_year ≈ t_mean + t_amplitude = 25.0
        assert!(
            (t_at_shift - 5.0).abs() < 0.1,
            "at phase_shift, expected ~5.0 got {t_at_shift}"
        );
        assert!(
            (t_half_year - 25.0).abs() < 0.1,
            "at phase_shift+182.5, expected ~25.0 got {t_half_year}"
        );
        // Their average should be t_mean
        let avg = (t_at_shift + t_half_year) / 2.0;
        assert!(
            (avg - 15.0).abs() < 0.1,
            "average of symmetric points should be t_mean, got {avg}"
        );
    }

    #[test]
    fn monthly_for_month_boundary() {
        // month=0 and month=13 should not panic; for_month clamps via saturating_sub and min(11)
        let temps = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0];
        let mgt = MonthlyGroundTemperature::new(temps);

        // month=0: saturating_sub(1) → 0, min(11) → 0 → January
        let t0 = mgt.for_month(0);
        assert!(
            (t0 - 1.0).abs() < 1e-10,
            "month=0 should clamp to January, got {t0}"
        );

        // month=13: (13-1)=12 as usize, min(11)=11 → December
        let t13 = mgt.for_month(13);
        assert!(
            (t13 - 12.0).abs() < 1e-10,
            "month=13 should clamp to December, got {t13}"
        );
    }

    #[test]
    fn monthly_for_day_end_of_year() {
        // day=365 should produce a valid interpolated result without panic
        let temps = [0.0, 1.0, 3.0, 7.0, 12.0, 17.0, 20.0, 19.0, 15.0, 9.0, 4.0, 1.0];
        let mgt = MonthlyGroundTemperature::new(temps);

        let t = mgt.for_day(365.0);
        // Should be a finite number within the range of the temperature data
        assert!(t.is_finite(), "day=365 should return finite temp, got {t}");
        assert!(
            t >= -5.0 && t <= 25.0,
            "day=365 temperature out of reasonable range: {t}"
        );
    }

    #[test]
    fn monthly_for_day_interpolation() {
        // Day 15 (mid-January) should interpolate between January and February values
        let temps = [0.0, 10.0, 20.0, 20.0, 20.0, 20.0, 20.0, 20.0, 20.0, 20.0, 20.0, 20.0];
        let mgt = MonthlyGroundTemperature::new(temps);

        let t = mgt.for_day(15.0);
        // day=15 → month_frac = (15-1)/30.44 ≈ 0.46
        // interpolating between temps[0]=0 and temps[1]=10
        // result should be between 0 and 10
        assert!(
            t > 0.0 && t < 10.0,
            "day 15 should interpolate between Jan(0) and Feb(10), got {t}"
        );
    }

    #[test]
    fn shallow_ground_temp_basic() {
        // At depth=0.5m, temperature should be within avg ± amplitude range
        let shallow = ShallowGroundTemperature {
            depth: 0.5,
            t_annual_avg: 12.0,
            t_amplitude: 10.0,
        };
        for day in (1..=365).step_by(30) {
            let t = shallow.temperature(day as f64, 30.0);
            assert!(
                t >= 12.0 - 10.0 - 0.5 && t <= 12.0 + 10.0 + 0.5,
                "shallow temp at day={day} out of range: {t}"
            );
        }
    }

    #[test]
    fn shallow_ground_temp_surface() {
        // At depth=0, ShallowGroundTemperature should match KusudaAchenbach surface temperature
        let shallow = ShallowGroundTemperature {
            depth: 0.0,
            t_annual_avg: 12.0,
            t_amplitude: 15.0,
        };
        let kusuda = KusudaAchenbach::new(12.0, 15.0, 30.0);

        for day in (1..=365).step_by(10) {
            let t_shallow = shallow.temperature(day as f64, 30.0);
            let t_kusuda = kusuda.surface_temperature(day as f64);
            assert!(
                (t_shallow - t_kusuda).abs() < 1e-10,
                "depth=0 mismatch at day={day}: shallow={t_shallow}, kusuda={t_kusuda}"
            );
        }
    }

    #[test]
    fn soil_diffusivity_all_types() {
        // All SoilType variants should return positive diffusivity values
        let types = [
            SoilType::HeavyDamp,
            SoilType::HeavyDry,
            SoilType::LightDamp,
            SoilType::LightDry,
        ];
        for soil in &types {
            let d = soil_diffusivity(*soil);
            assert!(d > 0.0, "diffusivity for {:?} should be > 0, got {}", soil, d);
        }
    }

    #[test]
    fn kusuda_from_monthly_constant() {
        // 12 months all at 15°C → t_mean=15, amplitude≈0
        let temps = [15.0; 12];
        let model = KusudaAchenbach::from_monthly_temperatures(&temps);

        assert!(
            (model.t_mean - 15.0).abs() < 1e-10,
            "t_mean should be 15.0, got {}",
            model.t_mean
        );
        assert!(
            model.t_amplitude.abs() < 1e-10,
            "t_amplitude should be ~0, got {}",
            model.t_amplitude
        );

        // With zero amplitude, temperature at any depth/day should be t_mean
        for depth in [0.0, 1.0, 5.0] {
            for day in [1.0, 100.0, 200.0, 365.0] {
                let t = model.temperature(depth, day);
                assert!(
                    (t - 15.0).abs() < 1e-10,
                    "constant monthly temps at depth={depth}, day={day} should give 15.0, got {t}"
                );
            }
        }
    }
}
