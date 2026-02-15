//! Zone air heat balance predictor-corrector for EnergyPlus-rs.
//!
//! Implements the zone air energy balance with three solution algorithms:
//! - ThirdOrder: 3rd-order backward difference (default, most accurate)
//! - AnalyticalSolution: exact solution for linear ODE
//! - EulerMethod: simple forward Euler
//!
//! The fundamental equation:
//! C_zone * dT/dt = Q_conv_surfaces + Q_internal_gains + Q_infiltration
//!                + Q_ventilation + Q_mixing + Q_system + Q_non_air
//!
//! Predictor: estimates load required from HVAC
//! Corrector: solves for zone temperature after HVAC response

pub mod corrector;
pub mod predictor;

use ep_psychrometrics::cp_air;

/// Zone air solution algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SolutionAlgorithm {
    /// 3rd-order backward difference using 3 history values.
    #[default]
    ThirdOrder,
    /// Exact analytical solution for linear ODE using 1 prior value.
    AnalyticalSolution,
    /// Simple forward Euler using 1 prior value.
    EulerMethod,
}

/// Thermostat control type for a zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThermostatControl {
    /// No thermostat — zone floats freely.
    #[default]
    Uncontrolled,
    /// Heating-only thermostat.
    SingleHeat,
    /// Cooling-only thermostat.
    SingleCool,
    /// Heating or cooling (mutually exclusive).
    SingleHeatCool,
    /// Independent heating and cooling setpoints.
    DualSetPoint,
}

/// Zone air state at a point in time.
#[derive(Debug, Clone, Default)]
pub struct ZoneAirState {
    /// Mean air temperature (C).
    pub temperature: f64,
    /// Humidity ratio (kg/kg).
    pub humidity_ratio: f64,
    /// Temperature history for multi-step methods: [T(t-1), T(t-2), T(t-3)].
    pub temp_history: [f64; 3],
    /// Humidity ratio history.
    pub w_history: [f64; 3],
}

impl ZoneAirState {
    /// Create a new state at uniform initial conditions.
    pub fn new(temperature: f64, humidity_ratio: f64) -> Self {
        Self {
            temperature,
            humidity_ratio,
            temp_history: [temperature; 3],
            w_history: [humidity_ratio; 3],
        }
    }

    /// Push current values into history and update current.
    pub fn advance(&mut self, new_temp: f64, new_w: f64) {
        self.temp_history[2] = self.temp_history[1];
        self.temp_history[1] = self.temp_history[0];
        self.temp_history[0] = self.temperature;
        self.w_history[2] = self.w_history[1];
        self.w_history[1] = self.w_history[0];
        self.w_history[0] = self.humidity_ratio;
        self.temperature = new_temp;
        self.humidity_ratio = new_w;
    }
}

/// Heat balance coefficients collected from all sources.
///
/// The zone air energy balance is formulated as:
/// TempDepCoef * T_zone + AirPowerCap * dT/dt = TempIndCoef
///
/// Where:
/// - TempDepCoef (W/K): sum of Hc*A (surfaces) + MCp (airflows)
/// - TempIndCoef (W): sum of Hc*A*T_surf + MCp*T_source + Q_convective_gains
#[derive(Debug, Clone, Default)]
pub struct HeatBalanceCoefficients {
    /// Sum of Hc*Area for all surfaces (W/K).
    pub sum_ha: f64,
    /// Sum of Hc*Area*T_surface for all surfaces (W).
    pub sum_hat_surf: f64,
    /// Sum of convective internal gains (W).
    pub sum_internal_convective: f64,
    /// Sum of MCp for non-system airflows: infiltration, ventilation, mixing (W/K).
    pub sum_mcp: f64,
    /// Sum of MCp*T for non-system airflows (W).
    pub sum_mcpt: f64,
    /// Sum of MCp for system (HVAC supply) airflows (W/K).
    pub sum_sys_mcp: f64,
    /// Sum of MCp*T for system (HVAC supply) airflows (W).
    pub sum_sys_mcpt: f64,
    /// Non-air system response: radiant systems, etc. (W).
    pub non_air_system_response: f64,
}

impl HeatBalanceCoefficients {
    /// Temperature-dependent coefficient (W/K).
    /// This goes on the T_zone side of the equation.
    pub fn temp_dep_coef(&self) -> f64 {
        self.sum_ha + self.sum_mcp + self.sum_sys_mcp
    }

    /// Temperature-independent coefficient (W).
    /// This goes on the known/source side of the equation.
    pub fn temp_ind_coef(&self) -> f64 {
        self.sum_internal_convective
            + self.sum_hat_surf
            + self.sum_mcpt
            + self.sum_sys_mcpt
            + self.non_air_system_response
    }
}

/// Zone thermal capacity (C_zone = rho * V * Cp / dt).
pub fn zone_air_power_cap(
    zone_volume: f64,
    humidity_ratio: f64,
    air_density: f64,
    timestep_seconds: f64,
    capacity_multiplier: f64,
) -> f64 {
    let cp = cp_air(humidity_ratio);
    zone_volume * capacity_multiplier * air_density * cp / timestep_seconds
}

/// Result of the zone air balance: predicted load or corrected temperature.
#[derive(Debug, Clone, Default)]
pub struct ZoneBalanceResult {
    /// Zone air temperature (C).
    pub temperature: f64,
    /// System load required (W). Positive = heating needed.
    pub load_to_heating_setpoint: f64,
    /// System load required (W). Negative = cooling needed.
    pub load_to_cooling_setpoint: f64,
    /// Is zone in deadband (no heating or cooling needed)?
    pub in_deadband: bool,
}

/// High-level wrapper combining the predictor-corrector cycle.
///
/// Encapsulates the zone air solution algorithm, thermostat control type,
/// and zone thermal capacity for convenient use in the simulation driver.
#[derive(Debug, Clone)]
pub struct ZonePredictorCorrector {
    /// Solution algorithm for this zone.
    pub algorithm: SolutionAlgorithm,
    /// Thermostat control type.
    pub control: ThermostatControl,
    /// Zone thermal capacity C = rho*V*Cp/dt (W/K).
    pub air_power_cap: f64,
    /// Heating setpoint (C).
    pub heating_setpoint: f64,
    /// Cooling setpoint (C).
    pub cooling_setpoint: f64,
}

impl ZonePredictorCorrector {
    pub fn new(
        algorithm: SolutionAlgorithm,
        control: ThermostatControl,
        air_power_cap: f64,
    ) -> Self {
        Self {
            algorithm,
            control,
            air_power_cap,
            heating_setpoint: 20.0,
            cooling_setpoint: 26.0,
        }
    }

    /// Predictor step: estimate the system load required to meet setpoints.
    pub fn predict(
        &self,
        state: &ZoneAirState,
        coeffs: &HeatBalanceCoefficients,
    ) -> ZoneBalanceResult {
        predictor::predict_system_load(
            self.algorithm,
            self.control,
            state,
            coeffs,
            self.air_power_cap,
            self.heating_setpoint,
            self.cooling_setpoint,
        )
    }

    /// Corrector step: solve for actual zone temperature after HVAC response,
    /// advance zone air state history.
    pub fn correct(
        &self,
        zone_state: &mut ZoneAirState,
        coeffs: &HeatBalanceCoefficients,
    ) -> corrector::CorrectorResult {
        let new_temp = corrector::correct_zone_temperature(
            self.algorithm,
            zone_state,
            coeffs,
            self.air_power_cap,
        );

        // Keep humidity constant for simplified model (no latent loads)
        let new_w = zone_state.humidity_ratio;

        // Calculate sensible load met
        let cp = cp_air(zone_state.humidity_ratio);
        let sys_mass_flow = if cp > 0.0 {
            coeffs.sum_sys_mcp / cp
        } else {
            0.0
        };
        let supply_temp = if coeffs.sum_sys_mcp > 1e-10 {
            coeffs.sum_sys_mcpt / coeffs.sum_sys_mcp
        } else {
            new_temp
        };
        let sensible_load = corrector::calculate_sensible_load_met(
            sys_mass_flow,
            supply_temp,
            new_temp,
            zone_state.humidity_ratio,
            coeffs.non_air_system_response,
        );

        // Advance state history
        zone_state.advance(new_temp, new_w);

        corrector::CorrectorResult {
            temperature: new_temp,
            humidity_ratio: new_w,
            sensible_load_met: sensible_load,
            latent_load_met: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_air_state_advance() {
        let mut state = ZoneAirState::new(22.0, 0.008);
        state.advance(23.0, 0.009);

        assert!((state.temperature - 23.0).abs() < 1e-10);
        assert!((state.temp_history[0] - 22.0).abs() < 1e-10);
        assert!((state.temp_history[1] - 22.0).abs() < 1e-10);

        state.advance(24.0, 0.010);
        assert!((state.temperature - 24.0).abs() < 1e-10);
        assert!((state.temp_history[0] - 23.0).abs() < 1e-10);
        assert!((state.temp_history[1] - 22.0).abs() < 1e-10);
    }

    #[test]
    fn heat_balance_coefficients() {
        let hb = HeatBalanceCoefficients {
            sum_ha: 100.0,
            sum_hat_surf: 2200.0,
            sum_internal_convective: 500.0,
            sum_mcp: 50.0,
            sum_mcpt: 1000.0,
            sum_sys_mcp: 200.0,
            sum_sys_mcpt: 4000.0,
            non_air_system_response: 0.0,
        };

        assert!((hb.temp_dep_coef() - 350.0).abs() < 1e-10);
        assert!((hb.temp_ind_coef() - 7700.0).abs() < 1e-10);
    }

    #[test]
    fn zone_air_power_cap_value() {
        // 300 m3, rho~1.2, Cp~1005, dt=3600s
        let cap = zone_air_power_cap(300.0, 0.008, 1.2, 3600.0, 1.0);
        // cap = 300 * 1.0 * 1.2 * 1005 / 3600 ≈ 100.5 W/K
        assert!(cap > 90.0 && cap < 110.0, "cap={cap}");
    }

    #[test]
    fn predictor_corrector_wrapper_uncontrolled() {
        let pc = ZonePredictorCorrector::new(
            SolutionAlgorithm::ThirdOrder,
            ThermostatControl::Uncontrolled,
            100.0, // ~100 W/K
        );
        let state = ZoneAirState::new(22.0, 0.008);
        let coeffs = HeatBalanceCoefficients {
            sum_ha: 200.0,
            sum_hat_surf: 4800.0, // surfaces at 24C
            sum_internal_convective: 500.0,
            ..Default::default()
        };

        let result = pc.predict(&state, &coeffs);
        assert!(result.in_deadband);
        // Temperature should be above 22 due to warm surfaces + gains
        assert!(result.temperature > 22.0, "T={}", result.temperature);
    }

    #[test]
    fn predictor_corrector_wrapper_correct_advances_state() {
        let pc = ZonePredictorCorrector::new(
            SolutionAlgorithm::ThirdOrder,
            ThermostatControl::DualSetPoint,
            100.0,
        );
        let mut state = ZoneAirState::new(22.0, 0.008);
        let coeffs = HeatBalanceCoefficients {
            sum_ha: 200.0,
            sum_hat_surf: 4400.0,
            sum_internal_convective: 0.0,
            sum_sys_mcp: 100.0,
            sum_sys_mcpt: 2200.0, // system at 22C
            ..Default::default()
        };

        let result = pc.correct(&mut state, &coeffs);
        // State should have been advanced
        assert!((state.temp_history[0] - 22.0).abs() < 0.01);
        assert!((result.temperature - state.temperature).abs() < 1e-10);
    }

    #[test]
    fn predictor_corrector_wrapper_setpoints() {
        let mut pc = ZonePredictorCorrector::new(
            SolutionAlgorithm::ThirdOrder,
            ThermostatControl::SingleHeat,
            100.0,
        );
        pc.heating_setpoint = 21.0;
        pc.cooling_setpoint = 25.0;

        // Cold room with cold surfaces
        let state = ZoneAirState::new(15.0, 0.008);
        let coeffs = HeatBalanceCoefficients {
            sum_ha: 200.0,
            sum_hat_surf: 2000.0, // surfaces at 10C
            ..Default::default()
        };

        let result = pc.predict(&state, &coeffs);
        // Should need heating to reach 21C
        assert!(result.load_to_heating_setpoint > 0.0,
            "load={}", result.load_to_heating_setpoint);
    }

    #[test]
    fn predictor_corrector_wrapper_default_setpoints() {
        let pc = ZonePredictorCorrector::new(
            SolutionAlgorithm::ThirdOrder,
            ThermostatControl::DualSetPoint,
            100.0,
        );
        assert!((pc.heating_setpoint - 20.0).abs() < 1e-10);
        assert!((pc.cooling_setpoint - 26.0).abs() < 1e-10);
    }
}
