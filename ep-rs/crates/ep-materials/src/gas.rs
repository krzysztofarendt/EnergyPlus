//! Gas properties for window gap fills.
//!
//! Implements polynomial-based gas property evaluation (conductivity, viscosity,
//! specific heat) per ISO 15099 and EnergyPlus conventions.

use super::GasType;

/// Polynomial coefficients for a gas property: value = c0 + c1*T + c2*T^2
#[derive(Debug, Clone, Copy)]
pub struct GasCoeffs {
    pub c0: f64,
    pub c1: f64,
    pub c2: f64,
}

impl GasCoeffs {
    /// Evaluate the property at temperature T (Kelvin).
    pub fn eval(&self, t: f64) -> f64 {
        self.c0 + self.c1 * t + self.c2 * t * t
    }
}

/// Complete set of gas properties (polynomial coefficients).
#[derive(Debug, Clone, Copy)]
pub struct GasData {
    pub conductivity: GasCoeffs,
    pub viscosity: GasCoeffs,
    pub specific_heat: GasCoeffs,
    pub molecular_weight: f64,
    pub specific_heat_ratio: f64,
}

/// Get standard gas properties for a known gas type.
pub fn standard_gas_data(gas_type: GasType) -> GasData {
    match gas_type {
        GasType::Air => GasData {
            conductivity: GasCoeffs { c0: 2.873e-3, c1: 7.76e-5, c2: 0.0 },
            viscosity: GasCoeffs { c0: 3.723e-6, c1: 4.94e-8, c2: 0.0 },
            specific_heat: GasCoeffs { c0: 1002.737, c1: 1.2324e-2, c2: 0.0 },
            molecular_weight: 28.97,
            specific_heat_ratio: 1.4,
        },
        GasType::Argon => GasData {
            conductivity: GasCoeffs { c0: 2.285e-3, c1: 5.149e-5, c2: 0.0 },
            viscosity: GasCoeffs { c0: 3.379e-6, c1: 6.451e-8, c2: 0.0 },
            specific_heat: GasCoeffs { c0: 521.929, c1: 0.0, c2: 0.0 },
            molecular_weight: 39.948,
            specific_heat_ratio: 1.67,
        },
        GasType::Krypton => GasData {
            conductivity: GasCoeffs { c0: 9.443e-4, c1: 2.826e-5, c2: 0.0 },
            viscosity: GasCoeffs { c0: 2.213e-6, c1: 7.777e-8, c2: 0.0 },
            specific_heat: GasCoeffs { c0: 248.091, c1: 0.0, c2: 0.0 },
            molecular_weight: 83.80,
            specific_heat_ratio: 1.68,
        },
        GasType::Xenon => GasData {
            conductivity: GasCoeffs { c0: 4.538e-4, c1: 1.723e-5, c2: 0.0 },
            viscosity: GasCoeffs { c0: 1.069e-6, c1: 7.414e-8, c2: 0.0 },
            specific_heat: GasCoeffs { c0: 158.340, c1: 0.0, c2: 0.0 },
            molecular_weight: 131.30,
            specific_heat_ratio: 1.66,
        },
        GasType::Custom => GasData {
            // Default to air if custom not configured
            conductivity: GasCoeffs { c0: 2.873e-3, c1: 7.76e-5, c2: 0.0 },
            viscosity: GasCoeffs { c0: 3.723e-6, c1: 4.94e-8, c2: 0.0 },
            specific_heat: GasCoeffs { c0: 1002.737, c1: 1.2324e-2, c2: 0.0 },
            molecular_weight: 28.97,
            specific_heat_ratio: 1.4,
        },
    }
}

/// Universal gas constant (J/(mol-K)).
pub const R_UNIVERSAL: f64 = 8.314;

/// Calculate gas density from ideal gas law: rho = P * M / (R * T).
pub fn gas_density(pressure: f64, molecular_weight: f64, temperature_k: f64) -> f64 {
    if temperature_k > 0.0 {
        pressure * molecular_weight / (R_UNIVERSAL * 1000.0 * temperature_k)
    } else {
        0.0
    }
}

/// Calculate Prandtl number: Pr = cp * mu / k.
pub fn prandtl_number(specific_heat: f64, viscosity: f64, conductivity: f64) -> f64 {
    if conductivity > 0.0 {
        specific_heat * viscosity / conductivity
    } else {
        0.0
    }
}

/// Calculate gap gas conductance for a gas fill.
///
/// Returns (conductance, prandtl, grashof).
pub fn gap_conductance(
    gas_type: GasType,
    gap_width: f64,
    t_left: f64,
    t_right: f64,
    pressure: f64,
) -> (f64, f64, f64) {
    let data = standard_gas_data(gas_type);
    let t_mean = 0.5 * (t_left + t_right);
    let dt = (t_left - t_right).abs();

    let k = data.conductivity.eval(t_mean);
    let mu = data.viscosity.eval(t_mean);
    let cp = data.specific_heat.eval(t_mean);
    let rho = gas_density(pressure, data.molecular_weight, t_mean);

    let pr = prandtl_number(cp, mu, k);

    // Grashof number: Gr = rho^2 * g * gap^3 * dT / (T_mean * mu^2)
    let g = 9.81;
    let gr = if t_mean > 0.0 && mu > 0.0 {
        rho * rho * g * gap_width.powi(3) * dt / (t_mean * mu * mu)
    } else {
        0.0
    };

    // Simple conductance (conduction only): h = k / gap_width
    let h_cond = if gap_width > 0.0 { k / gap_width } else { 0.0 };

    (h_cond, pr, gr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_properties_at_300k() {
        let data = standard_gas_data(GasType::Air);
        let k = data.conductivity.eval(300.0);
        let mu = data.viscosity.eval(300.0);
        let cp = data.specific_heat.eval(300.0);

        // Air conductivity at 300K should be around 0.026 W/(m-K)
        assert!(k > 0.020 && k < 0.035, "k={k}");
        // Air viscosity at 300K should be around 1.85e-5 Pa-s
        assert!(mu > 1.5e-5 && mu < 2.2e-5, "mu={mu}");
        // Air specific heat at 300K should be around 1006 J/(kg-K)
        assert!(cp > 1000.0 && cp < 1010.0, "cp={cp}");
    }

    #[test]
    fn gas_density_calculation() {
        // Air at 101325 Pa, 300 K
        let rho = gas_density(101325.0, 28.97, 300.0);
        assert!(rho > 1.0 && rho < 1.3, "rho={rho}");
    }

    #[test]
    fn gap_conductance_air() {
        let (h, pr, _gr) = gap_conductance(GasType::Air, 0.012, 290.0, 280.0, 101325.0);
        // Conductance should be reasonable
        assert!(h > 1.0 && h < 5.0, "h={h}");
        assert!(pr > 0.5 && pr < 1.0, "Pr={pr}");
    }
}
