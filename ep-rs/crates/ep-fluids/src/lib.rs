//! Fluid property lookups for EnergyPlus-rs.
//!
//! Provides temperature-dependent property lookups for water, glycol solutions,
//! and refrigerants using linear interpolation of tabulated data.
//! Ported from EnergyPlus FluidProperties.cc.

pub mod glycol;
pub mod refrigerant;
pub mod water;

/// Error type for fluid property lookups.
#[derive(Debug, Clone)]
pub enum FluidError {
    TemperatureBelowRange { temp: f64, min: f64 },
    TemperatureAboveRange { temp: f64, max: f64 },
    PressureBelowRange { press: f64, min: f64 },
    PressureAboveRange { press: f64, max: f64 },
    PropertyNotAvailable { property: &'static str },
    InvalidQuality { quality: f64 },
}

/// Result of a property lookup: the value plus any out-of-range warning.
#[derive(Debug, Clone)]
pub struct PropertyResult {
    pub value: f64,
    pub warning: Option<FluidError>,
}

impl PropertyResult {
    pub fn ok(value: f64) -> Self {
        Self { value, warning: None }
    }

    pub fn with_warning(value: f64, warning: FluidError) -> Self {
        Self {
            value,
            warning: Some(warning),
        }
    }
}

/// Find the lower index for linear interpolation using binary search.
///
/// Returns the index i such that temps[i] <= temp < temps[i+1].
/// Assumes temps is sorted in ascending order.
pub fn find_array_index(temp: f64, temps: &[f64]) -> usize {
    if temps.is_empty() {
        return 0;
    }
    if temp <= temps[0] {
        return 0;
    }
    if temp >= temps[temps.len() - 1] {
        return temps.len().saturating_sub(2);
    }

    // Binary search
    let mut lo = 0;
    let mut hi = temps.len() - 1;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if temp < temps[mid] {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    lo
}

/// Linear interpolation between two points.
#[inline]
pub fn interpolate(t: f64, t_lo: f64, t_hi: f64, v_lo: f64, v_hi: f64) -> f64 {
    if (t_hi - t_lo).abs() < 1.0e-30 {
        return v_lo;
    }
    v_lo + (t - t_lo) / (t_hi - t_lo) * (v_hi - v_lo)
}

/// Generic tabulated property data with temperature-dependent values.
#[derive(Debug, Clone)]
pub struct PropertyTable {
    pub temps: Vec<f64>,
    pub values: Vec<f64>,
}

impl PropertyTable {
    pub fn new(temps: Vec<f64>, values: Vec<f64>) -> Self {
        assert_eq!(temps.len(), values.len(), "temps and values must have same length");
        Self { temps, values }
    }

    /// Look up a property value at a given temperature using linear interpolation.
    pub fn lookup(&self, temp: f64) -> PropertyResult {
        if self.temps.is_empty() {
            return PropertyResult::ok(0.0);
        }

        if temp < self.temps[0] {
            return PropertyResult::with_warning(
                self.values[0],
                FluidError::TemperatureBelowRange {
                    temp,
                    min: self.temps[0],
                },
            );
        }

        let last = self.temps.len() - 1;
        if temp > self.temps[last] {
            return PropertyResult::with_warning(
                self.values[last],
                FluidError::TemperatureAboveRange {
                    temp,
                    max: self.temps[last],
                },
            );
        }

        let i = find_array_index(temp, &self.temps);
        let value = interpolate(temp, self.temps[i], self.temps[i + 1], self.values[i], self.values[i + 1]);
        PropertyResult::ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_array_index_basic() {
        let temps = vec![0.0, 10.0, 20.0, 30.0, 40.0];
        assert_eq!(find_array_index(-5.0, &temps), 0);
        assert_eq!(find_array_index(0.0, &temps), 0);
        assert_eq!(find_array_index(5.0, &temps), 0);
        assert_eq!(find_array_index(15.0, &temps), 1);
        assert_eq!(find_array_index(25.0, &temps), 2);
        assert_eq!(find_array_index(35.0, &temps), 3);
        assert_eq!(find_array_index(50.0, &temps), 3);
    }

    #[test]
    fn interpolation_basic() {
        assert_eq!(interpolate(5.0, 0.0, 10.0, 100.0, 200.0), 150.0);
        assert_eq!(interpolate(0.0, 0.0, 10.0, 100.0, 200.0), 100.0);
        assert_eq!(interpolate(10.0, 0.0, 10.0, 100.0, 200.0), 200.0);
    }

    #[test]
    fn property_table_lookup() {
        let table = PropertyTable::new(vec![0.0, 10.0, 20.0, 30.0], vec![4217.0, 4192.0, 4182.0, 4178.0]);

        let r = table.lookup(5.0);
        assert!(r.warning.is_none());
        assert!((r.value - 4204.5).abs() < 0.1);

        let r_low = table.lookup(-10.0);
        assert!(r_low.warning.is_some());
        assert_eq!(r_low.value, 4217.0);

        let r_high = table.lookup(40.0);
        assert!(r_high.warning.is_some());
        assert_eq!(r_high.value, 4178.0);
    }
}
