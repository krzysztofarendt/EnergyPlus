//! Unitary system models (packaged rooftop, heat pumps, furnaces).
//!
//! A unitary system combines a fan, cooling coil, and/or heating coil
//! into a single packaged unit with integrated controls.

/// Unitary system type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitaryType {
    /// Single-speed DX cooling with gas/electric heating.
    CoolHeatSingleSpeed,
    /// Heat pump with DX cooling and DX heating.
    HeatPump,
    /// Furnace with heating only.
    FurnaceHeatingOnly,
}

/// Unitary system control mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnitaryControlMode {
    /// Load-based: system modulates to meet zone load.
    #[default]
    LoadBased,
    /// Setpoint-based: system targets outlet air setpoint.
    SetpointBased,
}

/// Unitary system specification.
#[derive(Debug, Clone)]
pub struct UnitarySystem {
    pub name: String,
    pub system_type: UnitaryType,
    pub control_mode: UnitaryControlMode,
    /// Design supply air flow rate for cooling (m3/s).
    pub cooling_supply_flow: f64,
    /// Design supply air flow rate for heating (m3/s).
    pub heating_supply_flow: f64,
    /// Cooling capacity (W).
    pub cooling_capacity: f64,
    /// Heating capacity (W).
    pub heating_capacity: f64,
    /// Fan location.
    pub fan_placement: FanPlacement,
    /// Availability (schedule-based).
    pub is_available: bool,
}

/// Fan placement in unitary system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FanPlacement {
    /// Fan blows through coils (blow-through).
    #[default]
    BlowThrough,
    /// Fan draws through coils (draw-through).
    DrawThrough,
}

/// Unitary system operating mode for the current timestep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitaryOperatingMode {
    Off,
    Cooling,
    Heating,
    /// Supplemental heating (heat pump backup).
    SupplementalHeating,
}

/// Unitary system calculation result.
#[derive(Debug, Clone, Copy)]
pub struct UnitaryResult {
    pub operating_mode: UnitaryOperatingMode,
    /// Total cooling delivered (W).
    pub cooling_rate: f64,
    /// Total heating delivered (W).
    pub heating_rate: f64,
    /// Fan electrical power (W).
    pub fan_power: f64,
    /// Compressor/burner power (W).
    pub compressor_power: f64,
    /// Supplemental heating power (W).
    pub supplemental_power: f64,
    /// Supply air outlet temperature (C).
    pub outlet_temp: f64,
    /// Part-load ratio.
    pub part_load_ratio: f64,
}

impl UnitarySystem {
    /// Create a cooling+heating unitary system.
    pub fn cool_heat(
        name: impl Into<String>,
        cooling_capacity: f64,
        heating_capacity: f64,
        supply_flow: f64,
    ) -> Self {
        Self {
            name: name.into(),
            system_type: UnitaryType::CoolHeatSingleSpeed,
            control_mode: UnitaryControlMode::LoadBased,
            cooling_supply_flow: supply_flow,
            heating_supply_flow: supply_flow,
            cooling_capacity,
            heating_capacity,
            fan_placement: FanPlacement::BlowThrough,
            is_available: true,
        }
    }

    /// Create a heat pump system.
    pub fn heat_pump(
        name: impl Into<String>,
        cooling_capacity: f64,
        heating_capacity: f64,
        supply_flow: f64,
    ) -> Self {
        Self {
            name: name.into(),
            system_type: UnitaryType::HeatPump,
            control_mode: UnitaryControlMode::LoadBased,
            cooling_supply_flow: supply_flow,
            heating_supply_flow: supply_flow,
            cooling_capacity,
            heating_capacity,
            fan_placement: FanPlacement::BlowThrough,
            is_available: true,
        }
    }

    /// Determine operating mode from zone load.
    pub fn determine_mode(&self, zone_load: f64) -> UnitaryOperatingMode {
        if !self.is_available {
            return UnitaryOperatingMode::Off;
        }
        if zone_load < -100.0 && self.cooling_capacity > 0.0 {
            UnitaryOperatingMode::Cooling
        } else if zone_load > 100.0 && self.heating_capacity > 0.0 {
            UnitaryOperatingMode::Heating
        } else {
            UnitaryOperatingMode::Off
        }
    }

    /// Simplified performance calculation.
    ///
    /// `zone_load` — zone sensible load (W, positive = heating, negative = cooling).
    /// `inlet_temp` — air inlet temperature (C).
    /// `inlet_w` — air inlet humidity ratio (kg/kg).
    /// `air_density` — air density (kg/m3).
    pub fn calculate(
        &self,
        zone_load: f64,
        inlet_temp: f64,
        inlet_w: f64,
        air_density: f64,
    ) -> UnitaryResult {
        let mode = self.determine_mode(zone_load);

        match mode {
            UnitaryOperatingMode::Off => UnitaryResult {
                operating_mode: mode,
                cooling_rate: 0.0,
                heating_rate: 0.0,
                fan_power: 0.0,
                compressor_power: 0.0,
                supplemental_power: 0.0,
                outlet_temp: inlet_temp,
                part_load_ratio: 0.0,
            },
            UnitaryOperatingMode::Cooling => {
                let plr = (-zone_load / self.cooling_capacity).clamp(0.0, 1.0);
                let cooling = self.cooling_capacity * plr;
                let mass_flow = self.cooling_supply_flow * air_density;
                let cp = ep_psychrometrics::cp_air(inlet_w);

                // Approximate compressor power (COP ~3.0)
                let compressor_power = cooling / 3.0;

                // Fan power (simplified)
                let fan_power = mass_flow * 600.0 / (0.7 * 0.9); // pressure * flow / eff

                let outlet_temp = inlet_temp - cooling / (mass_flow * cp).max(1e-10);

                UnitaryResult {
                    operating_mode: mode,
                    cooling_rate: cooling,
                    heating_rate: 0.0,
                    fan_power,
                    compressor_power,
                    supplemental_power: 0.0,
                    outlet_temp,
                    part_load_ratio: plr,
                }
            }
            UnitaryOperatingMode::Heating => {
                let plr = (zone_load / self.heating_capacity).clamp(0.0, 1.0);
                let heating = self.heating_capacity * plr;
                let mass_flow = self.heating_supply_flow * air_density;
                let cp = ep_psychrometrics::cp_air(inlet_w);

                let compressor_power = match self.system_type {
                    UnitaryType::HeatPump => heating / 3.5, // Heat pump COP ~3.5
                    _ => 0.0,
                };

                let fan_power = mass_flow * 600.0 / (0.7 * 0.9);
                let outlet_temp = inlet_temp + heating / (mass_flow * cp).max(1e-10);

                UnitaryResult {
                    operating_mode: mode,
                    cooling_rate: 0.0,
                    heating_rate: heating,
                    fan_power,
                    compressor_power,
                    supplemental_power: 0.0,
                    outlet_temp,
                    part_load_ratio: plr,
                }
            }
            UnitaryOperatingMode::SupplementalHeating => UnitaryResult {
                operating_mode: mode,
                cooling_rate: 0.0,
                heating_rate: 0.0,
                fan_power: 0.0,
                compressor_power: 0.0,
                supplemental_power: 0.0,
                outlet_temp: inlet_temp,
                part_load_ratio: 0.0,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Packaged Terminal Air Conditioner (PTAC)
// ---------------------------------------------------------------------------

/// Packaged terminal air conditioner (PTAC).
///
/// Combines DX cooling + heating (electric/gas/HW) + fan + OA mixing
/// in a through-the-wall package.
#[derive(Debug, Clone)]
pub struct PackagedTerminalAC {
    pub name: String,
    /// Cooling capacity (W).
    pub cooling_capacity: f64,
    /// Cooling COP.
    pub cooling_cop: f64,
    /// Heating capacity (W).
    pub heating_capacity: f64,
    /// Heating efficiency (1.0 for electric, 0.8 for gas).
    pub heating_efficiency: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Design supply air flow (m3/s).
    pub supply_air_flow: f64,
    /// Outdoor air fraction (0-1).
    pub oa_fraction: f64,
}

/// PTAC operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PTACMode {
    Off,
    Cooling,
    Heating,
}

/// PTAC result.
#[derive(Debug, Clone, Copy)]
pub struct PTACResult {
    pub mode: PTACMode,
    /// Cooling delivered (W).
    pub cooling_rate: f64,
    /// Heating delivered (W).
    pub heating_rate: f64,
    /// Compressor/burner power (W).
    pub power: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Supply temp (C).
    pub supply_temp: f64,
    /// PLR.
    pub plr: f64,
}

impl PackagedTerminalAC {
    pub fn new(
        name: impl Into<String>,
        cooling_capacity: f64,
        cooling_cop: f64,
        heating_capacity: f64,
    ) -> Self {
        Self {
            name: name.into(),
            cooling_capacity,
            cooling_cop,
            heating_capacity,
            heating_efficiency: 1.0,
            fan_power: 200.0,
            supply_air_flow: 0.5,
            oa_fraction: 0.3,
        }
    }

    pub fn with_gas_heat(mut self, efficiency: f64) -> Self {
        self.heating_efficiency = efficiency.clamp(0.01, 1.0);
        self
    }

    /// Calculate PTAC performance.
    pub fn calculate(
        &self,
        zone_temp: f64,
        zone_load: f64,
        outdoor_temp: f64,
        air_density: f64,
    ) -> PTACResult {
        let mass_flow = self.supply_air_flow * air_density;
        let cp = ep_psychrometrics::cp_air(0.008);

        // OA mixing
        let mixed_temp = self.oa_fraction * outdoor_temp + (1.0 - self.oa_fraction) * zone_temp;

        if zone_load < -100.0 && self.cooling_capacity > 0.0 {
            let plr = (-zone_load / self.cooling_capacity).clamp(0.0, 1.0);
            let cooling = self.cooling_capacity * plr;
            let power = cooling / self.cooling_cop.max(0.01);
            let supply_temp = mixed_temp - cooling / (mass_flow * cp).max(1e-10);
            PTACResult {
                mode: PTACMode::Cooling, cooling_rate: cooling, heating_rate: 0.0,
                power, fan_power: self.fan_power, supply_temp, plr,
            }
        } else if zone_load > 100.0 && self.heating_capacity > 0.0 {
            let plr = (zone_load / self.heating_capacity).clamp(0.0, 1.0);
            let heating = self.heating_capacity * plr;
            let power = heating / self.heating_efficiency.max(0.01);
            let supply_temp = mixed_temp + heating / (mass_flow * cp).max(1e-10);
            PTACResult {
                mode: PTACMode::Heating, cooling_rate: 0.0, heating_rate: heating,
                power, fan_power: self.fan_power, supply_temp, plr,
            }
        } else {
            PTACResult {
                mode: PTACMode::Off, cooling_rate: 0.0, heating_rate: 0.0,
                power: 0.0, fan_power: 0.0, supply_temp: zone_temp, plr: 0.0,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Packaged Terminal Heat Pump (PTHP)
// ---------------------------------------------------------------------------

/// Packaged terminal heat pump (PTHP).
///
/// DX cooling + DX heating + supplemental electric + fan + OA.
#[derive(Debug, Clone)]
pub struct PackagedTerminalHP {
    pub name: String,
    /// DX cooling capacity (W).
    pub cooling_capacity: f64,
    /// Cooling COP.
    pub cooling_cop: f64,
    /// DX heating capacity (W).
    pub heating_capacity: f64,
    /// Heating COP.
    pub heating_cop: f64,
    /// Supplemental electric heating capacity (W).
    pub supplemental_capacity: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Supply air flow (m3/s).
    pub supply_air_flow: f64,
    /// OA fraction.
    pub oa_fraction: f64,
    /// Defrost onset temperature (C).
    pub defrost_onset_temp: f64,
    /// Minimum compressor outdoor temp (C).
    pub min_outdoor_temp: f64,
}

/// PTHP operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PTHPMode {
    Off,
    Cooling,
    Heating,
    SupplementalHeating,
}

/// PTHP result.
#[derive(Debug, Clone, Copy)]
pub struct PTHPResult {
    pub mode: PTHPMode,
    pub cooling_rate: f64,
    pub heating_rate: f64,
    pub compressor_power: f64,
    pub supplemental_power: f64,
    pub fan_power: f64,
    pub supply_temp: f64,
    pub plr: f64,
    pub defrost_active: bool,
}

impl PackagedTerminalHP {
    pub fn new(
        name: impl Into<String>,
        cooling_capacity: f64,
        cooling_cop: f64,
        heating_capacity: f64,
        heating_cop: f64,
    ) -> Self {
        Self {
            name: name.into(),
            cooling_capacity, cooling_cop,
            heating_capacity, heating_cop,
            supplemental_capacity: 5000.0,
            fan_power: 200.0,
            supply_air_flow: 0.5,
            oa_fraction: 0.3,
            defrost_onset_temp: 5.0,
            min_outdoor_temp: -15.0,
        }
    }

    pub fn with_supplemental(mut self, capacity: f64) -> Self {
        self.supplemental_capacity = capacity;
        self
    }

    /// Calculate PTHP performance.
    pub fn calculate(
        &self,
        zone_temp: f64,
        zone_load: f64,
        outdoor_temp: f64,
        air_density: f64,
    ) -> PTHPResult {
        let mass_flow = self.supply_air_flow * air_density;
        let cp = ep_psychrometrics::cp_air(0.008);
        let mixed_temp = self.oa_fraction * outdoor_temp + (1.0 - self.oa_fraction) * zone_temp;

        if zone_load < -100.0 && self.cooling_capacity > 0.0 {
            let plr = (-zone_load / self.cooling_capacity).clamp(0.0, 1.0);
            let cooling = self.cooling_capacity * plr;
            let power = cooling / self.cooling_cop.max(0.01);
            let supply_temp = mixed_temp - cooling / (mass_flow * cp).max(1e-10);
            PTHPResult {
                mode: PTHPMode::Cooling, cooling_rate: cooling, heating_rate: 0.0,
                compressor_power: power, supplemental_power: 0.0,
                fan_power: self.fan_power, supply_temp, plr, defrost_active: false,
            }
        } else if zone_load > 100.0 && (self.heating_capacity > 0.0 || self.supplemental_capacity > 0.0) {
            // Defrost reduces HP capacity when cold
            let defrost_active = outdoor_temp < self.defrost_onset_temp;
            let defrost_reduction = if defrost_active {
                0.1 * ((self.defrost_onset_temp - outdoor_temp) / self.defrost_onset_temp.abs().max(1.0)).clamp(0.0, 0.3)
            } else {
                0.0
            };

            let compressor_ok = outdoor_temp >= self.min_outdoor_temp;
            let available_hp_cap = if compressor_ok {
                self.heating_capacity * (1.0 - defrost_reduction)
            } else {
                0.0
            };

            let hp_heating = zone_load.min(available_hp_cap);
            let remaining = (zone_load - hp_heating).max(0.0);
            let supplemental = remaining.min(self.supplemental_capacity);
            let total = hp_heating + supplemental;

            let plr = if self.heating_capacity > 0.0 {
                (hp_heating / self.heating_capacity).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let compressor_power = if compressor_ok && hp_heating > 0.0 {
                hp_heating / self.heating_cop.max(0.01)
            } else {
                0.0
            };
            let supply_temp = mixed_temp + total / (mass_flow * cp).max(1e-10);

            let mode = if supplemental > 0.0 {
                PTHPMode::SupplementalHeating
            } else {
                PTHPMode::Heating
            };

            PTHPResult {
                mode, cooling_rate: 0.0, heating_rate: total,
                compressor_power, supplemental_power: supplemental,
                fan_power: self.fan_power, supply_temp, plr, defrost_active,
            }
        } else {
            PTHPResult {
                mode: PTHPMode::Off, cooling_rate: 0.0, heating_rate: 0.0,
                compressor_power: 0.0, supplemental_power: 0.0,
                fan_power: 0.0, supply_temp: zone_temp, plr: 0.0, defrost_active: false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unitary_cool_heat_basic() {
        let sys = UnitarySystem::cool_heat("RTU-1", 20000.0, 15000.0, 1.0);
        assert!((sys.cooling_capacity - 20000.0).abs() < 1e-10);
        assert!((sys.heating_capacity - 15000.0).abs() < 1e-10);
    }

    #[test]
    fn unitary_cooling_mode() {
        let sys = UnitarySystem::cool_heat("RTU", 20000.0, 15000.0, 1.0);
        let result = sys.calculate(-10000.0, 24.0, 0.009, 1.2);
        assert_eq!(result.operating_mode, UnitaryOperatingMode::Cooling);
        assert!((result.cooling_rate - 10000.0).abs() < 100.0);
        assert!(result.outlet_temp < 24.0, "T_out={}", result.outlet_temp);
        assert!((result.part_load_ratio - 0.5).abs() < 0.01);
    }

    #[test]
    fn unitary_heating_mode() {
        let sys = UnitarySystem::cool_heat("RTU", 20000.0, 15000.0, 1.0);
        let result = sys.calculate(10000.0, 18.0, 0.006, 1.2);
        assert_eq!(result.operating_mode, UnitaryOperatingMode::Heating);
        assert!((result.heating_rate - 10000.0).abs() < 100.0);
        assert!(result.outlet_temp > 18.0, "T_out={}", result.outlet_temp);
    }

    #[test]
    fn unitary_off_small_load() {
        let sys = UnitarySystem::cool_heat("RTU", 20000.0, 15000.0, 1.0);
        let result = sys.calculate(50.0, 22.0, 0.008, 1.2); // Small load
        assert_eq!(result.operating_mode, UnitaryOperatingMode::Off);
    }

    #[test]
    fn unitary_unavailable() {
        let mut sys = UnitarySystem::cool_heat("RTU", 20000.0, 15000.0, 1.0);
        sys.is_available = false;
        let result = sys.calculate(-10000.0, 24.0, 0.009, 1.2);
        assert_eq!(result.operating_mode, UnitaryOperatingMode::Off);
    }

    #[test]
    fn heat_pump_mode() {
        let sys = UnitarySystem::heat_pump("HP", 20000.0, 18000.0, 1.0);
        let result = sys.calculate(10000.0, 18.0, 0.006, 1.2);
        assert_eq!(result.operating_mode, UnitaryOperatingMode::Heating);
        assert!(result.compressor_power > 0.0, "comp_power={}", result.compressor_power);
    }

    #[test]
    fn gas_furnace_no_compressor() {
        let sys = UnitarySystem::cool_heat("Furnace", 20000.0, 15000.0, 1.0);
        let result = sys.calculate(10000.0, 18.0, 0.006, 1.2);
        // Gas furnace: no compressor power in heating
        assert!(result.compressor_power.abs() < 1e-10);
    }

    // ====================================================================
    // PTAC Tests
    // ====================================================================

    #[test]
    fn ptac_cooling() {
        let ptac = PackagedTerminalAC::new("PTAC-1", 8000.0, 3.0, 6000.0);
        let result = ptac.calculate(26.0, -5000.0, 35.0, 1.2);
        assert_eq!(result.mode, PTACMode::Cooling);
        assert!((result.cooling_rate - 5000.0).abs() < 100.0);
        assert!(result.power > 0.0);
        assert!(result.supply_temp < 26.0);
    }

    #[test]
    fn ptac_heating() {
        let ptac = PackagedTerminalAC::new("PTAC-1", 8000.0, 3.0, 6000.0);
        let result = ptac.calculate(18.0, 4000.0, -5.0, 1.2);
        assert_eq!(result.mode, PTACMode::Heating);
        assert!((result.heating_rate - 4000.0).abs() < 100.0);
    }

    #[test]
    fn ptac_off() {
        let ptac = PackagedTerminalAC::new("PTAC-1", 8000.0, 3.0, 6000.0);
        let result = ptac.calculate(22.0, 50.0, 25.0, 1.2);
        assert_eq!(result.mode, PTACMode::Off);
    }

    #[test]
    fn ptac_gas_heat() {
        let ptac = PackagedTerminalAC::new("PTAC-Gas", 8000.0, 3.0, 6000.0)
            .with_gas_heat(0.80);
        let result = ptac.calculate(18.0, 6000.0, -5.0, 1.2);
        // Fuel = 6000 / 0.80 = 7500
        assert!(result.power > 6000.0, "power={}", result.power);
    }

    #[test]
    fn ptac_oa_mixing() {
        let ptac = PackagedTerminalAC::new("PTAC-OA", 8000.0, 3.0, 6000.0);
        let result = ptac.calculate(26.0, -8000.0, 35.0, 1.2);
        // With 30% OA at 35C and 70% return at 26C:
        // mixed = 0.3*35 + 0.7*26 = 28.7
        // Supply should be below 28.7
        assert!(result.supply_temp < 28.7, "T_sup={}", result.supply_temp);
    }

    #[test]
    fn ptac_plr_clamped() {
        let ptac = PackagedTerminalAC::new("PTAC", 8000.0, 3.0, 6000.0);
        let result = ptac.calculate(26.0, -20000.0, 35.0, 1.2);
        assert!((result.plr - 1.0).abs() < 0.01);
        assert!((result.cooling_rate - 8000.0).abs() < 100.0);
    }

    // ====================================================================
    // PTHP Tests
    // ====================================================================

    #[test]
    fn pthp_cooling() {
        let pthp = PackagedTerminalHP::new("PTHP-1", 8000.0, 3.5, 7000.0, 3.0);
        let result = pthp.calculate(26.0, -5000.0, 35.0, 1.2);
        assert_eq!(result.mode, PTHPMode::Cooling);
        assert!((result.cooling_rate - 5000.0).abs() < 100.0);
        assert!(result.compressor_power > 0.0);
    }

    #[test]
    fn pthp_heating_mild() {
        let pthp = PackagedTerminalHP::new("PTHP-1", 8000.0, 3.5, 7000.0, 3.0);
        let result = pthp.calculate(18.0, 5000.0, 10.0, 1.2);
        assert_eq!(result.mode, PTHPMode::Heating);
        assert!((result.heating_rate - 5000.0).abs() < 100.0);
        assert!(result.supplemental_power.abs() < 1e-10);
        assert!(!result.defrost_active);
    }

    #[test]
    fn pthp_heating_cold_defrost() {
        let pthp = PackagedTerminalHP::new("PTHP-1", 8000.0, 3.5, 7000.0, 3.0);
        let result = pthp.calculate(18.0, 5000.0, -5.0, 1.2);
        assert!(result.defrost_active, "Defrost should be active below onset");
    }

    #[test]
    fn pthp_supplemental_needed() {
        let pthp = PackagedTerminalHP::new("PTHP-1", 8000.0, 3.5, 7000.0, 3.0)
            .with_supplemental(5000.0);
        // Load exceeds HP capacity
        let result = pthp.calculate(18.0, 10000.0, 10.0, 1.2);
        assert_eq!(result.mode, PTHPMode::SupplementalHeating);
        assert!(result.supplemental_power > 0.0, "sup={}", result.supplemental_power);
        assert!((result.heating_rate - 10000.0).abs() < 100.0);
    }

    #[test]
    fn pthp_compressor_lockout() {
        let pthp = PackagedTerminalHP::new("PTHP-1", 8000.0, 3.5, 7000.0, 3.0)
            .with_supplemental(10000.0);
        // Below min outdoor temp: HP off, supplemental only
        let result = pthp.calculate(18.0, 5000.0, -20.0, 1.2);
        assert!(result.compressor_power.abs() < 1e-10, "HP should be off");
        assert!(result.supplemental_power > 0.0, "Supplemental should run");
    }

    #[test]
    fn pthp_off() {
        let pthp = PackagedTerminalHP::new("PTHP-1", 8000.0, 3.5, 7000.0, 3.0);
        let result = pthp.calculate(22.0, 50.0, 25.0, 1.2);
        assert_eq!(result.mode, PTHPMode::Off);
    }

    #[test]
    fn pthp_supply_temp_rises_heating() {
        let pthp = PackagedTerminalHP::new("PTHP", 8000.0, 3.5, 7000.0, 3.0);
        let result = pthp.calculate(18.0, 5000.0, 10.0, 1.2);
        // Mixed temp = 0.3*10 + 0.7*18 = 15.6
        assert!(result.supply_temp > 15.6, "T_sup={}", result.supply_temp);
    }
}
