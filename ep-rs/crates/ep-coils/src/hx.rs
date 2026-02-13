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
}
