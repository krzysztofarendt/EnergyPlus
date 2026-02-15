//! Pipe models for plant and condenser water loops.
//!
//! - `AdiabaticPipe`: no heat loss, pure pass-through for topology completeness
//! - `PipeHeatTransfer`: UA-based heat loss model for outdoor/underground pipes

/// Pipe calculation result.
#[derive(Debug, Clone, Copy)]
pub struct PipeResult {
    /// Outlet water temperature (C).
    pub outlet_temp: f64,
    /// Heat loss from fluid (W, positive = heat lost).
    pub heat_loss: f64,
}

/// Adiabatic pipe — no heat loss, pure pass-through.
#[derive(Debug, Clone)]
pub struct AdiabaticPipe {
    pub name: String,
}

impl AdiabaticPipe {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// Pass-through: outlet = inlet, zero heat loss.
    pub fn calculate(&self, inlet_temp: f64, _mass_flow: f64) -> PipeResult {
        PipeResult {
            outlet_temp: inlet_temp,
            heat_loss: 0.0,
        }
    }
}

/// Pipe with heat transfer to surroundings via UA model.
///
/// Uses effectiveness-NTU approach:
///   ε = 1 - exp(-UA / (m_dot * cp))
///   T_out = T_env + (T_in - T_env) * exp(-UA / (m_dot * cp))
#[derive(Debug, Clone)]
pub struct PipeHeatTransfer {
    pub name: String,
    /// Overall heat transfer coefficient times area (W/K).
    pub ua: f64,
    /// Environment temperature surrounding the pipe (C).
    pub environment_temp: f64,
    /// Pipe length (m).
    pub length: f64,
    /// Pipe inner diameter (m).
    pub diameter: f64,
}

impl PipeHeatTransfer {
    pub fn new(
        name: impl Into<String>,
        ua: f64,
        environment_temp: f64,
        length: f64,
        diameter: f64,
    ) -> Self {
        Self {
            name: name.into(),
            ua: ua.max(0.0),
            environment_temp,
            length,
            diameter,
        }
    }

    /// Calculate pipe outlet temperature and heat loss.
    pub fn calculate(&self, inlet_temp: f64, mass_flow: f64) -> PipeResult {
        if mass_flow <= 1e-10 || self.ua <= 0.0 {
            return PipeResult {
                outlet_temp: inlet_temp,
                heat_loss: 0.0,
            };
        }

        let cp = ep_psychrometrics::cp_water(inlet_temp);
        let ntu = self.ua / (mass_flow * cp);

        // Effectiveness-NTU for constant wall temperature
        let outlet_temp =
            self.environment_temp + (inlet_temp - self.environment_temp) * (-ntu).exp();

        let heat_loss = mass_flow * cp * (inlet_temp - outlet_temp);

        PipeResult {
            outlet_temp,
            heat_loss,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adiabatic_pipe_passthrough() {
        let pipe = AdiabaticPipe::new("Bypass");
        let result = pipe.calculate(45.0, 5.0);
        assert!((result.outlet_temp - 45.0).abs() < 1e-10);
        assert!(result.heat_loss.abs() < 1e-10);
    }

    #[test]
    fn adiabatic_pipe_zero_flow() {
        let pipe = AdiabaticPipe::new("Bypass");
        let result = pipe.calculate(45.0, 0.0);
        assert!((result.outlet_temp - 45.0).abs() < 1e-10);
    }

    #[test]
    fn pipe_heat_transfer_hot_fluid() {
        // Hot fluid (80 C) in cold environment (5 C) loses heat
        let pipe = PipeHeatTransfer::new("Underground", 100.0, 5.0, 50.0, 0.05);
        let result = pipe.calculate(80.0, 2.0);

        assert!(result.outlet_temp < 80.0, "T_out={}", result.outlet_temp);
        assert!(result.outlet_temp > 5.0, "T_out={}", result.outlet_temp);
        assert!(result.heat_loss > 0.0, "Q_loss={}", result.heat_loss);
    }

    #[test]
    fn pipe_heat_transfer_cold_fluid() {
        // Cold fluid (5 C) in warm environment (25 C) gains heat (negative loss)
        let pipe = PipeHeatTransfer::new("Outdoor", 50.0, 25.0, 30.0, 0.05);
        let result = pipe.calculate(5.0, 3.0);

        assert!(result.outlet_temp > 5.0, "T_out={}", result.outlet_temp);
        assert!(result.outlet_temp < 25.0, "T_out={}", result.outlet_temp);
        assert!(result.heat_loss < 0.0, "Q_loss={}", result.heat_loss);
    }

    #[test]
    fn pipe_heat_transfer_energy_balance() {
        let pipe = PipeHeatTransfer::new("Test", 80.0, 10.0, 40.0, 0.05);
        let mdot = 4.0;
        let t_in = 60.0;
        let result = pipe.calculate(t_in, mdot);

        let cp = ep_psychrometrics::cp_water(t_in);
        let expected_loss = mdot * cp * (t_in - result.outlet_temp);
        assert!(
            (result.heat_loss - expected_loss).abs() < 1.0,
            "Q_loss={}, expected={}",
            result.heat_loss,
            expected_loss
        );
    }

    #[test]
    fn pipe_heat_transfer_no_flow() {
        let pipe = PipeHeatTransfer::new("Test", 100.0, 10.0, 50.0, 0.05);
        let result = pipe.calculate(60.0, 0.0);
        assert!((result.outlet_temp - 60.0).abs() < 1e-10);
        assert!(result.heat_loss.abs() < 1e-10);
    }

    #[test]
    fn pipe_heat_transfer_high_ua_approaches_env() {
        // Very high UA → outlet approaches environment temp
        let pipe = PipeHeatTransfer::new("HighUA", 100_000.0, 15.0, 100.0, 0.1);
        let result = pipe.calculate(80.0, 1.0);
        assert!(
            (result.outlet_temp - 15.0).abs() < 1.0,
            "T_out={}, expected ~15.0",
            result.outlet_temp
        );
    }
}
