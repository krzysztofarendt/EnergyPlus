//! Water coil models using effectiveness-NTU method.
//!
//! Supports counterflow and crossflow configurations for both
//! heating and cooling applications.

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
    /// Effectiveness (0-1).
    pub effectiveness: f64,
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

    /// Calculate coil performance at current conditions (dry coil).
    ///
    /// Uses effectiveness-NTU method for sensible heat transfer only.
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
                effectiveness: 0.0,
            };
        }

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
            effectiveness,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effectiveness_counterflow_basic() {
        // NTU=1, Cr=0.5: well-defined result
        let eps = effectiveness_counterflow(1.0, 0.5);
        assert!(eps > 0.5 && eps < 0.8, "eps={eps}");
    }

    #[test]
    fn effectiveness_counterflow_equal_capacity() {
        // Cr=1: NTU/(1+NTU)
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
        let result = coil.calculate(
            10.0,   // air in: 10C
            0.005,  // W
            1.0,    // air mass flow
            80.0,   // hot water in: 80C
            0.5,    // water mass flow
        );
        // Should heat the air
        assert!(result.heat_rate > 0.0, "Q={}", result.heat_rate);
        assert!(result.air_outlet_temp > 10.0, "T_air_out={}", result.air_outlet_temp);
        assert!(result.water_outlet_temp < 80.0, "T_water_out={}", result.water_outlet_temp);
        assert!(result.effectiveness > 0.0 && result.effectiveness <= 1.0);
    }

    #[test]
    fn cooling_coil_calculation() {
        let coil = WaterCoil::cooling("CHW Coil", 8000.0, 0.8, 1.5);
        let result = coil.calculate(
            30.0,   // air in: 30C
            0.012,  // W
            1.5,    // air mass flow
            7.0,    // chilled water in: 7C
            0.8,    // water mass flow
        );
        // Should cool the air
        assert!(result.heat_rate < 0.0, "Q={}", result.heat_rate);
        assert!(result.air_outlet_temp < 30.0, "T_air_out={}", result.air_outlet_temp);
        assert!(result.water_outlet_temp > 7.0, "T_water_out={}", result.water_outlet_temp);
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

        // Energy balance: Q_air ≈ Q_water
        assert!((q_air - q_water).abs() / q_air.abs().max(1.0) < 0.01,
                "q_air={q_air}, q_water={q_water}");
    }

    #[test]
    fn ua_sizing() {
        let ua = WaterCoil::ua_for_load(
            HeatExchangerType::CounterFlow,
            10000.0, // 10 kW heating
            10.0,    // air in
            1.0,     // air flow
            0.005,   // W
            80.0,    // water in
            0.5,     // water flow
        );
        assert!(ua > 0.0, "UA={ua}");

        // Verify: create coil with this UA and check it delivers ~10 kW
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
}
