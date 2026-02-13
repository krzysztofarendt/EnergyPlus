//! Water heater (tank) models.
//!
//! Implements a mixed (single-node) water heater tank with heat loss
//! to ambient and on/off or modulating burner control.

/// Water heater fuel type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaterHeaterFuel {
    Electric,
    NaturalGas,
    Propane,
}

/// Water heater control mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterHeaterControl {
    /// On/off: heater cycles between setpoint deadband.
    #[default]
    Cycle,
    /// Modulating: heater modulates to maintain setpoint.
    Modulate,
}

/// Mixed (single-node) water heater tank.
#[derive(Debug, Clone)]
pub struct WaterHeater {
    pub name: String,
    pub fuel_type: WaterHeaterFuel,
    pub control: WaterHeaterControl,
    /// Tank volume (m3).
    pub tank_volume: f64,
    /// Setpoint temperature (C).
    pub setpoint_temp: f64,
    /// Deadband temperature below setpoint (C).
    pub deadband: f64,
    /// Maximum heater capacity (W).
    pub max_capacity: f64,
    /// Heater thermal efficiency (0-1).
    pub heater_efficiency: f64,
    /// Off-cycle loss coefficient to ambient (W/K).
    pub off_cycle_loss_coeff: f64,
    /// On-cycle loss coefficient to ambient (W/K).
    pub on_cycle_loss_coeff: f64,
    /// Ambient temperature (C).
    pub ambient_temp: f64,
    /// Current tank temperature (C).
    pub tank_temp: f64,
    /// Use-side design flow rate (kg/s).
    pub use_side_flow: f64,
}

/// Water heater calculation result.
#[derive(Debug, Clone, Copy)]
pub struct WaterHeaterResult {
    /// Heater fuel consumption rate (W).
    pub fuel_rate: f64,
    /// Heat delivered by heater to water (W).
    pub heater_rate: f64,
    /// Tank loss to ambient (W, positive = losing heat).
    pub loss_rate: f64,
    /// Use side heat delivery (W, positive = delivering hot water).
    pub use_rate: f64,
    /// Tank temperature at end of timestep (C).
    pub tank_temp: f64,
    /// Outlet (use side) temperature (C).
    pub outlet_temp: f64,
    /// Heater part-load ratio.
    pub heater_plr: f64,
    /// Runtime fraction.
    pub runtime_fraction: f64,
}

impl WaterHeater {
    pub fn new(
        name: impl Into<String>,
        volume: f64,
        capacity: f64,
        efficiency: f64,
        setpoint: f64,
    ) -> Self {
        Self {
            name: name.into(),
            fuel_type: WaterHeaterFuel::NaturalGas,
            control: WaterHeaterControl::Cycle,
            tank_volume: volume,
            setpoint_temp: setpoint,
            deadband: 2.0,
            max_capacity: capacity,
            heater_efficiency: efficiency.clamp(0.01, 1.0),
            off_cycle_loss_coeff: 5.0,
            on_cycle_loss_coeff: 5.0,
            ambient_temp: 20.0,
            tank_temp: setpoint,
            use_side_flow: 0.0,
        }
    }

    /// Calculate water heater performance for one timestep.
    ///
    /// `use_inlet_temp` — temperature of incoming cold water (C).
    /// `use_mass_flow` — use-side flow rate (kg/s).
    /// `timestep` — duration (s).
    pub fn calculate(
        &mut self,
        use_inlet_temp: f64,
        use_mass_flow: f64,
        timestep: f64,
    ) -> WaterHeaterResult {
        if timestep <= 0.0 || self.tank_volume <= 0.0 {
            return WaterHeaterResult {
                fuel_rate: 0.0,
                heater_rate: 0.0,
                loss_rate: 0.0,
                use_rate: 0.0,
                tank_temp: self.tank_temp,
                outlet_temp: self.tank_temp,
                heater_plr: 0.0,
                runtime_fraction: 0.0,
            };
        }

        let cp = ep_psychrometrics::cp_water(self.tank_temp);
        let rho = 998.0; // water density kg/m3
        let tank_mass = self.tank_volume * rho;

        // Tank thermal capacity (J/K)
        let tank_cap = tank_mass * cp;

        // Heat loss to ambient
        let loss_coeff = self.off_cycle_loss_coeff;
        let loss_rate = loss_coeff * (self.tank_temp - self.ambient_temp);

        // Use-side heat extraction
        let use_rate = if use_mass_flow > 1e-10 {
            use_mass_flow * cp * (self.tank_temp - use_inlet_temp)
        } else {
            0.0
        };

        // Total heat loss from tank (positive = losing heat)
        let total_loss = loss_rate + use_rate;

        // Determine heater requirement
        let heater_needed = if self.tank_temp < (self.setpoint_temp - self.deadband) {
            // Below deadband: need to heat back to setpoint
            let energy_to_setpoint = tank_cap * (self.setpoint_temp - self.tank_temp) / timestep;
            (energy_to_setpoint + total_loss).max(0.0)
        } else if self.tank_temp < self.setpoint_temp {
            // In deadband but cooling: just offset losses
            total_loss.max(0.0)
        } else {
            0.0
        };

        let heater_rate = heater_needed.min(self.max_capacity);
        let heater_plr = if self.max_capacity > 0.0 {
            heater_rate / self.max_capacity
        } else {
            0.0
        };

        // Fuel consumption
        let fuel_rate = heater_rate / self.heater_efficiency;

        // Net heat to tank
        let net_heat = heater_rate - total_loss;

        // Update tank temperature
        let delta_t = net_heat * timestep / tank_cap;
        let new_tank_temp = self.tank_temp + delta_t;

        // Outlet temperature is the tank temperature (mixed tank)
        let outlet_temp = new_tank_temp;

        // Runtime fraction (for cycling control)
        let runtime_fraction = heater_plr;

        self.tank_temp = new_tank_temp;

        WaterHeaterResult {
            fuel_rate,
            heater_rate,
            loss_rate,
            use_rate,
            tank_temp: new_tank_temp,
            outlet_temp,
            heater_plr,
            runtime_fraction,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_heater_basic() {
        let wh = WaterHeater::new("WH-1", 0.3, 10_000.0, 0.80, 60.0);
        assert!((wh.tank_volume - 0.3).abs() < 1e-10);
        assert!((wh.setpoint_temp - 60.0).abs() < 1e-10);
    }

    #[test]
    fn water_heater_at_setpoint_no_draw() {
        let mut wh = WaterHeater::new("Test", 0.3, 10_000.0, 0.80, 60.0);
        wh.tank_temp = 60.0;
        wh.ambient_temp = 20.0;

        let result = wh.calculate(15.0, 0.0, 3600.0);
        // At setpoint, no draw: heater should be off
        assert!(result.heater_rate.abs() < 1e-10, "heater={}", result.heater_rate);
        // But there should be standby losses
        assert!(result.loss_rate > 0.0, "loss={}", result.loss_rate);
        // Tank should cool slightly
        assert!(result.tank_temp < 60.0, "T={}", result.tank_temp);
    }

    #[test]
    fn water_heater_recovery() {
        let mut wh = WaterHeater::new("Test", 0.3, 10_000.0, 0.80, 60.0);
        wh.tank_temp = 50.0; // Below deadband (60 - 2 = 58)
        wh.off_cycle_loss_coeff = 0.0; // No standby loss

        let result = wh.calculate(15.0, 0.0, 3600.0);
        // Heater should be on to recover
        assert!(result.heater_rate > 0.0, "heater={}", result.heater_rate);
        assert!(result.tank_temp > 50.0, "T={}", result.tank_temp);
        assert!(result.fuel_rate > result.heater_rate,
                "fuel={}, heat={}", result.fuel_rate, result.heater_rate);
    }

    #[test]
    fn water_heater_draw() {
        let mut wh = WaterHeater::new("Test", 0.3, 10_000.0, 0.80, 60.0);
        wh.tank_temp = 60.0;
        wh.off_cycle_loss_coeff = 0.0;

        // Draw hot water
        let result = wh.calculate(15.0, 0.1, 600.0); // 10 min draw
        // Use rate should be positive (extracting heat)
        assert!(result.use_rate > 0.0, "use={}", result.use_rate);
        // Tank temp should drop
        assert!(result.tank_temp < 60.0, "T={}", result.tank_temp);
    }

    #[test]
    fn water_heater_fuel_efficiency() {
        let mut wh = WaterHeater::new("Test", 0.3, 10_000.0, 0.80, 60.0);
        wh.tank_temp = 50.0;
        wh.off_cycle_loss_coeff = 0.0;

        let result = wh.calculate(15.0, 0.0, 3600.0);
        // Fuel = heating / efficiency
        if result.heater_rate > 0.0 {
            let expected_fuel = result.heater_rate / 0.80;
            assert!((result.fuel_rate - expected_fuel).abs() < 10.0,
                    "fuel={}, expected={}", result.fuel_rate, expected_fuel);
        }
    }

    #[test]
    fn water_heater_standby_loss() {
        let mut wh = WaterHeater::new("Test", 0.3, 10_000.0, 0.80, 60.0);
        wh.tank_temp = 60.0;
        wh.ambient_temp = 20.0;
        wh.off_cycle_loss_coeff = 10.0; // 10 W/K

        let result = wh.calculate(15.0, 0.0, 3600.0);
        // Loss = UA * (T_tank - T_ambient) = 10 * (60-20) = 400 W
        assert!((result.loss_rate - 400.0).abs() < 10.0,
                "loss={}", result.loss_rate);
    }

    #[test]
    fn water_heater_max_capacity_limit() {
        let mut wh = WaterHeater::new("Test", 0.3, 5_000.0, 0.80, 60.0);
        wh.tank_temp = 20.0; // Very cold
        wh.off_cycle_loss_coeff = 0.0;

        let result = wh.calculate(15.0, 0.0, 3600.0);
        // Heater should be limited to max_capacity = 5000W
        assert!(result.heater_rate <= 5000.0 + 1.0,
                "heater={}", result.heater_rate);
    }
}
