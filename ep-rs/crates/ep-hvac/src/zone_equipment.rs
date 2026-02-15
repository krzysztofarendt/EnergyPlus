//! Zone-level HVAC equipment models.
//!
//! Terminal units (VAV boxes, single duct), baseboard heaters,
//! zone equipment list management, and zone load calculation.

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

    /// Create a constant-volume terminal with reheat.
    pub fn constant_volume_reheat(
        name: impl Into<String>,
        max_flow: f64,
        reheat_capacity: f64,
        inlet: usize,
        outlet: usize,
    ) -> Self {
        Self {
            name: name.into(),
            terminal_type: TerminalType::SingleDuctConstantVolume,
            reheat_type: ReheatType::Electric,
            max_air_flow: max_flow,
            min_flow_fraction: 1.0,
            reheat_capacity,
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

// ---------------------------------------------------------------------------
// Zone Equipment Manager
// ---------------------------------------------------------------------------

/// Zone equipment priority sequence for cooling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZoneEquipmentPriority {
    #[default]
    /// Heating priority first, then cooling.
    HeatingFirst,
    /// Cooling priority first, then heating.
    CoolingFirst,
}

/// Type of zone equipment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneEquipmentType {
    AirTerminal,
    Baseboard,
    FanCoil,
    UnitarySystem,
    Radiant,
}

/// A piece of equipment serving a zone.
#[derive(Debug, Clone)]
pub struct ZoneEquipment {
    pub name: String,
    pub equipment_type: ZoneEquipmentType,
    /// Priority sequence number (lower = higher priority).
    pub cooling_priority: usize,
    /// Priority sequence number for heating.
    pub heating_priority: usize,
}

/// Zone load calculation result.
#[derive(Debug, Clone, Copy)]
pub struct ZoneLoadResult {
    /// Sensible load (W). Positive = heating needed, negative = cooling needed.
    pub sensible_load: f64,
    /// Latent load (W). Positive = humidification needed.
    pub latent_load: f64,
    /// Whether the zone needs cooling.
    pub cooling_needed: bool,
    /// Whether the zone needs heating.
    pub heating_needed: bool,
}

/// Calculate zone sensible load from the zone air balance.
///
/// The zone air energy balance is:
///   C * dT/dt = TempIndCoef - TempDepCoef * T_zone
///
/// At steady-state (dT/dt = 0):
///   sensible_load = TempIndCoef - TempDepCoef * T_zone_setpoint
///
/// Positive = heating needed, negative = cooling needed.
pub fn calculate_zone_load(
    temp_ind_coef: f64,
    temp_dep_coef: f64,
    zone_temp: f64,
    cooling_setpoint: f64,
    heating_setpoint: f64,
) -> ZoneLoadResult {
    // Load at cooling setpoint (how much cooling needed to stay at setpoint)
    let load_at_cool = temp_ind_coef - temp_dep_coef * cooling_setpoint;
    // Load at heating setpoint
    let load_at_heat = temp_ind_coef - temp_dep_coef * heating_setpoint;

    // Determine which setpoint is active
    let (sensible_load, cooling_needed, heating_needed) = if zone_temp > cooling_setpoint {
        // Zone is above cooling setpoint — cooling needed
        (load_at_cool, true, false)
    } else if zone_temp < heating_setpoint {
        // Zone is below heating setpoint — heating needed
        (load_at_heat, false, true)
    } else {
        // Zone is in deadband
        (0.0, false, false)
    };

    ZoneLoadResult {
        sensible_load,
        latent_load: 0.0,
        cooling_needed,
        heating_needed,
    }
}

/// Result of zone equipment dispatch.
#[derive(Debug, Clone, Copy)]
pub struct ZoneEquipmentDispatchResult {
    /// Total delivered cooling (W, positive value).
    pub delivered_cooling: f64,
    /// Total delivered heating (W, positive value).
    pub delivered_heating: f64,
    /// Total reheat energy (W).
    pub reheat_energy: f64,
    /// Total fan power from zone equipment (W).
    pub fan_power: f64,
    /// Remaining unmet load (W, positive = unmet heating, negative = unmet cooling).
    pub unmet_load: f64,
    /// Return air temperature (C).
    pub return_air_temp: f64,
    /// Return air humidity ratio (kg/kg).
    pub return_air_w: f64,
}

/// Dispatch zone equipment to meet zone load.
///
/// Processes terminal units in priority order, distributing the zone load
/// among them. Each terminal gets a share of the remaining load.
pub fn dispatch_zone_equipment(
    terminals: &[SingleDuctTerminal],
    zone_load: f64,
    supply_temp: f64,
    supply_w: f64,
    zone_temp: f64,
    air_density: f64,
) -> ZoneEquipmentDispatchResult {
    let mut remaining_load = zone_load;
    let mut total_cooling = 0.0;
    let mut total_heating = 0.0;
    let mut total_reheat = 0.0;
    let mut total_mass_flow = 0.0;

    for terminal in terminals {
        let result = terminal.calculate(supply_temp, supply_w, zone_temp, remaining_load, air_density);

        // Calculate delivered capacity from this terminal
        let cp = ep_psychrometrics::cp_air(supply_w);
        let delivered = result.air_mass_flow * cp * (result.supply_temp - zone_temp);

        if delivered < 0.0 {
            total_cooling += delivered.abs();
        } else {
            total_heating += delivered;
        }
        total_reheat += result.reheat_rate;

        remaining_load -= delivered;
        total_mass_flow += result.air_mass_flow;
    }

    let return_temp = if total_mass_flow > 1e-10 {
        // Return air is approximately zone temp (air picks up zone load)
        zone_temp
    } else {
        zone_temp
    };

    ZoneEquipmentDispatchResult {
        delivered_cooling: total_cooling,
        delivered_heating: total_heating,
        reheat_energy: total_reheat,
        fan_power: 0.0,
        unmet_load: remaining_load,
        return_air_temp: return_temp,
        return_air_w: supply_w,
    }
}

/// Calculate return air conditions from multiple zone returns.
///
/// Returns mass-weighted average temperature and humidity ratio.
pub fn calculate_return_air(
    zone_temps: &[f64],
    zone_humidities: &[f64],
    zone_return_flows: &[f64],
) -> (f64, f64, f64) {
    let total_flow: f64 = zone_return_flows.iter().sum();
    if total_flow <= 1e-10 || zone_temps.is_empty() {
        return (20.0, 0.008, 0.0);
    }

    let mut sum_t = 0.0;
    let mut sum_w = 0.0;
    for i in 0..zone_temps.len() {
        let flow = zone_return_flows.get(i).copied().unwrap_or(0.0);
        sum_t += flow * zone_temps[i];
        sum_w += flow * zone_humidities.get(i).copied().unwrap_or(0.008);
    }

    (sum_t / total_flow, sum_w / total_flow, total_flow)
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
    fn cv_terminal_with_reheat() {
        let term = SingleDuctTerminal::constant_volume_reheat("CV-R", 0.5, 5000.0, 10, 20);
        let result = term.calculate(13.0, 0.008, 20.0, 3000.0, 1.2);
        assert!((result.reheat_rate - 3000.0).abs() < 1.0);
        assert!(result.supply_temp > 13.0, "T={}", result.supply_temp);
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

    // --- Zone load calculation tests ---

    #[test]
    fn zone_load_cooling_needed() {
        let result = calculate_zone_load(
            5000.0,   // TempIndCoef (internal gains, solar, etc.)
            200.0,    // TempDepCoef (infiltration, ventilation)
            26.0,     // zone_temp (above cooling setpoint)
            24.0,     // cooling_setpoint
            20.0,     // heating_setpoint
        );
        assert!(result.cooling_needed);
        assert!(!result.heating_needed);
        // Load at cooling setpoint: 5000 - 200*24 = 5000 - 4800 = 200W (still net heating)
        // But zone is above cooling setpoint so cooling_needed is true
    }

    #[test]
    fn zone_load_heating_needed() {
        let result = calculate_zone_load(
            1000.0,   // low gains
            300.0,    // high loss
            18.0,     // zone_temp (below heating setpoint)
            24.0,     // cooling_setpoint
            20.0,     // heating_setpoint
        );
        assert!(!result.cooling_needed);
        assert!(result.heating_needed);
        // Load at heating setpoint: 1000 - 300*20 = 1000 - 6000 = -5000W (needs heating)
    }

    #[test]
    fn zone_load_deadband() {
        let result = calculate_zone_load(
            3000.0, 150.0,
            22.0,  // in deadband between 20 and 24
            24.0, 20.0,
        );
        assert!(!result.cooling_needed);
        assert!(!result.heating_needed);
        assert!(result.sensible_load.abs() < 1e-10);
    }

    // --- Zone equipment dispatch tests ---

    #[test]
    fn dispatch_single_terminal_cooling() {
        let terminals = vec![
            SingleDuctTerminal::vav_reheat("VAV-1", 1.0, 0.3, 5000.0, 0, 1),
        ];
        let result = dispatch_zone_equipment(
            &terminals, -10000.0, 13.0, 0.008, 24.0, 1.2,
        );
        assert!(result.delivered_cooling > 0.0, "cool={}", result.delivered_cooling);
        assert!(result.reheat_energy.abs() < 1e-10);
    }

    #[test]
    fn dispatch_single_terminal_heating() {
        let terminals = vec![
            SingleDuctTerminal::vav_reheat("VAV-1", 1.0, 0.3, 5000.0, 0, 1),
        ];
        let result = dispatch_zone_equipment(
            &terminals, 3000.0, 13.0, 0.008, 20.0, 1.2,
        );
        assert!(result.delivered_heating > 0.0 || result.reheat_energy > 0.0);
    }

    #[test]
    fn dispatch_multi_terminal() {
        let terminals = vec![
            SingleDuctTerminal::vav_reheat("VAV-1", 0.5, 0.3, 3000.0, 0, 1),
            SingleDuctTerminal::vav_reheat("VAV-2", 0.5, 0.3, 3000.0, 2, 3),
        ];
        let result = dispatch_zone_equipment(
            &terminals, -8000.0, 13.0, 0.008, 24.0, 1.2,
        );
        assert!(result.delivered_cooling > 0.0);
    }

    // --- Return air tests ---

    #[test]
    fn return_air_single_zone() {
        let (t, w, flow) = calculate_return_air(&[24.0], &[0.009], &[1.0]);
        assert!((t - 24.0).abs() < 0.01);
        assert!((w - 0.009).abs() < 1e-6);
        assert!((flow - 1.0).abs() < 1e-10);
    }

    #[test]
    fn return_air_multi_zone() {
        let (t, w, flow) = calculate_return_air(
            &[22.0, 26.0],
            &[0.008, 0.010],
            &[1.0, 1.0],
        );
        // Equal flows → average
        assert!((t - 24.0).abs() < 0.01, "T_ret={}", t);
        assert!((w - 0.009).abs() < 1e-6, "W_ret={}", w);
        assert!((flow - 2.0).abs() < 1e-10);
    }

    #[test]
    fn return_air_weighted() {
        let (t, _w, flow) = calculate_return_air(
            &[20.0, 30.0],
            &[0.008, 0.012],
            &[3.0, 1.0],
        );
        // 3*20 + 1*30 = 90, /4 = 22.5
        assert!((t - 22.5).abs() < 0.01, "T_ret={}", t);
        assert!((flow - 4.0).abs() < 1e-10);
    }

    #[test]
    fn return_air_zero_flow() {
        let (t, _w, flow) = calculate_return_air(&[24.0], &[0.009], &[0.0]);
        assert!((flow).abs() < 1e-10);
        assert!((t - 20.0).abs() < 0.01); // default
    }
}
