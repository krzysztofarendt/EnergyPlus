//! Photovoltaic panel models.
//!
//! Simple model: Power = Irradiance * Area * Efficiency
//! One-diode model: Equivalent circuit with temperature/irradiance corrections.

/// PV model type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PvModelType {
    /// Fixed efficiency model.
    Simple,
    /// Equivalent one-diode circuit model (TRNSYS-based).
    OneDiode,
}

/// Cell temperature integration mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellIntegration {
    /// Decoupled energy balance.
    Decoupled,
    /// NOCT-based simple model.
    Noct,
}

/// Simple PV panel parameters.
#[derive(Debug, Clone)]
pub struct SimplePvParams {
    /// Effective collection area (m2).
    pub area: f64,
    /// Active cell fraction (0-1).
    pub active_fraction: f64,
    /// Fixed conversion efficiency (0-1).
    pub efficiency: f64,
}

/// One-diode PV parameters.
#[derive(Debug, Clone)]
pub struct OneDiodeParams {
    pub area: f64,
    pub cells_in_series: u32,
    /// Reference short-circuit current (A).
    pub ref_isc: f64,
    /// Reference open-circuit voltage (V).
    pub ref_voc: f64,
    /// Reference max-power current (A).
    pub ref_imp: f64,
    /// Reference max-power voltage (V).
    pub ref_vmp: f64,
    /// Temperature coefficient of Isc (A/K).
    pub temp_coef_isc: f64,
    /// Temperature coefficient of Voc (V/K).
    pub temp_coef_voc: f64,
    /// NOCT ambient temperature (C).
    pub noct_ambient: f64,
    /// NOCT cell temperature (C).
    pub noct_cell: f64,
    /// Reference temperature (C), typically 25.
    pub ref_temperature: f64,
    /// Reference insolation (W/m2), typically 1000.
    pub ref_insolation: f64,
    /// Number of modules in series.
    pub modules_in_series: u32,
    /// Number of strings in parallel.
    pub strings_in_parallel: u32,
}

/// PV panel specification.
#[derive(Debug, Clone)]
pub struct PvPanel {
    pub name: String,
    pub model_type: PvModelType,
    pub simple: Option<SimplePvParams>,
    pub one_diode: Option<OneDiodeParams>,
}

/// PV calculation result.
#[derive(Debug, Clone, Copy)]
pub struct PvResult {
    /// DC power output (W).
    pub dc_power: f64,
    /// Array efficiency.
    pub efficiency: f64,
    /// Cell temperature (C).
    pub cell_temp: f64,
}

impl PvPanel {
    /// Create a simple efficiency PV panel.
    pub fn simple(name: impl Into<String>, area: f64, efficiency: f64) -> Self {
        Self {
            name: name.into(),
            model_type: PvModelType::Simple,
            simple: Some(SimplePvParams {
                area,
                active_fraction: 1.0,
                efficiency,
            }),
            one_diode: None,
        }
    }

    /// Create a one-diode PV panel.
    pub fn one_diode(name: impl Into<String>, params: OneDiodeParams) -> Self {
        Self {
            name: name.into(),
            model_type: PvModelType::OneDiode,
            simple: None,
            one_diode: Some(params),
        }
    }

    /// Calculate PV output.
    ///
    /// `irradiance` — total incident irradiance on panel surface (W/m2).
    /// `ambient_temp` — outdoor dry-bulb temperature (C).
    /// `wind_speed` — wind speed at panel height (m/s).
    pub fn calculate(
        &self,
        irradiance: f64,
        ambient_temp: f64,
        _wind_speed: f64,
    ) -> PvResult {
        if irradiance <= 0.0 {
            return PvResult { dc_power: 0.0, efficiency: 0.0, cell_temp: ambient_temp };
        }

        match self.model_type {
            PvModelType::Simple => {
                let p = self.simple.as_ref().unwrap();
                let power = irradiance * p.area * p.active_fraction * p.efficiency;
                PvResult {
                    dc_power: power,
                    efficiency: p.efficiency,
                    cell_temp: ambient_temp + irradiance * 0.03, // Approximate
                }
            }
            PvModelType::OneDiode => {
                let p = self.one_diode.as_ref().unwrap();
                // NOCT cell temperature estimate
                let cell_temp = ambient_temp
                    + (p.noct_cell - p.noct_ambient) * (irradiance / p.ref_insolation);

                // Temperature-corrected parameters
                let delta_t = cell_temp - p.ref_temperature;
                let isc = p.ref_isc * (irradiance / p.ref_insolation)
                    + p.temp_coef_isc * delta_t;
                let voc = p.ref_voc + p.temp_coef_voc * delta_t;

                // Simplified max power point (fill factor approach)
                let isc = isc.max(0.0);
                let voc = voc.max(0.0);
                let fill_factor = if isc > 0.0 && voc > 0.0 {
                    (p.ref_imp * p.ref_vmp) / (p.ref_isc * p.ref_voc)
                } else {
                    0.0
                };

                let module_power = fill_factor * isc * voc;
                let array_power = module_power
                    * p.modules_in_series as f64
                    * p.strings_in_parallel as f64;

                let total_area = p.area * p.modules_in_series as f64
                    * p.strings_in_parallel as f64;
                let eff = if irradiance * total_area > 0.0 {
                    array_power / (irradiance * total_area)
                } else {
                    0.0
                };

                PvResult {
                    dc_power: array_power.max(0.0),
                    efficiency: eff,
                    cell_temp,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_pv_full_sun() {
        let panel = PvPanel::simple("PV-1", 10.0, 0.18);
        let result = panel.calculate(1000.0, 25.0, 3.0);
        assert!((result.dc_power - 1800.0).abs() < 1.0);
        assert!((result.efficiency - 0.18).abs() < 1e-10);
    }

    #[test]
    fn simple_pv_partial_sun() {
        let panel = PvPanel::simple("PV", 10.0, 0.18);
        let result = panel.calculate(500.0, 25.0, 3.0);
        assert!((result.dc_power - 900.0).abs() < 1.0);
    }

    #[test]
    fn simple_pv_no_irradiance() {
        let panel = PvPanel::simple("PV", 10.0, 0.18);
        let result = panel.calculate(0.0, 25.0, 3.0);
        assert!(result.dc_power.abs() < 1e-10);
    }

    #[test]
    fn one_diode_basic() {
        let params = OneDiodeParams {
            area: 1.6,
            cells_in_series: 60,
            ref_isc: 9.0,
            ref_voc: 37.0,
            ref_imp: 8.5,
            ref_vmp: 30.0,
            temp_coef_isc: 0.003,
            temp_coef_voc: -0.12,
            noct_ambient: 20.0,
            noct_cell: 45.0,
            ref_temperature: 25.0,
            ref_insolation: 1000.0,
            modules_in_series: 10,
            strings_in_parallel: 2,
        };
        let panel = PvPanel::one_diode("PV Array", params);
        let result = panel.calculate(1000.0, 25.0, 3.0);

        // Module rated power ≈ 8.5 * 30.0 = 255W (fill factor = 255 / (9*37) ≈ 0.766)
        // Array = 255 * 20 modules = 5100W approximately
        assert!(result.dc_power > 3000.0 && result.dc_power < 7000.0,
                "power={}", result.dc_power);
        assert!(result.efficiency > 0.0 && result.efficiency < 0.3);
        assert!(result.cell_temp > 25.0); // NOCT heating
    }

    #[test]
    fn one_diode_high_temp_derating() {
        let params = OneDiodeParams {
            area: 1.6,
            cells_in_series: 60,
            ref_isc: 9.0,
            ref_voc: 37.0,
            ref_imp: 8.5,
            ref_vmp: 30.0,
            temp_coef_isc: 0.003,
            temp_coef_voc: -0.12,
            noct_ambient: 20.0,
            noct_cell: 45.0,
            ref_temperature: 25.0,
            ref_insolation: 1000.0,
            modules_in_series: 1,
            strings_in_parallel: 1,
        };
        let panel = PvPanel::one_diode("PV", params);

        let cool = panel.calculate(1000.0, 10.0, 3.0);
        let hot = panel.calculate(1000.0, 40.0, 3.0);

        // Higher temperature → lower voltage → lower power
        assert!(cool.dc_power > hot.dc_power,
                "cool={}W, hot={}W", cool.dc_power, hot.dc_power);
    }
}
