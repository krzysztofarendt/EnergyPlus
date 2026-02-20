//! BESTEST Case 600 simulation callback with real physics.
//!
//! Implements `SimulationCallback` using:
//! - CTF conduction transfer functions for opaque envelope surfaces
//! - Iterative window thermal solver (tridiagonal)
//! - Solar position and incident radiation on tilted surfaces
//! - Window angular transmittance model
//! - Zone air predictor-corrector with ideal loads
//!
//! Reference: ASHRAE Standard 140 (BESTEST), Case 600 — lightweight box.

use ep_core::state::SimulationState;
use ep_envelope::convection;
use ep_envelope::ctf::generate_ctf;
use ep_envelope::heat_balance::{
    solve_zone_surfaces, SurfaceHistory, SurfaceState,
};
use ep_materials::{
    Construction, CtfCoefficients, GasType, GlassMaterial, Material, MaterialDatabase,
    OpaqueMaterial, ResistanceOnlyMaterial, SurfaceRoughness,
};
use ep_output::variables::{TimeStamp, TimeStepType};
use ep_output::OutputManager;
use ep_psychrometrics::cp_air;
use ep_solar::incident::{beam_on_tilted, cos_angle_of_incidence, ground_reflected, isotropic_diffuse};
use ep_units::*;
use ep_weather::solar_position::solar_position;
use ep_weather::sky_models::sky_temperature_berdahl_martin;
use ep_weather::WeatherFile;
use ep_windows::thermal::{
    solve_window_heat_balance, ExteriorConditions, InteriorConditions,
};
use ep_zone_air::{
    corrector, HeatBalanceCoefficients, SolutionAlgorithm, ThermostatControl, ZoneAirState,
    ZonePredictorCorrector,
};

use crate::building::ZoneSimState;
use crate::sizing::ZoneSizingData;
use crate::SimulationCallback;

// ─── BESTEST Case 600 geometry and construction constants ──────────

/// Zone dimensions (m).
const ZONE_WIDTH: f64 = 8.0;
const ZONE_DEPTH: f64 = 6.0;
const ZONE_HEIGHT: f64 = 2.7;
const ZONE_VOLUME: f64 = ZONE_WIDTH * ZONE_DEPTH * ZONE_HEIGHT; // 129.6 m³
const ZONE_FLOOR_AREA: f64 = ZONE_WIDTH * ZONE_DEPTH; // 48 m²

/// Window: 12 m² on south wall (two 3m × 2m windows).
const WINDOW_AREA: f64 = 12.0;

/// South wall opaque area = total - window.
const SOUTH_WALL_OPAQUE_AREA: f64 = ZONE_WIDTH * ZONE_HEIGHT - WINDOW_AREA; // 9.6 m²

/// Internal gains: 200 W constant (convective only for simplicity).
const INTERNAL_GAINS_W: f64 = 200.0;

/// Infiltration: 0.5 ACH.
const INFILTRATION_ACH: f64 = 0.5;

/// Heating/cooling setpoints (°C).
const HEATING_SETPOINT: f64 = 20.0;
const COOLING_SETPOINT: f64 = 27.0;

/// Ground albedo.
const GROUND_ALBEDO: f64 = 0.2;

/// Ground temperature (°C) — used for floor exterior BC and LW exchange.
const GROUND_TEMP_C: f64 = 10.0;

/// Number of opaque surfaces.
const NUM_SURFACES: usize = 6;

/// Surface indices.
#[allow(dead_code)]
const SURF_SOUTH_WALL: usize = 0;
#[allow(dead_code)]
const SURF_NORTH_WALL: usize = 1;
#[allow(dead_code)]
const SURF_EAST_WALL: usize = 2;
#[allow(dead_code)]
const SURF_WEST_WALL: usize = 3;
#[allow(dead_code)]
const SURF_ROOF: usize = 4;
const SURF_FLOOR: usize = 5;

/// Hourly output record.
#[derive(Debug, Clone, Default)]
pub struct HourlyRecord {
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub outdoor_temp: f64,
    pub zone_temp: f64,
    pub heating_rate: f64,
    pub cooling_rate: f64,
    pub beam_solar_south: f64,
    pub transmitted_solar: f64,
}

/// BESTEST Case 600 simulation callback.
pub struct Bestest600Callback {
    // Zone air
    pub zone: ZoneSimState,

    // Surfaces (6 opaque)
    surface_states: Vec<SurfaceState>,
    surface_ctfs: Vec<CtfCoefficients>,
    surface_histories: Vec<SurfaceHistory>,
    surface_areas: Vec<f64>,
    surface_tilts: Vec<Angle>,
    surface_azimuths: Vec<Angle>,
    surface_is_exterior: Vec<bool>,
    surface_solar_abs_outside: Vec<f64>,
    surface_solar_abs_inside: Vec<f64>,

    // Window
    window_glass: Vec<GlassMaterial>,
    window_gap_gas: Vec<GasType>,
    window_gap_width: Vec<f64>,

    // Weather
    pub weather: WeatherFile,
    lat: Angle,
    lon: Angle,
    tz: f64,

    // Output
    pub output_manager: OutputManager,
    pub hourly_data: Vec<HourlyRecord>,
    pub annual_heating_j: f64,
    pub annual_cooling_j: f64,
    pub peak_heating_w: f64,
    pub peak_cooling_w: f64,
    timestep_seconds: f64,
    timesteps_per_hour: u8,
}

impl Bestest600Callback {
    /// Create a new BESTEST Case 600 callback.
    ///
    /// Builds all materials, constructions, CTFs, and initializes surface states.
    pub fn new(weather: WeatherFile, timesteps_per_hour: u8) -> Self {
        let timestep_seconds = 3600.0 / timesteps_per_hour as f64;

        // ── Build material database ──
        let mut mat_db = MaterialDatabase::new();

        // Wall materials (outside to inside): wood siding → fiberglass → plasterboard
        let idx_wood_siding = mat_db.add_material(Material::Opaque(OpaqueMaterial {
            name: "WoodSiding".into(),
            roughness: SurfaceRoughness::MediumSmooth,
            thickness: Length::new(0.009),
            conductivity: 0.14,
            density: 530.0,
            specific_heat: 900.0,
            absorptance_thermal: 0.9,
            absorptance_solar: 0.6,
            absorptance_visible: 0.6,
        }));
        let idx_fiberglass_wall = mat_db.add_material(Material::Opaque(OpaqueMaterial {
            name: "FiberglassWall".into(),
            roughness: SurfaceRoughness::MediumRough,
            thickness: Length::new(0.066),
            conductivity: 0.04,
            density: 12.0,
            specific_heat: 840.0,
            absorptance_thermal: 0.9,
            absorptance_solar: 0.6,
            absorptance_visible: 0.6,
        }));
        let idx_plasterboard = mat_db.add_material(Material::Opaque(OpaqueMaterial {
            name: "Plasterboard".into(),
            roughness: SurfaceRoughness::Smooth,
            thickness: Length::new(0.012),
            conductivity: 0.16,
            density: 950.0,
            specific_heat: 840.0,
            absorptance_thermal: 0.9,
            absorptance_solar: 0.6,
            absorptance_visible: 0.6,
        }));

        // Roof materials: roof deck → fiberglass → plasterboard
        let idx_roof_deck = mat_db.add_material(Material::Opaque(OpaqueMaterial {
            name: "RoofDeck".into(),
            roughness: SurfaceRoughness::MediumRough,
            thickness: Length::new(0.019),
            conductivity: 0.14,
            density: 530.0,
            specific_heat: 900.0,
            absorptance_thermal: 0.9,
            absorptance_solar: 0.6,
            absorptance_visible: 0.6,
        }));
        let idx_fiberglass_roof = mat_db.add_material(Material::Opaque(OpaqueMaterial {
            name: "FiberglassRoof".into(),
            roughness: SurfaceRoughness::MediumRough,
            thickness: Length::new(0.1118),
            conductivity: 0.04,
            density: 12.0,
            specific_heat: 840.0,
            absorptance_thermal: 0.9,
            absorptance_solar: 0.6,
            absorptance_visible: 0.6,
        }));
        let idx_plasterboard_roof = mat_db.add_material(Material::Opaque(OpaqueMaterial {
            name: "PlasterboardRoof".into(),
            roughness: SurfaceRoughness::Smooth,
            thickness: Length::new(0.010),
            conductivity: 0.16,
            density: 950.0,
            specific_heat: 840.0,
            absorptance_thermal: 0.9,
            absorptance_solar: 0.6,
            absorptance_visible: 0.6,
        }));

        // Floor materials: timber → insulation (R-only)
        let idx_timber = mat_db.add_material(Material::Opaque(OpaqueMaterial {
            name: "Timber".into(),
            roughness: SurfaceRoughness::MediumRough,
            thickness: Length::new(0.025),
            conductivity: 0.14,
            density: 650.0,
            specific_heat: 1200.0,
            absorptance_thermal: 0.9,
            absorptance_solar: 0.6,
            absorptance_visible: 0.6,
        }));
        let idx_floor_insulation = mat_db.add_material(Material::ResistanceOnly(ResistanceOnlyMaterial {
            name: "FloorInsulation".into(),
            resistance: 25.075,
        }));

        // ── Build constructions ──

        // Wall construction (outside → inside)
        let mut wall_con = Construction::new("WallConstruction");
        wall_con.layers = vec![idx_wood_siding, idx_fiberglass_wall, idx_plasterboard];
        wall_con.outside_absorptance_solar = 0.6;
        wall_con.outside_absorptance_thermal = 0.9;
        wall_con.inside_absorptance_solar = 0.6;
        wall_con.inside_absorptance_thermal = 0.9;
        wall_con.outside_roughness = SurfaceRoughness::MediumSmooth;
        let wall_ctf = generate_ctf(&wall_con, &mat_db, timestep_seconds)
            .expect("wall CTF generation failed");

        // Roof construction (outside → inside)
        let mut roof_con = Construction::new("RoofConstruction");
        roof_con.layers = vec![idx_roof_deck, idx_fiberglass_roof, idx_plasterboard_roof];
        roof_con.outside_absorptance_solar = 0.6;
        roof_con.outside_absorptance_thermal = 0.9;
        roof_con.inside_absorptance_solar = 0.6;
        roof_con.inside_absorptance_thermal = 0.9;
        roof_con.outside_roughness = SurfaceRoughness::MediumRough;
        let roof_ctf = generate_ctf(&roof_con, &mat_db, timestep_seconds)
            .expect("roof CTF generation failed");

        // Floor construction (outside=bottom → inside=top)
        let mut floor_con = Construction::new("FloorConstruction");
        floor_con.layers = vec![idx_floor_insulation, idx_timber];
        floor_con.outside_absorptance_solar = 0.6;
        floor_con.outside_absorptance_thermal = 0.9;
        floor_con.inside_absorptance_solar = 0.6;
        floor_con.inside_absorptance_thermal = 0.9;
        let floor_ctf = generate_ctf(&floor_con, &mat_db, timestep_seconds)
            .expect("floor CTF generation failed");

        // ── Window glass ──
        // BESTEST double-pane clear 3mm, air gap 12mm
        let glass = GlassMaterial {
            name: "Clear3mm".into(),
            thickness: Length::new(0.003),
            conductivity: 1.0,
            solar_transmittance: 0.834,
            solar_reflectance_front: 0.075,
            solar_reflectance_back: 0.075,
            visible_transmittance: 0.90,
            visible_reflectance_front: 0.08,
            visible_reflectance_back: 0.08,
            ir_transmittance: 0.0,
            emissivity_front: 0.84,
            emissivity_back: 0.84,
            dirt_correction_factor: 1.0,
            is_solar_diffusing: false,
        };

        // ── Surface definitions ──
        // Order: south wall, north wall, east wall, west wall, roof, floor
        let surface_areas = vec![
            SOUTH_WALL_OPAQUE_AREA, // south wall (minus window)
            ZONE_WIDTH * ZONE_HEIGHT, // north wall = 21.6 m²
            ZONE_DEPTH * ZONE_HEIGHT, // east wall = 16.2 m²
            ZONE_DEPTH * ZONE_HEIGHT, // west wall = 16.2 m²
            ZONE_FLOOR_AREA,          // roof = 48 m²
            ZONE_FLOOR_AREA,          // floor = 48 m²
        ];

        // Tilts: walls=90°, roof=0° (horizontal face-up), floor=180° (face-down)
        let surface_tilts = vec![
            Angle::from_degrees(90.0),  // south wall
            Angle::from_degrees(90.0),  // north wall
            Angle::from_degrees(90.0),  // east wall
            Angle::from_degrees(90.0),  // west wall
            Angle::from_degrees(0.0),   // roof (face up)
            Angle::from_degrees(180.0), // floor (face down)
        ];

        // Azimuths from south (BESTEST convention): S=0, N=180, E=-90(=270), W=90
        let surface_azimuths = vec![
            Angle::from_degrees(0.0),   // south
            Angle::from_degrees(180.0), // north
            Angle::from_degrees(-90.0), // east
            Angle::from_degrees(90.0),  // west
            Angle::from_degrees(0.0),   // roof (azimuth irrelevant)
            Angle::from_degrees(0.0),   // floor (azimuth irrelevant)
        ];

        // Floor is NOT exterior (adiabatic bottom)
        let surface_is_exterior = vec![true, true, true, true, true, false];

        // Solar absorptances
        let surface_solar_abs_outside = vec![0.6, 0.6, 0.6, 0.6, 0.6, 0.6];
        let surface_solar_abs_inside = vec![0.6, 0.6, 0.6, 0.6, 0.6, 0.6];

        // ── CTFs for each surface ──
        let surface_ctfs = vec![
            wall_ctf.clone(), // south wall
            wall_ctf.clone(), // north wall
            wall_ctf.clone(), // east wall
            wall_ctf.clone(), // west wall
            roof_ctf,         // roof
            floor_ctf,        // floor
        ];

        // ── Initial surface states ──
        let init_temp = 20.0;
        let surface_states: Vec<SurfaceState> = (0..NUM_SURFACES)
            .map(|i| SurfaceState {
                t_outside: if surface_is_exterior[i] { 10.0 } else { init_temp },
                t_inside: init_temp,
                h_conv_inside: 3.0,
                h_conv_outside: 15.0,
                area: surface_areas[i],
                emissivity_inside: 0.9,
                emissivity_outside: 0.9,
                cos_tilt: surface_tilts[i].cos(),
                is_exterior: surface_is_exterior[i],
                q_solar_outside: 0.0,
                q_sw_inside: 0.0,
            })
            .collect();

        // ── Surface histories ──
        let num_hist = surface_ctfs.iter().map(|c| c.num_histories).max().unwrap_or(1).max(1);
        let surface_histories: Vec<SurfaceHistory> = (0..NUM_SURFACES)
            .map(|i| {
                let t_out = if surface_is_exterior[i] { 10.0 } else { init_temp };
                SurfaceHistory::new(num_hist, t_out, init_temp)
            })
            .collect();

        // ── Zone air state ──
        let air_density = 1.204; // kg/m³ at sea level; Denver is ~1.0 but use standard
        let cap = ep_zone_air::zone_air_power_cap(ZONE_VOLUME, 0.008, air_density, timestep_seconds, 1.0);
        let mut pc = ZonePredictorCorrector::new(
            SolutionAlgorithm::ThirdOrder,
            ThermostatControl::DualSetPoint,
            cap,
        );
        pc.heating_setpoint = HEATING_SETPOINT;
        pc.cooling_setpoint = COOLING_SETPOINT;

        let mut output_manager = OutputManager::new();
        let output_indices = output_manager.register_zone_variables("ZONE1");

        let zone = ZoneSimState {
            name: "ZONE1".into(),
            air_state: ZoneAirState::new(init_temp, 0.008),
            predictor_corrector: pc,
            volume: ZONE_VOLUME,
            floor_area: ZONE_FLOOR_AREA,
            multiplier: 1,
            convective_gains: 0.0,
            surface_ha: 0.0,
            surface_hat: 0.0,
            infiltration_mcp: 0.0,
            infiltration_mcpt: 0.0,
            system_mcp: 0.0,
            system_mcpt: 0.0,
            output_indices,
            sizing: ZoneSizingData::new("ZONE1"),
            predicted_heating_load: 0.0,
            predicted_cooling_load: 0.0,
        };

        let lat = weather.location.latitude;
        let lon = weather.location.longitude;
        let tz = weather.location.time_zone;

        Self {
            zone,
            surface_states,
            surface_ctfs,
            surface_histories,
            surface_areas,
            surface_tilts,
            surface_azimuths,
            surface_is_exterior,
            surface_solar_abs_outside,
            surface_solar_abs_inside,
            window_glass: vec![glass.clone(), glass],
            window_gap_gas: vec![GasType::Air],
            window_gap_width: vec![0.012],
            weather,
            lat,
            lon,
            tz,
            output_manager,
            hourly_data: Vec::with_capacity(8760),
            annual_heating_j: 0.0,
            annual_cooling_j: 0.0,
            peak_heating_w: 0.0,
            peak_cooling_w: 0.0,
            timestep_seconds,
            timesteps_per_hour,
        }
    }

    /// Double-pane angular transmittance using 5-coefficient polynomial.
    ///
    /// For clear glass: τ(θ) ≈ τ_normal × poly(cos_θ)
    /// Polynomial fit from Duffie & Beckman for double-clear:
    fn window_beam_transmittance(&self, cos_incidence: f64) -> f64 {
        if cos_incidence <= 0.0 {
            return 0.0;
        }
        // Normal incidence transmittance for double-pane clear ≈ 0.74
        // (product of two panes: ~0.86 × 0.86 ≈ 0.74)
        let tau_normal = 0.74;

        // Angular correction polynomial: fitted ratio τ(θ)/τ(0)
        // Coefficients for double-clear from BESTEST/Duffie-Beckman
        let x = cos_incidence.clamp(0.0, 1.0);
        // At normal incidence (x=1), ratio = 1.0
        // At 60° (x=0.5), ratio ≈ 0.81
        // At 80° (x=0.17), ratio ≈ 0.25
        let ratio = -0.0015 + 2.7266 * x - 2.4136 * x * x + 0.6935 * x * x * x;
        let ratio = ratio.clamp(0.0, 1.0);

        tau_normal * ratio
    }

    /// Diffuse transmittance (hemispherical average).
    fn window_diffuse_transmittance(&self) -> f64 {
        // Diffuse ≈ τ(60°) for isotropic sky
        self.window_beam_transmittance(60.0_f64.to_radians().cos())
    }

    /// Solar absorptance per glass layer at a given angle.
    /// Returns (outer_layer_abs, inner_layer_abs).
    fn window_layer_absorptance(&self, cos_incidence: f64) -> (f64, f64) {
        if cos_incidence <= 0.0 {
            return (0.0, 0.0);
        }
        // Single-pane absorptance at normal ≈ 0.091 (= 1 - 0.834 - 0.075)
        // Increases at oblique angles. Simplified model:
        let abs_normal = 0.091;
        let x = cos_incidence.clamp(0.0, 1.0);
        // Absorptance increases roughly as 1/cos(θ) up to a limit
        let abs_single = (abs_normal / x).min(0.5);

        // Outer pane absorbs first, inner pane absorbs from reduced beam
        let tau_single = self.window_glass[0].solar_transmittance * x.max(0.1);
        let outer_abs = abs_single;
        let inner_abs = abs_single * tau_single.min(0.9);

        (outer_abs, inner_abs)
    }

    /// Apply ideal loads to meet zone heating/cooling load.
    fn apply_ideal_loads(&mut self) {
        let heating_load = self.zone.predicted_heating_load;
        let cooling_load = self.zone.predicted_cooling_load;

        let system_load = if heating_load > 0.0 {
            heating_load
        } else if cooling_load < 0.0 {
            cooling_load
        } else {
            self.zone.system_mcp = 0.0;
            self.zone.system_mcpt = 0.0;
            return;
        };

        let supply_temp = if system_load > 0.0 {
            HEATING_SETPOINT + 20.0
        } else {
            COOLING_SETPOINT - 12.0
        };

        let zone_temp = self.zone.air_state.temperature;
        let dt = (supply_temp - zone_temp).abs().max(1.0);
        let cp = cp_air(0.008);
        let mass_flow = system_load.abs() / (cp * dt);

        self.zone.system_mcp = mass_flow * cp;
        self.zone.system_mcpt = mass_flow * cp * supply_temp;
    }

    /// Build heat balance coefficients from current zone state.
    fn build_coefficients(&self) -> HeatBalanceCoefficients {
        HeatBalanceCoefficients {
            sum_ha: self.zone.surface_ha,
            sum_hat_surf: self.zone.surface_hat,
            sum_internal_convective: self.zone.convective_gains,
            sum_mcp: self.zone.infiltration_mcp,
            sum_mcpt: self.zone.infiltration_mcpt,
            sum_sys_mcp: self.zone.system_mcp,
            sum_sys_mcpt: self.zone.system_mcpt,
            non_air_system_response: 0.0,
        }
    }

    /// Write hourly results to CSV.
    pub fn write_csv(&self, path: &str) -> std::io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(path)?;
        writeln!(f, "Month,Day,Hour,OutdoorTemp_C,ZoneTemp_C,HeatingRate_W,CoolingRate_W,BeamSolarSouth_Wm2,TransmittedSolar_W")?;
        for r in &self.hourly_data {
            writeln!(
                f,
                "{},{},{},{:.1},{:.2},{:.1},{:.1},{:.1},{:.1}",
                r.month, r.day, r.hour, r.outdoor_temp, r.zone_temp,
                r.heating_rate, r.cooling_rate, r.beam_solar_south, r.transmitted_solar,
            )?;
        }
        Ok(())
    }

    /// Print validation summary against BESTEST acceptance ranges.
    pub fn print_validation_summary(&self) {
        let heating_mwh = self.annual_heating_j / 3.6e9;
        let cooling_mwh = self.annual_cooling_j / 3.6e9;
        let peak_heating_kw = self.peak_heating_w / 1000.0;
        let peak_cooling_kw = self.peak_cooling_w / 1000.0;

        println!("\n═══════════════════════════════════════════════════");
        println!("  BESTEST Case 600 — Validation Summary");
        println!("═══════════════════════════════════════════════════");
        println!("  Annual Heating:  {heating_mwh:8.3} MWh");
        println!("  Annual Cooling:  {cooling_mwh:8.3} MWh");
        println!("  Peak Heating:    {peak_heating_kw:8.3} kW");
        println!("  Peak Cooling:    {peak_cooling_kw:8.3} kW");
        println!("───────────────────────────────────────────────────");

        let ranges = ep_validation::bestest::case_600_acceptance();
        let results: Vec<(String, f64)> = vec![
            ("Annual Heating".into(), heating_mwh),
            ("Annual Cooling".into(), cooling_mwh),
            ("Peak Heating".into(), peak_heating_kw),
            ("Peak Cooling".into(), peak_cooling_kw),
        ];

        let mut all_pass = true;
        for range in &ranges {
            if let Some((_, val)) = results.iter().find(|(name, _)| *name == range.metric) {
                let pass = range.check(*val);
                let status = if pass { "PASS" } else { "FAIL" };
                if !pass {
                    all_pass = false;
                }
                println!(
                    "  {}: {:.3} {} [{:.3} - {:.3}] {}",
                    range.metric, val, range.unit, range.min, range.max, status
                );
            }
        }

        println!("───────────────────────────────────────────────────");
        if all_pass {
            println!("  Result: ALL CHECKS PASSED");
        } else {
            println!("  Result: SOME CHECKS FAILED");
        }
        println!("═══════════════════════════════════════════════════\n");
    }
}

impl SimulationCallback for Bestest600Callback {
    fn begin_environment(&mut self, _state: &mut SimulationState) {
        // Reset zone air state
        self.zone.air_state = ZoneAirState::new(20.0, 0.008);
        self.zone.system_mcp = 0.0;
        self.zone.system_mcpt = 0.0;
        self.zone.predicted_heating_load = 0.0;
        self.zone.predicted_cooling_load = 0.0;

        // Reset surface states and histories
        let num_hist = self.surface_ctfs.iter().map(|c| c.num_histories).max().unwrap_or(1).max(1);
        for i in 0..NUM_SURFACES {
            let t_out = if self.surface_is_exterior[i] { 10.0 } else { 20.0 };
            self.surface_states[i].t_outside = t_out;
            self.surface_states[i].t_inside = 20.0;
            self.surface_states[i].q_solar_outside = 0.0;
            self.surface_states[i].q_sw_inside = 0.0;
            self.surface_histories[i] = SurfaceHistory::new(num_hist, t_out, 20.0);
        }

        // Reset tracking
        self.annual_heating_j = 0.0;
        self.annual_cooling_j = 0.0;
        self.peak_heating_w = 0.0;
        self.peak_cooling_w = 0.0;
        self.hourly_data.clear();
    }

    fn do_timestep(&mut self, state: &mut SimulationState) {
        let month = state.clock.month;
        let day = state.clock.day_of_month;
        let hour = state.clock.hour_of_day;
        let ts_in_hr = state.clock.timestep_in_hour;
        let fractional_hour = hour as f64 + (ts_in_hr as f64 + 0.5) / self.timesteps_per_hour as f64;

        // ── 1. Look up weather ──
        // EPW convention: hour 1 = 00:00-01:00. We use mid-timestep.
        let epw_hour = hour + 1; // convert 0-based to 1-based
        let default_record = &self.weather.records[0];
        let record = self.weather.record_at(month, day, epw_hour).unwrap_or(default_record);

        let t_outdoor_c = record.dry_bulb.to_celsius();
        let dni = record.direct_normal_irradiance.value();
        let dhi = record.diffuse_horizontal_irradiance.value();
        let wind_speed = record.wind_speed.value();
        let pressure = record.pressure.value();

        // Update outdoor conditions
        state.outdoor.dry_bulb = t_outdoor_c;
        state.outdoor.beam_solar = dni;
        state.outdoor.diffuse_solar = dhi;
        state.outdoor.wind_speed = wind_speed;
        state.outdoor.wind_direction = record.wind_direction.to_degrees();
        state.outdoor.barometric_pressure = pressure;

        // Sky temperature
        let t_sky_c = sky_temperature_berdahl_martin(record).to_celsius();
        state.outdoor.sky_temperature = t_sky_c;

        let t_sky_k = t_sky_c + 273.15;
        let t_ground_k = GROUND_TEMP_C + 273.15;

        // ── 2. Solar position ──
        let doy = state.clock.day_of_year;
        let sun = solar_position(self.lat, self.lon, self.tz, doy, fractional_hour);

        // ── 3. Incident solar on each surface ──
        let ghi = Irradiance::new(dni * sun.altitude.sin().max(0.0) + dhi);
        let dni_irr = Irradiance::new(dni);
        let dhi_irr = Irradiance::new(dhi);

        for i in 0..NUM_SURFACES {
            if !self.surface_is_exterior[i] {
                self.surface_states[i].q_solar_outside = 0.0;
                continue;
            }

            let cos_inc = if sun.sun_is_up && sun.altitude.to_degrees() > 0.0 {
                cos_angle_of_incidence(
                    sun.altitude,
                    sun.azimuth,
                    self.surface_tilts[i],
                    self.surface_azimuths[i],
                )
            } else {
                0.0
            };

            let cos_tilt = self.surface_tilts[i].cos();

            let beam = beam_on_tilted(dni_irr, cos_inc).value();
            let diffuse = isotropic_diffuse(dhi_irr, cos_tilt).value();
            let ground_ref = ground_reflected(ghi, cos_tilt, GROUND_ALBEDO).value();
            let total_incident = beam + diffuse + ground_ref;

            self.surface_states[i].q_solar_outside =
                total_incident * self.surface_solar_abs_outside[i];
        }

        // ── 4. Window solar ──
        let cos_inc_south = if sun.sun_is_up && sun.altitude.to_degrees() > 0.0 {
            cos_angle_of_incidence(
                sun.altitude,
                sun.azimuth,
                Angle::from_degrees(90.0),  // vertical
                Angle::from_degrees(0.0),   // south
            )
        } else {
            0.0
        };

        let beam_on_window = beam_on_tilted(dni_irr, cos_inc_south).value();
        let diffuse_on_window = isotropic_diffuse(dhi_irr, 0.0).value(); // cos(90°)=0
        let ground_on_window = ground_reflected(ghi, 0.0, GROUND_ALBEDO).value();

        // Transmitted solar through window
        let tau_beam = self.window_beam_transmittance(cos_inc_south);
        let tau_diffuse = self.window_diffuse_transmittance();

        let transmitted_beam = beam_on_window * tau_beam * WINDOW_AREA;
        let transmitted_diffuse = (diffuse_on_window + ground_on_window) * tau_diffuse * WINDOW_AREA;
        let total_transmitted_solar = transmitted_beam + transmitted_diffuse;

        // Distribute transmitted solar to interior surfaces.
        // Beam goes to floor, diffuse distributed by area.
        let total_interior_area: f64 = self.surface_areas.iter().sum();
        for i in 0..NUM_SURFACES {
            let from_beam = if i == SURF_FLOOR {
                // All beam to floor
                transmitted_beam * self.surface_solar_abs_inside[i]
            } else {
                0.0
            };
            let from_diffuse =
                transmitted_diffuse * (self.surface_areas[i] / total_interior_area) * self.surface_solar_abs_inside[i];

            self.surface_states[i].q_sw_inside = (from_beam + from_diffuse) / self.surface_areas[i];
        }

        // ── 5. Window thermal ──
        let zone_temp_c = self.zone.air_state.temperature;

        // Window absorbed solar per layer
        let (abs_outer, abs_inner) = self.window_layer_absorptance(cos_inc_south);
        let total_window_incident = beam_on_window + diffuse_on_window + ground_on_window;
        let solar_abs_per_layer = vec![
            total_window_incident * abs_outer,
            total_window_incident * abs_inner,
        ];

        let h_ext_conv = convection::doe2_exterior_convection(
            t_outdoor_c - 15.0, // approximate surface-air dt
            0.0,                // vertical
            wind_speed,
            SurfaceRoughness::VerySmooth,
        );

        let ext_cond = ExteriorConditions {
            t_air: t_outdoor_c + 273.15,
            t_sky: t_sky_k,
            h_conv: h_ext_conv,
        };
        let int_cond = InteriorConditions {
            t_air: zone_temp_c + 273.15,
            t_mrt: zone_temp_c + 273.15, // approximate MRT = zone air
            h_conv: 3.6, // interior natural convection for window
            t_dew_point: 280.0, // ~7°C dew point
        };

        let win_result = solve_window_heat_balance(
            &self.window_glass,
            &self.window_gap_gas,
            &self.window_gap_width,
            &ext_cond,
            &int_cond,
            &solar_abs_per_layer,
            30,
            0.01,
        );

        // Window convective heat flow to zone (W). heat_flow_in is W/m².
        // Positive = heat flows from window surface to zone air.
        let _window_conv_to_zone = win_result.heat_flow_in * WINDOW_AREA;
        let window_inside_temp_c = win_result.glass_temps.last().copied().unwrap_or(zone_temp_c + 273.15) - 273.15;

        // ── 6. Update convection coefficients ──
        for i in 0..NUM_SURFACES {
            let dt_inside = self.surface_states[i].t_inside - zone_temp_c;
            self.surface_states[i].h_conv_inside = convection::tarp_interior_convection(
                dt_inside,
                self.surface_states[i].cos_tilt,
                ZONE_HEIGHT,
            );

            if self.surface_is_exterior[i] {
                let dt_outside = self.surface_states[i].t_outside - t_outdoor_c;
                self.surface_states[i].h_conv_outside = convection::doe2_exterior_convection(
                    dt_outside,
                    self.surface_states[i].cos_tilt,
                    wind_speed,
                    SurfaceRoughness::MediumSmooth,
                );
            }
        }

        // ── 7. Solve opaque surface heat balance ──
        let _result = solve_zone_surfaces(
            &mut self.surface_states,
            &self.surface_ctfs,
            &self.surface_histories,
            zone_temp_c,
            t_outdoor_c,
            t_sky_k,
            t_ground_k,
            50,
            0.001,
        );

        // ── 8. Build zone air balance ──
        // Surface HA and HAT from opaque surfaces
        let mut total_ha = 0.0;
        let mut total_hat = 0.0;
        for i in 0..NUM_SURFACES {
            let ha = self.surface_states[i].h_conv_inside * self.surface_areas[i];
            total_ha += ha;
            total_hat += ha * self.surface_states[i].t_inside;
        }

        // Window contribution to HA and HAT
        let win_h_inside = 3.6; // Same as used in window solver
        let win_ha = win_h_inside * WINDOW_AREA;
        total_ha += win_ha;
        total_hat += win_ha * window_inside_temp_c;

        self.zone.surface_ha = total_ha;
        self.zone.surface_hat = total_hat;

        // Convective gains: internal + transmitted solar to air (small fraction)
        // For BESTEST Case 600, internal gains are 200W constant (convective).
        // Transmitted solar is mostly absorbed by surfaces (handled above).
        // A small fraction (~10%) goes to air directly.
        let solar_to_air = total_transmitted_solar * 0.1;
        self.zone.convective_gains = INTERNAL_GAINS_W + solar_to_air;

        // Infiltration
        let air_density = pressure / (287.05 * (t_outdoor_c + 273.15)); // ideal gas
        let mass_flow = ZONE_VOLUME * air_density * INFILTRATION_ACH / 3600.0;
        let cp = cp_air(0.008);
        self.zone.infiltration_mcp = mass_flow * cp;
        self.zone.infiltration_mcpt = mass_flow * cp * t_outdoor_c;

        // ── 9. Predictor step ──
        self.zone.system_mcp = 0.0;
        self.zone.system_mcpt = 0.0;
        let coeffs = self.build_coefficients();
        let predict_result = self.zone.predictor_corrector.predict(
            &self.zone.air_state,
            &coeffs,
        );
        self.zone.predicted_heating_load = predict_result.load_to_heating_setpoint;
        self.zone.predicted_cooling_load = predict_result.load_to_cooling_setpoint;
    }

    fn do_hvac_iteration(&mut self, _state: &mut SimulationState) -> f64 {
        // Apply ideal loads
        self.apply_ideal_loads();

        // Corrector step
        let coeffs = self.build_coefficients();
        let new_temp = corrector::correct_zone_temperature(
            self.zone.predictor_corrector.algorithm,
            &self.zone.air_state,
            &coeffs,
            self.zone.predictor_corrector.air_power_cap,
        );

        // Compute sensible load met
        let w = self.zone.air_state.humidity_ratio;
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

        // Update zone temperature (do NOT advance history)
        self.zone.air_state.temperature = new_temp;

        // Track residual
        let predicted = if self.zone.predicted_heating_load > 0.0 {
            self.zone.predicted_heating_load
        } else if self.zone.predicted_cooling_load < 0.0 {
            self.zone.predicted_cooling_load
        } else {
            0.0
        };
        (sensible_load - predicted).abs()
    }

    fn end_timestep(&mut self, state: &mut SimulationState) {
        // ── Advance zone air state history ──
        let t = self.zone.air_state.temperature;
        let w = self.zone.air_state.humidity_ratio;
        self.zone.air_state.advance(t, w);

        // ── Advance surface histories ──
        for i in 0..NUM_SURFACES {
            let t_out = self.surface_states[i].t_outside;
            let t_in = self.surface_states[i].t_inside;
            // Compute fluxes for history
            let ctf = &self.surface_ctfs[i];
            let q_out = ctf.outside[0] * t_out + ctf.cross[0] * t_in;
            let q_in = ctf.cross[0] * t_out + ctf.inside[0] * t_in;
            self.surface_histories[i].push(t_out, t_in, q_out, q_in);
        }

        // ── Accumulate energy and track peaks ──
        let heating_rate = self.zone.predicted_heating_load.max(0.0);
        let cooling_rate = (-self.zone.predicted_cooling_load).max(0.0);

        if !state.flags.warmup {
            self.annual_heating_j += heating_rate * self.timestep_seconds;
            self.annual_cooling_j += cooling_rate * self.timestep_seconds;
            self.peak_heating_w = self.peak_heating_w.max(heating_rate);
            self.peak_cooling_w = self.peak_cooling_w.max(cooling_rate);

            // Store hourly record (at last timestep of each hour)
            if state.clock.timestep_in_hour == self.timesteps_per_hour - 1 {
                self.hourly_data.push(HourlyRecord {
                    month: state.clock.month,
                    day: state.clock.day_of_month,
                    hour: state.clock.hour_of_day,
                    outdoor_temp: state.outdoor.dry_bulb,
                    zone_temp: self.zone.air_state.temperature,
                    heating_rate,
                    cooling_rate,
                    beam_solar_south: state.outdoor.beam_solar,
                    transmitted_solar: 0.0, // Could track this
                });
            }
        }

        // ── Update output variables ──
        let ts = TimeStamp {
            month: state.clock.month as u32,
            day: state.clock.day_of_month as u32,
            hour: state.clock.hour_of_day as u32,
            minute: 0,
        };

        self.output_manager.variables[self.zone.output_indices.temperature]
            .set_value(self.zone.air_state.temperature);
        self.output_manager.variables[self.zone.output_indices.heating_rate]
            .set_value(heating_rate);
        self.output_manager.variables[self.zone.output_indices.cooling_rate]
            .set_value(cooling_rate);

        let time_fraction = 1.0 / self.timesteps_per_hour as f64;
        self.output_manager.update_data(TimeStepType::Zone, time_fraction, &ts);
        self.output_manager.update_data(TimeStepType::System, time_fraction, &ts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a minimal synthetic weather file for testing (1 day of data).
    fn make_test_weather() -> WeatherFile {
        use ep_weather::{Location, WeatherRecord};
        // Denver: lat=39.76, lon=-104.86, tz=-7, elev=1610
        let location = Location {
            name: "Denver".into(),
            latitude: Angle::from_degrees(39.76),
            longitude: Angle::from_degrees(-104.86),
            time_zone: -7.0,
            elevation: Length::new(1610.0),
            station_id: "725650".into(),
        };

        // Create 24 hours of winter weather (January)
        let mut records = Vec::new();
        for hour in 1..=24u8 {
            let frac = (hour as f64 - 1.0) / 24.0;
            // Diurnal temperature variation: -10°C min at 6am, -2°C max at 15:00
            let t_db_c = -6.0 + 4.0 * (2.0 * std::f64::consts::PI * (frac - 0.625)).sin();
            let t_dp_c = t_db_c - 8.0; // dry winter

            // Solar: peak at noon, zero at night
            let solar_frac = if hour >= 8 && hour <= 17 {
                let h = (hour as f64 - 12.5).abs();
                (1.0 - h / 5.0).max(0.0)
            } else {
                0.0
            };

            records.push(WeatherRecord {
                year: 2017,
                month: 1,
                day: 1,
                hour,
                minute: 0,
                dry_bulb: Temperature::from_celsius(t_db_c),
                dew_point: Temperature::from_celsius(t_dp_c),
                rel_humidity: RelativeHumidity::new(40.0),
                pressure: Pressure::new(83400.0), // Denver altitude
                direct_normal_irradiance: Irradiance::new(800.0 * solar_frac),
                diffuse_horizontal_irradiance: Irradiance::new(100.0 * solar_frac),
                global_horizontal_irradiance: Irradiance::new(600.0 * solar_frac),
                wind_speed: Velocity::new(3.0),
                wind_direction: Angle::from_degrees(180.0),
                sky_cover: 2.0,
                opaque_sky_cover: 2.0,
                visibility: Length::new(16000.0),
                ceiling_height: Length::new(77777.0),
                precipitation: 0.0,
                snow_depth: Length::new(0.0),
                is_rain: false,
                is_snow: false,
            });
        }

        WeatherFile { location, records }
    }

    #[test]
    fn bestest600_callback_creates() {
        let weather = make_test_weather();
        let cb = Bestest600Callback::new(weather, 1);

        assert_eq!(cb.surface_states.len(), NUM_SURFACES);
        assert_eq!(cb.surface_ctfs.len(), NUM_SURFACES);
        assert_eq!(cb.surface_histories.len(), NUM_SURFACES);
        assert!((cb.zone.volume - ZONE_VOLUME).abs() < 0.01);
    }

    #[test]
    fn bestest600_ctfs_generated() {
        let weather = make_test_weather();
        let cb = Bestest600Callback::new(weather, 1);

        for (i, ctf) in cb.surface_ctfs.iter().enumerate() {
            assert!(
                ctf.outside[0] > 0.0,
                "CTF outside[0] should be positive for surface {i}: {}",
                ctf.outside[0]
            );
            assert!(
                ctf.inside[0] > 0.0,
                "CTF inside[0] should be positive for surface {i}: {}",
                ctf.inside[0]
            );
        }
    }

    #[test]
    fn bestest600_window_transmittance() {
        let weather = make_test_weather();
        let cb = Bestest600Callback::new(weather, 1);

        // Normal incidence
        let tau_normal = cb.window_beam_transmittance(1.0);
        assert!(tau_normal > 0.5 && tau_normal < 0.85,
            "Normal transmittance should be ~0.74: {tau_normal}");

        // 60° incidence
        let tau_60 = cb.window_beam_transmittance(0.5); // cos(60°)
        assert!(tau_60 < tau_normal, "60° should transmit less than normal");
        assert!(tau_60 > 0.3, "60° transmittance should be reasonable: {tau_60}");

        // Behind surface
        let tau_neg = cb.window_beam_transmittance(-0.5);
        assert!((tau_neg).abs() < 1e-10, "Negative incidence = 0 transmittance");
    }

    #[test]
    fn bestest600_one_timestep_runs() {
        let weather = make_test_weather();
        let mut cb = Bestest600Callback::new(weather, 1);
        let mut state = SimulationState::new(1);

        // Set up clock for January 1, hour 12 (noon)
        state.clock.month = 1;
        state.clock.day_of_month = 1;
        state.clock.hour_of_day = 12;
        state.clock.timestep_in_hour = 0;
        state.clock.day_of_year = 1;

        cb.begin_environment(&mut state);
        cb.do_timestep(&mut state);

        // Zone should have surface coupling
        assert!(cb.zone.surface_ha > 0.0, "surface_ha should be positive: {}", cb.zone.surface_ha);
        assert!(cb.zone.infiltration_mcp > 0.0, "infiltration should be set");

        // HVAC iteration
        let residual = cb.do_hvac_iteration(&mut state);
        assert!(residual.is_finite(), "residual should be finite: {residual}");

        cb.end_timestep(&mut state);

        // Zone temp should be reasonable
        let t = cb.zone.air_state.temperature;
        assert!(t > -50.0 && t < 60.0, "Zone temp should be physical: {t}");
    }

    #[test]
    fn bestest600_full_day_simulation() {
        use crate::{SimulationConfig, SimulationDriver, DesignDayRef};
        use ep_core::state::EnvironmentType;

        let weather = make_test_weather();
        let mut cb = Bestest600Callback::new(weather, 1);

        let config = SimulationConfig {
            timesteps_per_hour: 1,
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

        let mut state = SimulationState::new(1);
        state.clock.month = 1;
        state.clock.day_of_month = 1;
        state.clock.day_of_year = 1;

        let result = driver.run(&mut state, &mut cb);

        assert_eq!(result.environments_completed, 1);
        assert_eq!(result.total_timesteps, 24);

        // Zone should be near setpoint with ideal loads
        let final_temp = cb.zone.air_state.temperature;
        assert!(
            final_temp > 10.0 && final_temp < 35.0,
            "Zone temp after full day should be reasonable: {final_temp}"
        );
    }
}
