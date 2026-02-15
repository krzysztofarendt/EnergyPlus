//! DX (direct expansion) cooling coil models.
//!
//! Uses rated capacity with three performance curves:
//! - CapFTemp: capacity modifier = f(T_wb_inlet, T_cond_inlet)
//! - EIRFTemp: energy input ratio modifier = f(T_wb_inlet, T_cond_inlet)
//! - EIRFPLR: energy input ratio modifier = f(PLR)
//!
//! Enhanced with defrost, crankcase heater, PLF curve, and temperature-dependent SHR.

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
    /// Minimum outdoor temperature for compressor operation (C).
    pub min_outdoor_temp: f64,
    /// Crankcase heater capacity (W).
    pub crankcase_heater_capacity: f64,
    /// Maximum outdoor temp for crankcase heater operation (C).
    pub crankcase_max_outdoor_temp: f64,
    /// Defrost control type.
    pub defrost_control: DefrostControl,
    /// Defrost onset temperature (C). Defrost active when OAT below this.
    pub defrost_onset_temp: f64,
    /// Maximum defrost time fraction (0-1).
    pub max_defrost_fraction: f64,
    /// Resistive defrost heater capacity (W, for Resistive type).
    pub resistive_defrost_capacity: f64,
}

/// Defrost control strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DefrostControl {
    #[default]
    /// No defrost.
    None,
    /// Reverse-cycle defrost (heat pump reverses to melt frost).
    ReverseCycle,
    /// Resistive defrost heater.
    Resistive,
    /// Timed defrost (fractional reduction in capacity).
    Timed,
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
    /// Defrost energy consumption (W).
    pub defrost_power: f64,
    /// Crankcase heater power (W).
    pub crankcase_power: f64,
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
            min_outdoor_temp: -20.0,
            crankcase_heater_capacity: 0.0,
            crankcase_max_outdoor_temp: 10.0,
            defrost_control: DefrostControl::None,
            defrost_onset_temp: 5.0,
            max_defrost_fraction: 0.058, // ~3.5 min/hr
            resistive_defrost_capacity: 0.0,
        }
    }

    /// Set crankcase heater parameters.
    pub fn with_crankcase_heater(mut self, capacity: f64, max_outdoor_temp: f64) -> Self {
        self.crankcase_heater_capacity = capacity;
        self.crankcase_max_outdoor_temp = max_outdoor_temp;
        self
    }

    /// Set defrost parameters.
    pub fn with_defrost(mut self, control: DefrostControl, onset_temp: f64) -> Self {
        self.defrost_control = control;
        self.defrost_onset_temp = onset_temp;
        self
    }

    /// Set resistive defrost heater capacity.
    pub fn with_resistive_defrost(mut self, capacity: f64) -> Self {
        self.defrost_control = DefrostControl::Resistive;
        self.resistive_defrost_capacity = capacity;
        self
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
        cond_inlet_temp: f64,
        load_requested: f64,
        cap_f_temp: f64,
        eir_f_temp: f64,
        eir_f_plr_curve: Option<&Curve>,
    ) -> DXCoilResult {
        // Check if compressor can run
        let compressor_can_run = cond_inlet_temp >= self.min_outdoor_temp;

        if air_mass_flow <= 1e-10 || load_requested >= 0.0 || !compressor_can_run {
            let crankcase_power = self.calc_crankcase_power(cond_inlet_temp);
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
                defrost_power: 0.0,
                crankcase_power,
            };
        }

        // Defrost adjustment
        let defrost_result = self.calc_defrost(cond_inlet_temp);

        // Available capacity at operating conditions (reduced by defrost)
        let available_capacity = self.rated_capacity * cap_f_temp.max(0.0) * (1.0 - defrost_result.capacity_reduction);

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
        let plf = 0.85 + 0.15 * plr; // Default PLF curve
        let runtime_fraction = if plf > 0.0 { (plr / plf).min(1.0) } else { plr };

        // Compressor power
        let power = available_capacity * eir_rated * eir_f_temp.max(0.0)
            * eir_f_plr_val.max(0.0) * runtime_fraction;

        // SHR calculation (temperature-dependent)
        let shr = self.calc_shr(air_inlet_temp, air_inlet_w, air_mass_flow, total_cooling);

        let sensible_cooling = total_cooling * shr;
        let latent_cooling = total_cooling * (1.0 - shr);

        // Outlet conditions
        let cp_air = ep_psychrometrics::cp_air(air_inlet_w);
        let h_inlet = ep_psychrometrics::enthalpy(air_inlet_temp, air_inlet_w);

        let outlet_temp = air_inlet_temp - sensible_cooling / (air_mass_flow * cp_air);
        let h_outlet = h_inlet - total_cooling / air_mass_flow;
        let outlet_w = if h_outlet > 0.0 {
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

        let crankcase_power = 0.0; // Compressor is running

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
            defrost_power: defrost_result.power,
            crankcase_power,
        }
    }

    /// Calculate temperature-dependent SHR.
    ///
    /// SHR varies with entering conditions: higher humidity → lower SHR (more latent).
    fn calc_shr(&self, inlet_temp: f64, inlet_w: f64, mass_flow: f64, total_capacity: f64) -> f64 {
        if total_capacity <= 0.0 || mass_flow <= 1e-10 {
            return self.rated_shr;
        }

        // Apparatus dew point approach for SHR calculation
        let cp = ep_psychrometrics::cp_air(inlet_w);
        let h_inlet = ep_psychrometrics::enthalpy(inlet_temp, inlet_w);
        let h_outlet = h_inlet - total_capacity / mass_flow;

        // Maximum sensible capacity (if coil could cool to 0% RH)
        // Limited by the temperature difference available
        let cbf = self.rated_cbf;

        if cbf >= 1.0 || cbf <= 0.0 {
            return self.rated_shr;
        }

        // Effective coil surface conditions using bypass factor
        let h_adp = (h_outlet - cbf * h_inlet) / (1.0 - cbf);

        // Estimate ADP temperature (simplified)
        let t_adp = ep_psychrometrics::t_db_from_enthalpy_w(h_adp.max(0.0), inlet_w);

        // Sensible capacity limited by ADP temperature
        let sensible_max = mass_flow * cp * (inlet_temp - t_adp).max(0.0) * (1.0 - cbf);
        let shr = if total_capacity > 0.0 {
            (sensible_max / total_capacity).clamp(0.0, 1.0)
        } else {
            self.rated_shr
        };

        // Blend with rated SHR for stability
        (0.5 * shr + 0.5 * self.rated_shr).clamp(0.0, 1.0)
    }

    /// Calculate crankcase heater power.
    fn calc_crankcase_power(&self, outdoor_temp: f64) -> f64 {
        if self.crankcase_heater_capacity > 0.0 && outdoor_temp < self.crankcase_max_outdoor_temp {
            self.crankcase_heater_capacity
        } else {
            0.0
        }
    }

    /// Calculate defrost energy and capacity reduction.
    fn calc_defrost(&self, outdoor_temp: f64) -> DefrostResult {
        if self.defrost_control == DefrostControl::None || outdoor_temp > self.defrost_onset_temp {
            return DefrostResult { power: 0.0, capacity_reduction: 0.0 };
        }

        let defrost_fraction = self.max_defrost_fraction
            * ((self.defrost_onset_temp - outdoor_temp) / self.defrost_onset_temp.abs().max(1.0))
                .clamp(0.0, 1.0);

        match self.defrost_control {
            DefrostControl::ReverseCycle => {
                // Reverse-cycle: capacity reduced during defrost, power = fraction of rated
                let power = self.rated_capacity / self.rated_cop.max(1.0) * defrost_fraction;
                DefrostResult {
                    power,
                    capacity_reduction: defrost_fraction,
                }
            }
            DefrostControl::Resistive => {
                DefrostResult {
                    power: self.resistive_defrost_capacity * defrost_fraction,
                    capacity_reduction: defrost_fraction * 0.5, // Less capacity loss than reverse-cycle
                }
            }
            DefrostControl::Timed => {
                DefrostResult {
                    power: 0.0,
                    capacity_reduction: defrost_fraction,
                }
            }
            DefrostControl::None => DefrostResult { power: 0.0, capacity_reduction: 0.0 },
        }
    }
}

/// Internal defrost calculation result.
#[derive(Debug, Clone, Copy)]
struct DefrostResult {
    power: f64,
    capacity_reduction: f64,
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
        ep_psychrometrics::t_db_from_enthalpy_w(h_adp, air_inlet_w)
    } else {
        air_inlet_temp
    }
}

/// Part-load performance factor (PLF) from PLR.
///
/// PLF accounts for cycling losses at part load.
/// Default: PLF = 0.85 + 0.15 * PLR (linear)
/// With curve: PLF = c0 + c1*PLR
pub fn part_load_fraction(plr: f64, plf_curve: Option<&Curve>) -> f64 {
    match plf_curve {
        Some(curve) => curve.evaluate1(plr).max(0.7),
        None => (0.85 + 0.15 * plr).max(0.7),
    }
}

/// Calculate runtime fraction from PLR and PLF.
pub fn runtime_fraction(plr: f64, plf: f64) -> f64 {
    if plf > 0.0 {
        (plr / plf).min(1.0)
    } else {
        plr
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
        let result = coil.calculate(
            26.7, 19.4, 0.011, 0.6, 35.0,
            -10000.0, 1.0, 1.0, None,
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
            -5000.0, 1.0, 1.0, None,
        );
        assert!((result.part_load_ratio - 0.5).abs() < 0.01, "PLR={}", result.part_load_ratio);
        assert!((result.total_cooling - 5000.0).abs() < 100.0);
    }

    #[test]
    fn dx_coil_temperature_effects() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5);
        let result_hot = coil.calculate(
            26.7, 19.4, 0.011, 0.6, 45.0,
            -10000.0, 0.85, 1.1, None,
        );
        assert!((result_hot.total_cooling - 8500.0).abs() < 100.0);
    }

    #[test]
    fn dx_coil_sensible_latent_split() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5);
        let result = coil.calculate(
            26.7, 19.4, 0.011, 0.6, 35.0,
            -10000.0, 1.0, 1.0, None,
        );
        // SHR is now temperature-dependent, but should be close to rated
        assert!(result.shr > 0.5 && result.shr < 1.0, "SHR={}", result.shr);
        assert!(result.sensible_cooling > 0.0);
        assert!(result.latent_cooling >= 0.0);
        assert!((result.sensible_cooling + result.latent_cooling - result.total_cooling).abs() < 10.0);
    }

    #[test]
    fn dx_coil_cop_at_rated() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5);
        let result = coil.calculate(
            26.7, 19.4, 0.011, 0.6, 35.0,
            -10000.0, 1.0, 1.0, None,
        );
        assert!(result.cop > 2.5 && result.cop < 5.0, "COP={}", result.cop);
    }

    #[test]
    fn adp_calculation() {
        let adp = apparatus_dew_point(26.7, 0.011, 0.1, 10000.0, 0.6);
        assert!(adp < 26.7, "ADP={adp}");
        assert!(adp > -10.0, "ADP={adp}");
    }

    // --- Defrost tests ---

    #[test]
    fn dx_coil_no_defrost() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5);
        let result = coil.calculate(
            26.7, 19.4, 0.011, 0.6, 35.0,
            -10000.0, 1.0, 1.0, None,
        );
        assert!(result.defrost_power.abs() < 1e-10);
    }

    #[test]
    fn dx_coil_reverse_cycle_defrost() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5)
            .with_defrost(DefrostControl::ReverseCycle, 5.0);
        // Outdoor at -5C: defrost active
        let result = coil.calculate(
            26.7, 19.4, 0.011, 0.6, -5.0,
            -10000.0, 1.0, 1.0, None,
        );
        assert!(result.defrost_power > 0.0, "defrost_power={}", result.defrost_power);
        // Capacity should be reduced
        assert!(result.total_cooling < 10000.0, "Q={}", result.total_cooling);
    }

    #[test]
    fn dx_coil_resistive_defrost() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5)
            .with_resistive_defrost(2000.0);
        let result = coil.calculate(
            26.7, 19.4, 0.011, 0.6, 0.0,
            -10000.0, 1.0, 1.0, None,
        );
        assert!(result.defrost_power > 0.0, "defrost_power={}", result.defrost_power);
        assert!(result.defrost_power <= 2000.0, "defrost_power={}", result.defrost_power);
    }

    #[test]
    fn dx_coil_defrost_warm_day() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5)
            .with_defrost(DefrostControl::ReverseCycle, 5.0);
        // Outdoor at 20C: no defrost
        let result = coil.calculate(
            26.7, 19.4, 0.011, 0.6, 20.0,
            -10000.0, 1.0, 1.0, None,
        );
        assert!(result.defrost_power.abs() < 1e-10);
    }

    // --- Crankcase heater tests ---

    #[test]
    fn crankcase_heater_compressor_off() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5)
            .with_crankcase_heater(200.0, 10.0);
        // No load (compressor off), cold day
        let result = coil.calculate(
            20.0, 14.0, 0.008, 0.6, 5.0,
            0.0, 1.0, 1.0, None,
        );
        assert!((result.crankcase_power - 200.0).abs() < 1.0, "crank={}", result.crankcase_power);
    }

    #[test]
    fn crankcase_heater_warm_day() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5)
            .with_crankcase_heater(200.0, 10.0);
        // No load, warm day (above max outdoor temp for crankcase)
        let result = coil.calculate(
            20.0, 14.0, 0.008, 0.6, 15.0,
            0.0, 1.0, 1.0, None,
        );
        assert!(result.crankcase_power.abs() < 1e-10);
    }

    #[test]
    fn crankcase_heater_compressor_on() {
        let coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5)
            .with_crankcase_heater(200.0, 10.0);
        // Compressor running: no crankcase heater
        let result = coil.calculate(
            26.7, 19.4, 0.011, 0.6, 5.0,
            -10000.0, 1.0, 1.0, None,
        );
        assert!(result.crankcase_power.abs() < 1e-10);
    }

    // --- PLF tests ---

    #[test]
    fn plf_default_curve() {
        let plf_50 = part_load_fraction(0.5, None);
        assert!((plf_50 - 0.925).abs() < 0.01, "PLF={}", plf_50);

        let plf_100 = part_load_fraction(1.0, None);
        assert!((plf_100 - 1.0).abs() < 0.01);
    }

    #[test]
    fn plf_custom_curve() {
        let curve = Curve::linear(0.75, 0.25); // PLF = 0.75 + 0.25*PLR
        let plf = part_load_fraction(0.5, Some(&curve));
        assert!((plf - 0.875).abs() < 0.01, "PLF={}", plf);
    }

    #[test]
    fn runtime_fraction_calc() {
        let rtf = runtime_fraction(0.5, 0.925);
        assert!(rtf > 0.5, "RTF={}", rtf); // RTF > PLR due to cycling losses
        assert!(rtf < 0.6, "RTF={}", rtf);
    }

    // --- Min outdoor temp test ---

    #[test]
    fn dx_coil_below_min_outdoor_temp() {
        let mut coil = DXCoil::new("Test", 10000.0, 0.75, 3.5, 0.5);
        coil.min_outdoor_temp = -10.0;
        // Outdoor temp at -15C, below min: compressor cannot run
        let result = coil.calculate(
            26.7, 19.4, 0.011, 0.6, -15.0,
            -10000.0, 1.0, 1.0, None,
        );
        assert!(result.total_cooling.abs() < 1e-10);
        assert!(result.power.abs() < 1e-10);
    }
}
