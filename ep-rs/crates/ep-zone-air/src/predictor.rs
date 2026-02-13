//! Zone load predictor.
//!
//! Predicts the HVAC system load required to maintain thermostat setpoints.
//! Uses the zone air energy balance with known terms (surface convection,
//! internal gains, airflows) and solves for the system load.

use crate::{
    HeatBalanceCoefficients, SolutionAlgorithm, ThermostatControl, ZoneAirState,
    ZoneBalanceResult,
};

/// Predict the system load required to meet zone thermostat setpoints.
///
/// This is the "predictor" step: given all non-system heat balance terms,
/// calculate what HVAC load is needed to reach each setpoint.
///
/// # Arguments
/// * `algorithm` - Solution method
/// * `control` - Thermostat control type
/// * `state` - Current zone air state with temperature history
/// * `coeffs` - Heat balance coefficients (without system airflows)
/// * `air_power_cap` - Zone thermal capacitance C = rho*V*Cp/dt (W/K)
/// * `heating_setpoint` - Heating setpoint temperature (C)
/// * `cooling_setpoint` - Cooling setpoint temperature (C)
pub fn predict_system_load(
    algorithm: SolutionAlgorithm,
    control: ThermostatControl,
    state: &ZoneAirState,
    coeffs: &HeatBalanceCoefficients,
    air_power_cap: f64,
    heating_setpoint: f64,
    cooling_setpoint: f64,
) -> ZoneBalanceResult {
    let temp_dep = coeffs.temp_dep_coef();
    let temp_ind = coeffs.temp_ind_coef();

    // Calculate load to heating setpoint
    let load_heat = load_to_setpoint(
        algorithm, state, temp_dep, temp_ind, air_power_cap, heating_setpoint,
    );

    // Calculate load to cooling setpoint
    let load_cool = load_to_setpoint(
        algorithm, state, temp_dep, temp_ind, air_power_cap, cooling_setpoint,
    );

    // Determine deadband and actual required load based on control type
    match control {
        ThermostatControl::Uncontrolled => {
            // Free-floating: solve for temperature with zero system load
            let t_float = solve_temperature(
                algorithm, state, temp_dep, temp_ind, air_power_cap, 0.0,
            );
            ZoneBalanceResult {
                temperature: t_float,
                load_to_heating_setpoint: 0.0,
                load_to_cooling_setpoint: 0.0,
                in_deadband: true,
            }
        }
        ThermostatControl::SingleHeat => {
            ZoneBalanceResult {
                temperature: heating_setpoint,
                load_to_heating_setpoint: load_heat.max(0.0),
                load_to_cooling_setpoint: 0.0,
                in_deadband: load_heat <= 0.0,
            }
        }
        ThermostatControl::SingleCool => {
            ZoneBalanceResult {
                temperature: cooling_setpoint,
                load_to_heating_setpoint: 0.0,
                load_to_cooling_setpoint: load_cool.min(0.0),
                in_deadband: load_cool >= 0.0,
            }
        }
        ThermostatControl::SingleHeatCool | ThermostatControl::DualSetPoint => {
            let in_deadband = load_heat <= 0.0 && load_cool >= 0.0;
            ZoneBalanceResult {
                temperature: if in_deadband {
                    // Free-float in deadband
                    solve_temperature(
                        algorithm, state, temp_dep, temp_ind, air_power_cap, 0.0,
                    )
                } else if load_heat > 0.0 {
                    heating_setpoint
                } else {
                    cooling_setpoint
                },
                load_to_heating_setpoint: load_heat.max(0.0),
                load_to_cooling_setpoint: load_cool.min(0.0),
                in_deadband,
            }
        }
    }
}

/// Calculate system load required to reach a target setpoint.
///
/// Load = TempDep * T_setpoint + AirPowerCap * dT/dt_term - TempInd
///
/// Positive load = heating needed, negative = cooling needed.
fn load_to_setpoint(
    algorithm: SolutionAlgorithm,
    state: &ZoneAirState,
    temp_dep: f64,
    temp_ind: f64,
    air_power_cap: f64,
    setpoint: f64,
) -> f64 {
    match algorithm {
        SolutionAlgorithm::ThirdOrder => {
            // 3rd-order backward difference:
            // (11/6)*C*T_new - C*(3*T[0] - 3/2*T[1] + 1/3*T[2]) = TempInd - TempDep*T_new + Load
            // Load = (11/6*C + TempDep)*T_sp - TempInd - C*(3*T[0] - 3/2*T[1] + 1/3*T[2])
            let history_term = air_power_cap
                * (3.0 * state.temp_history[0]
                    - 1.5 * state.temp_history[1]
                    + (1.0 / 3.0) * state.temp_history[2]);
            let total_dep = (11.0 / 6.0) * air_power_cap + temp_dep;
            total_dep * setpoint - temp_ind - history_term
        }
        SolutionAlgorithm::AnalyticalSolution => {
            // Analytical: T(t+dt) = (T_prev - TempInd/TempDep)*exp(-TempDep*dt/C) + TempInd/TempDep
            // With system load:
            // T_sp = (T_prev - (TempInd+Load)/TempDep_total) * exp(-TempDep_total/C) + (TempInd+Load)/TempDep_total
            // Simplification for load calculation:
            if temp_dep.abs() < 1e-10 {
                // No temperature-dependent terms: simple capacity
                air_power_cap * (setpoint - state.temp_history[0]) - temp_ind
            } else {
                let exp_term = (-temp_dep / air_power_cap.max(1e-10)).exp();
                temp_dep * (setpoint - state.temp_history[0] * exp_term) / (1.0 - exp_term) - temp_ind
            }
        }
        SolutionAlgorithm::EulerMethod => {
            // Euler: C*(T_new - T_prev) = TempInd - TempDep*T_new + Load
            // Load = (C + TempDep)*T_sp - C*T_prev - TempInd
            (air_power_cap + temp_dep) * setpoint - air_power_cap * state.temp_history[0] - temp_ind
        }
    }
}

/// Solve for zone temperature given a known system load.
fn solve_temperature(
    algorithm: SolutionAlgorithm,
    state: &ZoneAirState,
    temp_dep: f64,
    temp_ind: f64,
    air_power_cap: f64,
    system_load: f64,
) -> f64 {
    let total_ind = temp_ind + system_load;

    match algorithm {
        SolutionAlgorithm::ThirdOrder => {
            let history_term = air_power_cap
                * (3.0 * state.temp_history[0]
                    - 1.5 * state.temp_history[1]
                    + (1.0 / 3.0) * state.temp_history[2]);
            let denom = (11.0 / 6.0) * air_power_cap + temp_dep;
            if denom.abs() > 1e-10 {
                (total_ind + history_term) / denom
            } else {
                state.temperature
            }
        }
        SolutionAlgorithm::AnalyticalSolution => {
            if temp_dep.abs() < 1e-10 {
                state.temp_history[0] + total_ind / air_power_cap.max(1e-10)
            } else {
                let exp_term = (-temp_dep / air_power_cap.max(1e-10)).exp();
                (state.temp_history[0] - total_ind / temp_dep) * exp_term + total_ind / temp_dep
            }
        }
        SolutionAlgorithm::EulerMethod => {
            let denom = air_power_cap + temp_dep;
            if denom.abs() > 1e-10 {
                (air_power_cap * state.temp_history[0] + total_ind) / denom
            } else {
                state.temperature
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zone_air_power_cap;

    fn make_state(t: f64) -> ZoneAirState {
        ZoneAirState::new(t, 0.008)
    }

    fn make_coeffs(sum_ha: f64, sum_hat: f64, q_conv: f64) -> HeatBalanceCoefficients {
        HeatBalanceCoefficients {
            sum_ha,
            sum_hat_surf: sum_hat,
            sum_internal_convective: q_conv,
            ..Default::default()
        }
    }

    #[test]
    fn uncontrolled_zone_float() {
        let state = make_state(22.0);
        // Surfaces at 24C, HA=100 W/K, plus 500W gains
        let coeffs = make_coeffs(100.0, 2400.0, 500.0);
        let cap = zone_air_power_cap(300.0, 0.008, 1.2, 3600.0, 1.0);

        let result = predict_system_load(
            SolutionAlgorithm::ThirdOrder,
            ThermostatControl::Uncontrolled,
            &state,
            &coeffs,
            cap,
            20.0, 26.0,
        );

        assert!(result.in_deadband);
        assert!((result.load_to_heating_setpoint).abs() < 1e-10);
        // Temperature should rise due to gains + warm surfaces
        assert!(result.temperature > 22.0, "T={}", result.temperature);
    }

    #[test]
    fn heating_load_cold_room() {
        // Room at 15C, surfaces at 10C, no gains, setpoint 22C
        let state = make_state(15.0);
        let coeffs = make_coeffs(200.0, 2000.0, 0.0); // Surfaces at avg 10C
        let cap = zone_air_power_cap(300.0, 0.008, 1.2, 3600.0, 1.0);

        let result = predict_system_load(
            SolutionAlgorithm::ThirdOrder,
            ThermostatControl::SingleHeat,
            &state,
            &coeffs,
            cap,
            22.0, 26.0,
        );

        // Should need heating
        assert!(result.load_to_heating_setpoint > 0.0,
                "load={}", result.load_to_heating_setpoint);
    }

    #[test]
    fn cooling_load_warm_room() {
        // Room at 30C, surfaces at 32C, 2000W gains, setpoint 24C
        let state = make_state(30.0);
        let coeffs = make_coeffs(200.0, 6400.0, 2000.0);
        let cap = zone_air_power_cap(300.0, 0.008, 1.2, 3600.0, 1.0);

        let result = predict_system_load(
            SolutionAlgorithm::ThirdOrder,
            ThermostatControl::SingleCool,
            &state,
            &coeffs,
            cap,
            20.0, 24.0,
        );

        assert!(result.load_to_cooling_setpoint < 0.0,
                "load={}", result.load_to_cooling_setpoint);
    }

    #[test]
    fn deadband_dual_setpoint() {
        // Room at 22C, surfaces at 22C, moderate gains
        let state = make_state(22.0);
        let coeffs = make_coeffs(200.0, 4400.0, 300.0); // 200*22=4400, gains push slightly up
        let cap = zone_air_power_cap(300.0, 0.008, 1.2, 3600.0, 1.0);

        let result = predict_system_load(
            SolutionAlgorithm::ThirdOrder,
            ThermostatControl::DualSetPoint,
            &state,
            &coeffs,
            cap,
            20.0, 26.0,
        );

        // With surfaces at setpoint and small gains, likely in deadband
        // Load to heat setpoint (20C) should be negative (room already warm)
        // Load to cool setpoint (26C) should be positive (room cooler than 26)
        assert!(result.in_deadband, "heat={}, cool={}",
                result.load_to_heating_setpoint, result.load_to_cooling_setpoint);
    }

    #[test]
    fn algorithms_consistent_steady_state() {
        // All three methods should agree at steady state (history = current = setpoint)
        let state = make_state(22.0);
        let coeffs = make_coeffs(200.0, 4400.0, 0.0);
        let cap = zone_air_power_cap(300.0, 0.008, 1.2, 3600.0, 1.0);

        let load_3rd = load_to_setpoint(
            SolutionAlgorithm::ThirdOrder, &state, coeffs.temp_dep_coef(),
            coeffs.temp_ind_coef(), cap, 22.0,
        );
        let load_ana = load_to_setpoint(
            SolutionAlgorithm::AnalyticalSolution, &state, coeffs.temp_dep_coef(),
            coeffs.temp_ind_coef(), cap, 22.0,
        );
        let load_eul = load_to_setpoint(
            SolutionAlgorithm::EulerMethod, &state, coeffs.temp_dep_coef(),
            coeffs.temp_ind_coef(), cap, 22.0,
        );

        // At steady state (all history = 22, setpoint = 22, surfaces at 22C):
        // Load should be zero (or very small)
        assert!(load_3rd.abs() < 1.0, "3rd={load_3rd}");
        assert!(load_ana.abs() < 1.0, "ana={load_ana}");
        assert!(load_eul.abs() < 1.0, "eul={load_eul}");
    }
}
