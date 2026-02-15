//! Air-to-air heat exchanger (heat recovery) models.
//!
//! Implements sensible-only and sensible+latent heat/energy recovery
//! using effectiveness-based approach. Supports plate, rotary (wheel),
//! and generic types with optional frost control and economizer bypass.

/// Heat exchanger arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HXType {
    /// Plate (fixed-plate, no latent by default).
    #[default]
    Plate,
    /// Rotary wheel (enthalpy wheel, typically has latent recovery).
    Rotary,
}

/// Air-to-air heat exchanger specification.
#[derive(Debug, Clone)]
pub struct AirToAirHX {
    /// Name.
    pub name: String,
    /// Heat exchanger type.
    pub hx_type: HXType,
    /// Nominal supply air flow rate (m3/s).
    pub nominal_supply_flow: f64,
    /// Sensible effectiveness at 100% airflow (heating condition).
    pub sensible_effectiveness_100_heating: f64,
    /// Sensible effectiveness at 75% airflow (heating condition).
    pub sensible_effectiveness_75_heating: f64,
    /// Latent effectiveness at 100% airflow (heating condition).
    pub latent_effectiveness_100_heating: f64,
    /// Latent effectiveness at 75% airflow (heating condition).
    pub latent_effectiveness_75_heating: f64,
    /// Sensible effectiveness at 100% airflow (cooling condition).
    pub sensible_effectiveness_100_cooling: f64,
    /// Sensible effectiveness at 75% airflow (cooling condition).
    pub sensible_effectiveness_75_cooling: f64,
    /// Latent effectiveness at 100% airflow (cooling condition).
    pub latent_effectiveness_100_cooling: f64,
    /// Latent effectiveness at 75% airflow (cooling condition).
    pub latent_effectiveness_75_cooling: f64,
    /// Nominal electric power (W), fans/controls.
    pub nominal_electric_power: f64,
    /// Supply air inlet temperature threshold below which frost control activates (C).
    pub frost_control_threshold: f64,
    /// Whether economizer bypass is enabled.
    pub economizer_lockout: bool,
    /// Frost control type.
    pub frost_control_type: FrostControlType,
}

/// Heat exchanger calculation result.
#[derive(Debug, Clone, Copy)]
pub struct AirToAirHXResult {
    /// Sensible heat recovery rate (W). Positive = heating supply air.
    pub sensible_heat_rate: f64,
    /// Latent heat recovery rate (W). Positive = humidifying supply air.
    pub latent_heat_rate: f64,
    /// Total heat recovery rate (W).
    pub total_heat_rate: f64,
    /// Supply air outlet temperature (C).
    pub supply_outlet_temp: f64,
    /// Supply air outlet humidity ratio (kg/kg).
    pub supply_outlet_w: f64,
    /// Exhaust air outlet temperature (C).
    pub exhaust_outlet_temp: f64,
    /// Exhaust air outlet humidity ratio (kg/kg).
    pub exhaust_outlet_w: f64,
    /// Sensible effectiveness at operating conditions.
    pub sensible_effectiveness: f64,
    /// Latent effectiveness at operating conditions.
    pub latent_effectiveness: f64,
    /// Electric power consumption (W).
    pub electric_power: f64,
}

impl AirToAirHX {
    /// Create a sensible-only plate heat exchanger.
    pub fn plate(
        name: impl Into<String>,
        nominal_supply_flow: f64,
        sensible_effectiveness: f64,
    ) -> Self {
        Self {
            name: name.into(),
            hx_type: HXType::Plate,
            nominal_supply_flow,
            sensible_effectiveness_100_heating: sensible_effectiveness,
            sensible_effectiveness_75_heating: sensible_effectiveness,
            sensible_effectiveness_100_cooling: sensible_effectiveness,
            sensible_effectiveness_75_cooling: sensible_effectiveness,
            latent_effectiveness_100_heating: 0.0,
            latent_effectiveness_75_heating: 0.0,
            latent_effectiveness_100_cooling: 0.0,
            latent_effectiveness_75_cooling: 0.0,
            nominal_electric_power: 0.0,
            frost_control_threshold: -23.3,
            economizer_lockout: false,
            frost_control_type: FrostControlType::None,
        }
    }

    /// Create an enthalpy wheel (rotary) heat exchanger.
    pub fn rotary(
        name: impl Into<String>,
        nominal_supply_flow: f64,
        sensible_effectiveness: f64,
        latent_effectiveness: f64,
    ) -> Self {
        Self {
            name: name.into(),
            hx_type: HXType::Rotary,
            nominal_supply_flow,
            sensible_effectiveness_100_heating: sensible_effectiveness,
            sensible_effectiveness_75_heating: sensible_effectiveness,
            sensible_effectiveness_100_cooling: sensible_effectiveness,
            sensible_effectiveness_75_cooling: sensible_effectiveness,
            latent_effectiveness_100_heating: latent_effectiveness,
            latent_effectiveness_75_heating: latent_effectiveness,
            latent_effectiveness_100_cooling: latent_effectiveness,
            latent_effectiveness_75_cooling: latent_effectiveness,
            nominal_electric_power: 0.0,
            frost_control_threshold: -23.3,
            economizer_lockout: false,
            frost_control_type: FrostControlType::None,
        }
    }

    /// Interpolate effectiveness between 75% and 100% airflow operating points.
    fn interpolated_effectiveness(&self, flow_fraction: f64, is_heating: bool) -> (f64, f64) {
        let ff = flow_fraction.clamp(0.0, 1.0);

        let (sens_100, sens_75, lat_100, lat_75) = if is_heating {
            (
                self.sensible_effectiveness_100_heating,
                self.sensible_effectiveness_75_heating,
                self.latent_effectiveness_100_heating,
                self.latent_effectiveness_75_heating,
            )
        } else {
            (
                self.sensible_effectiveness_100_cooling,
                self.sensible_effectiveness_75_cooling,
                self.latent_effectiveness_100_cooling,
                self.latent_effectiveness_75_cooling,
            )
        };

        // Linear interpolation/extrapolation between 75% and 100%
        let t = (ff - 0.75) / 0.25; // t=0 at 75%, t=1 at 100%
        let sens_eff = (sens_75 + t * (sens_100 - sens_75)).clamp(0.0, 1.0);
        let lat_eff = (lat_75 + t * (lat_100 - lat_75)).clamp(0.0, 1.0);

        (sens_eff, lat_eff)
    }

    /// Calculate heat exchanger performance.
    ///
    /// Supply = outdoor air entering the building.
    /// Exhaust = indoor air leaving the building.
    pub fn calculate(
        &self,
        supply_inlet_temp: f64,
        supply_inlet_w: f64,
        supply_mass_flow: f64,
        exhaust_inlet_temp: f64,
        exhaust_inlet_w: f64,
        exhaust_mass_flow: f64,
    ) -> AirToAirHXResult {
        if supply_mass_flow <= 1e-10 || exhaust_mass_flow <= 1e-10 {
            return AirToAirHXResult {
                sensible_heat_rate: 0.0,
                latent_heat_rate: 0.0,
                total_heat_rate: 0.0,
                supply_outlet_temp: supply_inlet_temp,
                supply_outlet_w: supply_inlet_w,
                exhaust_outlet_temp: exhaust_inlet_temp,
                exhaust_outlet_w: exhaust_inlet_w,
                sensible_effectiveness: 0.0,
                latent_effectiveness: 0.0,
                electric_power: 0.0,
            };
        }

        // Determine if heating or cooling mode
        let is_heating = supply_inlet_temp < exhaust_inlet_temp;

        // Flow fraction relative to nominal
        let nominal_mass_flow = self.nominal_supply_flow * 1.2; // approx density
        let flow_fraction = supply_mass_flow / nominal_mass_flow.max(1e-10);

        // Get effectiveness at operating flow
        let (sens_eff, lat_eff) = self.interpolated_effectiveness(flow_fraction, is_heating);

        // Use minimum mass flow for capacity calculation
        let m_min = supply_mass_flow.min(exhaust_mass_flow);

        // Sensible heat recovery
        let cp_supply = ep_psychrometrics::cp_air(supply_inlet_w);
        let q_sens_max = m_min * cp_supply * (exhaust_inlet_temp - supply_inlet_temp);
        let q_sens = sens_eff * q_sens_max;

        // Supply outlet temperature
        let supply_outlet_temp = supply_inlet_temp + q_sens / (supply_mass_flow * cp_supply);

        // Exhaust outlet temperature
        let cp_exhaust = ep_psychrometrics::cp_air(exhaust_inlet_w);
        let exhaust_outlet_temp = exhaust_inlet_temp - q_sens / (exhaust_mass_flow * cp_exhaust);

        // Latent heat recovery
        let h_fg = 2_501_000.0; // latent heat of vaporization (J/kg)
        let dw_max = exhaust_inlet_w - supply_inlet_w;
        let dw = lat_eff * dw_max;
        let q_lat = m_min * h_fg * dw;

        let supply_outlet_w = (supply_inlet_w + dw * m_min / supply_mass_flow).max(0.0);
        let exhaust_outlet_w = (exhaust_inlet_w - dw * m_min / exhaust_mass_flow).max(0.0);

        let total = q_sens + q_lat;
        let power = if supply_mass_flow > 1e-10 {
            self.nominal_electric_power * flow_fraction.min(1.0)
        } else {
            0.0
        };

        AirToAirHXResult {
            sensible_heat_rate: q_sens,
            latent_heat_rate: q_lat,
            total_heat_rate: total,
            supply_outlet_temp,
            supply_outlet_w,
            exhaust_outlet_temp,
            exhaust_outlet_w,
            sensible_effectiveness: sens_eff,
            latent_effectiveness: lat_eff,
            electric_power: power,
        }
    }
}

// ---------------------------------------------------------------------------
// Frost Control
// ---------------------------------------------------------------------------

/// Frost control strategy for air-to-air heat exchangers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrostControlType {
    #[default]
    /// No frost control.
    None,
    /// Limit minimum exhaust air temperature to prevent frost.
    MinimumExhaustTemp,
    /// Recirculate exhaust air to preheat supply.
    ExhaustAirRecirculation,
    /// Use exhaust air only (bypass supply).
    ExhaustOnly,
}

impl AirToAirHX {
    /// Set frost control parameters.
    pub fn with_frost_control(mut self, control: FrostControlType, threshold: f64) -> Self {
        self.frost_control_threshold = threshold;
        self.frost_control_type = control;
        self
    }

    /// Set economizer lockout.
    pub fn with_economizer_lockout(mut self, lockout: bool) -> Self {
        self.economizer_lockout = lockout;
        self
    }

    /// Calculate with frost control applied.
    pub fn calculate_with_frost_control(
        &self,
        supply_inlet_temp: f64,
        supply_inlet_w: f64,
        supply_mass_flow: f64,
        exhaust_inlet_temp: f64,
        exhaust_inlet_w: f64,
        exhaust_mass_flow: f64,
        economizer_active: bool,
    ) -> AirToAirHXResult {
        // Economizer bypass: if lockout enabled and economizer is active, bypass HX
        if self.economizer_lockout && economizer_active {
            return AirToAirHXResult {
                sensible_heat_rate: 0.0,
                latent_heat_rate: 0.0,
                total_heat_rate: 0.0,
                supply_outlet_temp: supply_inlet_temp,
                supply_outlet_w: supply_inlet_w,
                exhaust_outlet_temp: exhaust_inlet_temp,
                exhaust_outlet_w: exhaust_inlet_w,
                sensible_effectiveness: 0.0,
                latent_effectiveness: 0.0,
                electric_power: 0.0,
            };
        }

        // First, compute normal result
        let mut result = self.calculate(
            supply_inlet_temp, supply_inlet_w, supply_mass_flow,
            exhaust_inlet_temp, exhaust_inlet_w, exhaust_mass_flow,
        );

        // Apply frost control
        match self.frost_control_type {
            FrostControlType::MinimumExhaustTemp => {
                // If exhaust outlet drops below threshold, reduce effectiveness
                if result.exhaust_outlet_temp < self.frost_control_threshold {
                    // Limit heat transfer to keep exhaust above threshold
                    let cp = ep_psychrometrics::cp_air(exhaust_inlet_w);
                    let q_max_frost = exhaust_mass_flow * cp
                        * (exhaust_inlet_temp - self.frost_control_threshold);
                    if q_max_frost > 0.0 && result.sensible_heat_rate > q_max_frost {
                        let ratio = q_max_frost / result.sensible_heat_rate;
                        result.sensible_heat_rate = q_max_frost;
                        result.latent_heat_rate *= ratio;
                        result.total_heat_rate = result.sensible_heat_rate + result.latent_heat_rate;
                        let cp_sup = ep_psychrometrics::cp_air(supply_inlet_w);
                        result.supply_outlet_temp = supply_inlet_temp
                            + result.sensible_heat_rate / (supply_mass_flow * cp_sup).max(1e-10);
                        result.exhaust_outlet_temp = self.frost_control_threshold;
                        result.sensible_effectiveness *= ratio;
                    }
                }
            }
            FrostControlType::ExhaustAirRecirculation => {
                // If supply inlet is very cold, reduce effective flow
                if supply_inlet_temp < self.frost_control_threshold {
                    let ratio = 0.5; // Reduce to 50% effectiveness to prevent frost
                    result.sensible_heat_rate *= ratio;
                    result.latent_heat_rate *= ratio;
                    result.total_heat_rate = result.sensible_heat_rate + result.latent_heat_rate;
                    let cp_sup = ep_psychrometrics::cp_air(supply_inlet_w);
                    result.supply_outlet_temp = supply_inlet_temp
                        + result.sensible_heat_rate / (supply_mass_flow * cp_sup).max(1e-10);
                    let cp_exh = ep_psychrometrics::cp_air(exhaust_inlet_w);
                    result.exhaust_outlet_temp = exhaust_inlet_temp
                        - result.sensible_heat_rate / (exhaust_mass_flow * cp_exh).max(1e-10);
                    result.sensible_effectiveness *= ratio;
                }
            }
            FrostControlType::ExhaustOnly | FrostControlType::None => {}
        }

        result
    }
}

// ---------------------------------------------------------------------------
// Flat Plate Heat Exchanger (NTU-effectiveness counterflow)
// ---------------------------------------------------------------------------

/// Flat plate heat exchanger using NTU-effectiveness counterflow model.
#[derive(Debug, Clone)]
pub struct FlatPlateHX {
    pub name: String,
    /// Overall UA-value (W/K).
    pub ua: f64,
}

/// Flat plate HX result.
#[derive(Debug, Clone, Copy)]
pub struct FlatPlateHXResult {
    /// Heat transfer rate (W). Positive = supply gains heat.
    pub heat_rate: f64,
    /// Supply outlet temperature (C).
    pub supply_outlet_temp: f64,
    /// Exhaust outlet temperature (C).
    pub exhaust_outlet_temp: f64,
    /// Effectiveness (0-1).
    pub effectiveness: f64,
}

impl FlatPlateHX {
    pub fn new(name: impl Into<String>, ua: f64) -> Self {
        Self { name: name.into(), ua }
    }

    /// Calculate counterflow heat exchanger using NTU-effectiveness method.
    pub fn calculate(
        &self,
        supply_inlet_temp: f64,
        supply_mass_flow: f64,
        supply_w: f64,
        exhaust_inlet_temp: f64,
        exhaust_mass_flow: f64,
        exhaust_w: f64,
    ) -> FlatPlateHXResult {
        if supply_mass_flow <= 1e-10 || exhaust_mass_flow <= 1e-10 || self.ua <= 0.0 {
            return FlatPlateHXResult {
                heat_rate: 0.0,
                supply_outlet_temp: supply_inlet_temp,
                exhaust_outlet_temp: exhaust_inlet_temp,
                effectiveness: 0.0,
            };
        }

        let cp_s = ep_psychrometrics::cp_air(supply_w);
        let cp_e = ep_psychrometrics::cp_air(exhaust_w);
        let c_supply = supply_mass_flow * cp_s;
        let c_exhaust = exhaust_mass_flow * cp_e;

        let c_min = c_supply.min(c_exhaust);
        let c_max = c_supply.max(c_exhaust);
        let c_ratio = c_min / c_max;

        let ntu = self.ua / c_min;

        // Counterflow effectiveness
        let effectiveness = if (c_ratio - 1.0).abs() < 1e-10 {
            // Special case: C_min = C_max
            ntu / (1.0 + ntu)
        } else {
            let exp_term = (-(1.0 - c_ratio) * ntu).exp();
            (1.0 - exp_term) / (1.0 - c_ratio * exp_term)
        };
        let effectiveness = effectiveness.clamp(0.0, 1.0);

        let q_max = c_min * (exhaust_inlet_temp - supply_inlet_temp);
        let q = effectiveness * q_max;

        let supply_outlet = supply_inlet_temp + q / c_supply;
        let exhaust_outlet = exhaust_inlet_temp - q / c_exhaust;

        FlatPlateHXResult {
            heat_rate: q,
            supply_outlet_temp: supply_outlet,
            exhaust_outlet_temp: exhaust_outlet,
            effectiveness,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plate_hx_basic() {
        let hx = AirToAirHX::plate("ERV", 1.0, 0.7);
        assert_eq!(hx.hx_type, HXType::Plate);
        assert!((hx.sensible_effectiveness_100_heating - 0.7).abs() < 1e-10);
        assert!(hx.latent_effectiveness_100_heating.abs() < 1e-10);
    }

    #[test]
    fn rotary_hx_basic() {
        let hx = AirToAirHX::rotary("Wheel", 1.0, 0.75, 0.65);
        assert_eq!(hx.hx_type, HXType::Rotary);
        assert!((hx.latent_effectiveness_100_heating - 0.65).abs() < 1e-10);
    }

    #[test]
    fn plate_hx_heating_mode() {
        let hx = AirToAirHX::plate("Test", 1.0, 0.7);
        // Winter: cold outdoor supply, warm indoor exhaust
        let result = hx.calculate(
            -10.0, 0.001, 1.2,  // supply: -10C, very dry
            22.0, 0.008, 1.2,   // exhaust: 22C, indoor
        );
        // Should heat supply air
        assert!(result.sensible_heat_rate > 0.0, "Q_sens={}", result.sensible_heat_rate);
        assert!(result.supply_outlet_temp > -10.0, "T_sup_out={}", result.supply_outlet_temp);
        assert!(result.exhaust_outlet_temp < 22.0, "T_exh_out={}", result.exhaust_outlet_temp);
        // No latent recovery for plate
        assert!(result.latent_heat_rate.abs() < 1.0);
    }

    #[test]
    fn plate_hx_cooling_mode() {
        let hx = AirToAirHX::plate("Test", 1.0, 0.7);
        // Summer: hot outdoor supply, cool indoor exhaust
        let result = hx.calculate(
            35.0, 0.015, 1.2,   // supply: 35C, humid
            24.0, 0.009, 1.2,   // exhaust: 24C, indoor
        );
        // Should cool supply air
        assert!(result.sensible_heat_rate < 0.0, "Q_sens={}", result.sensible_heat_rate);
        assert!(result.supply_outlet_temp < 35.0, "T_sup_out={}", result.supply_outlet_temp);
    }

    #[test]
    fn rotary_hx_latent_recovery() {
        let hx = AirToAirHX::rotary("Wheel", 1.0, 0.75, 0.65);
        // Winter: dry cold outdoor, humid indoor exhaust
        let result = hx.calculate(
            -10.0, 0.001, 1.2,
            22.0, 0.008, 1.2,
        );
        // Should transfer both heat and moisture to supply
        assert!(result.sensible_heat_rate > 0.0);
        assert!(result.latent_heat_rate > 0.0, "Q_lat={}", result.latent_heat_rate);
        assert!(result.supply_outlet_w > 0.001, "W_sup_out={}", result.supply_outlet_w);
        assert!(result.total_heat_rate > result.sensible_heat_rate);
    }

    #[test]
    fn no_flow_returns_inlet() {
        let hx = AirToAirHX::plate("Test", 1.0, 0.7);
        let result = hx.calculate(-10.0, 0.002, 0.0, 22.0, 0.008, 1.2);
        assert!((result.supply_outlet_temp - (-10.0)).abs() < 1e-10);
        assert!(result.sensible_heat_rate.abs() < 1e-10);
    }

    #[test]
    fn equal_temps_no_transfer() {
        let hx = AirToAirHX::plate("Test", 1.0, 0.7);
        let result = hx.calculate(22.0, 0.008, 1.2, 22.0, 0.008, 1.2);
        assert!(result.sensible_heat_rate.abs() < 1.0, "Q_sens={}", result.sensible_heat_rate);
        assert!((result.supply_outlet_temp - 22.0).abs() < 0.1);
    }

    #[test]
    fn energy_balance_sensible() {
        let hx = AirToAirHX::plate("Test", 1.0, 0.7);
        let result = hx.calculate(-5.0, 0.002, 1.0, 22.0, 0.008, 1.0);

        let cp = ep_psychrometrics::cp_air(0.002);
        let q_supply = 1.0 * cp * (result.supply_outlet_temp - (-5.0));
        // Supply-side heat gain should match reported sensible rate
        assert!((q_supply - result.sensible_heat_rate).abs() / result.sensible_heat_rate.abs().max(1.0) < 0.01,
                "q_supply={}, q_reported={}", q_supply, result.sensible_heat_rate);
    }

    #[test]
    fn effectiveness_bounds() {
        let hx = AirToAirHX::plate("Test", 1.0, 0.7);
        let result = hx.calculate(-10.0, 0.001, 1.2, 22.0, 0.008, 1.2);
        assert!(result.sensible_effectiveness >= 0.0 && result.sensible_effectiveness <= 1.0,
                "eff={}", result.sensible_effectiveness);
    }

    // ====================================================================
    // Frost Control Tests
    // ====================================================================

    #[test]
    fn frost_control_min_exhaust_temp() {
        let hx = AirToAirHX::plate("FC", 1.0, 0.8)
            .with_frost_control(FrostControlType::MinimumExhaustTemp, 1.0);
        // Very cold supply, should limit heat transfer to keep exhaust above 1C
        let result = hx.calculate_with_frost_control(
            -30.0, 0.001, 1.2, 22.0, 0.008, 1.2, false,
        );
        assert!(result.exhaust_outlet_temp >= 0.5,
                "Exhaust should stay above threshold: {}", result.exhaust_outlet_temp);
    }

    #[test]
    fn frost_control_none_allows_cold_exhaust() {
        let hx = AirToAirHX::plate("NoFC", 1.0, 0.8);
        let result = hx.calculate_with_frost_control(
            -30.0, 0.001, 1.2, 22.0, 0.008, 1.2, false,
        );
        // Without frost control, exhaust can get colder
        assert!(result.exhaust_outlet_temp < 10.0,
                "Without frost control, exhaust drops: {}", result.exhaust_outlet_temp);
    }

    #[test]
    fn frost_control_recirculation() {
        let hx = AirToAirHX::plate("Recirc", 1.0, 0.8)
            .with_frost_control(FrostControlType::ExhaustAirRecirculation, -10.0);
        // Below threshold: effectiveness reduced
        let result = hx.calculate_with_frost_control(
            -20.0, 0.001, 1.2, 22.0, 0.008, 1.2, false,
        );
        // Compare with no frost control
        let r_normal = hx.calculate(-20.0, 0.001, 1.2, 22.0, 0.008, 1.2);
        assert!(result.sensible_heat_rate < r_normal.sensible_heat_rate,
                "Recirculation should reduce heat transfer: {} vs {}",
                result.sensible_heat_rate, r_normal.sensible_heat_rate);
    }

    #[test]
    fn frost_control_recirculation_above_threshold() {
        let hx = AirToAirHX::plate("Recirc", 1.0, 0.8)
            .with_frost_control(FrostControlType::ExhaustAirRecirculation, -10.0);
        // Above threshold: no reduction
        let result = hx.calculate_with_frost_control(
            0.0, 0.003, 1.2, 22.0, 0.008, 1.2, false,
        );
        let r_normal = hx.calculate(0.0, 0.003, 1.2, 22.0, 0.008, 1.2);
        assert!((result.sensible_heat_rate - r_normal.sensible_heat_rate).abs() < 1.0,
                "Above threshold, should be the same");
    }

    // ====================================================================
    // Economizer Lockout Tests
    // ====================================================================

    #[test]
    fn economizer_lockout_bypasses_hx() {
        let hx = AirToAirHX::plate("Lock", 1.0, 0.7)
            .with_economizer_lockout(true);
        let result = hx.calculate_with_frost_control(
            15.0, 0.007, 1.2, 22.0, 0.008, 1.2, true,
        );
        assert!(result.sensible_heat_rate.abs() < 1e-10,
                "Should bypass when economizer active");
        assert!((result.supply_outlet_temp - 15.0).abs() < 1e-10);
    }

    #[test]
    fn economizer_lockout_no_bypass_when_inactive() {
        let hx = AirToAirHX::plate("Lock", 1.0, 0.7)
            .with_economizer_lockout(true);
        let result = hx.calculate_with_frost_control(
            -5.0, 0.002, 1.2, 22.0, 0.008, 1.2, false,
        );
        assert!(result.sensible_heat_rate > 0.0,
                "Should NOT bypass when economizer inactive");
    }

    #[test]
    fn no_lockout_allows_hx_with_economizer() {
        let hx = AirToAirHX::plate("NoLock", 1.0, 0.7);
        let result = hx.calculate_with_frost_control(
            15.0, 0.007, 1.2, 22.0, 0.008, 1.2, true,
        );
        assert!(result.sensible_heat_rate.abs() > 0.0,
                "Without lockout, HX operates with economizer");
    }

    // ====================================================================
    // Flat Plate HX (NTU-effectiveness) Tests
    // ====================================================================

    #[test]
    fn flat_plate_basic() {
        let hx = FlatPlateHX::new("FP-1", 5000.0);
        assert!((hx.ua - 5000.0).abs() < 1e-10);
    }

    #[test]
    fn flat_plate_counterflow_heating() {
        let hx = FlatPlateHX::new("FP", 3000.0);
        let result = hx.calculate(-10.0, 1.0, 0.002, 22.0, 1.0, 0.008);
        assert!(result.heat_rate > 0.0, "Q={}", result.heat_rate);
        assert!(result.supply_outlet_temp > -10.0);
        assert!(result.exhaust_outlet_temp < 22.0);
        assert!(result.effectiveness > 0.0 && result.effectiveness <= 1.0,
                "eff={}", result.effectiveness);
    }

    #[test]
    fn flat_plate_energy_balance() {
        let hx = FlatPlateHX::new("FP", 2000.0);
        let result = hx.calculate(5.0, 0.8, 0.003, 22.0, 1.0, 0.008);
        let cp_s = ep_psychrometrics::cp_air(0.003);
        let cp_e = ep_psychrometrics::cp_air(0.008);
        let q_supply = 0.8 * cp_s * (result.supply_outlet_temp - 5.0);
        let q_exhaust = 1.0 * cp_e * (22.0 - result.exhaust_outlet_temp);
        assert!((q_supply - q_exhaust).abs() / q_supply.abs().max(1.0) < 0.01,
                "q_s={}, q_e={}", q_supply, q_exhaust);
    }

    #[test]
    fn flat_plate_no_flow() {
        let hx = FlatPlateHX::new("FP", 3000.0);
        let result = hx.calculate(-10.0, 0.0, 0.002, 22.0, 1.0, 0.008);
        assert!(result.heat_rate.abs() < 1e-10);
        assert!((result.supply_outlet_temp - (-10.0)).abs() < 1e-10);
    }

    #[test]
    fn flat_plate_equal_capacity_rates() {
        // C_min = C_max, special effectiveness formula
        let hx = FlatPlateHX::new("FP", 2000.0);
        let result = hx.calculate(0.0, 1.0, 0.005, 20.0, 1.0, 0.005);
        assert!(result.effectiveness > 0.0 && result.effectiveness <= 1.0);
        assert!(result.heat_rate > 0.0);
    }
}
