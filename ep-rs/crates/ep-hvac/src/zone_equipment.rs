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
// Four-Pipe Fan Coil Unit
// ---------------------------------------------------------------------------

/// Four-pipe fan coil unit (fan + CHW coil + HW coil).
#[derive(Debug, Clone)]
pub struct FourPipeFanCoil {
    pub name: String,
    /// Maximum cooling capacity (W).
    pub max_cooling_capacity: f64,
    /// Maximum heating capacity (W).
    pub max_heating_capacity: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Chilled water UA (W/K).
    pub cooling_ua: f64,
    /// Hot water UA (W/K).
    pub heating_ua: f64,
    /// Design air flow rate (m3/s).
    pub design_air_flow: f64,
}

/// Fan coil result.
#[derive(Debug, Clone, Copy)]
pub struct FanCoilResult {
    /// Cooling delivered (W, positive).
    pub cooling_rate: f64,
    /// Heating delivered (W, positive).
    pub heating_rate: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Supply air temperature (C).
    pub supply_temp: f64,
}

impl FourPipeFanCoil {
    pub fn new(
        name: impl Into<String>,
        max_cooling: f64,
        max_heating: f64,
        fan_power: f64,
    ) -> Self {
        Self {
            name: name.into(),
            max_cooling_capacity: max_cooling,
            max_heating_capacity: max_heating,
            fan_power,
            cooling_ua: 1000.0,
            heating_ua: 800.0,
            design_air_flow: 0.5,
        }
    }

    /// Calculate fan coil performance.
    ///
    /// `zone_load` — sensible load (positive = heating, negative = cooling).
    pub fn calculate(
        &self,
        zone_temp: f64,
        zone_load: f64,
        chw_inlet_temp: f64,
        chw_flow: f64,
        hw_inlet_temp: f64,
        hw_flow: f64,
        air_density: f64,
    ) -> FanCoilResult {
        let air_mass_flow = self.design_air_flow * air_density;
        let cp_air = ep_psychrometrics::cp_air(0.008);

        if zone_load < -100.0 && chw_flow > 1e-10 {
            // Cooling mode
            let cp_w = ep_psychrometrics::cp_water(chw_inlet_temp);
            let c_w = chw_flow * cp_w;
            let ntu = if c_w > 1e-10 { self.cooling_ua / c_w } else { 0.0 };
            let eff = 1.0 - (-ntu).exp();
            let q_max = c_w * (zone_temp - chw_inlet_temp).max(0.0);
            let q = (eff * q_max).min(self.max_cooling_capacity).min(-zone_load);
            let supply_temp = zone_temp - q / (air_mass_flow * cp_air).max(1e-10);
            FanCoilResult { cooling_rate: q, heating_rate: 0.0, fan_power: self.fan_power, supply_temp }
        } else if zone_load > 100.0 && hw_flow > 1e-10 {
            // Heating mode
            let cp_w = ep_psychrometrics::cp_water(hw_inlet_temp);
            let c_w = hw_flow * cp_w;
            let ntu = if c_w > 1e-10 { self.heating_ua / c_w } else { 0.0 };
            let eff = 1.0 - (-ntu).exp();
            let q_max = c_w * (hw_inlet_temp - zone_temp).max(0.0);
            let q = (eff * q_max).min(self.max_heating_capacity).min(zone_load);
            let supply_temp = zone_temp + q / (air_mass_flow * cp_air).max(1e-10);
            FanCoilResult { cooling_rate: 0.0, heating_rate: q, fan_power: self.fan_power, supply_temp }
        } else {
            FanCoilResult { cooling_rate: 0.0, heating_rate: 0.0, fan_power: 0.0, supply_temp: zone_temp }
        }
    }
}

// ---------------------------------------------------------------------------
// Electric Baseboard
// ---------------------------------------------------------------------------

/// Electric baseboard heater (no water connection).
#[derive(Debug, Clone)]
pub struct ElectricBaseboard {
    pub name: String,
    /// Maximum heating capacity (W).
    pub capacity: f64,
    /// Efficiency (typically 1.0 for electric).
    pub efficiency: f64,
}

/// Electric baseboard result.
#[derive(Debug, Clone, Copy)]
pub struct ElectricBaseboardResult {
    /// Heating delivered (W).
    pub heating_rate: f64,
    /// Electric power consumed (W).
    pub electric_power: f64,
    /// Part-load ratio.
    pub plr: f64,
}

impl ElectricBaseboard {
    pub fn new(name: impl Into<String>, capacity: f64) -> Self {
        Self { name: name.into(), capacity, efficiency: 1.0 }
    }

    pub fn calculate(&self, zone_load: f64) -> ElectricBaseboardResult {
        if zone_load <= 0.0 || self.capacity <= 0.0 {
            return ElectricBaseboardResult { heating_rate: 0.0, electric_power: 0.0, plr: 0.0 };
        }
        let plr = (zone_load / self.capacity).clamp(0.0, 1.0);
        let heating = self.capacity * plr;
        let power = heating / self.efficiency.max(0.01);
        ElectricBaseboardResult { heating_rate: heating, electric_power: power, plr }
    }
}

// ---------------------------------------------------------------------------
// Radiant/Convective Baseboard (Hot Water)
// ---------------------------------------------------------------------------

/// Hot water baseboard with radiant/convective split.
#[derive(Debug, Clone)]
pub struct RadiantConvectiveBaseboard {
    pub name: String,
    /// Rated capacity (W).
    pub rated_capacity: f64,
    /// UA-value (W/K).
    pub ua: f64,
    /// Design water flow (kg/s).
    pub design_water_flow: f64,
    /// Fraction of output that is radiant (0-1).
    pub radiant_fraction: f64,
}

/// Radiant/convective baseboard result.
#[derive(Debug, Clone, Copy)]
pub struct RadiantConvectiveResult {
    /// Total heating output (W).
    pub total_heating: f64,
    /// Radiant portion (W).
    pub radiant_heating: f64,
    /// Convective portion (W).
    pub convective_heating: f64,
    /// Water outlet temperature (C).
    pub water_outlet_temp: f64,
}

impl RadiantConvectiveBaseboard {
    pub fn new(name: impl Into<String>, capacity: f64, ua: f64, water_flow: f64, radiant_fraction: f64) -> Self {
        Self {
            name: name.into(),
            rated_capacity: capacity,
            ua,
            design_water_flow: water_flow,
            radiant_fraction: radiant_fraction.clamp(0.0, 1.0),
        }
    }

    pub fn calculate(&self, zone_temp: f64, water_inlet_temp: f64, water_mass_flow: f64) -> RadiantConvectiveResult {
        if water_mass_flow <= 1e-10 || water_inlet_temp <= zone_temp {
            return RadiantConvectiveResult {
                total_heating: 0.0, radiant_heating: 0.0,
                convective_heating: 0.0, water_outlet_temp: water_inlet_temp,
            };
        }
        let cp_w = ep_psychrometrics::cp_water(water_inlet_temp);
        let c_w = water_mass_flow * cp_w;
        let ntu = if c_w > 1e-10 { self.ua / c_w } else { 0.0 };
        let eff = 1.0 - (-ntu).exp();
        let q_max = c_w * (water_inlet_temp - zone_temp);
        let q = (eff * q_max).min(self.rated_capacity);
        let water_outlet = water_inlet_temp - q / c_w;

        RadiantConvectiveResult {
            total_heating: q,
            radiant_heating: q * self.radiant_fraction,
            convective_heating: q * (1.0 - self.radiant_fraction),
            water_outlet_temp: water_outlet,
        }
    }
}

// ---------------------------------------------------------------------------
// Unit Heater
// ---------------------------------------------------------------------------

/// Heating coil type for unit heaters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitHeaterCoilType {
    Electric,
    HotWater,
    Gas,
}

/// Unit heater (fan + heating coil).
#[derive(Debug, Clone)]
pub struct UnitHeater {
    pub name: String,
    pub coil_type: UnitHeaterCoilType,
    /// Heating capacity (W).
    pub capacity: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Coil efficiency (for gas, typically 0.8).
    pub efficiency: f64,
    /// Design air flow (m3/s).
    pub design_air_flow: f64,
}

/// Unit heater result.
#[derive(Debug, Clone, Copy)]
pub struct UnitHeaterResult {
    pub heating_rate: f64,
    pub fan_power: f64,
    pub fuel_rate: f64,
    pub supply_temp: f64,
    pub plr: f64,
}

impl UnitHeater {
    pub fn new(name: impl Into<String>, coil_type: UnitHeaterCoilType, capacity: f64, fan_power: f64) -> Self {
        let efficiency = match coil_type {
            UnitHeaterCoilType::Gas => 0.80,
            _ => 1.0,
        };
        Self {
            name: name.into(), coil_type, capacity, fan_power,
            efficiency, design_air_flow: 0.5,
        }
    }

    pub fn calculate(&self, zone_temp: f64, zone_load: f64, air_density: f64) -> UnitHeaterResult {
        if zone_load <= 0.0 || self.capacity <= 0.0 {
            return UnitHeaterResult {
                heating_rate: 0.0, fan_power: 0.0, fuel_rate: 0.0,
                supply_temp: zone_temp, plr: 0.0,
            };
        }
        let plr = (zone_load / self.capacity).clamp(0.0, 1.0);
        let heating = self.capacity * plr;
        let fuel = heating / self.efficiency.max(0.01);
        let mass_flow = self.design_air_flow * air_density;
        let cp = ep_psychrometrics::cp_air(0.008);
        let supply_temp = zone_temp + heating / (mass_flow * cp).max(1e-10);
        UnitHeaterResult {
            heating_rate: heating, fan_power: self.fan_power * plr.max(0.3),
            fuel_rate: fuel, supply_temp, plr,
        }
    }
}

// ---------------------------------------------------------------------------
// Unit Ventilator
// ---------------------------------------------------------------------------

/// Unit ventilator (fan + OA mixer + optional heating/cooling coils).
#[derive(Debug, Clone)]
pub struct UnitVentilator {
    pub name: String,
    /// Maximum cooling capacity (W).
    pub cooling_capacity: f64,
    /// Maximum heating capacity (W).
    pub heating_capacity: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Design air flow (m3/s).
    pub design_air_flow: f64,
    /// Minimum OA fraction.
    pub min_oa_fraction: f64,
    /// Maximum OA fraction.
    pub max_oa_fraction: f64,
}

/// Unit ventilator result.
#[derive(Debug, Clone, Copy)]
pub struct UnitVentilatorResult {
    pub cooling_rate: f64,
    pub heating_rate: f64,
    pub fan_power: f64,
    pub supply_temp: f64,
    pub oa_fraction: f64,
}

impl UnitVentilator {
    pub fn new(name: impl Into<String>, cooling_cap: f64, heating_cap: f64, fan_power: f64) -> Self {
        Self {
            name: name.into(), cooling_capacity: cooling_cap,
            heating_capacity: heating_cap, fan_power,
            design_air_flow: 0.5, min_oa_fraction: 0.3, max_oa_fraction: 1.0,
        }
    }

    pub fn calculate(
        &self,
        zone_temp: f64,
        zone_load: f64,
        outdoor_temp: f64,
        air_density: f64,
    ) -> UnitVentilatorResult {
        let mass_flow = self.design_air_flow * air_density;
        let cp = ep_psychrometrics::cp_air(0.008);

        // Determine OA fraction: use more OA for free cooling if beneficial
        let oa_fraction = if zone_load < -100.0 && outdoor_temp < zone_temp {
            self.max_oa_fraction // Free cooling
        } else {
            self.min_oa_fraction
        };

        let mixed_temp = oa_fraction * outdoor_temp + (1.0 - oa_fraction) * zone_temp;

        if zone_load < -100.0 {
            let free_cooling = mass_flow * cp * (zone_temp - mixed_temp).max(0.0);
            let remaining = (-zone_load - free_cooling).max(0.0);
            let coil_cooling = remaining.min(self.cooling_capacity);
            let total = free_cooling + coil_cooling;
            let supply_temp = zone_temp - total / (mass_flow * cp).max(1e-10);
            UnitVentilatorResult {
                cooling_rate: total, heating_rate: 0.0, fan_power: self.fan_power,
                supply_temp, oa_fraction,
            }
        } else if zone_load > 100.0 {
            let heating = zone_load.min(self.heating_capacity);
            let supply_temp = mixed_temp + heating / (mass_flow * cp).max(1e-10);
            UnitVentilatorResult {
                cooling_rate: 0.0, heating_rate: heating, fan_power: self.fan_power,
                supply_temp, oa_fraction: self.min_oa_fraction,
            }
        } else {
            UnitVentilatorResult {
                cooling_rate: 0.0, heating_rate: 0.0, fan_power: 0.0,
                supply_temp: zone_temp, oa_fraction: 0.0,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Window AC (Packaged DX Cooling)
// ---------------------------------------------------------------------------

/// Window air conditioner (packaged DX cooling with fan).
#[derive(Debug, Clone)]
pub struct WindowAC {
    pub name: String,
    /// Total cooling capacity (W).
    pub cooling_capacity: f64,
    /// COP.
    pub cop: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// SHR.
    pub shr: f64,
}

/// Window AC result.
#[derive(Debug, Clone, Copy)]
pub struct WindowACResult {
    pub total_cooling: f64,
    pub sensible_cooling: f64,
    pub power: f64,
    pub fan_power: f64,
    pub supply_temp: f64,
    pub plr: f64,
}

impl WindowAC {
    pub fn new(name: impl Into<String>, capacity: f64, cop: f64, fan_power: f64) -> Self {
        Self { name: name.into(), cooling_capacity: capacity, cop, fan_power, shr: 0.75 }
    }

    pub fn calculate(
        &self,
        air_inlet_temp: f64,
        air_inlet_w: f64,
        zone_load: f64,
        air_density: f64,
    ) -> WindowACResult {
        if zone_load >= 0.0 || self.cooling_capacity <= 0.0 {
            return WindowACResult {
                total_cooling: 0.0, sensible_cooling: 0.0, power: 0.0,
                fan_power: 0.0, supply_temp: air_inlet_temp, plr: 0.0,
            };
        }
        let plr = (-zone_load / self.cooling_capacity).clamp(0.0, 1.0);
        let total = self.cooling_capacity * plr;
        let sensible = total * self.shr;
        let power = total / self.cop.max(0.01);
        let cp = ep_psychrometrics::cp_air(air_inlet_w);
        let mass_flow = 0.5 * air_density; // Typical window AC flow
        let supply_temp = air_inlet_temp - sensible / (mass_flow * cp).max(1e-10);
        WindowACResult {
            total_cooling: total, sensible_cooling: sensible, power,
            fan_power: self.fan_power * plr.max(0.3), supply_temp, plr,
        }
    }
}

// ---------------------------------------------------------------------------
// Low Temperature Radiant Electric Panel
// ---------------------------------------------------------------------------

/// Low-temperature radiant electric heating panel.
#[derive(Debug, Clone)]
pub struct LowTempRadiantElectric {
    pub name: String,
    /// Maximum capacity (W).
    pub capacity: f64,
    /// Radiant fraction (0-1).
    pub radiant_fraction: f64,
}

/// Radiant panel result.
#[derive(Debug, Clone, Copy)]
pub struct RadiantPanelResult {
    pub total_heating: f64,
    pub radiant_heating: f64,
    pub convective_heating: f64,
    pub electric_power: f64,
    pub plr: f64,
}

impl LowTempRadiantElectric {
    pub fn new(name: impl Into<String>, capacity: f64, radiant_fraction: f64) -> Self {
        Self {
            name: name.into(), capacity,
            radiant_fraction: radiant_fraction.clamp(0.0, 1.0),
        }
    }

    pub fn calculate(&self, zone_load: f64) -> RadiantPanelResult {
        if zone_load <= 0.0 || self.capacity <= 0.0 {
            return RadiantPanelResult {
                total_heating: 0.0, radiant_heating: 0.0,
                convective_heating: 0.0, electric_power: 0.0, plr: 0.0,
            };
        }
        let plr = (zone_load / self.capacity).clamp(0.0, 1.0);
        let total = self.capacity * plr;
        RadiantPanelResult {
            total_heating: total,
            radiant_heating: total * self.radiant_fraction,
            convective_heating: total * (1.0 - self.radiant_fraction),
            electric_power: total,
            plr,
        }
    }
}

// ---------------------------------------------------------------------------
// Dual Duct Terminal
// ---------------------------------------------------------------------------

/// Dual-duct terminal mixing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DualDuctMode {
    /// Constant volume — both ducts at fixed flow.
    ConstantVolume,
    /// Variable air volume — modulate flows to meet load.
    VAV,
}

/// Dual-duct terminal — mixes hot and cold supply ducts.
#[derive(Debug, Clone)]
pub struct DualDuctTerminal {
    pub name: String,
    pub mode: DualDuctMode,
    /// Maximum total air flow (m3/s).
    pub max_air_flow: f64,
    /// Minimum flow fraction (VAV only).
    pub min_flow_fraction: f64,
}

/// Dual-duct terminal result.
#[derive(Debug, Clone, Copy)]
pub struct DualDuctResult {
    /// Hot duct mass flow (kg/s).
    pub hot_mass_flow: f64,
    /// Cold duct mass flow (kg/s).
    pub cold_mass_flow: f64,
    /// Mixed supply air temperature (C).
    pub supply_temp: f64,
    /// Total mass flow (kg/s).
    pub total_mass_flow: f64,
}

impl DualDuctTerminal {
    pub fn constant_volume(name: impl Into<String>, max_flow: f64) -> Self {
        Self {
            name: name.into(),
            mode: DualDuctMode::ConstantVolume,
            max_air_flow: max_flow,
            min_flow_fraction: 1.0,
        }
    }

    pub fn vav(name: impl Into<String>, max_flow: f64, min_fraction: f64) -> Self {
        Self {
            name: name.into(),
            mode: DualDuctMode::VAV,
            max_air_flow: max_flow,
            min_flow_fraction: min_fraction,
        }
    }

    /// Calculate dual-duct terminal performance.
    ///
    /// `hot_deck_temp` — hot duct supply temperature (C).
    /// `cold_deck_temp` — cold duct supply temperature (C).
    /// `zone_temp` — current zone temperature (C).
    /// `zone_load` — sensible load (positive = heating, negative = cooling).
    /// `air_density` — air density (kg/m3).
    pub fn calculate(
        &self,
        hot_deck_temp: f64,
        cold_deck_temp: f64,
        zone_temp: f64,
        zone_load: f64,
        air_density: f64,
    ) -> DualDuctResult {
        let max_mass_flow = self.max_air_flow * air_density;
        let cp = ep_psychrometrics::cp_air(0.008);

        match self.mode {
            DualDuctMode::ConstantVolume => {
                // Mix hot and cold to achieve desired supply temp
                let dt_hot = hot_deck_temp - cold_deck_temp;
                if dt_hot.abs() < 0.1 {
                    // Both ducts same temp — all from one duct
                    return DualDuctResult {
                        hot_mass_flow: max_mass_flow * 0.5,
                        cold_mass_flow: max_mass_flow * 0.5,
                        supply_temp: hot_deck_temp,
                        total_mass_flow: max_mass_flow,
                    };
                }

                // Target supply temp to meet load at constant flow
                let target_supply = zone_temp + zone_load / (max_mass_flow * cp).max(1e-10);
                let target_supply = target_supply.clamp(cold_deck_temp, hot_deck_temp);

                // Mixing ratio: target = hot_frac * T_hot + (1-hot_frac) * T_cold
                let hot_frac = ((target_supply - cold_deck_temp) / dt_hot).clamp(0.0, 1.0);
                let hot_flow = max_mass_flow * hot_frac;
                let cold_flow = max_mass_flow * (1.0 - hot_frac);
                let supply_temp = if max_mass_flow > 1e-10 {
                    (hot_flow * hot_deck_temp + cold_flow * cold_deck_temp) / max_mass_flow
                } else {
                    zone_temp
                };

                DualDuctResult {
                    hot_mass_flow: hot_flow,
                    cold_mass_flow: cold_flow,
                    supply_temp,
                    total_mass_flow: max_mass_flow,
                }
            }
            DualDuctMode::VAV => {
                let min_mass_flow = max_mass_flow * self.min_flow_fraction;

                if zone_load < -100.0 {
                    // Cooling: use cold duct only, modulate flow
                    let needed_flow = if (zone_temp - cold_deck_temp).abs() > 0.1 {
                        (-zone_load / (cp * (zone_temp - cold_deck_temp))).clamp(min_mass_flow, max_mass_flow)
                    } else {
                        max_mass_flow
                    };
                    DualDuctResult {
                        hot_mass_flow: 0.0,
                        cold_mass_flow: needed_flow,
                        supply_temp: cold_deck_temp,
                        total_mass_flow: needed_flow,
                    }
                } else if zone_load > 100.0 {
                    // Heating: use hot duct only, modulate flow
                    let needed_flow = if (hot_deck_temp - zone_temp).abs() > 0.1 {
                        (zone_load / (cp * (hot_deck_temp - zone_temp))).clamp(min_mass_flow, max_mass_flow)
                    } else {
                        max_mass_flow
                    };
                    DualDuctResult {
                        hot_mass_flow: needed_flow,
                        cold_mass_flow: 0.0,
                        supply_temp: hot_deck_temp,
                        total_mass_flow: needed_flow,
                    }
                } else {
                    // Deadband: minimum flow from cold duct
                    DualDuctResult {
                        hot_mass_flow: 0.0,
                        cold_mass_flow: min_mass_flow,
                        supply_temp: cold_deck_temp,
                        total_mass_flow: min_mass_flow,
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Series PIU (Powered Induction Unit) Terminal
// ---------------------------------------------------------------------------

/// Series powered induction unit — primary air + recirculated secondary air.
///
/// In a series PIU, the fan runs continuously. Primary air from the AHU
/// mixes with recirculated plenum/zone air. Total flow is constant.
#[derive(Debug, Clone)]
pub struct SeriesPIUTerminal {
    pub name: String,
    /// Maximum primary air flow (m3/s).
    pub max_primary_flow: f64,
    /// Minimum primary air flow fraction.
    pub min_primary_fraction: f64,
    /// Total (constant) discharge flow (m3/s).
    pub total_flow: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Reheat coil capacity (W).
    pub reheat_capacity: f64,
}

/// Series PIU result.
#[derive(Debug, Clone, Copy)]
pub struct SeriesPIUResult {
    /// Primary air mass flow (kg/s).
    pub primary_mass_flow: f64,
    /// Secondary (recirculated) mass flow (kg/s).
    pub secondary_mass_flow: f64,
    /// Mixed air temperature before reheat (C).
    pub mixed_temp: f64,
    /// Discharge temperature (C).
    pub discharge_temp: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Reheat rate (W).
    pub reheat_rate: f64,
}

impl SeriesPIUTerminal {
    pub fn new(
        name: impl Into<String>,
        max_primary_flow: f64,
        min_primary_fraction: f64,
        total_flow: f64,
        fan_power: f64,
        reheat_capacity: f64,
    ) -> Self {
        Self {
            name: name.into(),
            max_primary_flow,
            min_primary_fraction,
            total_flow,
            fan_power,
            reheat_capacity,
        }
    }

    /// Calculate series PIU performance.
    ///
    /// `primary_temp` — primary air temperature from AHU (C).
    /// `zone_temp` — zone temperature (C), also used as secondary air temp.
    /// `zone_load` — sensible load (positive = heating, negative = cooling).
    /// `air_density` — air density (kg/m3).
    pub fn calculate(
        &self,
        primary_temp: f64,
        zone_temp: f64,
        zone_load: f64,
        air_density: f64,
    ) -> SeriesPIUResult {
        let total_mass_flow = self.total_flow * air_density;
        let max_primary_mass = self.max_primary_flow * air_density;
        let min_primary_mass = max_primary_mass * self.min_primary_fraction;
        let cp = ep_psychrometrics::cp_air(0.008);

        // In cooling mode: maximize primary (cold) air
        // In heating mode: minimize primary, maximize recirculated warm air + reheat
        let primary_mass = if zone_load < -100.0 {
            // Cooling: use max primary flow
            max_primary_mass.min(total_mass_flow)
        } else {
            // Heating or deadband: use min primary
            min_primary_mass.min(total_mass_flow)
        };

        let secondary_mass = (total_mass_flow - primary_mass).max(0.0);

        // Mixed temperature (primary + secondary)
        let mixed_temp = if total_mass_flow > 1e-10 {
            (primary_mass * primary_temp + secondary_mass * zone_temp) / total_mass_flow
        } else {
            zone_temp
        };

        // Reheat in heating mode
        let reheat = if zone_load > 100.0 {
            // How much heating still needed after mixing
            let delivered_by_mix = total_mass_flow * cp * (mixed_temp - zone_temp);
            let remaining = zone_load - delivered_by_mix;
            remaining.clamp(0.0, self.reheat_capacity)
        } else {
            0.0
        };

        let discharge_temp = mixed_temp + reheat / (total_mass_flow * cp).max(1e-10);

        SeriesPIUResult {
            primary_mass_flow: primary_mass,
            secondary_mass_flow: secondary_mass,
            mixed_temp,
            discharge_temp,
            fan_power: self.fan_power,
            reheat_rate: reheat,
        }
    }
}

// ---------------------------------------------------------------------------
// VAV with Variable Speed Fan
// ---------------------------------------------------------------------------

/// VAV terminal with variable-speed fan (fan power varies as cube of flow ratio).
#[derive(Debug, Clone)]
pub struct VAVVariableSpeedFan {
    pub name: String,
    /// Maximum air flow (m3/s).
    pub max_air_flow: f64,
    /// Minimum flow fraction.
    pub min_flow_fraction: f64,
    /// Fan power at design flow (W).
    pub design_fan_power: f64,
    /// Reheat coil capacity (W).
    pub reheat_capacity: f64,
}

/// VAV variable-speed fan result.
#[derive(Debug, Clone, Copy)]
pub struct VAVVariableSpeedResult {
    /// Air mass flow (kg/s).
    pub air_mass_flow: f64,
    /// Supply temperature (C).
    pub supply_temp: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Reheat rate (W).
    pub reheat_rate: f64,
    /// Flow fraction (0-1).
    pub flow_fraction: f64,
}

impl VAVVariableSpeedFan {
    pub fn new(
        name: impl Into<String>,
        max_flow: f64,
        min_fraction: f64,
        design_fan_power: f64,
        reheat_capacity: f64,
    ) -> Self {
        Self {
            name: name.into(),
            max_air_flow: max_flow,
            min_flow_fraction: min_fraction,
            design_fan_power,
            reheat_capacity,
        }
    }

    /// Calculate VAV with variable-speed fan.
    ///
    /// Fan power = design_power * (flow_fraction)^3 (affinity law).
    pub fn calculate(
        &self,
        supply_temp: f64,
        zone_temp: f64,
        zone_load: f64,
        air_density: f64,
    ) -> VAVVariableSpeedResult {
        let max_mass_flow = self.max_air_flow * air_density;
        let min_mass_flow = max_mass_flow * self.min_flow_fraction;
        let cp = ep_psychrometrics::cp_air(0.008);

        if zone_load < -100.0 {
            // Cooling: modulate flow
            let needed_flow = if (zone_temp - supply_temp).abs() > 0.1 {
                (-zone_load / (cp * (zone_temp - supply_temp))).clamp(min_mass_flow, max_mass_flow)
            } else {
                max_mass_flow
            };
            let flow_frac = needed_flow / max_mass_flow.max(1e-10);
            let fan_power = self.design_fan_power * flow_frac.powi(3);

            VAVVariableSpeedResult {
                air_mass_flow: needed_flow,
                supply_temp,
                fan_power,
                reheat_rate: 0.0,
                flow_fraction: flow_frac,
            }
        } else if zone_load > 100.0 {
            // Heating: minimum flow + reheat
            let flow_frac = self.min_flow_fraction;
            let fan_power = self.design_fan_power * flow_frac.powi(3);
            let reheat = zone_load.min(self.reheat_capacity);
            let t_supply = supply_temp + reheat / (min_mass_flow * cp).max(1e-10);

            VAVVariableSpeedResult {
                air_mass_flow: min_mass_flow,
                supply_temp: t_supply,
                fan_power,
                reheat_rate: reheat,
                flow_fraction: flow_frac,
            }
        } else {
            // Deadband: minimum flow, no reheat
            let flow_frac = self.min_flow_fraction;
            let fan_power = self.design_fan_power * flow_frac.powi(3);

            VAVVariableSpeedResult {
                air_mass_flow: min_mass_flow,
                supply_temp,
                fan_power,
                reheat_rate: 0.0,
                flow_fraction: flow_frac,
            }
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

    // ====================================================================
    // Four-Pipe Fan Coil Tests
    // ====================================================================

    #[test]
    fn fan_coil_cooling() {
        let fc = FourPipeFanCoil::new("FC-1", 10000.0, 8000.0, 200.0);
        let result = fc.calculate(24.0, -5000.0, 7.0, 0.5, 60.0, 0.0, 1.2);
        assert!(result.cooling_rate > 0.0, "cooling={}", result.cooling_rate);
        assert!(result.heating_rate.abs() < 1e-10);
        assert!((result.fan_power - 200.0).abs() < 1.0);
        assert!(result.supply_temp < 24.0);
    }

    #[test]
    fn fan_coil_heating() {
        let fc = FourPipeFanCoil::new("FC-1", 10000.0, 8000.0, 200.0);
        let result = fc.calculate(20.0, 4000.0, 7.0, 0.0, 60.0, 0.3, 1.2);
        assert!(result.heating_rate > 0.0);
        assert!(result.cooling_rate.abs() < 1e-10);
        assert!(result.supply_temp > 20.0);
    }

    #[test]
    fn fan_coil_deadband() {
        let fc = FourPipeFanCoil::new("FC-1", 10000.0, 8000.0, 200.0);
        let result = fc.calculate(22.0, 50.0, 7.0, 0.5, 60.0, 0.3, 1.2);
        assert!(result.cooling_rate.abs() < 1e-10);
        assert!(result.heating_rate.abs() < 1e-10);
        assert!(result.fan_power.abs() < 1e-10);
    }

    // ====================================================================
    // Electric Baseboard Tests
    // ====================================================================

    #[test]
    fn electric_baseboard_full_load() {
        let bb = ElectricBaseboard::new("EB-1", 3000.0);
        let result = bb.calculate(3000.0);
        assert!((result.heating_rate - 3000.0).abs() < 1.0);
        assert!((result.electric_power - 3000.0).abs() < 1.0);
        assert!((result.plr - 1.0).abs() < 0.01);
    }

    #[test]
    fn electric_baseboard_part_load() {
        let bb = ElectricBaseboard::new("EB-1", 3000.0);
        let result = bb.calculate(1500.0);
        assert!((result.heating_rate - 1500.0).abs() < 1.0);
        assert!((result.plr - 0.5).abs() < 0.01);
    }

    #[test]
    fn electric_baseboard_no_load() {
        let bb = ElectricBaseboard::new("EB-1", 3000.0);
        let result = bb.calculate(-1000.0);
        assert!(result.heating_rate.abs() < 1e-10);
    }

    // ====================================================================
    // Radiant/Convective Baseboard Tests
    // ====================================================================

    #[test]
    fn radiant_baseboard_split() {
        let bb = RadiantConvectiveBaseboard::new("RB-1", 5000.0, 500.0, 0.1, 0.3);
        let result = bb.calculate(20.0, 80.0, 0.1);
        assert!(result.total_heating > 0.0);
        assert!((result.radiant_heating - result.total_heating * 0.3).abs() < 1.0);
        assert!((result.convective_heating - result.total_heating * 0.7).abs() < 1.0);
    }

    #[test]
    fn radiant_baseboard_energy_balance() {
        let bb = RadiantConvectiveBaseboard::new("RB-1", 5000.0, 500.0, 0.1, 0.3);
        let result = bb.calculate(20.0, 80.0, 0.1);
        let sum = result.radiant_heating + result.convective_heating;
        assert!((sum - result.total_heating).abs() < 1.0, "sum={}, total={}", sum, result.total_heating);
    }

    #[test]
    fn radiant_baseboard_no_flow() {
        let bb = RadiantConvectiveBaseboard::new("RB-1", 5000.0, 500.0, 0.1, 0.3);
        let result = bb.calculate(20.0, 80.0, 0.0);
        assert!(result.total_heating.abs() < 1e-10);
    }

    // ====================================================================
    // Unit Heater Tests
    // ====================================================================

    #[test]
    fn unit_heater_gas() {
        let uh = UnitHeater::new("UH-1", UnitHeaterCoilType::Gas, 20000.0, 300.0);
        let result = uh.calculate(18.0, 10000.0, 1.2);
        assert!((result.heating_rate - 10000.0).abs() < 100.0);
        // Gas at 80% efficiency: fuel = 10000/0.8 = 12500
        assert!(result.fuel_rate > 10000.0, "fuel={}", result.fuel_rate);
        assert!(result.fan_power > 0.0);
        assert!(result.supply_temp > 18.0);
    }

    #[test]
    fn unit_heater_electric() {
        let uh = UnitHeater::new("UH-E", UnitHeaterCoilType::Electric, 10000.0, 200.0);
        let result = uh.calculate(18.0, 5000.0, 1.2);
        assert!((result.heating_rate - 5000.0).abs() < 100.0);
        assert!((result.fuel_rate - 5000.0).abs() < 100.0, "Electric eff=1.0");
    }

    #[test]
    fn unit_heater_no_load() {
        let uh = UnitHeater::new("UH-1", UnitHeaterCoilType::Gas, 20000.0, 300.0);
        let result = uh.calculate(22.0, 0.0, 1.2);
        assert!(result.heating_rate.abs() < 1e-10);
        assert!(result.fan_power.abs() < 1e-10);
    }

    // ====================================================================
    // Unit Ventilator Tests
    // ====================================================================

    #[test]
    fn unit_ventilator_cooling() {
        let uv = UnitVentilator::new("UV-1", 10000.0, 8000.0, 250.0);
        let result = uv.calculate(26.0, -5000.0, 20.0, 1.2);
        assert!(result.cooling_rate > 0.0);
        assert!(result.supply_temp < 26.0);
        // Should use max OA for free cooling
        assert!((result.oa_fraction - uv.max_oa_fraction).abs() < 0.01);
    }

    #[test]
    fn unit_ventilator_heating() {
        let uv = UnitVentilator::new("UV-1", 10000.0, 8000.0, 250.0);
        let result = uv.calculate(18.0, 5000.0, -5.0, 1.2);
        assert!(result.heating_rate > 0.0);
        assert!(result.supply_temp > 18.0);
        assert!((result.oa_fraction - uv.min_oa_fraction).abs() < 0.01);
    }

    #[test]
    fn unit_ventilator_off() {
        let uv = UnitVentilator::new("UV-1", 10000.0, 8000.0, 250.0);
        let result = uv.calculate(22.0, 50.0, 20.0, 1.2);
        assert!(result.cooling_rate.abs() < 1e-10);
        assert!(result.heating_rate.abs() < 1e-10);
    }

    // ====================================================================
    // Window AC Tests
    // ====================================================================

    #[test]
    fn window_ac_cooling() {
        let wac = WindowAC::new("WAC-1", 5000.0, 3.0, 150.0);
        let result = wac.calculate(26.0, 0.010, -3000.0, 1.2);
        assert!((result.total_cooling - 3000.0).abs() < 100.0);
        assert!(result.sensible_cooling > 0.0);
        assert!(result.power > 0.0);
        assert!(result.supply_temp < 26.0);
    }

    #[test]
    fn window_ac_no_cooling() {
        let wac = WindowAC::new("WAC-1", 5000.0, 3.0, 150.0);
        let result = wac.calculate(22.0, 0.010, 1000.0, 1.2);
        assert!(result.total_cooling.abs() < 1e-10);
        assert!(result.power.abs() < 1e-10);
    }

    #[test]
    fn window_ac_shr_split() {
        let wac = WindowAC::new("WAC-1", 5000.0, 3.0, 150.0);
        let result = wac.calculate(26.0, 0.010, -5000.0, 1.2);
        assert!((result.sensible_cooling - result.total_cooling * 0.75).abs() < 100.0);
    }

    // ====================================================================
    // Radiant Electric Panel Tests
    // ====================================================================

    #[test]
    fn radiant_panel_heating() {
        let panel = LowTempRadiantElectric::new("RP-1", 2000.0, 0.6);
        let result = panel.calculate(1000.0);
        assert!((result.total_heating - 1000.0).abs() < 1.0);
        assert!((result.radiant_heating - 600.0).abs() < 1.0);
        assert!((result.convective_heating - 400.0).abs() < 1.0);
        assert!((result.electric_power - 1000.0).abs() < 1.0);
    }

    #[test]
    fn radiant_panel_no_load() {
        let panel = LowTempRadiantElectric::new("RP-1", 2000.0, 0.6);
        let result = panel.calculate(-500.0);
        assert!(result.total_heating.abs() < 1e-10);
    }

    #[test]
    fn radiant_panel_full_load() {
        let panel = LowTempRadiantElectric::new("RP-1", 2000.0, 0.6);
        let result = panel.calculate(5000.0);
        assert!((result.total_heating - 2000.0).abs() < 1.0);
        assert!((result.plr - 1.0).abs() < 0.01);
    }

    // ====================================================================
    // Dual Duct Terminal Tests
    // ====================================================================

    #[test]
    fn dual_duct_cv_cooling() {
        let dd = DualDuctTerminal::constant_volume("DD-1", 1.0);
        let result = dd.calculate(40.0, 13.0, 24.0, -5000.0, 1.2);
        // Cooling load → mostly cold duct
        assert!(result.cold_mass_flow > result.hot_mass_flow,
            "cold={} hot={}", result.cold_mass_flow, result.hot_mass_flow);
        assert!(result.supply_temp < 24.0, "T_sup={}", result.supply_temp);
        assert!((result.total_mass_flow - 1.0 * 1.2).abs() < 0.01);
    }

    #[test]
    fn dual_duct_cv_heating() {
        let dd = DualDuctTerminal::constant_volume("DD-1", 1.0);
        // Large heating load to push target supply above midpoint of hot/cold
        let result = dd.calculate(40.0, 13.0, 20.0, 20000.0, 1.2);
        // Heating load → mostly hot duct
        assert!(result.hot_mass_flow > result.cold_mass_flow,
            "hot={} cold={}", result.hot_mass_flow, result.cold_mass_flow);
        assert!(result.supply_temp > 20.0, "T_sup={}", result.supply_temp);
    }

    #[test]
    fn dual_duct_vav_cooling() {
        let dd = DualDuctTerminal::vav("DD-V", 1.0, 0.3);
        let result = dd.calculate(40.0, 13.0, 24.0, -3000.0, 1.2);
        // VAV cooling: cold duct only, modulated flow
        assert!(result.hot_mass_flow.abs() < 1e-10);
        assert!(result.cold_mass_flow > 0.0);
        assert!((result.supply_temp - 13.0).abs() < 0.1);
    }

    #[test]
    fn dual_duct_vav_heating() {
        let dd = DualDuctTerminal::vav("DD-V", 1.0, 0.3);
        let result = dd.calculate(40.0, 13.0, 20.0, 3000.0, 1.2);
        // VAV heating: hot duct only
        assert!(result.cold_mass_flow.abs() < 1e-10);
        assert!(result.hot_mass_flow > 0.0);
        assert!((result.supply_temp - 40.0).abs() < 0.1);
    }

    #[test]
    fn dual_duct_vav_deadband() {
        let dd = DualDuctTerminal::vav("DD-V", 1.0, 0.3);
        let result = dd.calculate(40.0, 13.0, 22.0, 0.0, 1.2);
        // Deadband: minimum flow from cold duct
        let min_flow = 1.0 * 0.3 * 1.2;
        assert!((result.total_mass_flow - min_flow).abs() < 0.01);
    }

    // ====================================================================
    // Series PIU Terminal Tests
    // ====================================================================

    #[test]
    fn series_piu_cooling() {
        let piu = SeriesPIUTerminal::new("PIU-1", 0.5, 0.3, 0.8, 200.0, 5000.0);
        let result = piu.calculate(13.0, 24.0, -5000.0, 1.2);
        // Cooling: max primary flow
        let max_primary = 0.5 * 1.2;
        assert!((result.primary_mass_flow - max_primary).abs() < 0.01);
        // Secondary fills the rest
        let total = 0.8 * 1.2;
        assert!((result.secondary_mass_flow - (total - max_primary)).abs() < 0.01);
        // Mixed temp between primary (13C) and zone (24C)
        assert!(result.mixed_temp > 13.0 && result.mixed_temp < 24.0,
            "mixed={}", result.mixed_temp);
        assert!(result.reheat_rate.abs() < 1e-10);
        assert!((result.fan_power - 200.0).abs() < 1.0);
    }

    #[test]
    fn series_piu_heating() {
        let piu = SeriesPIUTerminal::new("PIU-1", 0.5, 0.3, 0.8, 200.0, 5000.0);
        let result = piu.calculate(13.0, 20.0, 3000.0, 1.2);
        // Heating: min primary
        let min_primary = 0.5 * 0.3 * 1.2;
        assert!((result.primary_mass_flow - min_primary).abs() < 0.01);
        // Reheat active
        assert!(result.reheat_rate > 0.0, "reheat={}", result.reheat_rate);
        assert!(result.discharge_temp > result.mixed_temp);
    }

    #[test]
    fn series_piu_constant_total_flow() {
        let piu = SeriesPIUTerminal::new("PIU-1", 0.5, 0.3, 0.8, 200.0, 5000.0);
        let total = 0.8 * 1.2;

        let r1 = piu.calculate(13.0, 24.0, -5000.0, 1.2);
        assert!((r1.primary_mass_flow + r1.secondary_mass_flow - total).abs() < 0.01);

        let r2 = piu.calculate(13.0, 20.0, 3000.0, 1.2);
        assert!((r2.primary_mass_flow + r2.secondary_mass_flow - total).abs() < 0.01);
    }

    // ====================================================================
    // VAV Variable Speed Fan Tests
    // ====================================================================

    #[test]
    fn vav_vsf_cooling_partial() {
        let vav = VAVVariableSpeedFan::new("VAVF-1", 1.0, 0.3, 500.0, 5000.0);
        let result = vav.calculate(13.0, 24.0, -5000.0, 1.2);
        assert!(result.air_mass_flow > 0.3 * 1.2);
        assert!(result.air_mass_flow <= 1.0 * 1.2);
        // Fan power = design * (flow_frac)^3
        let expected_fan = 500.0 * result.flow_fraction.powi(3);
        assert!((result.fan_power - expected_fan).abs() < 1.0,
            "fan={} expected={}", result.fan_power, expected_fan);
        assert!(result.reheat_rate.abs() < 1e-10);
    }

    #[test]
    fn vav_vsf_heating_min_flow() {
        let vav = VAVVariableSpeedFan::new("VAVF-1", 1.0, 0.3, 500.0, 5000.0);
        let result = vav.calculate(13.0, 20.0, 3000.0, 1.2);
        assert!((result.flow_fraction - 0.3).abs() < 0.01);
        assert!(result.reheat_rate > 0.0);
        // Fan power at min flow: 500 * 0.3^3 = 500 * 0.027 = 13.5
        let expected_fan = 500.0 * 0.3_f64.powi(3);
        assert!((result.fan_power - expected_fan).abs() < 1.0,
            "fan={} expected={}", result.fan_power, expected_fan);
    }

    #[test]
    fn vav_vsf_fan_cube_law() {
        let vav = VAVVariableSpeedFan::new("VAVF-1", 1.0, 0.3, 500.0, 5000.0);

        // Full load cooling
        let r_full = vav.calculate(13.0, 24.0, -100000.0, 1.2);
        assert!((r_full.flow_fraction - 1.0).abs() < 0.01);
        assert!((r_full.fan_power - 500.0).abs() < 1.0);

        // Half load → less flow → much less fan power
        let r_half = vav.calculate(13.0, 24.0, -3000.0, 1.2);
        assert!(r_half.fan_power < r_full.fan_power * 0.5,
            "half={} full={}", r_half.fan_power, r_full.fan_power);
    }
}
