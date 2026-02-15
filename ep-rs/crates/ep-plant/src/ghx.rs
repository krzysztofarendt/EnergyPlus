//! Ground heat exchanger models.
//!
//! - `VerticalBorehole`: cylindrical source model with g-functions for
//!   temporal superposition of load pulses and borehole resistance
//! - `SlinkyGhx`: horizontal ring source model for spiral pipes
//! - `SurfaceGhx`: simple UA model for pond/lake surface heat exchange

use std::f64::consts::PI;

/// Vertical borehole ground heat exchanger.
///
/// Uses g-function temporal superposition: the fluid outlet temperature
/// is computed from accumulated load history and thermal response factors.
///
/// T_fluid = T_ground + Q * R_borehole + Σ(q_i * g(t-t_i)) / (2π k_s H)
#[derive(Debug, Clone)]
pub struct VerticalBorehole {
    pub name: String,
    /// Number of boreholes.
    pub num_boreholes: usize,
    /// Borehole depth (m).
    pub depth: f64,
    /// Borehole radius (m).
    pub borehole_radius: f64,
    /// U-tube pipe outer radius (m).
    pub pipe_outer_radius: f64,
    /// U-tube pipe inner radius (m).
    pub pipe_inner_radius: f64,
    /// Center-to-center half-distance between U-tube legs (m).
    pub shank_spacing: f64,
    /// Soil thermal conductivity (W/m-K).
    pub soil_conductivity: f64,
    /// Soil thermal diffusivity (m²/s).
    pub soil_diffusivity: f64,
    /// Undisturbed ground temperature (C).
    pub ground_temp: f64,
    /// Grout thermal conductivity (W/m-K).
    pub grout_conductivity: f64,
    /// Pipe thermal conductivity (W/m-K).
    pub pipe_conductivity: f64,
    /// Design fluid flow rate through all boreholes (kg/s).
    pub design_flow: f64,
    /// Load history for temporal superposition (W per pulse).
    load_history: Vec<f64>,
    /// Time step for load aggregation (s).
    pub aggregation_timestep: f64,
}

/// Result of borehole GHX calculation.
#[derive(Debug, Clone, Copy)]
pub struct BoreholeResult {
    /// Fluid outlet temperature (C).
    pub outlet_temp: f64,
    /// Heat transfer rate to ground (W, positive = heat rejected).
    pub heat_rate: f64,
    /// Borehole wall temperature (C).
    pub borehole_wall_temp: f64,
    /// Effective borehole resistance (m-K/W).
    pub borehole_resistance: f64,
}

impl VerticalBorehole {
    pub fn new(
        name: impl Into<String>,
        num_boreholes: usize,
        depth: f64,
        soil_conductivity: f64,
        soil_diffusivity: f64,
        ground_temp: f64,
    ) -> Self {
        Self {
            name: name.into(),
            num_boreholes: num_boreholes.max(1),
            depth,
            borehole_radius: 0.075,
            pipe_outer_radius: 0.0167,
            pipe_inner_radius: 0.0137,
            shank_spacing: 0.025,
            soil_conductivity,
            soil_diffusivity,
            ground_temp,
            grout_conductivity: 1.0,
            pipe_conductivity: 0.4,
            design_flow: 1.0,
            load_history: Vec::new(),
            aggregation_timestep: 3600.0,
        }
    }

    /// Calculate borehole thermal resistance (m-K/W per borehole).
    ///
    /// Simplified model: R_b = R_grout + R_pipe
    /// R_grout = ln(r_borehole / r_pipe_outer) / (2π k_grout)
    /// R_pipe = ln(r_pipe_outer / r_pipe_inner) / (2π k_pipe)
    pub fn borehole_resistance(&self) -> f64 {
        let r_grout = if self.grout_conductivity > 0.0 {
            (self.borehole_radius / self.pipe_outer_radius).ln()
                / (2.0 * PI * self.grout_conductivity)
        } else {
            0.0
        };

        let r_pipe = if self.pipe_conductivity > 0.0 {
            (self.pipe_outer_radius / self.pipe_inner_radius).ln()
                / (2.0 * PI * self.pipe_conductivity)
        } else {
            0.0
        };

        // Two legs in parallel, so divide by 2
        (r_grout + r_pipe) / 2.0
    }

    /// Evaluate the cylindrical source g-function at dimensionless time.
    ///
    /// G(t*) ≈ -Ei(-1/(4*t*)) for t* = α*t/r² (infinite line source approx)
    /// where Ei is the exponential integral, approximated as:
    /// G ≈ ln(4*t*) - γ for t* > 5 (Euler's constant γ ≈ 0.5772)
    fn g_function(&self, time_s: f64) -> f64 {
        if time_s <= 0.0 {
            return 0.0;
        }

        let t_star =
            self.soil_diffusivity * time_s / (self.borehole_radius * self.borehole_radius);

        if t_star < 1e-10 {
            return 0.0;
        }

        // Infinite line source approximation (valid for t* > ~5)
        // G = ln(4*t*) - γ where γ = Euler-Mascheroni constant
        let euler_gamma = 0.5772;
        let g = (4.0 * t_star).ln() - euler_gamma;
        g.max(0.0)
    }

    /// Add a load pulse to the history for temporal superposition.
    pub fn add_load_pulse(&mut self, load_w: f64) {
        self.load_history.push(load_w);
    }

    /// Clear load history.
    pub fn clear_history(&mut self) {
        self.load_history.clear();
    }

    /// Calculate GHX outlet temperature at current conditions.
    ///
    /// `inlet_temp`: fluid entering temperature (C)
    /// `mass_flow`: fluid flow rate (kg/s)
    /// `current_load`: current heat rejection/extraction (W, positive = rejection)
    pub fn calculate(
        &mut self,
        inlet_temp: f64,
        mass_flow: f64,
        current_load: f64,
    ) -> BoreholeResult {
        let r_borehole = self.borehole_resistance();
        let total_depth = self.depth * self.num_boreholes as f64;

        if mass_flow <= 1e-10 || total_depth <= 0.0 {
            return BoreholeResult {
                outlet_temp: inlet_temp,
                heat_rate: 0.0,
                borehole_wall_temp: self.ground_temp,
                borehole_resistance: r_borehole,
            };
        }

        // Temporal superposition: compute ground temperature perturbation
        // ΔT_ground = Σ (q_i - q_{i-1}) * g(t - t_i) / (2π k_s H_total)
        let mut delta_t_ground = 0.0;
        let n = self.load_history.len();
        let two_pi_k_h = 2.0 * PI * self.soil_conductivity * total_depth;

        if two_pi_k_h > 0.0 {
            for i in 0..n {
                let q_i = self.load_history[i];
                let q_prev = if i > 0 { self.load_history[i - 1] } else { 0.0 };
                let delta_q = q_i - q_prev;
                let elapsed = (n - i) as f64 * self.aggregation_timestep;
                let g = self.g_function(elapsed);
                delta_t_ground += delta_q * g / two_pi_k_h;
            }

            // Add current load contribution
            let g_current = self.g_function(self.aggregation_timestep);
            let delta_q_current = current_load - if n > 0 { self.load_history[n - 1] } else { 0.0 };
            delta_t_ground += delta_q_current * g_current / two_pi_k_h;
        }

        let borehole_wall_temp = self.ground_temp + delta_t_ground;

        // Fluid temperature: T_fluid_avg = T_borehole_wall + Q * R_b / H_total
        let q_per_length = current_load / total_depth;
        let t_fluid_avg = borehole_wall_temp + q_per_length * r_borehole;

        // Outlet from energy balance: T_out = T_fluid_avg * 2 - T_in
        // (assuming T_avg = (T_in + T_out) / 2)
        let outlet_temp = 2.0 * t_fluid_avg - inlet_temp;

        // Store current load
        self.add_load_pulse(current_load);

        BoreholeResult {
            outlet_temp,
            heat_rate: current_load,
            borehole_wall_temp,
            borehole_resistance: r_borehole,
        }
    }
}

/// Horizontal slinky (ring source) ground heat exchanger.
///
/// Simplified model using effective length and ground temperature.
#[derive(Debug, Clone)]
pub struct SlinkyGhx {
    pub name: String,
    /// Total trench length (m).
    pub trench_length: f64,
    /// Coil diameter (m).
    pub coil_diameter: f64,
    /// Coil pitch (m, spacing between loops).
    pub coil_pitch: f64,
    /// Burial depth (m).
    pub burial_depth: f64,
    /// Soil thermal conductivity (W/m-K).
    pub soil_conductivity: f64,
    /// Undisturbed ground temperature (C).
    pub ground_temp: f64,
    /// Pipe outer diameter (m).
    pub pipe_diameter: f64,
    /// Design flow rate (kg/s).
    pub design_flow: f64,
}

/// Result of slinky GHX calculation.
#[derive(Debug, Clone, Copy)]
pub struct SlinkyResult {
    /// Fluid outlet temperature (C).
    pub outlet_temp: f64,
    /// Heat transfer rate (W, positive = rejected to ground).
    pub heat_rate: f64,
}

impl SlinkyGhx {
    pub fn new(
        name: impl Into<String>,
        trench_length: f64,
        coil_diameter: f64,
        soil_conductivity: f64,
        ground_temp: f64,
    ) -> Self {
        Self {
            name: name.into(),
            trench_length,
            coil_diameter,
            coil_pitch: 0.3,
            burial_depth: 1.5,
            soil_conductivity,
            ground_temp,
            pipe_diameter: 0.025,
            design_flow: 1.0,
        }
    }

    /// Calculate slinky GHX performance.
    ///
    /// Uses a simple UA-effectiveness model based on effective pipe length.
    pub fn calculate(&self, inlet_temp: f64, mass_flow: f64) -> SlinkyResult {
        if mass_flow <= 1e-10 || self.trench_length <= 0.0 {
            return SlinkyResult {
                outlet_temp: inlet_temp,
                heat_rate: 0.0,
            };
        }

        // Effective pipe length from coil geometry
        let num_coils = self.trench_length / self.coil_pitch.max(0.01);
        let pipe_length = num_coils * PI * self.coil_diameter;

        // Simple UA from soil conductivity and pipe geometry
        // UA ≈ 2π k_soil L / ln(2 * burial_depth / pipe_radius)
        let pipe_radius = self.pipe_diameter / 2.0;
        let ln_factor = (2.0 * self.burial_depth / pipe_radius.max(0.001)).ln();
        let ua = if ln_factor > 0.0 {
            2.0 * PI * self.soil_conductivity * pipe_length / ln_factor
        } else {
            0.0
        };

        if ua <= 0.0 {
            return SlinkyResult {
                outlet_temp: inlet_temp,
                heat_rate: 0.0,
            };
        }

        let cp = ep_psychrometrics::cp_water(inlet_temp);
        let ntu = ua / (mass_flow * cp);
        let effectiveness = 1.0 - (-ntu).exp();

        let q_max = mass_flow * cp * (inlet_temp - self.ground_temp);
        let heat_rate = effectiveness * q_max;
        let outlet_temp = inlet_temp - heat_rate / (mass_flow * cp);

        SlinkyResult {
            outlet_temp,
            heat_rate,
        }
    }
}

/// Simple surface/pond ground heat exchanger.
///
/// UA model for heat exchange with a body of water or ground surface.
#[derive(Debug, Clone)]
pub struct SurfaceGhx {
    pub name: String,
    /// Overall UA value (W/K).
    pub ua: f64,
    /// Surface/pond temperature (C).
    pub surface_temp: f64,
    /// Design flow rate (kg/s).
    pub design_flow: f64,
}

impl SurfaceGhx {
    pub fn new(name: impl Into<String>, ua: f64, surface_temp: f64) -> Self {
        Self {
            name: name.into(),
            ua: ua.max(0.0),
            surface_temp,
            design_flow: 1.0,
        }
    }

    /// Calculate surface GHX outlet temperature.
    pub fn calculate(&self, inlet_temp: f64, mass_flow: f64) -> SlinkyResult {
        if mass_flow <= 1e-10 || self.ua <= 0.0 {
            return SlinkyResult {
                outlet_temp: inlet_temp,
                heat_rate: 0.0,
            };
        }

        let cp = ep_psychrometrics::cp_water(inlet_temp);
        let ntu = self.ua / (mass_flow * cp);
        let effectiveness = 1.0 - (-ntu).exp();

        let q_max = mass_flow * cp * (inlet_temp - self.surface_temp);
        let heat_rate = effectiveness * q_max;
        let outlet_temp = inlet_temp - heat_rate / (mass_flow * cp);

        SlinkyResult {
            outlet_temp,
            heat_rate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── VerticalBorehole ───

    #[test]
    fn borehole_resistance_positive() {
        let ghx = VerticalBorehole::new("BH1", 4, 100.0, 2.0, 1e-6, 12.0);
        let rb = ghx.borehole_resistance();
        assert!(rb > 0.0, "R_borehole={}", rb);
        // Typical range: 0.05 - 0.3 m-K/W
        assert!(rb < 1.0, "R_borehole={} too high", rb);
    }

    #[test]
    fn borehole_no_load_returns_ground_temp_region() {
        let mut ghx = VerticalBorehole::new("BH", 4, 100.0, 2.0, 1e-6, 12.0);
        let result = ghx.calculate(12.0, 2.0, 0.0);
        // With zero load, outlet should be near ground temp
        assert!(
            (result.outlet_temp - 12.0).abs() < 2.0,
            "T_out={}, expected ~12.0",
            result.outlet_temp
        );
    }

    #[test]
    fn borehole_heat_rejection_cools_fluid() {
        let mut ghx = VerticalBorehole::new("BH", 4, 100.0, 2.0, 1e-6, 12.0);
        // Rejecting heat (cooling mode): fluid enters warm, load positive
        let result = ghx.calculate(35.0, 3.0, 20_000.0);

        // With heat rejection, fluid gives up heat to ground → outlet < inlet
        assert!(
            result.outlet_temp < 35.0,
            "T_out={}, expected < 35.0",
            result.outlet_temp
        );
        assert!(result.heat_rate > 0.0);
    }

    #[test]
    fn borehole_no_flow() {
        let mut ghx = VerticalBorehole::new("BH", 4, 100.0, 2.0, 1e-6, 12.0);
        let result = ghx.calculate(35.0, 0.0, 10_000.0);
        assert!((result.outlet_temp - 35.0).abs() < 1e-10);
        assert!(result.heat_rate.abs() < 1e-10);
    }

    #[test]
    fn borehole_history_accumulates() {
        let mut ghx = VerticalBorehole::new("BH", 4, 100.0, 2.0, 1e-6, 12.0);

        // First pulse
        let r1 = ghx.calculate(30.0, 2.0, 10_000.0);

        // Second pulse at same conditions — history effect makes wall warmer
        let r2 = ghx.calculate(30.0, 2.0, 10_000.0);

        // With accumulated history, ground is warmer so outlet is warmer
        assert!(
            r2.borehole_wall_temp >= r1.borehole_wall_temp - 0.1,
            "T_wall2={} should >= T_wall1={}",
            r2.borehole_wall_temp,
            r1.borehole_wall_temp
        );
    }

    // ─── SlinkyGhx ───

    #[test]
    fn slinky_heat_rejection() {
        let ghx = SlinkyGhx::new("Slinky", 50.0, 1.0, 1.5, 12.0);
        let result = ghx.calculate(35.0, 2.0);

        // Hot fluid, cold ground → heat rejected, outlet cools
        assert!(result.outlet_temp < 35.0, "T_out={}", result.outlet_temp);
        assert!(result.outlet_temp > 12.0, "T_out={}", result.outlet_temp);
        assert!(result.heat_rate > 0.0, "Q={}", result.heat_rate);
    }

    #[test]
    fn slinky_heat_extraction() {
        let ghx = SlinkyGhx::new("Slinky", 50.0, 1.0, 1.5, 12.0);
        let result = ghx.calculate(5.0, 2.0);

        // Cold fluid, warm ground → heat extracted, outlet warms
        assert!(result.outlet_temp > 5.0, "T_out={}", result.outlet_temp);
        assert!(result.heat_rate < 0.0, "Q={} should be negative", result.heat_rate);
    }

    #[test]
    fn slinky_no_flow() {
        let ghx = SlinkyGhx::new("Slinky", 50.0, 1.0, 1.5, 12.0);
        let result = ghx.calculate(35.0, 0.0);
        assert!((result.outlet_temp - 35.0).abs() < 1e-10);
    }

    // ─── SurfaceGhx ───

    #[test]
    fn surface_ghx_cooling() {
        let ghx = SurfaceGhx::new("Pond", 5000.0, 15.0);
        let result = ghx.calculate(30.0, 3.0);

        // Hot fluid, cool pond → fluid cools
        assert!(result.outlet_temp < 30.0, "T_out={}", result.outlet_temp);
        assert!(result.outlet_temp > 15.0, "T_out={}", result.outlet_temp);
        assert!(result.heat_rate > 0.0);
    }

    #[test]
    fn surface_ghx_energy_balance() {
        let ghx = SurfaceGhx::new("Pond", 5000.0, 15.0);
        let mdot = 3.0;
        let t_in = 30.0;
        let result = ghx.calculate(t_in, mdot);

        let cp = ep_psychrometrics::cp_water(t_in);
        let expected_q = mdot * cp * (t_in - result.outlet_temp);
        assert!(
            (result.heat_rate - expected_q).abs() < 1.0,
            "Q={}, expected={}",
            result.heat_rate,
            expected_q
        );
    }
}
