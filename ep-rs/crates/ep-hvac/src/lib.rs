//! HVAC air-side system models for EnergyPlus-rs.
//!
//! Modules:
//! - `air_loop`: Air loop (AHU) topology and control
//! - `zone_equipment`: Zone-level HVAC equipment (terminal units, baseboard)
//! - `unitary`: Unitary systems (packaged DX, heat pumps, furnaces)
//! - `setpoint`: Setpoint manager models
//! - `controller`: Air-side controller logic (OA, coil)

pub mod air_loop;
pub mod controller;
pub mod setpoint;
pub mod unitary;
pub mod vrf;
pub mod zone_equipment;

// ---------------------------------------------------------------------------
// Integration tests — cross-module scenarios
// ---------------------------------------------------------------------------
#[cfg(test)]
mod integration_tests {
    use crate::air_loop::*;
    use crate::controller::*;
    use crate::setpoint::*;
    use crate::zone_equipment::*;

    /// Full single-zone cooling scenario:
    /// OA controller → AHU (fan + cooling coil) → zone terminal → zone load met
    #[test]
    fn single_zone_cooling_pipeline() {
        let zone_temp = 26.0;
        let cooling_setpoint = 24.0;
        let outdoor_temp = 35.0;

        // Economizer should NOT be active (outdoor too hot)
        let econ = EconomizerController::fixed_dry_bulb("Econ1", 0.15, 28.0);
        let oa_h = ep_psychrometrics::enthalpy(outdoor_temp, 0.010);
        let ret_h = ep_psychrometrics::enthalpy(zone_temp, 0.009);
        let oa_result = econ.calculate(outdoor_temp, zone_temp, oa_h, ret_h, true);
        assert!(!oa_result.economizer_active);

        // Calculate setpoint using SingleZoneReheat
        let spm = SetpointManager::single_zone_reheat("SZR", 0, 12.0, 50.0);
        let sp = spm.calculate_from_zones(&[zone_temp], &[cooling_setpoint], &[20.0], outdoor_temp);
        assert!(sp <= cooling_setpoint, "Supply setpoint should be below zone cooling SP: sp={sp}");

        // Run AHU simulation
        let inlet = AirLoopInlet {
            temp: 28.0,
            humidity_ratio: 0.009,
            mass_flow_rate: 1.0,
            enthalpy: ep_psychrometrics::enthalpy(28.0, 0.009),
        };
        let components: Vec<Box<dyn AirLoopComponent>> = vec![
            Box::new(FanComponent::new("SupFan", 500.0, 0.9)),
            Box::new(CoolingCoilComponent::new("CC1", 20000.0, 3.5)),
        ];
        let sim = AirLoopSimulator::default();
        let result = sim.simulate(&inlet, &components, &[1.0, 0.8]);

        assert!(result.supply_temp < zone_temp, "Supply should be cooler: T={}", result.supply_temp);
        assert!(result.cooling_capacity > 0.0);
        assert!(result.fan_power > 0.0);
        assert!(result.converged);
    }

    /// Full single-zone heating scenario:
    /// OA controller → AHU (fan + heating coil) → zone load met
    #[test]
    fn single_zone_heating_pipeline() {
        let zone_temp = 18.0;
        let outdoor_temp = -5.0;

        // OA controller — minimum OA only (cold outside)
        let oa_ctrl = OutdoorAirController::new(
            "OA1",
            EconomizerController::no_economizer("NoEcon", 0.1),
            0.1, 1.0,
        );
        let oa_h = ep_psychrometrics::enthalpy(outdoor_temp, 0.002);
        let ret_h = ep_psychrometrics::enthalpy(zone_temp, 0.005);
        let oa_result = oa_ctrl.calculate(1.0, outdoor_temp, zone_temp, oa_h, ret_h, false, 0.0, 100.0, 1.2);
        assert!(oa_result.oa_mass_flow > 0.0);

        // Mixed air (10% OA, 90% return)
        let mixed_temp = 0.1 * outdoor_temp + 0.9 * zone_temp;
        assert!(mixed_temp < zone_temp);

        let inlet = AirLoopInlet {
            temp: mixed_temp,
            humidity_ratio: 0.004,
            mass_flow_rate: 1.0,
            enthalpy: ep_psychrometrics::enthalpy(mixed_temp, 0.004),
        };
        let components: Vec<Box<dyn AirLoopComponent>> = vec![
            Box::new(FanComponent::new("SupFan", 400.0, 0.85)),
            Box::new(HeatingCoilComponent::new("HC1", 25000.0, 0.80)),
        ];
        let sim = AirLoopSimulator::default();
        let result = sim.simulate(&inlet, &components, &[1.0, 0.7]);

        assert!(result.supply_temp > mixed_temp, "Heating coil should raise temp");
        assert!(result.heating_capacity > 0.0);
        assert!(result.converged);
    }

    /// Multi-zone setpoint feedback: warmest zone drives cooling setpoint
    #[test]
    fn multi_zone_warmest_setpoint_feedback() {
        let zone_temps = [23.0, 25.0, 27.0];
        let cooling_sps = [24.0, 24.0, 24.0];
        let heating_sps = [20.0, 20.0, 20.0];

        let spm = SetpointManager::warmest("Warmest", 0, 12.0, 18.0);
        let sp = spm.calculate_from_zones(&zone_temps, &cooling_sps, &heating_sps, 30.0);

        assert!(sp <= 18.0 && sp >= 12.0, "sp={sp}");
        assert!(sp < 15.0, "Should be near min due to large deviation: sp={sp}");
    }

    /// Multi-zone setpoint feedback: coldest zone drives heating setpoint
    #[test]
    fn multi_zone_coldest_setpoint_feedback() {
        let zone_temps = [22.0, 20.0, 17.0];
        let cooling_sps = [24.0, 24.0, 24.0];
        let heating_sps = [21.0, 21.0, 21.0];

        let spm = SetpointManager::coldest("Coldest", 0, 35.0, 50.0);
        let sp = spm.calculate_from_zones(&zone_temps, &cooling_sps, &heating_sps, -5.0);

        assert!(sp >= 35.0 && sp <= 50.0, "sp={sp}");
        assert!(sp > 40.0, "Should be near max due to large deviation: sp={sp}");
    }

    /// DCV: varying occupancy affects minimum OA flow (no economizer)
    #[test]
    fn dcv_varying_occupancy() {
        let oa_ctrl = OutdoorAirController::new(
            "OA-DCV",
            EconomizerController::no_economizer("NoEcon", 0.1),
            0.05, 2.0,
        ).with_dcv(0.01, 0.0006);

        let zone_area = 100.0;
        let oa_h = ep_psychrometrics::enthalpy(30.0, 0.012);
        let ret_h = ep_psychrometrics::enthalpy(24.0, 0.009);

        // Low occupancy (5 people): DCV = 5*0.01 + 100*0.0006 = 0.11 m³/s
        let r1 = oa_ctrl.calculate(2.0, 30.0, 24.0, oa_h, ret_h, false, 5.0, zone_area, 1.2);
        // High occupancy (50 people): DCV = 50*0.01 + 100*0.0006 = 0.56 m³/s
        let r2 = oa_ctrl.calculate(2.0, 30.0, 24.0, oa_h, ret_h, false, 50.0, zone_area, 1.2);

        assert!(r2.oa_mass_flow > r1.oa_mass_flow,
            "More occupants need more OA: {:.3} vs {:.3}", r2.oa_mass_flow, r1.oa_mass_flow);
    }

    /// Economizer free cooling: cool outdoor air reduces cooling coil load
    #[test]
    fn economizer_free_cooling_reduces_coil_load() {
        let outdoor_temp = 16.0;
        let zone_temp = 24.0;

        let econ = EconomizerController::fixed_dry_bulb("FDB", 0.15, 22.0);
        let oa_h = ep_psychrometrics::enthalpy(outdoor_temp, 0.007);
        let ret_h = ep_psychrometrics::enthalpy(zone_temp, 0.009);
        let result = econ.calculate(outdoor_temp, zone_temp, oa_h, ret_h, true);
        assert!(result.economizer_active);

        let mixed_temp_econ = outdoor_temp;
        let mixed_temp_no_econ = zone_temp;
        let supply_sp = 13.0;
        let load_with_econ = mixed_temp_econ - supply_sp;
        let load_without_econ = mixed_temp_no_econ - supply_sp;

        assert!(load_with_econ < load_without_econ,
            "Free cooling reduces load: {load_with_econ} vs {load_without_econ}");
    }

    /// Zone equipment dispatch: two terminals serving one zone
    #[test]
    fn multi_terminal_zone_dispatch() {
        let supply_temp = 14.0;
        let supply_w = 0.008;
        let zone_temp = 25.0;
        let zone_load = -5000.0;

        let t1 = SingleDuctTerminal::constant_volume("CV1", 0.5, 10, 20);
        let t2 = SingleDuctTerminal::constant_volume("CV2", 0.3, 30, 40);

        let result = dispatch_zone_equipment(
            &[t1, t2], zone_load, supply_temp, supply_w, zone_temp, 1.2,
        );

        // CV terminals deliver cooling (supply < zone temp)
        assert!(result.delivered_cooling > 0.0,
            "Should deliver cooling: {}", result.delivered_cooling);
        // Total cooling exceeds load (CV can't modulate, so it over-cools)
        assert!(result.delivered_cooling > zone_load.abs() * 0.5,
            "Should deliver significant cooling: {} vs {}", result.delivered_cooling, zone_load.abs());
    }

    /// VAV terminal in heating mode: minimum flow with reheat
    #[test]
    fn vav_heating_reheat_integration() {
        let terminal = SingleDuctTerminal::vav_reheat("VAV-1", 1.0, 0.3, 8000.0, 10, 20);
        let supply_temp = 14.0;
        let zone_temp = 19.0;
        let heating_load = 3000.0;

        let result = terminal.calculate(supply_temp, 0.008, zone_temp, heating_load, 1.2);

        assert!(result.air_mass_flow <= 0.3 * 1.2 + 0.01,
            "VAV heating: flow should be near min, got {}", result.air_mass_flow);
        assert!(result.reheat_rate > 0.0, "VAV should use reheat in heating mode");
        assert!(result.supply_temp > supply_temp,
            "Reheat raises supply temp: {} vs {}", result.supply_temp, supply_temp);
    }

    /// PI controller tracks setpoint over multiple timesteps
    #[test]
    fn pi_controller_tracks_setpoint() {
        let mut pi = PIController::new("PI-1", 13.0, 0.5, 0.1, false);

        let mut measurement = 20.0;
        let dt = 60.0;

        for _ in 0..10 {
            let signal = pi.calculate(measurement, dt);
            assert!(signal >= 0.0 && signal <= 1.0);
            measurement -= signal * 2.0;
        }

        assert!((measurement - 13.0).abs() < 10.0,
            "PI should move toward setpoint: meas={measurement}");
    }

    /// Water coil bisection finds correct flow for target temperature
    #[test]
    fn water_coil_bisection_target_temp() {
        let target_supply = 14.0;
        let tolerance = 0.5;
        let result = bisect_water_coil_flow(
            target_supply,
            0.0,
            2.0,
            tolerance,
            20,
            |water_flow| {
                let base_temp = 28.0;
                let max_cooling = 16.0;
                base_temp - max_cooling * (1.0 - (-water_flow * 3.0_f64).exp())
            },
        );

        assert!(result.converged, "Bisection should converge");
        assert!((result.outlet_temp - target_supply).abs() < tolerance,
            "Should achieve target: {:.2} vs {target_supply}", result.outlet_temp);
    }

    /// Return air mixing from multiple zones
    #[test]
    fn return_air_mixing_three_zones() {
        let zone_temps = [22.0, 24.0, 26.0];
        let zone_w = [0.008, 0.009, 0.010];
        let zone_flows = [0.5, 0.3, 0.2];

        let (t_ret, w_ret, total_flow) = calculate_return_air(&zone_temps, &zone_w, &zone_flows);

        let expected_total: f64 = zone_flows.iter().sum();
        let expected_t = zone_temps.iter().zip(&zone_flows)
            .map(|(t, f)| t * f).sum::<f64>() / expected_total;
        let expected_w = zone_w.iter().zip(&zone_flows)
            .map(|(w, f)| w * f).sum::<f64>() / expected_total;

        assert!((t_ret - expected_t).abs() < 0.01, "T_ret={t_ret} vs {expected_t}");
        assert!((w_ret - expected_w).abs() < 0.0001, "W_ret={w_ret} vs {expected_w}");
        assert!((total_flow - expected_total).abs() < 0.01);
    }

    /// Zone load calculation: deadband behavior
    #[test]
    fn zone_load_deadband_no_action() {
        let result = calculate_zone_load(5000.0, 200.0, 22.0, 24.0, 20.0);
        assert!(!result.cooling_needed);
        assert!(!result.heating_needed);
        assert!(result.sensible_load.abs() < 1e-6);
    }

    /// Zone load calculation: cooling needed
    #[test]
    fn zone_load_cooling_action() {
        let result = calculate_zone_load(6000.0, 200.0, 26.0, 24.0, 20.0);
        assert!(result.cooling_needed);
        assert!(!result.heating_needed);
    }

    /// Zone load calculation: heating needed
    #[test]
    fn zone_load_heating_action() {
        let result = calculate_zone_load(3000.0, 200.0, 18.0, 24.0, 20.0);
        assert!(!result.cooling_needed);
        assert!(result.heating_needed);
    }

    /// MixedAir setpoint stores reference setpoint
    #[test]
    fn mixed_air_setpoint_stores_reference() {
        let spm = SetpointManager::mixed_air("MA", 0, 13.0);
        let sp = spm.calculate(25.0);
        assert!((sp - 13.0).abs() < 0.01, "sp={sp}");
    }

    /// OA reset setpoint interpolates between endpoints
    #[test]
    fn oa_reset_setpoint_interpolation() {
        let spm = SetpointManager::outdoor_air_reset("OAReset", 0, 10.0, 16.0, 20.0, 12.0);

        // At low OA temp → low_setpoint (16)
        assert!((spm.calculate(10.0) - 16.0).abs() < 0.1);
        // At high OA temp → high_setpoint (12)
        assert!((spm.calculate(20.0) - 12.0).abs() < 0.1);
        // Midpoint
        let mid = spm.calculate(15.0);
        assert!(mid > 12.0 && mid < 16.0, "mid={mid}");
    }

    /// PTAC serving zone: cooling mode with OA mixing
    #[test]
    fn ptac_zone_cooling_pipeline() {
        use crate::unitary::*;

        let ptac = PackagedTerminalAC::new("PTAC-1", 8000.0, 3.5, 6000.0);
        let zone_temp = 26.0;
        let outdoor_temp = 35.0;
        let zone_load = -5000.0;

        let result = ptac.calculate(zone_temp, zone_load, outdoor_temp, 1.2);

        assert!(result.cooling_rate > 0.0, "cooling={}", result.cooling_rate);
        assert!(result.supply_temp < zone_temp, "T_sup={}", result.supply_temp);
        assert!(result.power > 0.0);
        assert!(result.fan_power > 0.0);
    }

    /// PTHP cold-day supplemental heating pipeline
    #[test]
    fn pthp_cold_day_supplemental_pipeline() {
        use crate::unitary::*;

        let pthp = PackagedTerminalHP::new("PTHP-1", 8000.0, 3.5, 10000.0, 3.0)
            .with_supplemental(5000.0);
        let zone_temp = 18.0;
        let outdoor_temp = -10.0;
        let zone_load = 8000.0;

        let result = pthp.calculate(zone_temp, zone_load, outdoor_temp, 1.2);

        // Very cold: defrost active, supplemental may kick in
        assert!(result.heating_rate > 0.0, "heating={}", result.heating_rate);
        assert!(result.supply_temp > zone_temp, "T_sup={}", result.supply_temp);
        // Below min_outdoor_temp → compressor off, supplemental only
        if outdoor_temp < pthp.min_outdoor_temp {
            assert!(result.supplemental_power > 0.0,
                "supp={}", result.supplemental_power);
        } else {
            // Defrost reduces capacity → supplemental fills gap
            assert!(result.supplemental_power > 0.0 || result.defrost_active,
                "supp={} defrost={}", result.supplemental_power, result.defrost_active);
        }
    }
}
