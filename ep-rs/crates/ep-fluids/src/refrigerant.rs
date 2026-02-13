//! Refrigerant property lookups (saturation and superheated regions).

use crate::{find_array_index, interpolate, PropertyResult, FluidError};

/// Refrigerant with saturation and superheated property tables.
#[derive(Debug, Clone)]
pub struct RefrigerantProperties {
    pub name: String,

    // Saturation pressure vs temperature
    pub sat_temps: Vec<f64>,
    pub sat_pressures: Vec<f64>,

    // Saturated liquid/vapor enthalpy
    pub h_f: Vec<f64>,  // liquid enthalpy
    pub h_fg: Vec<f64>, // enthalpy of vaporization

    // Saturated liquid/vapor specific heat
    pub cp_f: Vec<f64>,
    pub cp_fg: Vec<f64>,

    // Saturated liquid/vapor density
    pub rho_f: Vec<f64>,
    pub rho_fg: Vec<f64>,

    // Superheated region (2D: temperature x pressure)
    pub sup_temps: Vec<f64>,
    pub sup_pressures: Vec<f64>,
    pub sup_enthalpy: Vec<Vec<f64>>, // [pressure_idx][temp_idx]
    pub sup_density: Vec<Vec<f64>>,
}

impl RefrigerantProperties {
    /// Get saturation pressure at a given temperature (Pa).
    pub fn saturation_pressure(&self, temp: f64) -> PropertyResult {
        if self.sat_temps.is_empty() || self.sat_pressures.is_empty() {
            return PropertyResult::ok(0.0);
        }
        lookup_1d(&self.sat_temps, &self.sat_pressures, temp)
    }

    /// Get saturation temperature at a given pressure (°C).
    pub fn saturation_temperature(&self, pressure: f64) -> PropertyResult {
        if self.sat_pressures.is_empty() || self.sat_temps.is_empty() {
            return PropertyResult::ok(0.0);
        }
        // Reverse lookup: pressure -> temperature
        lookup_1d(&self.sat_pressures, &self.sat_temps, pressure)
    }

    /// Get enthalpy at a given temperature and quality (J/kg).
    ///
    /// Quality = 0.0 for saturated liquid, 1.0 for saturated vapor.
    pub fn enthalpy(&self, temp: f64, quality: f64) -> PropertyResult {
        if quality < 0.0 || quality > 1.0 {
            return PropertyResult::with_warning(0.0, FluidError::InvalidQuality { quality });
        }
        let h_f = lookup_1d(&self.sat_temps, &self.h_f, temp);
        let h_fg = lookup_1d(&self.sat_temps, &self.h_fg, temp);
        PropertyResult::ok(h_f.value + quality * h_fg.value)
    }

    /// Get density at a given temperature and quality (kg/m³).
    pub fn density(&self, temp: f64, quality: f64) -> PropertyResult {
        if quality < 0.0 || quality > 1.0 {
            return PropertyResult::with_warning(0.0, FluidError::InvalidQuality { quality });
        }
        let rho_f = lookup_1d(&self.sat_temps, &self.rho_f, temp);
        let rho_fg = lookup_1d(&self.sat_temps, &self.rho_fg, temp);
        PropertyResult::ok(rho_f.value + quality * rho_fg.value)
    }

    /// Get specific heat at a given temperature and quality (J/(kg·K)).
    pub fn specific_heat(&self, temp: f64, quality: f64) -> PropertyResult {
        if quality < 0.0 || quality > 1.0 {
            return PropertyResult::with_warning(0.0, FluidError::InvalidQuality { quality });
        }
        let cp_f = lookup_1d(&self.sat_temps, &self.cp_f, temp);
        let cp_fg = lookup_1d(&self.sat_temps, &self.cp_fg, temp);
        PropertyResult::ok(cp_f.value + quality * cp_fg.value)
    }

    /// Get superheated enthalpy at a given temperature and pressure (J/kg).
    pub fn superheated_enthalpy(&self, temp: f64, pressure: f64) -> PropertyResult {
        lookup_2d(&self.sup_temps, &self.sup_pressures, &self.sup_enthalpy, temp, pressure)
    }

    /// Get superheated density at a given temperature and pressure (kg/m³).
    pub fn superheated_density(&self, temp: f64, pressure: f64) -> PropertyResult {
        lookup_2d(&self.sup_temps, &self.sup_pressures, &self.sup_density, temp, pressure)
    }
}

/// 1D lookup with linear interpolation.
fn lookup_1d(x: &[f64], y: &[f64], target: f64) -> PropertyResult {
    if x.is_empty() || y.is_empty() {
        return PropertyResult::ok(0.0);
    }

    if target <= x[0] {
        return PropertyResult::with_warning(
            y[0],
            FluidError::TemperatureBelowRange {
                temp: target,
                min: x[0],
            },
        );
    }

    let last = x.len() - 1;
    if target >= x[last] {
        return PropertyResult::with_warning(
            y[last],
            FluidError::TemperatureAboveRange {
                temp: target,
                max: x[last],
            },
        );
    }

    let i = find_array_index(target, x);
    let value = interpolate(target, x[i], x[i + 1], y[i], y[i + 1]);
    PropertyResult::ok(value)
}

/// 2D lookup (temperature x pressure) with bilinear interpolation.
fn lookup_2d(
    temps: &[f64],
    pressures: &[f64],
    data: &[Vec<f64>],
    temp: f64,
    pressure: f64,
) -> PropertyResult {
    if temps.is_empty() || pressures.is_empty() || data.is_empty() {
        return PropertyResult::ok(0.0);
    }

    // Find pressure bracket
    let p_idx = find_array_index(pressure, pressures);
    let p_idx = p_idx.min(pressures.len().saturating_sub(2));

    // Find temperature bracket
    let t_idx = find_array_index(temp, temps);
    let t_idx = t_idx.min(temps.len().saturating_sub(2));

    // Bilinear interpolation
    let v00 = data[p_idx][t_idx];
    let v01 = data[p_idx][t_idx + 1];
    let v10 = data[p_idx + 1][t_idx];
    let v11 = data[p_idx + 1][t_idx + 1];

    let v_t_low = interpolate(temp, temps[t_idx], temps[t_idx + 1], v00, v01);
    let v_t_high = interpolate(temp, temps[t_idx], temps[t_idx + 1], v10, v11);
    let value = interpolate(pressure, pressures[p_idx], pressures[p_idx + 1], v_t_low, v_t_high);

    PropertyResult::ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_refrigerant() -> RefrigerantProperties {
        RefrigerantProperties {
            name: "TestRefrigerant".to_string(),
            sat_temps: vec![-40.0, -20.0, 0.0, 20.0, 40.0],
            sat_pressures: vec![100000.0, 200000.0, 400000.0, 700000.0, 1100000.0],
            h_f: vec![100.0, 200.0, 300.0, 400.0, 500.0],
            h_fg: vec![200.0, 190.0, 180.0, 160.0, 140.0],
            cp_f: vec![1000.0, 1100.0, 1200.0, 1300.0, 1400.0],
            cp_fg: vec![500.0, 400.0, 300.0, 200.0, 100.0],
            rho_f: vec![1400.0, 1350.0, 1300.0, 1250.0, 1200.0],
            rho_fg: vec![-1380.0, -1330.0, -1280.0, -1220.0, -1160.0],
            sup_temps: vec![],
            sup_pressures: vec![],
            sup_enthalpy: vec![],
            sup_density: vec![],
        }
    }

    #[test]
    fn saturation_pressure_lookup() {
        let r = make_test_refrigerant();
        let p = r.saturation_pressure(0.0);
        assert!((p.value - 400000.0).abs() < 1.0);
    }

    #[test]
    fn enthalpy_quality() {
        let r = make_test_refrigerant();
        // Saturated liquid at 0°C
        let h_liq = r.enthalpy(0.0, 0.0);
        assert!((h_liq.value - 300.0).abs() < 0.1);
        // Saturated vapor at 0°C
        let h_vap = r.enthalpy(0.0, 1.0);
        assert!((h_vap.value - 480.0).abs() < 0.1); // 300 + 180
    }
}
