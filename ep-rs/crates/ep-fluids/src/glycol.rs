//! Glycol mixture property lookups.
//!
//! Supports water-glycol mixtures (ethylene glycol, propylene glycol)
//! with concentration-dependent, temperature-interpolated properties.

use crate::{PropertyResult, PropertyTable};

/// A glycol mixture with pre-interpolated temperature-dependent properties.
#[derive(Debug, Clone)]
pub struct GlycolProperties {
    pub name: String,
    pub glycol_type: GlycolType,
    pub concentration: f64,
    pub specific_heat: Option<PropertyTable>,
    pub density: Option<PropertyTable>,
    pub conductivity: Option<PropertyTable>,
    pub viscosity: Option<PropertyTable>,
}

/// Type of glycol base fluid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlycolType {
    Water,
    EthyleneGlycol,
    PropyleneGlycol,
}

impl GlycolProperties {
    /// Create a pure water glycol (concentration = 0).
    pub fn water() -> Self {
        let water = crate::water::WaterProperties::new();
        Self {
            name: "Water".to_string(),
            glycol_type: GlycolType::Water,
            concentration: 0.0,
            specific_heat: Some(water.specific_heat),
            density: Some(water.density),
            conductivity: Some(water.conductivity),
            viscosity: Some(water.viscosity),
        }
    }

    /// Get specific heat at temperature (J/(kg·K)).
    pub fn cp(&self, temp: f64) -> PropertyResult {
        match &self.specific_heat {
            Some(table) => table.lookup(temp),
            None => PropertyResult::ok(4180.0), // Default water value
        }
    }

    /// Get density at temperature (kg/m³).
    pub fn rho(&self, temp: f64) -> PropertyResult {
        match &self.density {
            Some(table) => table.lookup(temp),
            None => PropertyResult::ok(998.0),
        }
    }

    /// Get thermal conductivity at temperature (W/(m·K)).
    pub fn conductivity(&self, temp: f64) -> PropertyResult {
        match &self.conductivity {
            Some(table) => table.lookup(temp),
            None => PropertyResult::ok(0.598),
        }
    }

    /// Get dynamic viscosity at temperature (Pa·s).
    pub fn viscosity(&self, temp: f64) -> PropertyResult {
        match &self.viscosity {
            Some(table) => table.lookup(temp),
            None => PropertyResult::ok(1.0e-3),
        }
    }
}

/// Interpolate property data at a specific concentration from multi-concentration raw data.
///
/// Raw data is organized as data[concentration_index][temperature_index].
pub fn interpolate_for_concentration(
    conc_data: &[f64],
    temp_count: usize,
    raw_data: &[Vec<f64>],
    concentration: f64,
) -> Vec<f64> {
    let num_concs = conc_data.len();
    let mut result = vec![0.0; temp_count];

    if concentration <= conc_data[0] {
        // Below minimum concentration - use lowest
        result.copy_from_slice(&raw_data[0][..temp_count]);
        return result;
    }

    if concentration >= conc_data[num_concs - 1] {
        // Above maximum concentration - use highest
        result.copy_from_slice(&raw_data[num_concs - 1][..temp_count]);
        return result;
    }

    // Find bracketing concentrations
    let hi_idx = conc_data.iter().position(|&c| c >= concentration).unwrap_or(num_concs - 1);

    if hi_idx == 0 {
        result.copy_from_slice(&raw_data[0][..temp_count]);
        return result;
    }

    let lo_idx = hi_idx - 1;
    let conc_frac = (conc_data[hi_idx] - concentration) / (conc_data[hi_idx] - conc_data[lo_idx]);

    for t in 0..temp_count {
        if raw_data[hi_idx][t].abs() < 1.0e-4 || raw_data[lo_idx][t].abs() < 1.0e-4 {
            result[t] = 0.0;
        } else {
            result[t] = raw_data[hi_idx][t] - conc_frac * (raw_data[hi_idx][t] - raw_data[lo_idx][t]);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_glycol_properties() {
        let w = GlycolProperties::water();
        let cp = w.cp(20.0);
        assert!((cp.value - 4182.0).abs() < 1.0);
        assert!(cp.warning.is_none());
    }

    #[test]
    fn concentration_interpolation() {
        let concs = vec![0.0, 0.5, 1.0];
        let raw = vec![vec![100.0, 200.0], vec![150.0, 250.0], vec![200.0, 300.0]];

        let result = interpolate_for_concentration(&concs, 2, &raw, 0.25);
        assert!((result[0] - 125.0).abs() < 0.1);
        assert!((result[1] - 225.0).abs() < 0.1);
    }
}
