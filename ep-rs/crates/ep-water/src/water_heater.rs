//! Water heater models — storage tank and tankless (instantaneous).
//!
//! Tank model uses a mixed (single-node) or stratified (multi-node)
//! energy balance. Tankless model sizes to meet instantaneous demand.

/// Water heater fuel type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuelType {
    Electricity,
    NaturalGas,
    Propane,
    FuelOil,
}

/// Tank model type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TankModel {
    Mixed,
    Stratified,
}

/// Heater control mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaterControl {
    /// Cycle on/off with deadband.
    Cycle,
    /// Modulate to meet load.
    Modulate,
}

/// Storage water heater (tank type).
#[derive(Debug, Clone)]
pub struct StorageWaterHeater {
    pub name: String,
    pub fuel_type: FuelType,
    pub tank_model: TankModel,
    /// Tank volume (m³).
    pub tank_volume: f64,
    /// Setpoint temperature (C).
    pub setpoint_temp: f64,
    /// Deadband temperature difference (K).
    pub deadband_dt: f64,
    /// Maximum heater capacity (W).
    pub max_capacity: f64,
    /// Thermal efficiency at rated conditions.
    pub thermal_efficiency: f64,
    /// Standby loss coefficient UA (W/K).
    pub off_cycle_loss_ua: f64,
    /// On-cycle parasitic power (W).
    pub on_cycle_parasitic: f64,
    /// Off-cycle parasitic power (W).
    pub off_cycle_parasitic: f64,
    /// Heater control mode.
    pub control: HeaterControl,
    /// Current tank temperature (C).
    pub tank_temp: f64,
    /// Ambient temperature around tank (C).
    pub ambient_temp: f64,
    /// Whether heater is currently on.
    pub heater_on: bool,
}

/// Water heater timestep result.
#[derive(Debug, Clone, Copy, Default)]
pub struct WaterHeaterResult {
    /// Final tank temperature (C).
    pub tank_temp: f64,
    /// Heater energy input (J).
    pub heater_energy: f64,
    /// Heater fuel consumption (J).
    pub fuel_energy: f64,
    /// Heater power (W).
    pub heater_power: f64,
    /// Standby loss (W).
    pub standby_loss: f64,
    /// Use side heat transfer (W).
    pub use_heat_rate: f64,
    /// Whether heater cycled on.
    pub heater_on: bool,
    /// Runtime fraction (0-1).
    pub runtime_fraction: f64,
}

impl StorageWaterHeater {
    pub fn new(name: impl Into<String>, tank_volume: f64, max_capacity: f64) -> Self {
        Self {
            name: name.into(),
            fuel_type: FuelType::NaturalGas,
            tank_model: TankModel::Mixed,
            tank_volume,
            setpoint_temp: 60.0,
            deadband_dt: 5.0,
            max_capacity,
            thermal_efficiency: 0.80,
            off_cycle_loss_ua: 5.0, // W/K
            on_cycle_parasitic: 0.0,
            off_cycle_parasitic: 0.0,
            control: HeaterControl::Cycle,
            tank_temp: 60.0,
            ambient_temp: 20.0,
            heater_on: false,
        }
    }

    /// Simulate one timestep using mixed-tank energy balance.
    ///
    /// `use_flow_rate` — hot water draw rate (m³/s).
    /// `inlet_temp` — cold water inlet temperature (C).
    /// `timestep_s` — timestep duration (seconds).
    pub fn simulate(
        &mut self,
        use_flow_rate: f64,
        inlet_temp: f64,
        timestep_s: f64,
    ) -> WaterHeaterResult {
        let cp_water = 4186.0; // J/(kg·K)
        let rho_water = 998.0; // kg/m³
        let tank_mass = self.tank_volume * rho_water;
        let tank_capacity = tank_mass * cp_water; // J/K

        // Use-side heat extraction rate
        let use_mass_flow = use_flow_rate * rho_water;
        let use_heat_rate = use_mass_flow * cp_water * (self.tank_temp - inlet_temp);

        // Standby loss
        let standby_loss = self.off_cycle_loss_ua * (self.tank_temp - self.ambient_temp);

        // Deadband control
        let cut_in_temp = self.setpoint_temp - self.deadband_dt;
        if !self.heater_on && self.tank_temp <= cut_in_temp {
            self.heater_on = true;
        } else if self.heater_on && self.tank_temp >= self.setpoint_temp {
            self.heater_on = false;
        }

        // Heater input
        let heater_power = if self.heater_on {
            match self.control {
                HeaterControl::Cycle => self.max_capacity,
                HeaterControl::Modulate => {
                    let needed = use_heat_rate + standby_loss;
                    needed.clamp(0.0, self.max_capacity)
                }
            }
        } else {
            0.0
        };

        // Energy balance: m*cp*dT/dt = Q_heater - Q_use - Q_standby
        let net_heat_rate = heater_power - use_heat_rate - standby_loss;
        let dt = if tank_capacity > 0.0 {
            net_heat_rate * timestep_s / tank_capacity
        } else {
            0.0
        };
        self.tank_temp += dt;

        // Clamp tank temp to physical limits
        self.tank_temp = self.tank_temp.clamp(inlet_temp, 99.0);

        let fuel_energy = if self.thermal_efficiency > 0.0 {
            heater_power * timestep_s / self.thermal_efficiency
        } else {
            0.0
        };

        let runtime_fraction = if self.max_capacity > 0.0 {
            (heater_power / self.max_capacity).min(1.0)
        } else {
            0.0
        };

        WaterHeaterResult {
            tank_temp: self.tank_temp,
            heater_energy: heater_power * timestep_s,
            fuel_energy,
            heater_power,
            standby_loss,
            use_heat_rate,
            heater_on: self.heater_on,
            runtime_fraction,
        }
    }
}

/// Tankless (instantaneous) water heater.
#[derive(Debug, Clone)]
pub struct TanklessWaterHeater {
    pub name: String,
    pub fuel_type: FuelType,
    /// Maximum heating capacity (W).
    pub max_capacity: f64,
    /// Thermal efficiency.
    pub thermal_efficiency: f64,
    /// Setpoint temperature (C).
    pub setpoint_temp: f64,
    /// Standby power when no flow (W).
    pub standby_power: f64,
}

/// Tankless heater result.
#[derive(Debug, Clone, Copy, Default)]
pub struct TanklessResult {
    /// Outlet temperature (C).
    pub outlet_temp: f64,
    /// Heating power (W).
    pub heater_power: f64,
    /// Fuel consumption rate (W).
    pub fuel_rate: f64,
    /// Whether capacity was sufficient.
    pub capacity_sufficient: bool,
}

impl TanklessWaterHeater {
    pub fn new(name: impl Into<String>, max_capacity: f64) -> Self {
        Self {
            name: name.into(),
            fuel_type: FuelType::NaturalGas,
            max_capacity,
            thermal_efficiency: 0.95,
            setpoint_temp: 60.0,
            standby_power: 5.0,
        }
    }

    /// Calculate instantaneous heating.
    ///
    /// `flow_rate` — water flow rate (m³/s).
    /// `inlet_temp` — cold water inlet temperature (C).
    pub fn calculate(&self, flow_rate: f64, inlet_temp: f64) -> TanklessResult {
        if flow_rate <= 0.0 {
            return TanklessResult {
                outlet_temp: self.setpoint_temp,
                heater_power: self.standby_power,
                fuel_rate: self.standby_power / self.thermal_efficiency,
                capacity_sufficient: true,
            };
        }

        let cp_water = 4186.0;
        let rho_water = 998.0;
        let mass_flow = flow_rate * rho_water;

        let required_power = mass_flow * cp_water * (self.setpoint_temp - inlet_temp);
        let actual_power = required_power.min(self.max_capacity);

        let outlet_temp = if mass_flow * cp_water > 0.0 {
            inlet_temp + actual_power / (mass_flow * cp_water)
        } else {
            self.setpoint_temp
        };

        let fuel_rate = if self.thermal_efficiency > 0.0 {
            actual_power / self.thermal_efficiency
        } else {
            0.0
        };

        TanklessResult {
            outlet_temp,
            heater_power: actual_power,
            fuel_rate,
            capacity_sufficient: required_power <= self.max_capacity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_heater_standby() {
        let mut wh = StorageWaterHeater::new("WH-1", 0.150, 4500.0);
        wh.tank_temp = 60.0;
        wh.ambient_temp = 20.0;

        let result = wh.simulate(0.0, 10.0, 900.0); // No draw, 15 min
        assert!(result.standby_loss > 0.0);
        assert!(result.tank_temp < 60.0); // Cooled slightly
    }

    #[test]
    fn storage_heater_draw() {
        let mut wh = StorageWaterHeater::new("WH-1", 0.150, 4500.0);
        wh.tank_temp = 60.0;

        // Moderate draw
        let result = wh.simulate(0.0001, 10.0, 900.0); // ~6 L/min
        assert!(result.use_heat_rate > 0.0);
        assert!(result.tank_temp < 60.0); // Tank cools
    }

    #[test]
    fn storage_heater_deadband_cycle() {
        let mut wh = StorageWaterHeater::new("WH-1", 0.150, 4500.0);
        wh.setpoint_temp = 60.0;
        wh.deadband_dt = 5.0;
        wh.tank_temp = 54.0; // Below cut-in (55°C)

        let result = wh.simulate(0.0, 10.0, 900.0);
        assert!(result.heater_on); // Should turn on
        assert!(result.heater_power > 0.0);
    }

    #[test]
    fn storage_heater_at_setpoint() {
        let mut wh = StorageWaterHeater::new("WH-1", 0.150, 4500.0);
        wh.tank_temp = 60.0;
        wh.heater_on = true;

        let result = wh.simulate(0.0, 10.0, 900.0);
        // At setpoint, heater should turn off
        assert!(!result.heater_on);
    }

    #[test]
    fn storage_heater_fuel_efficiency() {
        let mut wh = StorageWaterHeater::new("WH-1", 0.150, 4500.0);
        wh.thermal_efficiency = 0.80;
        wh.tank_temp = 50.0; // Triggers heating

        let result = wh.simulate(0.0, 10.0, 900.0);
        if result.heater_power > 0.0 {
            // Fuel = heater energy / efficiency
            let expected_fuel = result.heater_energy / 0.80;
            assert!((result.fuel_energy - expected_fuel).abs() < 1.0);
        }
    }

    #[test]
    fn tankless_basic() {
        let wh = TanklessWaterHeater::new("TL-1", 30000.0);
        let result = wh.calculate(0.0001, 10.0);

        assert!(result.outlet_temp > 10.0);
        assert!(result.heater_power > 0.0);
        assert!(result.capacity_sufficient);
    }

    #[test]
    fn tankless_no_flow() {
        let wh = TanklessWaterHeater::new("TL-1", 30000.0);
        let result = wh.calculate(0.0, 10.0);

        assert!((result.outlet_temp - 60.0).abs() < 0.1);
        assert!((result.heater_power - 5.0).abs() < 0.1); // Standby only
    }

    #[test]
    fn tankless_capacity_limit() {
        let wh = TanklessWaterHeater::new("TL-1", 5000.0); // Small unit
        let result = wh.calculate(0.001, 10.0); // Large flow

        // Can't reach setpoint
        assert!(!result.capacity_sufficient);
        assert!(result.outlet_temp < 60.0);
    }

    #[test]
    fn tankless_efficiency() {
        let mut wh = TanklessWaterHeater::new("TL-1", 30000.0);
        wh.thermal_efficiency = 0.95;
        let result = wh.calculate(0.0001, 10.0);

        assert!(result.fuel_rate > result.heater_power);
        let expected = result.heater_power / 0.95;
        assert!((result.fuel_rate - expected).abs() < 0.1);
    }
}
