//! Zone temperature corrector.
//!
//! After the HVAC system has responded with actual supply air flows,
//! recalculate the zone air temperature and sensible load met.

use crate::{HeatBalanceCoefficients, SolutionAlgorithm, ZoneAirState};
use ep_psychrometrics::cp_air;

/// Result of the corrector step.
#[derive(Debug, Clone, Default)]
pub struct CorrectorResult {
    /// Corrected zone air temperature (C).
    pub temperature: f64,
    /// Corrected zone humidity ratio (kg/kg).
    pub humidity_ratio: f64,
    /// Actual sensible load met by system (W). Positive = heating.
    pub sensible_load_met: f64,
    /// Actual latent load met by system (W).
    pub latent_load_met: f64,
}

/// Correct zone air temperature using actual HVAC system supply.
///
/// This is the "corrector" step: now that we know the actual system airflows
/// (MCp and MCpT), solve for the actual zone air temperature.
///
/// # Arguments
/// * `algorithm` - Solution method
/// * `state` - Current zone air state with history
/// * `coeffs` - Heat balance coefficients INCLUDING system airflows
/// * `air_power_cap` - Zone thermal capacitance (W/K)
pub fn correct_zone_temperature(
    algorithm: SolutionAlgorithm,
    state: &ZoneAirState,
    coeffs: &HeatBalanceCoefficients,
    air_power_cap: f64,
) -> f64 {
    let temp_dep = coeffs.temp_dep_coef();
    let temp_ind = coeffs.temp_ind_coef();

    match algorithm {
        SolutionAlgorithm::ThirdOrder => {
            let history_term = air_power_cap
                * (3.0 * state.temp_history[0]
                    - 1.5 * state.temp_history[1]
                    + (1.0 / 3.0) * state.temp_history[2]);
            let denom = (11.0 / 6.0) * air_power_cap + temp_dep;
            if denom.abs() > 1e-10 {
                (temp_ind + history_term) / denom
            } else {
                state.temperature
            }
        }
        SolutionAlgorithm::AnalyticalSolution => {
            if temp_dep.abs() < 1e-10 {
                state.temp_history[0] + temp_ind / air_power_cap.max(1e-10)
            } else {
                let exp_term = (-temp_dep / air_power_cap.max(1e-10)).exp();
                (state.temp_history[0] - temp_ind / temp_dep) * exp_term + temp_ind / temp_dep
            }
        }
        SolutionAlgorithm::EulerMethod => {
            let denom = air_power_cap + temp_dep;
            if denom.abs() > 1e-10 {
                (air_power_cap * state.temp_history[0] + temp_ind) / denom
            } else {
                state.temperature
            }
        }
    }
}

/// Correct zone humidity ratio using actual HVAC system supply.
///
/// The moisture balance is:
/// C_w * dW/dt = sum(m_dot * (W_supply - W_zone)) + W_gains
///
/// # Arguments
/// * `algorithm` - Solution method
/// * `state` - Current zone air state with humidity history
/// * `moisture_power_cap` - Zone moisture capacitance: rho*V/dt (kg/s)
/// * `moisture_dep_coef` - Mass flow rate sum (kg/s)
/// * `moisture_ind_coef` - m_dot*W_supply sum + latent gains (kg/s)
pub fn correct_zone_humidity(
    algorithm: SolutionAlgorithm,
    state: &ZoneAirState,
    moisture_power_cap: f64,
    moisture_dep_coef: f64,
    moisture_ind_coef: f64,
) -> f64 {
    match algorithm {
        SolutionAlgorithm::ThirdOrder => {
            let history_term = moisture_power_cap
                * (3.0 * state.w_history[0]
                    - 1.5 * state.w_history[1]
                    + (1.0 / 3.0) * state.w_history[2]);
            let denom = (11.0 / 6.0) * moisture_power_cap + moisture_dep_coef;
            if denom.abs() > 1e-10 {
                ((moisture_ind_coef + history_term) / denom).max(0.0)
            } else {
                state.humidity_ratio
            }
        }
        SolutionAlgorithm::AnalyticalSolution => {
            if moisture_dep_coef.abs() < 1e-10 {
                (state.w_history[0] + moisture_ind_coef / moisture_power_cap.max(1e-10)).max(0.0)
            } else {
                let exp_term = (-moisture_dep_coef / moisture_power_cap.max(1e-10)).exp();
                let w_ss = moisture_ind_coef / moisture_dep_coef;
                ((state.w_history[0] - w_ss) * exp_term + w_ss).max(0.0)
            }
        }
        SolutionAlgorithm::EulerMethod => {
            let denom = moisture_power_cap + moisture_dep_coef;
            if denom.abs() > 1e-10 {
                ((moisture_power_cap * state.w_history[0] + moisture_ind_coef) / denom).max(0.0)
            } else {
                state.humidity_ratio
            }
        }
    }
}

/// Calculate the sensible load actually met by the HVAC system.
///
/// SNLoad = sum(m_dot_supply * Cp * (T_supply - T_zone)) + NonAirSystemResponse
pub fn calculate_sensible_load_met(
    sys_mass_flow_rate: f64,
    supply_temp: f64,
    zone_temp: f64,
    supply_w: f64,
    non_air_response: f64,
) -> f64 {
    let cp = cp_air(supply_w);
    sys_mass_flow_rate * cp * (supply_temp - zone_temp) + non_air_response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zone_air_power_cap;

    fn make_state(t: f64) -> ZoneAirState {
        ZoneAirState::new(t, 0.008)
    }

    #[test]
    fn corrector_steady_state() {
        // At steady state with surfaces at 22C and system holding 22C
        let state = make_state(22.0);
        let coeffs = HeatBalanceCoefficients {
            sum_ha: 200.0,
            sum_hat_surf: 4400.0, // 200*22
            sum_internal_convective: 0.0,
            sum_mcp: 0.0,
            sum_mcpt: 0.0,
            sum_sys_mcp: 100.0,
            sum_sys_mcpt: 2200.0, // 100*22
            non_air_system_response: 0.0,
        };
        let cap = zone_air_power_cap(300.0, 0.008, 1.2, 3600.0, 1.0);

        let t = correct_zone_temperature(
            SolutionAlgorithm::ThirdOrder,
            &state,
            &coeffs,
            cap,
        );
        assert!((t - 22.0).abs() < 0.1, "t={t}");
    }

    #[test]
    fn corrector_with_heating() {
        // Cold room (15C), system supplies warm air
        let state = make_state(15.0);
        let coeffs = HeatBalanceCoefficients {
            sum_ha: 200.0,
            sum_hat_surf: 2000.0, // cold surfaces at ~10C
            sum_internal_convective: 0.0,
            sum_mcp: 0.0,
            sum_mcpt: 0.0,
            sum_sys_mcp: 200.0,
            sum_sys_mcpt: 8000.0, // 200*40 -> supply at 40C
            non_air_system_response: 0.0,
        };
        let cap = zone_air_power_cap(300.0, 0.008, 1.2, 3600.0, 1.0);

        let t = correct_zone_temperature(
            SolutionAlgorithm::ThirdOrder,
            &state,
            &coeffs,
            cap,
        );
        // Zone should warm up from 15C
        assert!(t > 15.0, "t={t}");
    }

    #[test]
    fn corrector_algorithms_consistent() {
        // All methods should agree at steady state
        let state = make_state(22.0);
        let coeffs = HeatBalanceCoefficients {
            sum_ha: 200.0,
            sum_hat_surf: 4400.0,
            sum_internal_convective: 500.0,
            sum_mcp: 50.0,
            sum_mcpt: 1100.0,
            sum_sys_mcp: 100.0,
            sum_sys_mcpt: 2200.0,
            non_air_system_response: 0.0,
        };
        let cap = zone_air_power_cap(300.0, 0.008, 1.2, 3600.0, 1.0);

        let t_3rd = correct_zone_temperature(SolutionAlgorithm::ThirdOrder, &state, &coeffs, cap);
        let t_ana = correct_zone_temperature(SolutionAlgorithm::AnalyticalSolution, &state, &coeffs, cap);
        let t_eul = correct_zone_temperature(SolutionAlgorithm::EulerMethod, &state, &coeffs, cap);

        // All should be very close at steady state
        assert!((t_3rd - t_ana).abs() < 0.5, "3rd={t_3rd}, ana={t_ana}");
        assert!((t_3rd - t_eul).abs() < 0.5, "3rd={t_3rd}, eul={t_eul}");
    }

    #[test]
    fn humidity_correction() {
        let state = ZoneAirState::new(22.0, 0.008);
        // System supplying drier air
        let w = correct_zone_humidity(
            SolutionAlgorithm::ThirdOrder,
            &state,
            1.0,   // moisture cap
            0.5,   // dep coef
            0.003, // ind coef (supplying 0.006 kg/kg at 0.5 kg/s)
        );
        assert!(w >= 0.0);
        assert!(w < 0.01);
    }

    #[test]
    fn sensible_load_met_heating() {
        // System supplies 0.5 kg/s at 40C to zone at 22C
        let load = calculate_sensible_load_met(0.5, 40.0, 22.0, 0.008, 0.0);
        // Q ≈ 0.5 * 1005 * 18 ≈ 9045 W
        assert!(load > 8000.0 && load < 10000.0, "load={load}");
    }

    #[test]
    fn sensible_load_met_cooling() {
        // System supplies 1.0 kg/s at 13C to zone at 24C
        let load = calculate_sensible_load_met(1.0, 13.0, 24.0, 0.008, 0.0);
        assert!(load < 0.0, "Should be cooling: {load}");
    }
}
