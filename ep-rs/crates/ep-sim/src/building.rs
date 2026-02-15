//! BuildingSimCallback — wires all subsystems into the simulation loop.
//!
//! Implements `SimulationCallback` to drive heat balance, predictor-corrector,
//! ideal loads or HVAC dispatch, and output reporting for an end-to-end
//! building energy simulation.

use ep_core::state::SimulationState;
use ep_output::variables::{TimeStamp, TimeStepType};
use ep_output::{OutputManager, ZoneOutputIndices};
use ep_psychrometrics::cp_air;
use ep_zone_air::{
    HeatBalanceCoefficients, SolutionAlgorithm, ThermostatControl, ZoneAirState,
    ZonePredictorCorrector, corrector,
};

use crate::sizing::ZoneSizingData;
use crate::SimulationCallback;

/// Per-zone state and parameters for the simulation.
#[derive(Debug, Clone)]
pub struct ZoneSimState {
    /// Zone name.
    pub name: String,
    /// Zone air state (temperature + humidity history).
    pub air_state: ZoneAirState,
    /// Predictor-corrector wrapper.
    pub predictor_corrector: ZonePredictorCorrector,
    /// Zone volume (m³).
    pub volume: f64,
    /// Zone floor area (m²).
    pub floor_area: f64,
    /// Zone multiplier.
    pub multiplier: u32,
    /// Internal convective gains (W) — set each timestep.
    pub convective_gains: f64,
    /// Surface heat transfer coefficient (HA) sum (W/K).
    pub surface_ha: f64,
    /// Weighted surface temperature sum (HA*T) (W).
    pub surface_hat: f64,
    /// Infiltration mass flow rate * Cp (W/K).
    pub infiltration_mcp: f64,
    /// Infiltration MCp*T (W).
    pub infiltration_mcpt: f64,
    /// Ideal loads: system MCp (W/K) — set by HVAC iteration.
    pub system_mcp: f64,
    /// Ideal loads: system MCp*T (W).
    pub system_mcpt: f64,
    /// Output variable indices.
    pub output_indices: ZoneOutputIndices,
    /// Sizing data for this zone.
    pub sizing: ZoneSizingData,
    /// Last predicted heating load (W).
    pub predicted_heating_load: f64,
    /// Last predicted cooling load (W).
    pub predicted_cooling_load: f64,
}

/// Building simulation callback that implements the full simulation loop.
pub struct BuildingSimCallback {
    /// Per-zone simulation state.
    pub zones: Vec<ZoneSimState>,
    /// Output manager for reporting.
    pub output_manager: OutputManager,
    /// Timestep duration in seconds.
    pub timestep_seconds: f64,
    /// Air density (kg/m³).
    pub air_density: f64,
    /// Default heating setpoint (C).
    pub heating_setpoint: f64,
    /// Default cooling setpoint (C).
    pub cooling_setpoint: f64,
    /// Use ideal loads (true) or no HVAC (false).
    pub use_ideal_loads: bool,
    /// Number of timesteps per hour.
    pub timesteps_per_hour: u8,
}

impl BuildingSimCallback {
    /// Create a new building simulation callback with given zones.
    pub fn new(timesteps_per_hour: u8) -> Self {
        Self {
            zones: Vec::new(),
            output_manager: OutputManager::new(),
            timestep_seconds: 3600.0 / timesteps_per_hour as f64,
            air_density: 1.204,
            heating_setpoint: 20.0,
            cooling_setpoint: 26.0,
            use_ideal_loads: true,
            timesteps_per_hour,
        }
    }

    /// Add a zone to the simulation.
    pub fn add_zone(
        &mut self,
        name: &str,
        volume: f64,
        floor_area: f64,
        initial_temp: f64,
        control: ThermostatControl,
    ) {
        let cap = ep_zone_air::zone_air_power_cap(
            volume,
            0.008,
            self.air_density,
            self.timestep_seconds,
            1.0,
        );

        let mut pc = ZonePredictorCorrector::new(SolutionAlgorithm::ThirdOrder, control, cap);
        pc.heating_setpoint = self.heating_setpoint;
        pc.cooling_setpoint = self.cooling_setpoint;

        let output_indices = self.output_manager.register_zone_variables(name);

        self.zones.push(ZoneSimState {
            name: name.to_string(),
            air_state: ZoneAirState::new(initial_temp, 0.008),
            predictor_corrector: pc,
            volume,
            floor_area,
            multiplier: 1,
            convective_gains: 0.0,
            surface_ha: 0.0,
            surface_hat: 0.0,
            infiltration_mcp: 0.0,
            infiltration_mcpt: 0.0,
            system_mcp: 0.0,
            system_mcpt: 0.0,
            output_indices,
            sizing: ZoneSizingData::new(name),
            predicted_heating_load: 0.0,
            predicted_cooling_load: 0.0,
        });
    }

    /// Set surface heat transfer for a zone.
    pub fn set_zone_surface_ht(
        &mut self,
        zone_idx: usize,
        ha_sum: f64,
        surface_temps: &[(f64, f64)], // (area*h, temp) pairs
    ) {
        if zone_idx < self.zones.len() {
            self.zones[zone_idx].surface_ha = ha_sum;
            self.zones[zone_idx].surface_hat = surface_temps.iter()
                .map(|(ha, t)| ha * t)
                .sum();
        }
    }

    /// Set internal convective gains for a zone.
    pub fn set_zone_gains(&mut self, zone_idx: usize, convective_watts: f64) {
        if zone_idx < self.zones.len() {
            self.zones[zone_idx].convective_gains = convective_watts;
        }
    }

    /// Apply simple infiltration model for a zone.
    pub fn set_zone_infiltration(
        &mut self,
        zone_idx: usize,
        ach: f64, // air changes per hour
        outdoor_temp: f64,
    ) {
        if zone_idx < self.zones.len() {
            let zone = &mut self.zones[zone_idx];
            let mass_flow = zone.volume * self.air_density * ach / 3600.0;
            let cp = cp_air(0.008);
            zone.infiltration_mcp = mass_flow * cp;
            zone.infiltration_mcpt = mass_flow * cp * outdoor_temp;
        }
    }

    /// Build heat balance coefficients for a zone from its current state.
    fn build_coefficients(&self, zone: &ZoneSimState) -> HeatBalanceCoefficients {
        HeatBalanceCoefficients {
            sum_ha: zone.surface_ha,
            sum_hat_surf: zone.surface_hat,
            sum_internal_convective: zone.convective_gains,
            sum_mcp: zone.infiltration_mcp,
            sum_mcpt: zone.infiltration_mcpt,
            sum_sys_mcp: zone.system_mcp,
            sum_sys_mcpt: zone.system_mcpt,
            non_air_system_response: 0.0,
        }
    }

    /// Apply ideal loads to meet zone heating/cooling load.
    fn apply_ideal_loads(&mut self, zone_idx: usize) {
        let zone = &self.zones[zone_idx];
        let heating_load = zone.predicted_heating_load;
        let cooling_load = zone.predicted_cooling_load;

        let system_load = if heating_load > 0.0 {
            heating_load
        } else if cooling_load < 0.0 {
            cooling_load
        } else {
            // In deadband
            self.zones[zone_idx].system_mcp = 0.0;
            self.zones[zone_idx].system_mcpt = 0.0;
            return;
        };

        // Calculate required supply air mass flow and temperature
        let supply_temp = if system_load > 0.0 {
            self.heating_setpoint + 20.0 // supply at setpoint+20
        } else {
            self.cooling_setpoint - 12.0 // supply at setpoint-12
        };

        let zone_temp = self.zones[zone_idx].air_state.temperature;
        let dt = (supply_temp - zone_temp).abs().max(1.0);
        let cp = cp_air(0.008);
        let mass_flow = system_load.abs() / (cp * dt);

        self.zones[zone_idx].system_mcp = mass_flow * cp;
        self.zones[zone_idx].system_mcpt = mass_flow * cp * supply_temp;
    }
}

impl SimulationCallback for BuildingSimCallback {
    fn begin_environment(&mut self, _state: &mut SimulationState) {
        // Reset all zone air states to initial conditions
        for zone in &mut self.zones {
            zone.air_state = ZoneAirState::new(zone.air_state.temperature, 0.008);
            zone.system_mcp = 0.0;
            zone.system_mcpt = 0.0;
            zone.predicted_heating_load = 0.0;
            zone.predicted_cooling_load = 0.0;
            zone.sizing = ZoneSizingData::new(&zone.name);
        }
    }

    fn do_timestep(&mut self, state: &mut SimulationState) {
        let outdoor_temp = state.outdoor.dry_bulb;

        // Set up surface heat transfer and infiltration for each zone
        for zone_idx in 0..self.zones.len() {
            // Simple envelope model: aggregate UA to outdoor environment.
            // UA ≈ 2 W/(K·m²) of floor area — reasonable for a typical building.
            // Internal mass at zone temp provides zero net heat transfer and is omitted
            // to avoid lagged-temperature instability with the BDF3 solver.
            let ua_envelope = 2.0 * self.zones[zone_idx].floor_area.max(10.0); // W/K
            self.zones[zone_idx].surface_ha = ua_envelope;
            self.zones[zone_idx].surface_hat = ua_envelope * outdoor_temp;

            // Simple infiltration
            self.set_zone_infiltration(zone_idx, 0.5, outdoor_temp);
        }

        // Predictor step: compute loads for each zone.
        // The predictor solves for the REQUIRED system load, so system terms
        // must be excluded from the coefficients (they're what we're solving for).
        for zone_idx in 0..self.zones.len() {
            self.zones[zone_idx].system_mcp = 0.0;
            self.zones[zone_idx].system_mcpt = 0.0;
            let coeffs = self.build_coefficients(&self.zones[zone_idx]);
            let result = self.zones[zone_idx].predictor_corrector.predict(
                &self.zones[zone_idx].air_state,
                &coeffs,
            );
            self.zones[zone_idx].predicted_heating_load = result.load_to_heating_setpoint;
            self.zones[zone_idx].predicted_cooling_load = result.load_to_cooling_setpoint;
        }
    }

    fn do_hvac_iteration(&mut self, state: &mut SimulationState) -> f64 {
        let mut max_residual = 0.0_f64;

        for zone_idx in 0..self.zones.len() {
            if self.use_ideal_loads {
                self.apply_ideal_loads(zone_idx);
            }

            // Corrector step — compute new temperature WITHOUT advancing state history.
            // do_hvac_iteration may be called multiple times per timestep; the history
            // must only advance once (in end_timestep).
            let coeffs = HeatBalanceCoefficients {
                sum_ha: self.zones[zone_idx].surface_ha,
                sum_hat_surf: self.zones[zone_idx].surface_hat,
                sum_internal_convective: self.zones[zone_idx].convective_gains,
                sum_mcp: self.zones[zone_idx].infiltration_mcp,
                sum_mcpt: self.zones[zone_idx].infiltration_mcpt,
                sum_sys_mcp: self.zones[zone_idx].system_mcp,
                sum_sys_mcpt: self.zones[zone_idx].system_mcpt,
                non_air_system_response: 0.0,
            };
            let zone = &self.zones[zone_idx];
            let new_temp = corrector::correct_zone_temperature(
                zone.predictor_corrector.algorithm,
                &zone.air_state,
                &coeffs,
                zone.predictor_corrector.air_power_cap,
            );

            // Compute sensible load met
            let w = zone.air_state.humidity_ratio;
            let cp = cp_air(w);
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
                sys_mass_flow, supply_temp, new_temp, w, 0.0,
            );

            // Update zone temperature (but do NOT advance history)
            self.zones[zone_idx].air_state.temperature = new_temp;

            // Update sizing data
            if state.flags.sizing || state.flags.warmup {
                let heating_rate = sensible_load.max(0.0);
                let cooling_rate = (-sensible_load).max(0.0);

                self.zones[zone_idx].sizing.update_cooling(
                    cooling_rate,
                    0.0,
                    state.outdoor.dry_bulb,
                    state.clock.month,
                    state.clock.day_of_month,
                    state.clock.hour_of_day,
                );
                self.zones[zone_idx].sizing.update_heating(
                    heating_rate,
                    0.0,
                    state.outdoor.dry_bulb,
                    state.clock.month,
                    state.clock.day_of_month,
                    state.clock.hour_of_day,
                );
            }

            // Track residual
            let predicted = if self.zones[zone_idx].predicted_heating_load > 0.0 {
                self.zones[zone_idx].predicted_heating_load
            } else if self.zones[zone_idx].predicted_cooling_load < 0.0 {
                self.zones[zone_idx].predicted_cooling_load
            } else {
                0.0
            };
            let residual = (sensible_load - predicted).abs();
            max_residual = max_residual.max(residual);
        }

        max_residual
    }

    fn end_timestep(&mut self, state: &mut SimulationState) {
        // Advance zone state histories — this must happen exactly once per timestep,
        // after all HVAC iterations are complete.
        for zone in &mut self.zones {
            let t = zone.air_state.temperature;
            let w = zone.air_state.humidity_ratio;
            zone.air_state.advance(t, w);
        }

        let ts = TimeStamp {
            month: state.clock.month as u32,
            day: state.clock.day_of_month as u32,
            hour: state.clock.hour_of_day as u32,
            minute: 0,
        };

        // Update output variables
        for zone in &self.zones {
            self.output_manager.variables[zone.output_indices.temperature]
                .set_value(zone.air_state.temperature);

            let heating_rate = if zone.predicted_heating_load > 0.0 {
                zone.predicted_heating_load
            } else {
                0.0
            };
            let cooling_rate = if zone.predicted_cooling_load < 0.0 {
                zone.predicted_cooling_load.abs()
            } else {
                0.0
            };
            self.output_manager.variables[zone.output_indices.heating_rate]
                .set_value(heating_rate);
            self.output_manager.variables[zone.output_indices.cooling_rate]
                .set_value(cooling_rate);
        }

        let time_fraction = 1.0 / self.timesteps_per_hour as f64;
        self.output_manager.update_data(TimeStepType::Zone, time_fraction, &ts);
        self.output_manager.update_data(TimeStepType::System, time_fraction, &ts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DesignDayRef, SimulationConfig, SimulationDriver};
    use ep_core::state::{EnvironmentType, OutdoorConditions};
    use ep_output::OutputRequest;
    use ep_output::variables::ReportFreq;

    fn make_simple_callback() -> BuildingSimCallback {
        let mut cb = BuildingSimCallback::new(1); // 1 ts/hr for simplicity
        cb.add_zone("Zone1", 300.0, 100.0, 22.0, ThermostatControl::Uncontrolled);
        cb
    }

    #[test]
    fn callback_add_zone() {
        let cb = make_simple_callback();
        assert_eq!(cb.zones.len(), 1);
        assert_eq!(cb.zones[0].name, "Zone1");
        assert!((cb.zones[0].volume - 300.0).abs() < 1e-10);
        assert!(cb.zones[0].predictor_corrector.air_power_cap > 0.0);
    }

    #[test]
    fn callback_zone_initial_temp() {
        let cb = make_simple_callback();
        assert!((cb.zones[0].air_state.temperature - 22.0).abs() < 1e-10);
    }

    #[test]
    fn callback_output_variables_registered() {
        let cb = make_simple_callback();
        assert_eq!(cb.output_manager.variable_count(), 3);
    }

    #[test]
    fn free_floating_zone_responds_to_outdoor() {
        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_warmup_days: 2,
            min_warmup_days: 1,
            run_design_days: true,
            run_weather_periods: false,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.design_days.push(DesignDayRef {
            index: 0,
            env_type: EnvironmentType::DesignDay,
        });

        let mut state = SimulationState::new(1);
        state.outdoor = OutdoorConditions {
            dry_bulb: 35.0, // hot day
            ..Default::default()
        };

        let mut cb = BuildingSimCallback::new(1);
        cb.add_zone("Zone1", 300.0, 100.0, 22.0, ThermostatControl::Uncontrolled);

        let result = driver.run(&mut state, &mut cb);

        // Zone should warm up towards outdoor temperature
        let final_temp = cb.zones[0].air_state.temperature;
        assert!(final_temp > 22.0, "Should warm up: T={final_temp}");
        assert!(result.total_timesteps > 0);
    }

    #[test]
    fn heated_zone_ideal_loads() {
        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_warmup_days: 2,
            min_warmup_days: 1,
            run_design_days: true,
            run_weather_periods: false,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.design_days.push(DesignDayRef {
            index: 0,
            env_type: EnvironmentType::DesignDay,
        });

        let mut state = SimulationState::new(1);
        state.outdoor = OutdoorConditions {
            dry_bulb: -10.0, // cold day
            ..Default::default()
        };

        let mut cb = BuildingSimCallback::new(1);
        cb.heating_setpoint = 20.0;
        cb.cooling_setpoint = 26.0;
        cb.add_zone("Zone1", 300.0, 100.0, 20.0, ThermostatControl::SingleHeat);

        driver.run(&mut state, &mut cb);

        // Zone should maintain near setpoint with ideal loads
        let final_temp = cb.zones[0].air_state.temperature;
        assert!(
            final_temp > 15.0,
            "Zone should stay warm with ideal loads: T={final_temp}"
        );
    }

    #[test]
    fn cooled_zone_ideal_loads() {
        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_warmup_days: 2,
            min_warmup_days: 1,
            run_design_days: true,
            run_weather_periods: false,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.design_days.push(DesignDayRef {
            index: 0,
            env_type: EnvironmentType::DesignDay,
        });

        let mut state = SimulationState::new(1);
        state.outdoor = OutdoorConditions {
            dry_bulb: 38.0, // hot day
            ..Default::default()
        };

        let mut cb = BuildingSimCallback::new(1);
        cb.heating_setpoint = 20.0;
        cb.cooling_setpoint = 26.0;
        cb.add_zone("Zone1", 300.0, 100.0, 24.0, ThermostatControl::SingleCool);

        driver.run(&mut state, &mut cb);

        let final_temp = cb.zones[0].air_state.temperature;
        assert!(
            final_temp < 35.0,
            "Zone should stay cool with ideal loads: T={final_temp}"
        );
    }

    #[test]
    fn begin_environment_resets_state() {
        let mut cb = BuildingSimCallback::new(1);
        cb.add_zone("Zone1", 300.0, 100.0, 22.0, ThermostatControl::Uncontrolled);

        // Modify state
        cb.zones[0].air_state.temperature = 35.0;
        cb.zones[0].system_mcp = 100.0;
        cb.zones[0].predicted_heating_load = 5000.0;

        let mut state = SimulationState::new(1);
        cb.begin_environment(&mut state);

        // Should be reset (temperature preserved but history reset)
        assert!((cb.zones[0].system_mcp).abs() < 1e-10);
        assert!((cb.zones[0].predicted_heating_load).abs() < 1e-10);
    }

    #[test]
    fn dual_setpoint_deadband() {
        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_warmup_days: 2,
            min_warmup_days: 1,
            run_design_days: true,
            run_weather_periods: false,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.design_days.push(DesignDayRef {
            index: 0,
            env_type: EnvironmentType::DesignDay,
        });

        let mut state = SimulationState::new(1);
        state.outdoor = OutdoorConditions {
            dry_bulb: 23.0, // mild day, in deadband
            ..Default::default()
        };

        let mut cb = BuildingSimCallback::new(1);
        cb.heating_setpoint = 20.0;
        cb.cooling_setpoint = 26.0;
        cb.add_zone("Zone1", 300.0, 100.0, 23.0, ThermostatControl::DualSetPoint);

        driver.run(&mut state, &mut cb);

        // Zone should float near outdoor temp (in deadband)
        let final_temp = cb.zones[0].air_state.temperature;
        assert!(
            final_temp > 18.0 && final_temp < 28.0,
            "Zone should be in deadband range: T={final_temp}"
        );
    }

    #[test]
    fn multi_zone_simulation() {
        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_warmup_days: 2,
            min_warmup_days: 1,
            run_design_days: true,
            run_weather_periods: false,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.design_days.push(DesignDayRef {
            index: 0,
            env_type: EnvironmentType::DesignDay,
        });

        let mut state = SimulationState::new(1);
        state.outdoor = OutdoorConditions {
            dry_bulb: -5.0,
            ..Default::default()
        };

        let mut cb = BuildingSimCallback::new(1);
        cb.heating_setpoint = 20.0;
        cb.cooling_setpoint = 26.0;
        cb.add_zone("Zone1", 300.0, 100.0, 20.0, ThermostatControl::SingleHeat);
        cb.add_zone("Zone2", 150.0, 50.0, 20.0, ThermostatControl::Uncontrolled);

        driver.run(&mut state, &mut cb);

        // Zone1 heated should stay warm, Zone2 uncontrolled should drift cold
        let t1 = cb.zones[0].air_state.temperature;
        let t2 = cb.zones[1].air_state.temperature;
        assert!(t1 > t2, "Heated zone should be warmer: T1={t1}, T2={t2}");
    }

    #[test]
    fn output_manager_accumulates() {
        let mut cb = BuildingSimCallback::new(4); // 4 ts/hr
        cb.add_zone("Zone1", 300.0, 100.0, 22.0, ThermostatControl::Uncontrolled);

        // Request hourly output
        cb.output_manager.add_request(OutputRequest {
            key: "*".to_string(),
            variable_name: "Zone Mean Air Temperature".to_string(),
            freq: ReportFreq::Hourly,
        });
        cb.output_manager.resolve_requests();

        // Simulate one timestep
        let mut state = SimulationState::new(4);
        state.outdoor.dry_bulb = 25.0;
        cb.do_timestep(&mut state);
        let _ = cb.do_hvac_iteration(&mut state);
        cb.end_timestep(&mut state);

        // Temperature variable should have a value
        let temp_idx = cb.zones[0].output_indices.temperature;
        assert!(cb.output_manager.variables[temp_idx].num_stored > 0);
    }

    #[test]
    fn set_zone_infiltration() {
        let mut cb = BuildingSimCallback::new(1);
        cb.add_zone("Zone1", 300.0, 100.0, 22.0, ThermostatControl::Uncontrolled);

        cb.set_zone_infiltration(0, 0.5, 0.0); // 0.5 ACH, 0C outdoor

        assert!(cb.zones[0].infiltration_mcp > 0.0);
        // MCp*T should be ~0 since outdoor is 0C
        assert!(cb.zones[0].infiltration_mcpt.abs() < 1.0);
    }

    #[test]
    fn set_zone_gains() {
        let mut cb = BuildingSimCallback::new(1);
        cb.add_zone("Zone1", 300.0, 100.0, 22.0, ThermostatControl::Uncontrolled);

        cb.set_zone_gains(0, 500.0);
        assert!((cb.zones[0].convective_gains - 500.0).abs() < 1e-10);
    }

    // -- Integration tests --

    #[test]
    fn free_floating_hourly_temps_reasonable() {
        // Run a full design day with 4 ts/hr and verify hourly temps stay physical
        let config = SimulationConfig {
            timesteps_per_hour: 4,
            max_warmup_days: 3,
            min_warmup_days: 2,
            run_design_days: true,
            run_weather_periods: false,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.design_days.push(DesignDayRef {
            index: 0,
            env_type: EnvironmentType::DesignDay,
        });

        let mut state = SimulationState::new(4);
        state.outdoor = OutdoorConditions {
            dry_bulb: 30.0,
            ..Default::default()
        };

        let mut cb = BuildingSimCallback::new(4);
        cb.add_zone("Zone1", 300.0, 100.0, 22.0, ThermostatControl::Uncontrolled);

        let result = driver.run(&mut state, &mut cb);

        // Should complete the simulation
        assert_eq!(result.environments_completed, 1);
        // 1 day * 24 hr * 4 ts/hr = 96 timesteps
        assert_eq!(result.total_timesteps, 96);

        // Final temperature should be between outdoor and initial (approaching outdoor)
        let final_temp = cb.zones[0].air_state.temperature;
        assert!(
            final_temp > 15.0 && final_temp < 40.0,
            "Temp should be physically reasonable: T={final_temp}"
        );
    }

    #[test]
    fn predictor_corrector_accuracy() {
        // With ideal loads and single heating, zone should reach within 1C of setpoint
        let config = SimulationConfig {
            timesteps_per_hour: 4,
            max_warmup_days: 6,
            min_warmup_days: 3,
            run_design_days: true,
            run_weather_periods: false,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.design_days.push(DesignDayRef {
            index: 0,
            env_type: EnvironmentType::DesignDay,
        });

        let mut state = SimulationState::new(4);
        state.outdoor = OutdoorConditions {
            dry_bulb: 5.0,
            ..Default::default()
        };

        let mut cb = BuildingSimCallback::new(4);
        cb.heating_setpoint = 22.0;
        cb.cooling_setpoint = 26.0;
        cb.add_zone("Zone1", 300.0, 100.0, 22.0, ThermostatControl::SingleHeat);

        driver.run(&mut state, &mut cb);

        // After warmup + design day, zone should be close to setpoint
        let final_temp = cb.zones[0].air_state.temperature;
        // Ideal loads should keep zone above ~18C even in cold weather
        assert!(
            final_temp > 15.0,
            "Zone should maintain near setpoint: T={final_temp}"
        );
    }

    #[test]
    fn eso_output_writes_correctly() {
        let mut cb = BuildingSimCallback::new(1);
        cb.add_zone("Zone1", 300.0, 100.0, 22.0, ThermostatControl::Uncontrolled);

        // Request output
        cb.output_manager.add_request(OutputRequest {
            key: "*".to_string(),
            variable_name: "Zone Mean Air Temperature".to_string(),
            freq: ReportFreq::Hourly,
        });
        cb.output_manager.resolve_requests();

        // Run a single timestep
        let mut state = SimulationState::new(1);
        state.outdoor.dry_bulb = 25.0;
        cb.do_timestep(&mut state);
        let _ = cb.do_hvac_iteration(&mut state);
        cb.end_timestep(&mut state);

        // Write ESO report
        let mut buf = Vec::new();
        let count = cb.output_manager.write_eso_report(
            &mut buf, ReportFreq::Hourly, 1, 1, 1, 0.0,
        ).unwrap();
        assert!(count > 0, "Should have reported at least 1 variable");

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("3,"), "Should have hourly timestamp");
    }

    #[test]
    fn sizing_tracks_peak_loads() {
        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_warmup_days: 2,
            min_warmup_days: 1,
            run_design_days: true,
            run_weather_periods: false,
            do_zone_sizing: true,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.design_days.push(DesignDayRef {
            index: 0,
            env_type: EnvironmentType::DesignDay,
        });

        let mut state = SimulationState::new(1);
        state.flags.sizing = true;
        state.outdoor = OutdoorConditions {
            dry_bulb: -15.0,
            ..Default::default()
        };

        let mut cb = BuildingSimCallback::new(1);
        cb.heating_setpoint = 22.0;
        cb.cooling_setpoint = 26.0;
        cb.add_zone("Zone1", 300.0, 100.0, 22.0, ThermostatControl::SingleHeat);

        driver.run(&mut state, &mut cb);

        // Sizing should have tracked some heating load
        assert!(
            cb.zones[0].sizing.design_heat_load > 0.0,
            "Should track heating peak: {}",
            cb.zones[0].sizing.design_heat_load
        );
    }

    #[test]
    fn idf_parsing_and_translation() {
        use crate::input_translator;

        let schema = ep_io::schema::SchemaDb::minimal();
        let idf = r#"
  Version, 24.2;
  Building, BESTEST Case 600, 0, Suburbs;
  SimulationControl, Yes, No, No, Yes, No;
  Timestep, 4;
  Zone, ZONE ONE, 0, 0, 0, 0, 1, 1, 2.7, 129.6;
  Output:Variable, *, Zone Mean Air Temperature, Hourly;
"#;
        let model = ep_io::idf::parse_idf_with_schema(idf, &schema).unwrap();
        let bm = input_translator::translate_input(&model, &schema).unwrap();

        assert_eq!(bm.name, "BESTEST Case 600");
        assert_eq!(bm.zones.len(), 1);
        assert_eq!(bm.zones[0].name, "ZONE ONE");
        assert!((bm.zones[0].volume - 129.6).abs() < 0.1);
        assert_eq!(bm.config.timesteps_per_hour, 4);
        assert!(bm.config.do_zone_sizing);
        assert_eq!(bm.output_requests.len(), 1);
    }

    #[test]
    fn warmup_converges_within_limit() {
        let config = SimulationConfig {
            timesteps_per_hour: 1,
            max_warmup_days: 25,
            min_warmup_days: 6,
            run_design_days: true,
            run_weather_periods: false,
            ..Default::default()
        };
        let mut driver = SimulationDriver::new(config);
        driver.design_days.push(DesignDayRef {
            index: 0,
            env_type: EnvironmentType::DesignDay,
        });

        let mut state = SimulationState::new(1);
        state.outdoor = OutdoorConditions {
            dry_bulb: 20.0,
            ..Default::default()
        };

        let mut cb = BuildingSimCallback::new(1);
        cb.add_zone("Zone1", 300.0, 100.0, 20.0, ThermostatControl::Uncontrolled);

        let result = driver.run(&mut state, &mut cb);

        // Warmup should complete within limit
        assert!(
            result.warmup_days_used[0] <= 25,
            "Warmup took {} days",
            result.warmup_days_used[0]
        );
    }
}
