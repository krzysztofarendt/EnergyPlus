//! Evaporative cooler models.
//!
//! Direct (CelDekPad) and indirect (wet coil) evaporative coolers.

/// Direct evaporative cooler (CelDekPad).
///
/// Cools air adiabatically toward the wet-bulb temperature.
#[derive(Debug, Clone)]
pub struct DirectEvapCooler {
    pub name: String,
    /// Pad effectiveness (0-1). Typical 0.7-0.9.
    pub effectiveness: f64,
    /// Recirculating water pump power (W).
    pub pump_power: f64,
}

/// Evaporative cooler result.
#[derive(Debug, Clone, Copy)]
pub struct EvapCoolerResult {
    /// Outlet dry-bulb temperature (C).
    pub outlet_temp: f64,
    /// Outlet humidity ratio (kg/kg).
    pub outlet_w: f64,
    /// Cooling rate (W, sensible).
    pub cooling_rate: f64,
    /// Parasitic power (W, pump).
    pub parasitic_power: f64,
    /// Water consumption rate (kg/s).
    pub water_consumption: f64,
}

impl DirectEvapCooler {
    pub fn new(name: impl Into<String>, effectiveness: f64) -> Self {
        Self {
            name: name.into(),
            effectiveness: effectiveness.clamp(0.0, 1.0),
            pump_power: 50.0,
        }
    }

    /// Calculate direct evaporative cooler performance.
    ///
    /// `inlet_db` — inlet dry-bulb (C).
    /// `inlet_wb` — inlet wet-bulb (C).
    /// `inlet_w` — inlet humidity ratio (kg/kg).
    /// `mass_flow` — air mass flow (kg/s).
    pub fn calculate(
        &self,
        inlet_db: f64,
        inlet_wb: f64,
        inlet_w: f64,
        mass_flow: f64,
    ) -> EvapCoolerResult {
        if mass_flow <= 1e-10 || inlet_db <= inlet_wb {
            return EvapCoolerResult {
                outlet_temp: inlet_db, outlet_w: inlet_w,
                cooling_rate: 0.0, parasitic_power: 0.0, water_consumption: 0.0,
            };
        }

        // Outlet DB approaches WB: T_out = T_db - eff * (T_db - T_wb)
        let outlet_db = inlet_db - self.effectiveness * (inlet_db - inlet_wb);

        // Sensible cooling
        let cp = ep_psychrometrics::cp_air(inlet_w);
        let q_sensible = mass_flow * cp * (inlet_db - outlet_db);

        // Moisture added (adiabatic process: enthalpy approximately constant)
        let h_fg = 2_501_000.0;
        let dw = q_sensible / (mass_flow * h_fg).max(1e-10);
        let outlet_w = inlet_w + dw;

        // Water consumption = moisture added to air
        let water_consumption = mass_flow * dw;

        EvapCoolerResult {
            outlet_temp: outlet_db,
            outlet_w,
            cooling_rate: q_sensible,
            parasitic_power: self.pump_power,
            water_consumption,
        }
    }
}

/// Indirect evaporative cooler (wet coil).
///
/// Uses a secondary air stream (wetted) to cool primary air
/// without adding moisture to the primary air.
#[derive(Debug, Clone)]
pub struct IndirectEvapCooler {
    pub name: String,
    /// Wet-coil effectiveness (0-1).
    pub effectiveness: f64,
    /// Secondary fan power (W).
    pub secondary_fan_power: f64,
    /// Pump power (W).
    pub pump_power: f64,
    /// Secondary-to-primary air flow ratio.
    pub secondary_flow_ratio: f64,
}

impl IndirectEvapCooler {
    pub fn new(name: impl Into<String>, effectiveness: f64) -> Self {
        Self {
            name: name.into(),
            effectiveness: effectiveness.clamp(0.0, 1.0),
            secondary_fan_power: 200.0,
            pump_power: 50.0,
            secondary_flow_ratio: 1.0,
        }
    }

    /// Calculate indirect evaporative cooler performance.
    ///
    /// Primary air is cooled without moisture addition.
    /// Secondary air (outdoor) is wetted and used as the cooling medium.
    pub fn calculate(
        &self,
        primary_inlet_db: f64,
        primary_inlet_w: f64,
        primary_mass_flow: f64,
        secondary_wb: f64,
    ) -> EvapCoolerResult {
        if primary_mass_flow <= 1e-10 || primary_inlet_db <= secondary_wb {
            return EvapCoolerResult {
                outlet_temp: primary_inlet_db, outlet_w: primary_inlet_w,
                cooling_rate: 0.0, parasitic_power: 0.0, water_consumption: 0.0,
            };
        }

        // Primary outlet approaches secondary WB
        let outlet_db = primary_inlet_db - self.effectiveness * (primary_inlet_db - secondary_wb);

        let cp = ep_psychrometrics::cp_air(primary_inlet_w);
        let q_sensible = primary_mass_flow * cp * (primary_inlet_db - outlet_db);

        // Secondary air evaporates water
        let h_fg = 2_501_000.0;
        let secondary_flow = primary_mass_flow * self.secondary_flow_ratio;
        let water_consumption = q_sensible / h_fg;

        EvapCoolerResult {
            outlet_temp: outlet_db,
            outlet_w: primary_inlet_w, // No moisture added to primary
            cooling_rate: q_sensible,
            parasitic_power: self.secondary_fan_power + self.pump_power,
            water_consumption: water_consumption.max(0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_evap_basic() {
        let ec = DirectEvapCooler::new("DEC", 0.85);
        let result = ec.calculate(35.0, 22.0, 0.008, 1.0);
        // T_out = 35 - 0.85*(35-22) = 35 - 11.05 = 23.95
        assert!((result.outlet_temp - 23.95).abs() < 0.1, "T={}", result.outlet_temp);
        assert!(result.cooling_rate > 0.0);
        assert!(result.outlet_w > 0.008, "Moisture added: W={}", result.outlet_w);
    }

    #[test]
    fn direct_evap_no_flow() {
        let ec = DirectEvapCooler::new("DEC", 0.85);
        let result = ec.calculate(35.0, 22.0, 0.008, 0.0);
        assert!(result.cooling_rate.abs() < 1e-10);
    }

    #[test]
    fn direct_evap_saturated() {
        // DB = WB: no cooling possible
        let ec = DirectEvapCooler::new("DEC", 0.85);
        let result = ec.calculate(22.0, 22.0, 0.008, 1.0);
        assert!(result.cooling_rate.abs() < 1e-10);
    }

    #[test]
    fn indirect_evap_basic() {
        let ec = IndirectEvapCooler::new("IEC", 0.7);
        let result = ec.calculate(35.0, 0.008, 1.0, 22.0);
        // T_out = 35 - 0.7*(35-22) = 35 - 9.1 = 25.9
        assert!((result.outlet_temp - 25.9).abs() < 0.1, "T={}", result.outlet_temp);
        assert!(result.cooling_rate > 0.0);
        // No moisture added to primary
        assert!((result.outlet_w - 0.008).abs() < 1e-6, "W={}", result.outlet_w);
    }

    #[test]
    fn indirect_evap_no_flow() {
        let ec = IndirectEvapCooler::new("IEC", 0.7);
        let result = ec.calculate(35.0, 0.008, 0.0, 22.0);
        assert!(result.cooling_rate.abs() < 1e-10);
    }

    #[test]
    fn indirect_evap_parasitic_power() {
        let ec = IndirectEvapCooler::new("IEC", 0.7);
        let result = ec.calculate(35.0, 0.008, 1.0, 22.0);
        assert!(result.parasitic_power > 0.0, "P={}", result.parasitic_power);
    }

    #[test]
    fn direct_evap_water_consumption() {
        let ec = DirectEvapCooler::new("DEC", 0.85);
        let result = ec.calculate(35.0, 22.0, 0.008, 2.0);
        assert!(result.water_consumption > 0.0, "water={}", result.water_consumption);
    }
}
