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
}
