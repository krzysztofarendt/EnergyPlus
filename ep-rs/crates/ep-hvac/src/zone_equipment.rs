//! Zone-level HVAC equipment models.
//!
//! Terminal units (VAV boxes, single duct), baseboard heaters,
//! and zone equipment list management.

/// Terminal unit type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalType {
    /// Single duct constant volume.
    SingleDuctConstantVolume,
    /// Single duct VAV with reheat.
    SingleDuctVAVReheat,
    /// Single duct VAV no-reheat.
    SingleDuctVAVNoReheat,
}

/// Reheat coil type for terminal units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReheatType {
    None,
    Electric,
    HotWater,
    Gas,
}

/// Single-duct terminal unit (air terminal).
#[derive(Debug, Clone)]
pub struct SingleDuctTerminal {
    pub name: String,
    pub terminal_type: TerminalType,
    pub reheat_type: ReheatType,
    /// Maximum air flow rate (m3/s).
    pub max_air_flow: f64,
    /// Minimum air flow fraction for VAV (0-1).
    pub min_flow_fraction: f64,
    /// Reheat coil capacity (W).
    pub reheat_capacity: f64,
    /// Zone supply node.
    pub inlet_node: usize,
    /// Zone air node.
    pub outlet_node: usize,
    /// Reheat coil outlet node.
    pub reheat_outlet_node: usize,
}

/// Terminal unit calculation result.
#[derive(Debug, Clone, Copy)]
pub struct TerminalResult {
    /// Air mass flow rate to zone (kg/s).
    pub air_mass_flow: f64,
    /// Supply air temperature to zone (C).
    pub supply_temp: f64,
    /// Reheat energy (W).
    pub reheat_rate: f64,
    /// Damper position (0-1).
    pub damper_position: f64,
}

impl SingleDuctTerminal {
    /// Create a constant-volume terminal.
    pub fn constant_volume(
        name: impl Into<String>,
        max_flow: f64,
        inlet: usize,
        outlet: usize,
    ) -> Self {
        Self {
            name: name.into(),
            terminal_type: TerminalType::SingleDuctConstantVolume,
            reheat_type: ReheatType::None,
            max_air_flow: max_flow,
            min_flow_fraction: 1.0,
            reheat_capacity: 0.0,
            inlet_node: inlet,
            outlet_node: outlet,
            reheat_outlet_node: outlet,
        }
    }

    /// Create a VAV terminal with reheat.
    pub fn vav_reheat(
        name: impl Into<String>,
        max_flow: f64,
        min_fraction: f64,
        reheat_capacity: f64,
        inlet: usize,
        outlet: usize,
    ) -> Self {
        Self {
            name: name.into(),
            terminal_type: TerminalType::SingleDuctVAVReheat,
            reheat_type: ReheatType::Electric,
            max_air_flow: max_flow,
            min_flow_fraction: min_fraction,
            reheat_capacity,
            inlet_node: inlet,
            outlet_node: outlet,
            reheat_outlet_node: outlet,
        }
    }

    /// Create a VAV terminal without reheat.
    pub fn vav_no_reheat(
        name: impl Into<String>,
        max_flow: f64,
        min_fraction: f64,
        inlet: usize,
        outlet: usize,
    ) -> Self {
        Self {
            name: name.into(),
            terminal_type: TerminalType::SingleDuctVAVNoReheat,
            reheat_type: ReheatType::None,
            max_air_flow: max_flow,
            min_flow_fraction: min_fraction,
            reheat_capacity: 0.0,
            inlet_node: inlet,
            outlet_node: outlet,
            reheat_outlet_node: outlet,
        }
    }

    /// Calculate terminal unit performance.
    ///
    /// `supply_temp` — supply air temperature from AHU (C).
    /// `zone_temp` — current zone air temperature (C).
    /// `zone_load` — zone sensible load (W, positive = heating, negative = cooling).
    /// `air_density` — air density (kg/m3).
    pub fn calculate(
        &self,
        supply_temp: f64,
        supply_w: f64,
        zone_temp: f64,
        zone_load: f64,
        air_density: f64,
    ) -> TerminalResult {
        let cp = ep_psychrometrics::cp_air(supply_w);
        let max_mass_flow = self.max_air_flow * air_density;
        let min_mass_flow = self.max_air_flow * self.min_flow_fraction * air_density;

        match self.terminal_type {
            TerminalType::SingleDuctConstantVolume => {
                // Always at max flow when on
                let reheat = if zone_load > 0.0 && self.reheat_capacity > 0.0 {
                    zone_load.min(self.reheat_capacity)
                } else {
                    0.0
                };
                let t_supply = supply_temp + reheat / (max_mass_flow * cp).max(1e-10);

                TerminalResult {
                    air_mass_flow: max_mass_flow,
                    supply_temp: t_supply,
                    reheat_rate: reheat,
                    damper_position: 1.0,
                }
            }

            TerminalType::SingleDuctVAVReheat => {
                if zone_load < 0.0 {
                    // Cooling mode: modulate damper to meet cooling load
                    let needed_flow = if (supply_temp - zone_temp).abs() > 0.1 {
                        (-zone_load / (cp * (zone_temp - supply_temp))).clamp(min_mass_flow, max_mass_flow)
                    } else {
                        max_mass_flow
                    };

                    TerminalResult {
                        air_mass_flow: needed_flow,
                        supply_temp,
                        reheat_rate: 0.0,
                        damper_position: needed_flow / max_mass_flow.max(1e-10),
                    }
                } else {
                    // Heating mode: minimum flow + reheat
                    let reheat = zone_load.min(self.reheat_capacity);
                    let t_supply = supply_temp + reheat / (min_mass_flow * cp).max(1e-10);

                    TerminalResult {
                        air_mass_flow: min_mass_flow,
                        supply_temp: t_supply,
                        reheat_rate: reheat,
                        damper_position: self.min_flow_fraction,
                    }
                }
            }

            TerminalType::SingleDuctVAVNoReheat => {
                if zone_load < 0.0 {
                    // Cooling: modulate damper
                    let needed_flow = if (supply_temp - zone_temp).abs() > 0.1 {
                        (-zone_load / (cp * (zone_temp - supply_temp))).clamp(min_mass_flow, max_mass_flow)
                    } else {
                        max_mass_flow
                    };

                    TerminalResult {
                        air_mass_flow: needed_flow,
                        supply_temp,
                        reheat_rate: 0.0,
                        damper_position: needed_flow / max_mass_flow.max(1e-10),
                    }
                } else {
                    // Heating: go to minimum flow, no reheat
                    TerminalResult {
                        air_mass_flow: min_mass_flow,
                        supply_temp,
                        reheat_rate: 0.0,
                        damper_position: self.min_flow_fraction,
                    }
                }
            }
        }
    }
}

/// Hot water baseboard heater.
#[derive(Debug, Clone)]
pub struct Baseboard {
    pub name: String,
    /// Rated capacity (W).
    pub rated_capacity: f64,
    /// UA-value (W/K).
    pub ua: f64,
    /// Design water flow (kg/s).
    pub design_water_flow: f64,
}

/// Baseboard calculation result.
#[derive(Debug, Clone, Copy)]
pub struct BaseboardResult {
    /// Heat output to zone (W).
    pub heating_rate: f64,
    /// Water outlet temperature (C).
    pub water_outlet_temp: f64,
}

impl Baseboard {
    pub fn new(name: impl Into<String>, capacity: f64, ua: f64, water_flow: f64) -> Self {
        Self {
            name: name.into(),
            rated_capacity: capacity,
            ua,
            design_water_flow: water_flow,
        }
    }

    /// Calculate baseboard performance.
    pub fn calculate(
        &self,
        zone_temp: f64,
        water_inlet_temp: f64,
        water_mass_flow: f64,
    ) -> BaseboardResult {
        if water_mass_flow <= 1e-10 || water_inlet_temp <= zone_temp {
            return BaseboardResult {
                heating_rate: 0.0,
                water_outlet_temp: water_inlet_temp,
            };
        }

        let cp_water = ep_psychrometrics::cp_water(water_inlet_temp);
        let c_water = water_mass_flow * cp_water;

        // Simplified effectiveness (assume air-side C = infinity → Cr = 0)
        // epsilon = 1 - exp(-NTU)
        let ntu = if c_water > 1e-10 { self.ua / c_water } else { 0.0 };
        let effectiveness = 1.0 - (-ntu).exp();

        let q_max = c_water * (water_inlet_temp - zone_temp);
        let q = (effectiveness * q_max).min(self.rated_capacity);

        let water_outlet_temp = water_inlet_temp - q / c_water;

        BaseboardResult {
            heating_rate: q,
            water_outlet_temp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cv_terminal_basic() {
        let term = SingleDuctTerminal::constant_volume("CV-1", 0.5, 10, 20);
        assert_eq!(term.terminal_type, TerminalType::SingleDuctConstantVolume);
    }

    #[test]
    fn cv_terminal_cooling() {
        let term = SingleDuctTerminal::constant_volume("CV", 0.5, 10, 20);
        let result = term.calculate(13.0, 0.008, 24.0, -5000.0, 1.2);
        assert!((result.air_mass_flow - 0.6).abs() < 0.01); // 0.5 * 1.2
        assert!((result.supply_temp - 13.0).abs() < 0.01);
        assert!(result.reheat_rate.abs() < 1e-10);
    }

    #[test]
    fn vav_reheat_cooling_mode() {
        let term = SingleDuctTerminal::vav_reheat("VAV-1", 1.0, 0.3, 5000.0, 10, 20);
        let result = term.calculate(13.0, 0.008, 24.0, -10000.0, 1.2);
        assert!(result.air_mass_flow >= 0.3 * 1.2);
        assert!(result.air_mass_flow <= 1.0 * 1.2);
        assert!(result.reheat_rate.abs() < 1e-10); // No reheat in cooling
        assert!(result.damper_position > 0.0 && result.damper_position <= 1.0);
    }

    #[test]
    fn vav_reheat_heating_mode() {
        let term = SingleDuctTerminal::vav_reheat("VAV-1", 1.0, 0.3, 5000.0, 10, 20);
        let result = term.calculate(13.0, 0.008, 20.0, 3000.0, 1.2);
        // Heating: minimum flow + reheat
        assert!((result.air_mass_flow - 0.3 * 1.2).abs() < 0.01);
        assert!((result.reheat_rate - 3000.0).abs() < 1.0);
        assert!(result.supply_temp > 13.0, "T_sup={}", result.supply_temp);
    }

    #[test]
    fn vav_reheat_limited() {
        let term = SingleDuctTerminal::vav_reheat("VAV-1", 1.0, 0.3, 5000.0, 10, 20);
        let result = term.calculate(13.0, 0.008, 20.0, 8000.0, 1.2);
        // Reheat limited to capacity
        assert!((result.reheat_rate - 5000.0).abs() < 1.0);
    }

    #[test]
    fn vav_no_reheat_heating() {
        let term = SingleDuctTerminal::vav_no_reheat("VAV-NR", 1.0, 0.3, 10, 20);
        let result = term.calculate(13.0, 0.008, 20.0, 3000.0, 1.2);
        // No reheat available
        assert!(result.reheat_rate.abs() < 1e-10);
        assert!((result.damper_position - 0.3).abs() < 0.01);
    }

    #[test]
    fn baseboard_basic() {
        let bb = Baseboard::new("BB-1", 5000.0, 500.0, 0.1);
        let result = bb.calculate(20.0, 80.0, 0.1);
        assert!(result.heating_rate > 0.0);
        assert!(result.water_outlet_temp < 80.0);
        assert!(result.water_outlet_temp > 20.0);
    }

    #[test]
    fn baseboard_no_flow() {
        let bb = Baseboard::new("BB", 5000.0, 500.0, 0.1);
        let result = bb.calculate(20.0, 80.0, 0.0);
        assert!(result.heating_rate.abs() < 1e-10);
    }

    #[test]
    fn baseboard_cold_water() {
        let bb = Baseboard::new("BB", 5000.0, 500.0, 0.1);
        // Water colder than zone: no heating
        let result = bb.calculate(20.0, 15.0, 0.1);
        assert!(result.heating_rate.abs() < 1e-10);
    }

    #[test]
    fn baseboard_energy_balance() {
        let bb = Baseboard::new("BB", 10000.0, 300.0, 0.2);
        let result = bb.calculate(20.0, 70.0, 0.2);

        let cp = ep_psychrometrics::cp_water(70.0);
        let q_water = 0.2 * cp * (70.0 - result.water_outlet_temp);
        assert!((q_water - result.heating_rate).abs() / result.heating_rate.abs().max(1.0) < 0.01,
                "q_water={}, q_reported={}", q_water, result.heating_rate);
    }
}
