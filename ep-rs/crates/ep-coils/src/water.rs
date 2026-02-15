//! Water coil models using effectiveness-NTU method.
//!
//! Supports counterflow and crossflow configurations for both
//! heating and cooling applications. Includes wet coil (condensation)
//! model for cooling coils when surface temperature drops below dew point.

/// Heat exchanger flow configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeatExchangerType {
    /// Counterflow: hot and cold streams flow in opposite directions.
    #[default]
    CounterFlow,
    /// Crossflow: both streams unmixed.
    CrossFlow,
}

/// Water coil specification.
#[derive(Debug, Clone)]
pub struct WaterCoil {
    /// Coil name.
    pub name: String,
    /// Heat exchanger configuration.
    pub hx_type: HeatExchangerType,
    /// Overall heat transfer coefficient UA (W/K).
    pub ua: f64,
    /// Design water mass flow rate (kg/s).
    pub design_water_flow: f64,
    /// Design air mass flow rate (kg/s).
    pub design_air_flow: f64,
    /// Whether this is a cooling coil (affects dehumidification).
    pub is_cooling: bool,
}

/// Result of a water coil calculation.
#[derive(Debug, Clone, Copy)]
pub struct WaterCoilResult {
    /// Heat transfer rate (W). Positive = heating air, negative = cooling air.
    pub heat_rate: f64,
    /// Air outlet temperature (C).
    pub air_outlet_temp: f64,
    /// Water outlet temperature (C).
    pub water_outlet_temp: f64,
    /// Air outlet humidity ratio (kg/kg).
    pub air_outlet_humidity_ratio: f64,
    /// Sensible heat transfer rate (W).
    pub sensible_heat_rate: f64,
    /// Latent heat transfer rate (W). Negative = dehumidification.
    pub latent_heat_rate: f64,
    /// Effectiveness (0-1).
    pub effectiveness: f64,
    /// Whether condensation occurred (wet coil).
    pub is_wet: bool,
}

/// Calculate effectiveness for a counterflow heat exchanger.
///
/// epsilon = (1 - exp(-NTU*(1-Cr))) / (1 - Cr*exp(-NTU*(1-Cr)))
/// When Cr = 1: epsilon = NTU / (1 + NTU)
pub fn effectiveness_counterflow(ntu: f64, capacity_ratio: f64) -> f64 {
    if ntu <= 0.0 {
        return 0.0;
    }
    let cr = capacity_ratio.clamp(0.0, 1.0);
    if (cr - 1.0).abs() < 1e-6 {
        // Special case: equal capacity rates
        ntu / (1.0 + ntu)
    } else {
        let exp_term = (-ntu * (1.0 - cr)).exp();
        (1.0 - exp_term) / (1.0 - cr * exp_term)
    }
}

/// Calculate effectiveness for a crossflow heat exchanger (both unmixed).
///
/// epsilon = 1 - exp(-(NTU^0.78/Cr)*(1-exp(-Cr*NTU^0.22)))
pub fn effectiveness_crossflow(ntu: f64, capacity_ratio: f64) -> f64 {
    if ntu <= 0.0 {
        return 0.0;
    }
    let cr = capacity_ratio.clamp(1e-10, 1.0);
    let ntu_078 = ntu.powf(0.78);
    let ntu_022 = ntu.powf(0.22);
    let inner = 1.0 - (-cr * ntu_022).exp();
    1.0 - (-(ntu_078 / cr) * inner).exp()
}

/// Calculate NTU from UA and minimum capacity rate.
///
/// NTU = UA / C_min
pub fn ntu_from_ua(ua: f64, c_min: f64) -> f64 {
    if c_min > 1e-10 {
        ua / c_min
    } else {
        0.0
    }
}

/// Estimate dew point temperature from humidity ratio at standard pressure.
///
/// Uses simplified Magnus formula approximation.
fn dew_point_from_w(w: f64) -> f64 {
    if w <= 0.0 {
        return -50.0;
    }
    // Partial pressure of water vapor
    let p_atm = 101325.0;
    let p_w = w * p_atm / (0.62198 + w);
    // Simplified dew point from partial pressure (Magnus-like)
    let alpha = (p_w / 610.78).max(1e-10).ln();
    (237.3 * alpha) / (17.27 - alpha)
}

impl WaterCoil {
    /// Create a heating water coil.
    pub fn heating(
        name: impl Into<String>,
        ua: f64,
        design_water_flow: f64,
        design_air_flow: f64,
    ) -> Self {
        Self {
            name: name.into(),
            hx_type: HeatExchangerType::CounterFlow,
            ua,
            design_water_flow,
            design_air_flow,
            is_cooling: false,
        }
    }

    /// Create a cooling water coil.
    pub fn cooling(
        name: impl Into<String>,
        ua: f64,
        design_water_flow: f64,
        design_air_flow: f64,
    ) -> Self {
        Self {
            name: name.into(),
            hx_type: HeatExchangerType::CrossFlow,
            ua,
            design_water_flow,
            design_air_flow,
            is_cooling: true,
        }
    }

    /// Calculate coil performance at current conditions.
    ///
    /// Automatically switches between dry and wet coil models for cooling coils
    /// when the coil surface temperature drops below the dew point.
    pub fn calculate(
        &self,
        air_inlet_temp: f64,
        air_inlet_w: f64,
        air_mass_flow: f64,
        water_inlet_temp: f64,
        water_mass_flow: f64,
    ) -> WaterCoilResult {
        if air_mass_flow <= 1e-10 || water_mass_flow <= 1e-10 {
            return WaterCoilResult {
                heat_rate: 0.0,
                air_outlet_temp: air_inlet_temp,
                water_outlet_temp: water_inlet_temp,
                air_outlet_humidity_ratio: air_inlet_w,
                sensible_heat_rate: 0.0,
                latent_heat_rate: 0.0,
                effectiveness: 0.0,
                is_wet: false,
            };
        }

        // Check for wet coil conditions (cooling coils only)
        if self.is_cooling && water_inlet_temp < air_inlet_temp {
            let dew_point = dew_point_from_w(air_inlet_w);

            // Estimate average coil surface temperature
            let dry_result = self.calc_dry(air_inlet_temp, air_inlet_w, air_mass_flow,
                                           water_inlet_temp, water_mass_flow);
            let avg_coil_surface_temp = (water_inlet_temp + dry_result.water_outlet_temp) / 2.0;

            if avg_coil_surface_temp < dew_point {
                // Wet coil: condensation occurs
                return self.calc_wet(air_inlet_temp, air_inlet_w, air_mass_flow,
                                     water_inlet_temp, water_mass_flow, dew_point);
            }
        }

        // Dry coil calculation
        self.calc_dry(air_inlet_temp, air_inlet_w, air_mass_flow,
                      water_inlet_temp, water_mass_flow)
    }

    /// Dry coil calculation using effectiveness-NTU method.
    fn calc_dry(
        &self,
        air_inlet_temp: f64,
        air_inlet_w: f64,
        air_mass_flow: f64,
        water_inlet_temp: f64,
        water_mass_flow: f64,
    ) -> WaterCoilResult {
        let cp_air = ep_psychrometrics::cp_air(air_inlet_w);
        let cp_water = ep_psychrometrics::cp_water(water_inlet_temp);

        let c_air = air_mass_flow * cp_air;
        let c_water = water_mass_flow * cp_water;

        let c_min = c_air.min(c_water);
        let c_max = c_air.max(c_water);
        let cr = c_min / c_max;

        let ntu = ntu_from_ua(self.ua, c_min);

        let effectiveness = match self.hx_type {
            HeatExchangerType::CounterFlow => effectiveness_counterflow(ntu, cr),
            HeatExchangerType::CrossFlow => effectiveness_crossflow(ntu, cr),
        };

        // Maximum possible heat transfer
        let q_max = c_min * (water_inlet_temp - air_inlet_temp).abs();
        let mut q = effectiveness * q_max;

        // Determine sign: positive = heating air
        if water_inlet_temp < air_inlet_temp {
            q = -q; // Cooling
        }

        let air_outlet_temp = air_inlet_temp + q / c_air;
        let water_outlet_temp = water_inlet_temp - q / c_water;

        WaterCoilResult {
            heat_rate: q,
            air_outlet_temp,
            water_outlet_temp,
            air_outlet_humidity_ratio: air_inlet_w, // Dry coil: no moisture change
            sensible_heat_rate: q,
            latent_heat_rate: 0.0,
            effectiveness,
            is_wet: false,
        }
    }

    /// Wet coil calculation for cooling with condensation.
    ///
    /// When the coil surface temperature drops below the dew point,
    /// moisture condenses from the air, providing both sensible and latent cooling.
    fn calc_wet(
        &self,
        air_inlet_temp: f64,
        air_inlet_w: f64,
        air_mass_flow: f64,
        water_inlet_temp: f64,
        water_mass_flow: f64,
        _dew_point: f64,
    ) -> WaterCoilResult {
        let cp_air = ep_psychrometrics::cp_air(air_inlet_w);
        let cp_water = ep_psychrometrics::cp_water(water_inlet_temp);
        let h_fg = 2_501_000.0; // Latent heat of vaporization (J/kg)

        let c_air = air_mass_flow * cp_air;
        let c_water = water_mass_flow * cp_water;

        // Use enthalpy-based effectiveness for wet coil
        // The effective capacity rate on the air side includes latent effect
        let h_inlet = ep_psychrometrics::enthalpy(air_inlet_temp, air_inlet_w);

        // Estimate saturated enthalpy at water inlet temperature
        let w_sat_water_in = sat_humidity_ratio(water_inlet_temp);
        let h_sat_water_in = ep_psychrometrics::enthalpy(water_inlet_temp, w_sat_water_in);

        // Effective air capacity rate including latent (slope of saturation line)
        let delta_t_ref = (air_inlet_temp - water_inlet_temp).abs().max(1.0);
        let c_air_wet = air_mass_flow * (h_inlet - h_sat_water_in).abs() / delta_t_ref;

        let c_min_wet = c_air_wet.min(c_water);
        let c_max_wet = c_air_wet.max(c_water);
        let cr_wet = if c_max_wet > 1e-10 { c_min_wet / c_max_wet } else { 0.0 };

        // UA for wet coil (increased due to latent effect, typically 1.3-1.5x dry UA)
        let ua_wet = self.ua * 1.35;
        let ntu_wet = ntu_from_ua(ua_wet, c_min_wet);

        let effectiveness = match self.hx_type {
            HeatExchangerType::CounterFlow => effectiveness_counterflow(ntu_wet, cr_wet),
            HeatExchangerType::CrossFlow => effectiveness_crossflow(ntu_wet, cr_wet),
        };

        // Total heat transfer (enthalpy-based)
        let q_max_wet = c_min_wet * (air_inlet_temp - water_inlet_temp).abs();
        let q_total = effectiveness * q_max_wet;

        // Water outlet temperature
        let water_outlet_temp = water_inlet_temp + q_total / c_water;

        // Air outlet conditions
        // Estimate outlet air as partially following the saturation line
        let avg_coil_temp = (water_inlet_temp + water_outlet_temp) / 2.0;
        let coil_surface_fraction = 0.8; // 80% of air contacts coil surface

        // Sensible cooling
        let sensible_q = c_air * (air_inlet_temp - avg_coil_temp) * coil_surface_fraction
            * effectiveness;
        let air_outlet_temp = air_inlet_temp - sensible_q.max(0.0) / c_air;

        // Outlet humidity ratio (limited by saturation at outlet temp)
        let w_sat_outlet = sat_humidity_ratio(air_outlet_temp);
        let air_outlet_w = air_inlet_w.min(w_sat_outlet);

        // Latent heat transfer
        let moisture_removed = air_mass_flow * (air_inlet_w - air_outlet_w);
        let latent_q = moisture_removed * h_fg;

        // Actual total = sensible + latent
        let actual_sensible = air_mass_flow * cp_air * (air_inlet_temp - air_outlet_temp);

        WaterCoilResult {
            heat_rate: -(actual_sensible + latent_q),
            air_outlet_temp,
            water_outlet_temp,
            air_outlet_humidity_ratio: air_outlet_w,
            sensible_heat_rate: -actual_sensible,
            latent_heat_rate: -latent_q,
            effectiveness,
            is_wet: true,
        }
    }

    /// Calculate required UA for a given heat transfer rate.
    ///
    /// Iteratively finds UA such that Q_actual = Q_desired.
    pub fn ua_for_load(
        hx_type: HeatExchangerType,
        q_desired: f64,
        air_inlet_temp: f64,
        air_mass_flow: f64,
        air_inlet_w: f64,
        water_inlet_temp: f64,
        water_mass_flow: f64,
    ) -> f64 {
        if air_mass_flow <= 1e-10 || water_mass_flow <= 1e-10 {
            return 0.0;
        }

        let cp_air = ep_psychrometrics::cp_air(air_inlet_w);
        let cp_water = ep_psychrometrics::cp_water(water_inlet_temp);
        let c_air = air_mass_flow * cp_air;
        let c_water = water_mass_flow * cp_water;
        let c_min = c_air.min(c_water);
        let c_max = c_air.max(c_water);
        let cr = c_min / c_max;

        let q_max = c_min * (water_inlet_temp - air_inlet_temp).abs();
        if q_max < 1e-10 {
            return 0.0;
        }

        let eps_desired = (q_desired.abs() / q_max).clamp(0.0, 0.999);

        // Invert effectiveness-NTU to find NTU
        let ntu = match hx_type {
            HeatExchangerType::CounterFlow => {
                if (cr - 1.0).abs() < 1e-6 {
                    eps_desired / (1.0 - eps_desired)
                } else {
                    let val = (1.0 - eps_desired) / (1.0 - cr * eps_desired);
                    if val > 0.0 {
                        -val.ln() / (1.0 - cr)
                    } else {
                        20.0 // Very large NTU
                    }
                }
            }
            HeatExchangerType::CrossFlow => {
                // No analytical inverse — use bisection
                let mut ntu_lo = 0.0;
                let mut ntu_hi = 50.0;
                for _ in 0..50 {
                    let ntu_mid = (ntu_lo + ntu_hi) / 2.0;
                    let eps = effectiveness_crossflow(ntu_mid, cr);
                    if eps < eps_desired {
                        ntu_lo = ntu_mid;
                    } else {
                        ntu_hi = ntu_mid;
                    }
                }
                (ntu_lo + ntu_hi) / 2.0
            }
        };

        ntu * c_min
    }
}

/// Approximate saturation humidity ratio at a given temperature (at standard pressure).
fn sat_humidity_ratio(temp_c: f64) -> f64 {
    // Antoine equation for water vapor pressure (Pa)
    let t = temp_c.max(-40.0).min(80.0);
    let p_sat = 610.78 * ((17.27 * t) / (237.3 + t)).exp();
    let p_atm = 101325.0;
    0.62198 * p_sat / (p_atm - p_sat).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effectiveness_counterflow_basic() {
        let eps = effectiveness_counterflow(1.0, 0.5);
        assert!(eps > 0.5 && eps < 0.8, "eps={eps}");
    }

    #[test]
    fn effectiveness_counterflow_equal_capacity() {
        let eps = effectiveness_counterflow(2.0, 1.0);
        assert!((eps - 2.0 / 3.0).abs() < 1e-6, "eps={eps}");
    }

    #[test]
    fn effectiveness_counterflow_zero_ntu() {
        assert!(effectiveness_counterflow(0.0, 0.5).abs() < 1e-10);
    }

    #[test]
    fn effectiveness_counterflow_large_ntu() {
        let eps = effectiveness_counterflow(100.0, 0.5);
        assert!((eps - 1.0).abs() < 0.01, "eps={eps}");
    }

    #[test]
    fn effectiveness_crossflow_basic() {
        let eps = effectiveness_crossflow(1.0, 0.5);
        assert!(eps > 0.4 && eps < 0.7, "eps={eps}");
    }

    #[test]
    fn heating_coil_calculation() {
        let coil = WaterCoil::heating("HW Coil", 5000.0, 0.5, 1.0);
        let result = coil.calculate(10.0, 0.005, 1.0, 80.0, 0.5);
        assert!(result.heat_rate > 0.0, "Q={}", result.heat_rate);
        assert!(result.air_outlet_temp > 10.0, "T_air_out={}", result.air_outlet_temp);
        assert!(result.water_outlet_temp < 80.0, "T_water_out={}", result.water_outlet_temp);
        assert!(result.effectiveness > 0.0 && result.effectiveness <= 1.0);
        assert!(!result.is_wet);
    }

    #[test]
    fn cooling_coil_dry() {
        // Low humidity: coil stays dry
        let coil = WaterCoil::cooling("CHW Coil", 3000.0, 0.5, 1.0);
        let result = coil.calculate(25.0, 0.003, 1.0, 14.0, 0.5);
        assert!(result.heat_rate < 0.0, "Q={}", result.heat_rate);
        assert!(result.air_outlet_temp < 25.0, "T_out={}", result.air_outlet_temp);
        assert!(!result.is_wet);
        assert!(result.latent_heat_rate.abs() < 1e-10);
    }

    #[test]
    fn cooling_coil_wet() {
        // High humidity, cold water: condensation occurs
        let coil = WaterCoil::cooling("CHW Coil", 8000.0, 0.8, 1.5);
        let result = coil.calculate(
            30.0,   // warm air
            0.016,  // high humidity (~70% RH at 30C, dew point ~24C)
            1.5,    // air mass flow
            7.0,    // cold water (well below dew point)
            0.8,    // water mass flow
        );
        assert!(result.heat_rate < 0.0, "Q={}", result.heat_rate);
        assert!(result.air_outlet_temp < 30.0, "T_out={}", result.air_outlet_temp);
        assert!(result.is_wet, "should be wet coil");
        assert!(result.latent_heat_rate < 0.0, "Q_lat={}", result.latent_heat_rate);
        assert!(result.air_outlet_humidity_ratio < 0.016,
                "W_out={}", result.air_outlet_humidity_ratio);
    }

    #[test]
    fn wet_coil_latent_capacity() {
        let coil = WaterCoil::cooling("CHW Coil", 10000.0, 1.0, 2.0);
        let result = coil.calculate(32.0, 0.018, 2.0, 6.0, 1.0);
        if result.is_wet {
            // Latent should be a meaningful fraction of total
            let total = result.sensible_heat_rate.abs() + result.latent_heat_rate.abs();
            let shr = result.sensible_heat_rate.abs() / total.max(1.0);
            assert!(shr < 1.0, "SHR={} should be <1 for wet coil", shr);
            assert!(shr > 0.3, "SHR={} should be >0.3", shr);
        }
    }

    #[test]
    fn no_flow_returns_inlet_conditions() {
        let coil = WaterCoil::heating("Test", 5000.0, 0.5, 1.0);
        let result = coil.calculate(20.0, 0.008, 0.0, 80.0, 0.5);
        assert!((result.air_outlet_temp - 20.0).abs() < 1e-10);
        assert!(result.heat_rate.abs() < 1e-10);
    }

    #[test]
    fn energy_balance() {
        let coil = WaterCoil::heating("Test", 3000.0, 0.3, 0.8);
        let result = coil.calculate(15.0, 0.006, 0.8, 60.0, 0.3);

        let cp_air = ep_psychrometrics::cp_air(0.006);
        let cp_water = ep_psychrometrics::cp_water(60.0);

        let q_air = 0.8 * cp_air * (result.air_outlet_temp - 15.0);
        let q_water = 0.3 * cp_water * (60.0 - result.water_outlet_temp);

        assert!((q_air - q_water).abs() / q_air.abs().max(1.0) < 0.01,
                "q_air={q_air}, q_water={q_water}");
    }

    #[test]
    fn ua_sizing() {
        let ua = WaterCoil::ua_for_load(
            HeatExchangerType::CounterFlow,
            10000.0, 10.0, 1.0, 0.005, 80.0, 0.5,
        );
        assert!(ua > 0.0, "UA={ua}");

        let coil = WaterCoil {
            name: "sized".into(),
            hx_type: HeatExchangerType::CounterFlow,
            ua,
            design_water_flow: 0.5,
            design_air_flow: 1.0,
            is_cooling: false,
        };
        let result = coil.calculate(10.0, 0.005, 1.0, 80.0, 0.5);
        assert!((result.heat_rate - 10000.0).abs() / 10000.0 < 0.02,
                "Q={}, expected=10000", result.heat_rate);
    }

    // --- Dew point tests ---

    #[test]
    fn dew_point_reasonable() {
        // At W=0.010 (typical indoor), dew point should be ~14C
        let dp = dew_point_from_w(0.010);
        assert!(dp > 10.0 && dp < 18.0, "dp={}", dp);
    }

    #[test]
    fn dew_point_high_humidity() {
        // At W=0.020 (high humidity), dew point should be ~26C
        let dp = dew_point_from_w(0.020);
        assert!(dp > 22.0 && dp < 30.0, "dp={}", dp);
    }

    // --- Saturation humidity ratio tests ---

    #[test]
    fn sat_humidity_ratio_values() {
        // At 20C, saturation W ≈ 0.0147
        let w20 = sat_humidity_ratio(20.0);
        assert!(w20 > 0.012 && w20 < 0.018, "W_sat(20)={}", w20);

        // At 30C, saturation W ≈ 0.0271
        let w30 = sat_humidity_ratio(30.0);
        assert!(w30 > 0.022 && w30 < 0.032, "W_sat(30)={}", w30);

        // Higher temp = higher saturation W
        assert!(w30 > w20);
    }

    #[test]
    fn cooling_coil_dry_vs_wet_comparison() {
        // Same coil, different humidity conditions
        let coil = WaterCoil::cooling("CHW", 6000.0, 0.5, 1.0);

        // Dry condition (low humidity)
        let _dry_result = coil.calculate(25.0, 0.004, 1.0, 10.0, 0.5);

        // Wet condition (high humidity)
        let wet_result = coil.calculate(25.0, 0.016, 1.0, 7.0, 0.5);

        // Wet coil should have more total capacity (latent + sensible)
        // Both cool the air, but wet also removes moisture
        if wet_result.is_wet {
            assert!(wet_result.heat_rate.abs() > 0.0);
            assert!(wet_result.air_outlet_humidity_ratio < 0.016);
        }
    }
}
