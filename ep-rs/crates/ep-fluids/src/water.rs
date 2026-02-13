//! Pure water property data.
//!
//! Default temperature-dependent properties for water from EnergyPlus FluidProperties.cc.

use crate::PropertyTable;

/// Default temperature points for water properties (-35°C to 125°C, 5°C spacing).
pub const WATER_TEMPS: &[f64] = &[
    -35.0, -30.0, -25.0, -20.0, -15.0, -10.0, -5.0, 0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0,
    50.0, 55.0, 60.0, 65.0, 70.0, 75.0, 80.0, 85.0, 90.0, 95.0, 100.0, 105.0, 110.0, 115.0, 120.0, 125.0,
];

/// Water specific heat data (J/(kg·K)).
pub const WATER_CP: &[f64] = &[
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 4217.0, 4205.0, 4194.0, 4186.0, 4182.0, 4180.0, 4178.0, 4178.0, 4179.0,
    4181.0, 4182.0, 4183.0, 4185.0, 4187.0, 4190.0, 4193.0, 4197.0, 4201.0, 4206.0, 4212.0, 4218.0, 4224.0, 4232.0,
    4240.0, 4248.0, 4257.0,
];

/// Water density data (kg/m³).
pub const WATER_RHO: &[f64] = &[
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 999.8, 999.9, 999.7, 999.1, 998.2, 997.0, 995.6, 994.0, 992.2, 990.2,
    988.0, 985.7, 983.2, 980.5, 977.7, 974.8, 971.8, 968.6, 965.3, 961.9, 958.3, 954.7, 950.9, 947.1, 943.1, 939.0,
];

/// Water thermal conductivity data (W/(m·K)).
pub const WATER_COND: &[f64] = &[
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.561, 0.571, 0.580, 0.589, 0.598, 0.607, 0.615, 0.623, 0.630, 0.637,
    0.644, 0.649, 0.654, 0.659, 0.663, 0.667, 0.670, 0.673, 0.675, 0.677, 0.679, 0.680, 0.681, 0.681, 0.681, 0.680,
];

/// Water dynamic viscosity data (Pa·s).
pub const WATER_VISC: &[f64] = &[
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.7912e-3, 1.5183e-3, 1.3060e-3, 1.1376e-3, 1.0016e-3, 8.901e-4, 7.975e-4,
    7.194e-4, 6.529e-4, 5.960e-4, 5.468e-4, 5.040e-4, 4.664e-4, 4.332e-4, 4.038e-4, 3.775e-4, 3.540e-4, 3.330e-4,
    3.140e-4, 2.968e-4, 2.812e-4, 2.670e-4, 2.540e-4, 2.420e-4, 2.310e-4, 2.204e-4,
];

/// Water properties collection.
pub struct WaterProperties {
    pub specific_heat: PropertyTable,
    pub density: PropertyTable,
    pub conductivity: PropertyTable,
    pub viscosity: PropertyTable,
}

impl WaterProperties {
    /// Create default water properties from built-in data.
    pub fn new() -> Self {
        // Filter out zero values (below freezing)
        let (cp_temps, cp_vals) = filter_nonzero(WATER_TEMPS, WATER_CP);
        let (rho_temps, rho_vals) = filter_nonzero(WATER_TEMPS, WATER_RHO);
        let (cond_temps, cond_vals) = filter_nonzero(WATER_TEMPS, WATER_COND);
        let (visc_temps, visc_vals) = filter_nonzero(WATER_TEMPS, WATER_VISC);

        Self {
            specific_heat: PropertyTable::new(cp_temps, cp_vals),
            density: PropertyTable::new(rho_temps, rho_vals),
            conductivity: PropertyTable::new(cond_temps, cond_vals),
            viscosity: PropertyTable::new(visc_temps, visc_vals),
        }
    }

    pub fn cp(&self, temp: f64) -> f64 {
        self.specific_heat.lookup(temp).value
    }

    pub fn rho(&self, temp: f64) -> f64 {
        self.density.lookup(temp).value
    }

    pub fn conductivity(&self, temp: f64) -> f64 {
        self.conductivity.lookup(temp).value
    }

    pub fn viscosity(&self, temp: f64) -> f64 {
        self.viscosity.lookup(temp).value
    }
}

impl Default for WaterProperties {
    fn default() -> Self {
        Self::new()
    }
}

/// Filter out (temp, value) pairs where value <= 0.
fn filter_nonzero(temps: &[f64], values: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut t = Vec::new();
    let mut v = Vec::new();
    for (&temp, &val) in temps.iter().zip(values.iter()) {
        if val > 0.0 {
            t.push(temp);
            v.push(val);
        }
    }
    (t, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_cp_at_20c() {
        let w = WaterProperties::new();
        let cp = w.cp(20.0);
        assert!((cp - 4182.0).abs() < 1.0, "cp={cp}");
    }

    #[test]
    fn water_density_at_20c() {
        let w = WaterProperties::new();
        let rho = w.rho(20.0);
        assert!((rho - 998.2).abs() < 0.5, "rho={rho}");
    }

    #[test]
    fn water_conductivity_at_20c() {
        let w = WaterProperties::new();
        let k = w.conductivity(20.0);
        assert!((k - 0.598).abs() < 0.005, "k={k}");
    }

    #[test]
    fn water_viscosity_at_20c() {
        let w = WaterProperties::new();
        let mu = w.viscosity(20.0);
        assert!((mu - 1.0016e-3).abs() < 1.0e-5, "mu={mu}");
    }
}
