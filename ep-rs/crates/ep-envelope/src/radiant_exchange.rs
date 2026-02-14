//! Interior radiant exchange between surfaces within an enclosure.
//!
//! Implements the ScriptF method (Hottel) for inter-surface longwave radiation
//! exchange and the Carroll MRT simplified method.
//!
//! Reference: EnergyPlus Engineering Reference — HeatBalanceIntRadExchange

use crate::convection::STEFAN_BOLTZMANN;

/// A radiant enclosure for interior longwave exchange.
#[derive(Debug, Clone)]
pub struct RadiantEnclosure {
    /// Number of surfaces in the enclosure.
    pub num_surfaces: usize,
    /// Surface areas (m²).
    pub areas: Vec<f64>,
    /// Surface emissivities (0..1).
    pub emissivities: Vec<f64>,
    /// View factor matrix F[i][j] — fraction of radiation leaving i that hits j.
    pub view_factors: Vec<Vec<f64>>,
    /// ScriptF matrix — Hottel's script-F accounting for emissivity and reflections.
    pub script_f: Vec<Vec<f64>>,
}

impl RadiantEnclosure {
    /// Create an enclosure with area-weighted view factors (diffuse enclosure model).
    ///
    /// F(i→j) = A_j / (Σ A_k - A_i) for i ≠ j, F(i→i) = 0.
    pub fn new(areas: Vec<f64>, emissivities: Vec<f64>) -> Self {
        let n = areas.len();
        assert_eq!(n, emissivities.len());

        let mut enc = Self {
            num_surfaces: n,
            areas,
            emissivities,
            view_factors: vec![vec![0.0; n]; n],
            script_f: vec![vec![0.0; n]; n],
        };
        enc.compute_area_weighted_view_factors();
        enc.compute_script_f();
        enc
    }

    /// Create from explicit view factors (for non-diffuse enclosures).
    pub fn with_view_factors(
        areas: Vec<f64>,
        emissivities: Vec<f64>,
        view_factors: Vec<Vec<f64>>,
    ) -> Self {
        let n = areas.len();
        let mut enc = Self {
            num_surfaces: n,
            areas,
            emissivities,
            view_factors,
            script_f: vec![vec![0.0; n]; n],
        };
        enc.compute_script_f();
        enc
    }

    /// Compute area-weighted view factors for a diffuse enclosure.
    fn compute_area_weighted_view_factors(&mut self) {
        let n = self.num_surfaces;
        let total_area: f64 = self.areas.iter().sum();

        for i in 0..n {
            let denom = total_area - self.areas[i];
            if denom > 1e-10 {
                for j in 0..n {
                    self.view_factors[i][j] = if i != j {
                        self.areas[j] / denom
                    } else {
                        0.0
                    };
                }
            }
        }
    }

    /// Compute ScriptF matrix using Hottel's method.
    ///
    /// ScriptF accounts for emissivity and multiple reflections.
    /// For an N-surface gray enclosure, we solve:
    ///   ScriptF[i][j] = F[i][j] * ε_j / (1 - Σ_k F[i][k]*(1 - ε_k) * ...)
    ///
    /// Simplified (first-order): ScriptF[i][j] ≈ F[i][j] * ε_j / (1 - ρ_eff)
    /// where ρ_eff accounts for average reflectivity of the enclosure.
    fn compute_script_f(&mut self) {
        let n = self.num_surfaces;

        // Compute effective reflectance seen from each surface
        for i in 0..n {
            // Average reflectance weighted by view factors
            let mut rho_eff = 0.0;
            for k in 0..n {
                if k != i {
                    rho_eff += self.view_factors[i][k] * (1.0 - self.emissivities[k]);
                }
            }

            for j in 0..n {
                if i != j {
                    let denom = 1.0 - rho_eff * (1.0 - self.emissivities[i]);
                    self.script_f[i][j] = if denom > 1e-10 {
                        self.view_factors[i][j] * self.emissivities[j] / denom
                    } else {
                        self.view_factors[i][j] * self.emissivities[j]
                    };
                } else {
                    self.script_f[i][j] = 0.0;
                }
            }
        }
    }

    /// Calculate net radiant heat flux for each surface using ScriptF method.
    ///
    /// Returns the net longwave radiant flux (W/m²) for each surface.
    /// Positive = net heat gain to the surface.
    ///
    /// Uses symmetrized exchange to guarantee energy conservation:
    ///   G_sym[i][j] = (A_i*ScriptF[i][j] + A_j*ScriptF[j][i]) / 2
    ///   Q_i = σ/A_i * Σ_j G_sym[i][j] * (T_j⁴ - T_i⁴)
    pub fn calc_exchange(&self, surface_temps_k: &[f64]) -> Vec<f64> {
        let n = self.num_surfaces;
        let mut net_flux = vec![0.0; n];

        for i in 0..n {
            let ti4 = surface_temps_k[i].powi(4);
            for j in (i + 1)..n {
                let tj4 = surface_temps_k[j].powi(4);
                // Symmetrized exchange factor ensures A_i*G_ij = A_j*G_ji
                let g_sym = 0.5
                    * (self.areas[i] * self.script_f[i][j]
                        + self.areas[j] * self.script_f[j][i]);
                let q_exchange = g_sym * STEFAN_BOLTZMANN * (tj4 - ti4);
                // Surface i gains q, surface j loses q (per unit area)
                if self.areas[i] > 1e-10 {
                    net_flux[i] += q_exchange / self.areas[i];
                }
                if self.areas[j] > 1e-10 {
                    net_flux[j] -= q_exchange / self.areas[j];
                }
            }
        }

        net_flux
    }

    /// Calculate linearized radiant exchange coefficients (h_rad) for each surface pair.
    ///
    /// Returns an NxN matrix where h_rad[i][j] is the symmetrized linearized
    /// radiation coefficient (W/(m²·K)) from surface j to surface i:
    ///   q_rad_i = Σ_j h_rad[i][j] * (T_j - T_i)
    pub fn linearized_coefficients(&self, surface_temps_k: &[f64]) -> Vec<Vec<f64>> {
        let n = self.num_surfaces;
        let mut h_rad = vec![vec![0.0; n]; n];

        for i in 0..n {
            for j in (i + 1)..n {
                let ti = surface_temps_k[i];
                let tj = surface_temps_k[j];
                // Linearized: σ*(Ti⁴-Tj⁴)/(Ti-Tj) = σ*(Ti²+Tj²)*(Ti+Tj)
                let h = STEFAN_BOLTZMANN * (ti * ti + tj * tj) * (ti + tj);
                // Symmetrized exchange factor
                let g_sym = 0.5
                    * (self.areas[i] * self.script_f[i][j]
                        + self.areas[j] * self.script_f[j][i]);

                if self.areas[i] > 1e-10 {
                    h_rad[i][j] = g_sym * h / self.areas[i];
                }
                if self.areas[j] > 1e-10 {
                    h_rad[j][i] = g_sym * h / self.areas[j];
                }
            }
        }

        h_rad
    }

    /// Calculate net radiant flux using linearized coefficients.
    ///
    /// More efficient for iterative heat balance since coefficients
    /// change slowly with temperature.
    pub fn calc_exchange_linearized(
        &self,
        surface_temps_k: &[f64],
        h_rad: &[Vec<f64>],
    ) -> Vec<f64> {
        let n = self.num_surfaces;
        let mut net_flux = vec![0.0; n];

        for i in 0..n {
            for j in 0..n {
                if i != j {
                    net_flux[i] += h_rad[i][j] * (surface_temps_k[j] - surface_temps_k[i]);
                }
            }
        }

        net_flux
    }
}

// ─── Carroll MRT Method ─────────────────────────────────────────────

/// Carroll Mean Radiant Temperature (MRT) method for interior radiant exchange.
///
/// Simpler than ScriptF: treats each surface as exchanging radiation with
/// a single "mean radiant node" at the area-weighted MRT.
///
/// q_rad_i = h_rad_i * (T_mrt - T_i)
/// where h_rad_i = 4 * ε_i * σ * T_avg³
pub struct CarrollMrtMethod {
    /// Per-surface radiation resistance (1/(4*ε*σ*A)).
    #[allow(dead_code)]
    resistances: Vec<f64>,
    /// Total number of surfaces.
    num_surfaces: usize,
}

impl CarrollMrtMethod {
    /// Create a new Carroll MRT calculator from surface areas and emissivities.
    pub fn new(areas: &[f64], emissivities: &[f64]) -> Self {
        let n = areas.len();
        let mut resistances = Vec::with_capacity(n);

        for i in 0..n {
            let eps_a = emissivities[i] * areas[i];
            if eps_a > 1e-10 {
                resistances.push(1.0 / (4.0 * STEFAN_BOLTZMANN * eps_a));
            } else {
                resistances.push(f64::MAX);
            }
        }

        Self {
            resistances,
            num_surfaces: n,
        }
    }

    /// Calculate MRT and per-surface net radiant flux.
    ///
    /// Returns (mrt_k, net_flux_per_surface_w_m2).
    pub fn calc_exchange(
        &self,
        surface_temps_k: &[f64],
        areas: &[f64],
        emissivities: &[f64],
    ) -> (f64, Vec<f64>) {
        let n = self.num_surfaces;

        // Approximate MRT as area-emissivity-weighted average temperature
        let mut sum_ea_t = 0.0;
        let mut sum_ea = 0.0;
        for i in 0..n {
            let ea = emissivities[i] * areas[i];
            sum_ea_t += ea * surface_temps_k[i];
            sum_ea += ea;
        }
        let mrt = if sum_ea > 1e-10 {
            sum_ea_t / sum_ea
        } else {
            surface_temps_k.iter().sum::<f64>() / n as f64
        };

        // Average temperature for linearization
        let t_avg = (mrt + surface_temps_k.iter().sum::<f64>() / n as f64) / 2.0;
        let h_rad_base = 4.0 * STEFAN_BOLTZMANN * t_avg * t_avg * t_avg;

        // Per-surface flux
        let mut net_flux = vec![0.0; n];
        for i in 0..n {
            net_flux[i] = emissivities[i] * h_rad_base * (mrt - surface_temps_k[i]);
        }

        (mrt, net_flux)
    }
}

/// Verify energy conservation: sum of (net_flux * area) should equal zero.
pub fn check_energy_conservation(net_flux: &[f64], areas: &[f64]) -> f64 {
    net_flux.iter().zip(areas.iter()).map(|(q, a)| q * a).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_weighted_view_factors_sum_to_one() {
        let enc = RadiantEnclosure::new(
            vec![10.0, 20.0, 10.0, 20.0],
            vec![0.9, 0.9, 0.9, 0.9],
        );
        for i in 0..4 {
            let sum: f64 = enc.view_factors[i].iter().sum();
            assert!((sum - 1.0).abs() < 1e-10, "row {i} sum={sum}");
        }
    }

    #[test]
    fn view_factor_reciprocity_equal_areas() {
        // For equal-area surfaces, area-weighted VFs satisfy reciprocity exactly
        let enc = RadiantEnclosure::new(
            vec![10.0, 10.0, 10.0],
            vec![0.9, 0.9, 0.9],
        );
        for i in 0..3 {
            for j in 0..3 {
                let lhs = enc.areas[i] * enc.view_factors[i][j];
                let rhs = enc.areas[j] * enc.view_factors[j][i];
                assert!(
                    (lhs - rhs).abs() < 1e-10,
                    "reciprocity failed: i={i} j={j} lhs={lhs} rhs={rhs}"
                );
            }
        }
    }

    #[test]
    fn view_factor_self_zero() {
        let enc = RadiantEnclosure::new(
            vec![10.0, 20.0, 15.0],
            vec![0.9, 0.9, 0.9],
        );
        for i in 0..3 {
            assert!(enc.view_factors[i][i].abs() < 1e-10, "F[{i}][{i}] should be 0");
        }
    }

    #[test]
    fn script_f_blackbody() {
        // For blackbody (ε=1), ScriptF = F
        let enc = RadiantEnclosure::new(
            vec![10.0, 10.0, 10.0, 10.0],
            vec![1.0, 1.0, 1.0, 1.0],
        );
        for i in 0..4 {
            for j in 0..4 {
                assert!(
                    (enc.script_f[i][j] - enc.view_factors[i][j]).abs() < 1e-10,
                    "ScriptF should equal F for blackbody: i={i} j={j}"
                );
            }
        }
    }

    #[test]
    fn script_f_less_than_view_factor() {
        // For gray surfaces (ε<1), ScriptF < F*ε ≤ F
        let enc = RadiantEnclosure::new(
            vec![10.0, 10.0],
            vec![0.5, 0.5],
        );
        for i in 0..2 {
            for j in 0..2 {
                if i != j {
                    assert!(
                        enc.script_f[i][j] <= enc.view_factors[i][j] + 1e-10,
                        "ScriptF should be ≤ F: i={i} j={j}"
                    );
                    assert!(
                        enc.script_f[i][j] > 0.0,
                        "ScriptF should be positive: i={i} j={j}"
                    );
                }
            }
        }
    }

    #[test]
    fn isothermal_enclosure_zero_exchange() {
        // All surfaces at same temperature → no net exchange
        let enc = RadiantEnclosure::new(
            vec![10.0, 20.0, 10.0, 20.0],
            vec![0.9, 0.9, 0.9, 0.9],
        );
        let temps = vec![300.0, 300.0, 300.0, 300.0];
        let flux = enc.calc_exchange(&temps);

        for (i, q) in flux.iter().enumerate() {
            assert!(q.abs() < 1e-10, "surface {i} flux={q} should be zero");
        }
    }

    #[test]
    fn two_surface_exchange_direction() {
        // Hot surface 0 (350K) and cold surface 1 (250K)
        let enc = RadiantEnclosure::new(
            vec![10.0, 10.0],
            vec![0.9, 0.9],
        );
        let temps = vec![350.0, 250.0];
        let flux = enc.calc_exchange(&temps);

        // Hot surface should lose heat (negative flux)
        assert!(flux[0] < 0.0, "hot surface flux={} should be negative", flux[0]);
        // Cold surface should gain heat (positive flux)
        assert!(flux[1] > 0.0, "cold surface flux={} should be positive", flux[1]);
    }

    #[test]
    fn energy_conservation_two_surface() {
        let enc = RadiantEnclosure::new(
            vec![10.0, 10.0],
            vec![0.9, 0.9],
        );
        let temps = vec![350.0, 250.0];
        let flux = enc.calc_exchange(&temps);
        let imbalance = check_energy_conservation(&flux, &enc.areas);
        assert!(
            imbalance.abs() < 1e-6,
            "energy imbalance={imbalance} W should be ~0"
        );
    }

    #[test]
    fn energy_conservation_six_surface_box() {
        // 6-surface box at different temperatures
        let areas = vec![10.0, 10.0, 15.0, 15.0, 12.0, 12.0];
        let emissivities = vec![0.9, 0.9, 0.8, 0.8, 0.95, 0.95];
        let enc = RadiantEnclosure::new(areas.clone(), emissivities);

        let temps = vec![300.0, 310.0, 295.0, 305.0, 298.0, 302.0];
        let flux = enc.calc_exchange(&temps);
        let imbalance = check_energy_conservation(&flux, &areas);
        assert!(
            imbalance.abs() < 1e-4,
            "6-surface box imbalance={imbalance} W"
        );
    }

    #[test]
    fn linearized_coefficients_positive() {
        let enc = RadiantEnclosure::new(
            vec![10.0, 10.0, 10.0],
            vec![0.9, 0.9, 0.9],
        );
        let temps = vec![290.0, 300.0, 310.0];
        let h_rad = enc.linearized_coefficients(&temps);

        for i in 0..3 {
            for j in 0..3 {
                if i != j {
                    assert!(h_rad[i][j] > 0.0, "h_rad[{i}][{j}]={} should be positive", h_rad[i][j]);
                    // Typical h_rad for room-temp surfaces: 4-6 W/(m²·K)
                    assert!(h_rad[i][j] < 10.0, "h_rad[{i}][{j}]={} unusually large", h_rad[i][j]);
                }
            }
        }
    }

    #[test]
    fn linearized_vs_exact_agreement() {
        let enc = RadiantEnclosure::new(
            vec![10.0, 10.0],
            vec![0.9, 0.9],
        );
        let temps = vec![298.0, 302.0]; // Small ΔT for linearization accuracy
        let flux_exact = enc.calc_exchange(&temps);
        let h_rad = enc.linearized_coefficients(&temps);
        let flux_linear = enc.calc_exchange_linearized(&temps, &h_rad);

        for i in 0..2 {
            let rel_err = if flux_exact[i].abs() > 1e-6 {
                ((flux_linear[i] - flux_exact[i]) / flux_exact[i]).abs()
            } else {
                (flux_linear[i] - flux_exact[i]).abs()
            };
            assert!(
                rel_err < 0.05,
                "surface {i}: exact={} linear={} rel_err={rel_err}",
                flux_exact[i], flux_linear[i]
            );
        }
    }

    #[test]
    fn carroll_mrt_isothermal_zero_exchange() {
        let areas = vec![10.0, 20.0, 10.0];
        let emissivities = vec![0.9, 0.9, 0.9];
        let mrt_calc = CarrollMrtMethod::new(&areas, &emissivities);

        let temps = vec![300.0, 300.0, 300.0];
        let (mrt, flux) = mrt_calc.calc_exchange(&temps, &areas, &emissivities);

        assert!((mrt - 300.0).abs() < 1e-6, "MRT={mrt} should be 300K");
        for (i, q) in flux.iter().enumerate() {
            assert!(q.abs() < 1e-6, "surface {i} flux={q} should be ~0");
        }
    }

    #[test]
    fn carroll_mrt_direction() {
        let areas = vec![10.0, 10.0];
        let emissivities = vec![0.9, 0.9];
        let mrt_calc = CarrollMrtMethod::new(&areas, &emissivities);

        let temps = vec![350.0, 250.0];
        let (mrt, flux) = mrt_calc.calc_exchange(&temps, &areas, &emissivities);

        // MRT should be between the two temperatures
        assert!(mrt > 250.0 && mrt < 350.0, "MRT={mrt}");
        // Hot surface loses heat
        assert!(flux[0] < 0.0, "hot surface flux={}", flux[0]);
        // Cold surface gains heat
        assert!(flux[1] > 0.0, "cold surface flux={}", flux[1]);
    }
}
