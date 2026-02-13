//! DX (direct expansion) cooling coil models.
//!
//! Uses rated capacity with three performance curves:
//! - CapFTemp: capacity modifier = f(T_wb_inlet, T_cond_inlet)
//! - EIRFTemp: energy input ratio modifier = f(T_wb_inlet, T_cond_inlet)
//! - EIRFPLR: energy input ratio modifier = f(PLR)

use ep_curves::Curve;

/// DX cooling coil rated conditions.
#[derive(Debug, Clone)]
pub struct DXCoil {
    /// Coil name.
    pub name: String,
    /// Rated total cooling capacity (W).
    pub rated_capacity: f64,
    /// Rated sensible heat ratio (0-1).
    pub rated_shr: f64,
    /// Rated COP (W/W).
    pub rated_cop: f64,
    /// Rated air volume flow rate (m3/s).
    pub rated_air_flow: f64,
    /// Rated condenser inlet temperature (C) — typically 35C outdoor.
    pub rated_cond_inlet_temp: f64,
    /// Rated coil bypass factor.
    pub rated_cbf: f64,
}

/// DX coil calculation result.
#[derive(Debug, Clone, Copy)]
pub struct DXCoilResult {
    /// Total cooling capacity at operating conditions (W).
    pub total_cooling: f64,
    /// Sensible cooling at operating conditions (W).
    pub sensible_cooling: f64,
    /// Latent cooling at operating conditions (W).
    pub latent_cooling: f64,
    /// Compressor electrical power (W).
    pub power: f64,
    /// Air outlet temperature (C).
    pub outlet_temp: f64,
    /// Air outlet humidity ratio (kg/kg).
    pub outlet_humidity_ratio: f64,
    /// Part-load ratio (0-1).
    pub part_load_ratio: f64,
    /// Run-time fraction (0-1).
    pub runtime_fraction: f64,
    /// Actual COP at operating conditions.
    pub cop: f64,
    /// Sensible heat ratio at operating conditions.
    pub shr: f64,
}

impl DXCoil {
    /// Create a DX coil with typical defaults.
    pub fn new(
        name: impl Into<String>,
        rated_capacity: f64,
        rated_shr: f64,
        rated_cop: f64,
        rated_air_flow: f64,
    ) -> Self {
        Self {
            name: name.into(),
            rated_capacity,
            rated_shr,
            rated_cop,
            rated_air_flow,
            rated_cond_inlet_temp: 35.0,
            rated_cbf: 0.1,
        }
    }

    /// Calculate DX coil performance at operating conditions.
    ///
    /// Uses performance curve modifiers (passed as f64 values) for:
    /// - cap_f_temp: capacity modifier as function of temperature
    /// - eir_f_temp: EIR modifier as function of temperature
    /// - eir_f_plr: EIR modifier as function of PLR
    /// - plf_f_plr: part-load performance (for cycling losses)
    pub fn calculate(
        &self,
        air_inlet_temp: f64,
        _air_inlet_wb: f64,
        air_inlet_w: f64,
        air_mass_flow: f64,
        _cond_inlet_temp: f64,
        load_requested: f64,
        cap_f_temp: f64,
        eir_f_temp: f64,
        eir_f_plr_curve: Option<&Curve>,
    ) -> DXCoilResult {
        if air_mass_flow <= 1e-10 || load_requested >= 0.0 {
            return DXCoilResult {
                total_cooling: 0.0,
                sensible_cooling: 0.0,
                latent_cooling: 0.0,
                power: 0.0,
                outlet_temp: air_inlet_temp,
                outlet_humidity_ratio: air_inlet_w,
                part_load_ratio: 0.0,
                runtime_fraction: 0.0,
                cop: 0.0,
                shr: self.rated_shr,
            };
        }

        // Available capacity at operating conditions
        let available_capacity = self.rated_capacity * cap_f_temp.max(0.0);

        // Part-load ratio
        let plr = (-load_requested / available_capacity).clamp(0.0, 1.0);

        // Actual total cooling
        let total_cooling = available_capacity * plr;

        // EIR at operating conditions
        let eir_rated = if self.rated_cop > 0.0 {
            1.0 / self.rated_cop
        } else {
            0.3
        };

        let eir_f_plr_val = match eir_f_plr_curve {
            Some(curve) => curve.evaluate1(plr),
            None => plr, // Default: EIR proportional to PLR
        };

        // Part-load performance factor (cycling losses)
        let plf = 0.85 + 0.15 * plr; // Simple default
        let runtime_fraction = if plf > 0.0 { (plr / plf).min(1.0) } else { plr };

        // Compressor power
        let power = available_capacity * eir_rated * eir_f_temp.max(0.0) * eir_f_plr_val.max(0.0) * runtime_fraction;

        // SHR calculation (simplified — use rated SHR adjusted for conditions)
        let shr = self.rated_shr.clamp(0.0, 1.0);

        let sensible_cooling = total_cooling * shr;
        let latent_cooling = total_cooling * (1.0 - shr);

        // Outlet conditions
        let cp_air = ep_psychrometrics::cp_air(air_inlet_w);
        let h_inlet = ep_psychrometrics::enthalpy(air_inlet_temp, air_inlet_w);

        let outlet_temp = air_inlet_temp - sensible_cooling / (air_mass_flow * cp_air);
        let h_outlet = h_inlet - total_cooling / air_mass_flow;
        let outlet_w = if h_outlet > 0.0 {
            // Calculate W from enthalpy and temperature
            let w = (h_outlet - 1006.0 * outlet_temp) / (2501000.0 + 1860.0 * outlet_temp);
            w.max(0.0).min(air_inlet_w)
        } else {
            air_inlet_w
        };

        let cop = if power > 0.0 {
            total_cooling / power
        } else {
            0.0
        };

        DXCoilResult {
            total_cooling,
            sensible_cooling,
            latent_cooling,
            power,
            outlet_temp,
            outlet_humidity_ratio: outlet_w,
            part_load_ratio: plr,
            runtime_fraction,
            cop,
            shr,
        }
    }
}

/// Calculate apparatus dew point (ADP) temperature for a cooling coil.
///
/// ADP is the effective surface temperature of the coil. Air leaving the coil
/// is a mix of bypassed air (at inlet conditions) and air at ADP conditions.
pub fn apparatus_dew_point(
    air_inlet_temp: f64,
    air_inlet_w: f64,
    coil_bypass_factor: f64,
    total_capacity: f64,
    air_mass_flow: f64,
) -> f64 {
    if air_mass_flow <= 1e-10 || coil_bypass_factor >= 1.0 {
        return air_inlet_temp;
    }

    let h_inlet = ep_psychrometrics::enthalpy(air_inlet_temp, air_inlet_w);
    let h_outlet = h_inlet - total_capacity / air_mass_flow;

    // h_ADP from bypass factor: h_outlet = CBF*h_inlet + (1-CBF)*h_ADP
    if (1.0 - coil_bypass_factor).abs() > 1e-10 {
        let h_adp = (h_outlet - coil_bypass_factor * h_inlet) / (1.0 - coil_bypass_factor);
        // Approximate ADP temperature from enthalpy (assuming saturated)
        // T ≈ (h - 2501000*W_sat) / (1006 + 1860*W_sat) — iterative in practice
        // Simplified: use dry-bulb approximation
        ep_psychrometrics::t_db_from_enthalpy_w(h_adp, air_inlet_w)
    } else {
        air_inlet_temp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dx_coil_basic() {
        let coil = DXCoil::new("Test DX", 10000.0, 0.75, 3.5, 0.5);
        assert!((coil.rated_capacity - 10000.0).abs() < 1e-10);
        assert!((coil.rated_cop - 3.5).abs() < 1e-10);
    }

    #[test]
    fn dx_coil_no_load() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5);
        let result = coil.calculate(25.0, 18.0, 0.010, 0.6, 35.0, 0.0, 1.0, 1.0, None);
        assert!(result.total_cooling.abs() < 1e-10);
        assert!(result.power.abs() < 1e-10);
    }

    #[test]
    fn dx_coil_at_rated() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5);
        // Request full load: -10000W (negative = cooling)
        let result = coil.calculate(
            26.7, 19.4, 0.011, 0.6, 35.0,
            -10000.0,  // Full load request
            1.0,       // CapFTemp = 1.0 (rated)
            1.0,       // EIRFTemp = 1.0 (rated)
            None,
        );
        assert!((result.total_cooling - 10000.0).abs() < 100.0,
                "Q={}", result.total_cooling);
        assert!((result.part_load_ratio - 1.0).abs() < 0.01);
        assert!(result.power > 0.0);
        assert!(result.outlet_temp < 26.7, "T_out={}", result.outlet_temp);
    }

    #[test]
    fn dx_coil_part_load() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5);
        let result = coil.calculate(
            26.7, 19.4, 0.011, 0.6, 35.0,
            -5000.0,   // Half load
            1.0, 1.0, None,
        );
        assert!((result.part_load_ratio - 0.5).abs() < 0.01, "PLR={}", result.part_load_ratio);
        assert!((result.total_cooling - 5000.0).abs() < 100.0);
    }

    #[test]
    fn dx_coil_temperature_effects() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5);
        // At hot condenser (cap_f_temp reduced)
        let result_hot = coil.calculate(
            26.7, 19.4, 0.011, 0.6, 45.0,
            -10000.0,
            0.85,  // Reduced capacity at hot conditions
            1.1,   // Increased EIR at hot conditions
            None,
        );
        // Available capacity = 10000 * 0.85 = 8500W
        assert!((result_hot.total_cooling - 8500.0).abs() < 100.0);
    }

    #[test]
    fn dx_coil_sensible_latent_split() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5);
        let result = coil.calculate(
            26.7, 19.4, 0.011, 0.6, 35.0,
            -10000.0, 1.0, 1.0, None,
        );
        assert!((result.sensible_cooling - 7500.0).abs() < 100.0,
                "Qsens={}", result.sensible_cooling);
        assert!((result.latent_cooling - 2500.0).abs() < 100.0,
                "Qlat={}", result.latent_cooling);
    }

    #[test]
    fn dx_coil_cop_at_rated() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5);
        let result = coil.calculate(
            26.7, 19.4, 0.011, 0.6, 35.0,
            -10000.0, 1.0, 1.0, None,
        );
        // COP should be near rated at full load, rated conditions
        assert!(result.cop > 2.5 && result.cop < 5.0, "COP={}", result.cop);
    }

    #[test]
    fn adp_calculation() {
        let adp = apparatus_dew_point(26.7, 0.011, 0.1, 10000.0, 0.6);
        // ADP should be below the inlet temp
        assert!(adp < 26.7, "ADP={adp}");
        assert!(adp > -10.0, "ADP={adp}"); // Sanity check
    }
}
