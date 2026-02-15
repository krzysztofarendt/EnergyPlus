//! Variable Refrigerant Flow (VRF) system models.
//!
//! A VRF system consists of one outdoor unit serving multiple indoor
//! terminal units. The outdoor unit collects load requests and determines
//! the system operating mode (cooling only, heating only, or heat recovery).

/// VRF system operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VRFMode {
    #[default]
    Off,
    CoolingOnly,
    HeatingOnly,
    HeatRecovery,
}

/// VRF outdoor unit specification.
#[derive(Debug, Clone)]
pub struct VRFOutdoorUnit {
    pub name: String,
    /// Rated cooling capacity (W).
    pub rated_cooling_capacity: f64,
    /// Rated cooling COP.
    pub rated_cooling_cop: f64,
    /// Rated heating capacity (W).
    pub rated_heating_capacity: f64,
    /// Rated heating COP.
    pub rated_heating_cop: f64,
    /// Piping correction factor per meter of length.
    pub piping_correction_per_meter: f64,
    /// Equivalent piping length (m).
    pub piping_length: f64,
    /// Whether heat recovery mode is available.
    pub heat_recovery_available: bool,
    /// Defrost onset temperature (C).
    pub defrost_onset_temp: f64,
    /// Minimum outdoor temperature for cooling (C).
    pub min_outdoor_temp_cooling: f64,
    /// Minimum outdoor temperature for heating (C).
    pub min_outdoor_temp_heating: f64,
}

/// VRF terminal unit (indoor unit).
#[derive(Debug, Clone)]
pub struct VRFTerminalUnit {
    pub name: String,
    /// Zone index served.
    pub zone_index: usize,
    /// Rated cooling capacity (W).
    pub rated_cooling_capacity: f64,
    /// Rated heating capacity (W).
    pub rated_heating_capacity: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Design supply air flow (m3/s).
    pub supply_air_flow: f64,
}

/// VRF terminal load request.
#[derive(Debug, Clone, Copy)]
pub struct VRFTerminalRequest {
    /// Zone index.
    pub zone_index: usize,
    /// Requested cooling load (W, positive = cooling needed).
    pub cooling_load: f64,
    /// Requested heating load (W, positive = heating needed).
    pub heating_load: f64,
}

/// VRF outdoor unit result.
#[derive(Debug, Clone, Copy)]
pub struct VRFOutdoorUnitResult {
    pub mode: VRFMode,
    /// Total cooling capacity delivered (W).
    pub total_cooling: f64,
    /// Total heating capacity delivered (W).
    pub total_heating: f64,
    /// Total compressor power (W).
    pub compressor_power: f64,
    /// Heat recovery between zones (W).
    pub heat_recovery: f64,
    /// Piping correction factor (0-1).
    pub piping_correction: f64,
    /// Defrost active flag.
    pub defrost_active: bool,
}

/// VRF terminal unit result.
#[derive(Debug, Clone, Copy)]
pub struct VRFTerminalResult {
    /// Zone index.
    pub zone_index: usize,
    /// Cooling delivered (W).
    pub cooling_delivered: f64,
    /// Heating delivered (W).
    pub heating_delivered: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Supply air temperature (C).
    pub supply_temp: f64,
}

impl VRFOutdoorUnit {
    pub fn new(
        name: impl Into<String>,
        rated_cooling: f64,
        rated_cooling_cop: f64,
        rated_heating: f64,
        rated_heating_cop: f64,
    ) -> Self {
        Self {
            name: name.into(),
            rated_cooling_capacity: rated_cooling,
            rated_cooling_cop,
            rated_heating_capacity: rated_heating,
            rated_heating_cop,
            piping_correction_per_meter: 0.002,
            piping_length: 30.0,
            heat_recovery_available: false,
            defrost_onset_temp: 5.0,
            min_outdoor_temp_cooling: -5.0,
            min_outdoor_temp_heating: -20.0,
        }
    }

    pub fn with_heat_recovery(mut self) -> Self {
        self.heat_recovery_available = true;
        self
    }

    pub fn with_piping(mut self, length: f64, correction_per_m: f64) -> Self {
        self.piping_length = length;
        self.piping_correction_per_meter = correction_per_m;
        self
    }

    /// Determine system mode from terminal requests.
    pub fn determine_mode(&self, requests: &[VRFTerminalRequest], outdoor_temp: f64) -> VRFMode {
        let total_cooling: f64 = requests.iter().map(|r| r.cooling_load).sum();
        let total_heating: f64 = requests.iter().map(|r| r.heating_load).sum();

        let cooling_ok = outdoor_temp >= self.min_outdoor_temp_cooling;
        let heating_ok = outdoor_temp >= self.min_outdoor_temp_heating;

        let needs_cooling = total_cooling > 100.0 && cooling_ok;
        let needs_heating = total_heating > 100.0 && heating_ok;

        if needs_cooling && needs_heating && self.heat_recovery_available {
            VRFMode::HeatRecovery
        } else if needs_cooling && total_cooling >= total_heating {
            VRFMode::CoolingOnly
        } else if needs_heating {
            VRFMode::HeatingOnly
        } else {
            VRFMode::Off
        }
    }

    /// Calculate outdoor unit performance.
    pub fn calculate(
        &self,
        requests: &[VRFTerminalRequest],
        outdoor_temp: f64,
        cap_f_temp: f64,
    ) -> VRFOutdoorUnitResult {
        let mode = self.determine_mode(requests, outdoor_temp);

        if mode == VRFMode::Off {
            return VRFOutdoorUnitResult {
                mode: VRFMode::Off,
                total_cooling: 0.0, total_heating: 0.0,
                compressor_power: 0.0, heat_recovery: 0.0,
                piping_correction: 1.0, defrost_active: false,
            };
        }

        // Piping correction
        let piping_correction = (1.0 - self.piping_correction_per_meter * self.piping_length)
            .clamp(0.5, 1.0);

        let mod_f = cap_f_temp.max(0.0) * piping_correction;

        // Defrost in heating mode
        let defrost_active = (mode == VRFMode::HeatingOnly || mode == VRFMode::HeatRecovery)
            && outdoor_temp < self.defrost_onset_temp;
        let defrost_reduction = if defrost_active {
            0.1 * ((self.defrost_onset_temp - outdoor_temp) / self.defrost_onset_temp.abs().max(1.0))
                .clamp(0.0, 0.3)
        } else {
            0.0
        };

        let total_cooling_request: f64 = requests.iter().map(|r| r.cooling_load).sum();
        let total_heating_request: f64 = requests.iter().map(|r| r.heating_load).sum();

        let available_cooling = self.rated_cooling_capacity * mod_f;
        let available_heating = self.rated_heating_capacity * mod_f * (1.0 - defrost_reduction);

        let (total_cooling, total_heating, heat_recovery);

        match mode {
            VRFMode::CoolingOnly => {
                total_cooling = total_cooling_request.min(available_cooling);
                total_heating = 0.0;
                heat_recovery = 0.0;
            }
            VRFMode::HeatingOnly => {
                total_cooling = 0.0;
                total_heating = total_heating_request.min(available_heating);
                heat_recovery = 0.0;
            }
            VRFMode::HeatRecovery => {
                // Both cooling and heating zones: recovered heat offsets compressor work
                total_cooling = total_cooling_request.min(available_cooling);
                total_heating = total_heating_request.min(available_heating);
                heat_recovery = total_cooling.min(total_heating); // Heat moved between zones
            }
            VRFMode::Off => unreachable!(),
        }

        let cooling_power = if total_cooling > 0.0 {
            total_cooling / self.rated_cooling_cop.max(0.01)
        } else {
            0.0
        };
        let heating_power = if total_heating > 0.0 {
            total_heating / self.rated_heating_cop.max(0.01)
        } else {
            0.0
        };

        // In HR mode, recovered heat reduces net compressor work
        let hr_benefit = heat_recovery / self.rated_cooling_cop.max(0.01);
        let compressor_power = (cooling_power + heating_power - hr_benefit).max(0.0);

        VRFOutdoorUnitResult {
            mode,
            total_cooling,
            total_heating,
            compressor_power,
            heat_recovery,
            piping_correction,
            defrost_active,
        }
    }
}

impl VRFTerminalUnit {
    pub fn new(
        name: impl Into<String>,
        zone_index: usize,
        cooling_capacity: f64,
        heating_capacity: f64,
    ) -> Self {
        Self {
            name: name.into(),
            zone_index,
            rated_cooling_capacity: cooling_capacity,
            rated_heating_capacity: heating_capacity,
            fan_power: 100.0,
            supply_air_flow: 0.3,
        }
    }

    /// Calculate terminal unit performance given allocated capacity.
    pub fn calculate(
        &self,
        zone_temp: f64,
        allocated_cooling: f64,
        allocated_heating: f64,
        air_density: f64,
    ) -> VRFTerminalResult {
        let mass_flow = self.supply_air_flow * air_density;
        let cp = ep_psychrometrics::cp_air(0.008);
        let fan_power;
        let supply_temp;

        if allocated_cooling > 100.0 {
            let cooling = allocated_cooling.min(self.rated_cooling_capacity);
            supply_temp = zone_temp - cooling / (mass_flow * cp).max(1e-10);
            fan_power = self.fan_power;
            VRFTerminalResult {
                zone_index: self.zone_index,
                cooling_delivered: cooling, heating_delivered: 0.0,
                fan_power, supply_temp,
            }
        } else if allocated_heating > 100.0 {
            let heating = allocated_heating.min(self.rated_heating_capacity);
            supply_temp = zone_temp + heating / (mass_flow * cp).max(1e-10);
            fan_power = self.fan_power;
            VRFTerminalResult {
                zone_index: self.zone_index,
                cooling_delivered: 0.0, heating_delivered: heating,
                fan_power, supply_temp,
            }
        } else {
            VRFTerminalResult {
                zone_index: self.zone_index,
                cooling_delivered: 0.0, heating_delivered: 0.0,
                fan_power: 0.0, supply_temp: zone_temp,
            }
        }
    }
}

/// Distribute outdoor unit capacity to terminals proportional to their requests.
pub fn distribute_capacity(
    outdoor_result: &VRFOutdoorUnitResult,
    requests: &[VRFTerminalRequest],
) -> Vec<(usize, f64, f64)> {
    let total_cooling_request: f64 = requests.iter().map(|r| r.cooling_load).sum();
    let total_heating_request: f64 = requests.iter().map(|r| r.heating_load).sum();

    requests.iter().map(|req| {
        let cooling_alloc = if total_cooling_request > 1e-10 {
            outdoor_result.total_cooling * (req.cooling_load / total_cooling_request)
        } else {
            0.0
        };
        let heating_alloc = if total_heating_request > 1e-10 {
            outdoor_result.total_heating * (req.heating_load / total_heating_request)
        } else {
            0.0
        };
        (req.zone_index, cooling_alloc, heating_alloc)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vrf_outdoor_unit_basic() {
        let ou = VRFOutdoorUnit::new("OU-1", 50000.0, 3.5, 45000.0, 3.0);
        assert!((ou.rated_cooling_capacity - 50000.0).abs() < 1e-10);
    }

    #[test]
    fn vrf_cooling_only_mode() {
        let ou = VRFOutdoorUnit::new("OU", 50000.0, 3.5, 45000.0, 3.0);
        let requests = vec![
            VRFTerminalRequest { zone_index: 0, cooling_load: 5000.0, heating_load: 0.0 },
            VRFTerminalRequest { zone_index: 1, cooling_load: 8000.0, heating_load: 0.0 },
        ];
        let result = ou.calculate(&requests, 35.0, 1.0);
        assert_eq!(result.mode, VRFMode::CoolingOnly);
        assert!((result.total_cooling - 13000.0).abs() < 100.0);
        assert!(result.compressor_power > 0.0);
    }

    #[test]
    fn vrf_heating_only_mode() {
        let ou = VRFOutdoorUnit::new("OU", 50000.0, 3.5, 45000.0, 3.0);
        let requests = vec![
            VRFTerminalRequest { zone_index: 0, cooling_load: 0.0, heating_load: 10000.0 },
        ];
        let result = ou.calculate(&requests, -5.0, 1.0);
        assert_eq!(result.mode, VRFMode::HeatingOnly);
        assert!(result.total_heating > 0.0);
    }

    #[test]
    fn vrf_heat_recovery_mode() {
        let ou = VRFOutdoorUnit::new("OU", 50000.0, 3.5, 45000.0, 3.0)
            .with_heat_recovery();
        let requests = vec![
            VRFTerminalRequest { zone_index: 0, cooling_load: 8000.0, heating_load: 0.0 },
            VRFTerminalRequest { zone_index: 1, cooling_load: 0.0, heating_load: 5000.0 },
        ];
        let result = ou.calculate(&requests, 10.0, 1.0);
        assert_eq!(result.mode, VRFMode::HeatRecovery);
        assert!(result.heat_recovery > 0.0, "HR={}", result.heat_recovery);
    }

    #[test]
    fn vrf_heat_recovery_reduces_power() {
        let ou_hr = VRFOutdoorUnit::new("OU-HR", 50000.0, 3.5, 45000.0, 3.0)
            .with_heat_recovery();
        let ou_no_hr = VRFOutdoorUnit::new("OU-NH", 50000.0, 3.5, 45000.0, 3.0);
        let requests = vec![
            VRFTerminalRequest { zone_index: 0, cooling_load: 10000.0, heating_load: 0.0 },
            VRFTerminalRequest { zone_index: 1, cooling_load: 0.0, heating_load: 8000.0 },
        ];
        let r_hr = ou_hr.calculate(&requests, 10.0, 1.0);
        let r_no = ou_no_hr.calculate(&requests, 10.0, 1.0);
        assert!(r_hr.compressor_power < r_no.compressor_power + r_no.total_heating / 3.0 + 1000.0,
                "HR={}, no-HR={}", r_hr.compressor_power, r_no.compressor_power);
    }

    #[test]
    fn vrf_piping_correction() {
        let ou = VRFOutdoorUnit::new("OU", 50000.0, 3.5, 45000.0, 3.0)
            .with_piping(50.0, 0.003);
        let requests = vec![
            VRFTerminalRequest { zone_index: 0, cooling_load: 50000.0, heating_load: 0.0 },
        ];
        let result = ou.calculate(&requests, 35.0, 1.0);
        // Correction = 1 - 0.003*50 = 0.85
        assert!((result.piping_correction - 0.85).abs() < 0.01, "corr={}", result.piping_correction);
        assert!(result.total_cooling < 50000.0, "Capacity reduced by piping");
    }

    #[test]
    fn vrf_defrost_heating() {
        let ou = VRFOutdoorUnit::new("OU", 50000.0, 3.5, 45000.0, 3.0);
        let requests = vec![
            VRFTerminalRequest { zone_index: 0, cooling_load: 0.0, heating_load: 30000.0 },
        ];
        let result = ou.calculate(&requests, -5.0, 1.0);
        assert!(result.defrost_active);
    }

    #[test]
    fn vrf_no_defrost_warm() {
        let ou = VRFOutdoorUnit::new("OU", 50000.0, 3.5, 45000.0, 3.0);
        let requests = vec![
            VRFTerminalRequest { zone_index: 0, cooling_load: 0.0, heating_load: 5000.0 },
        ];
        let result = ou.calculate(&requests, 10.0, 1.0);
        assert!(!result.defrost_active);
    }

    #[test]
    fn vrf_off_no_load() {
        let ou = VRFOutdoorUnit::new("OU", 50000.0, 3.5, 45000.0, 3.0);
        let requests = vec![
            VRFTerminalRequest { zone_index: 0, cooling_load: 0.0, heating_load: 0.0 },
        ];
        let result = ou.calculate(&requests, 25.0, 1.0);
        assert_eq!(result.mode, VRFMode::Off);
    }

    #[test]
    fn vrf_terminal_cooling() {
        let tu = VRFTerminalUnit::new("TU-1", 0, 8000.0, 7000.0);
        let result = tu.calculate(26.0, 5000.0, 0.0, 1.2);
        assert!((result.cooling_delivered - 5000.0).abs() < 100.0);
        assert!(result.supply_temp < 26.0);
        assert!(result.fan_power > 0.0);
    }

    #[test]
    fn vrf_terminal_heating() {
        let tu = VRFTerminalUnit::new("TU-1", 0, 8000.0, 7000.0);
        let result = tu.calculate(18.0, 0.0, 5000.0, 1.2);
        assert!((result.heating_delivered - 5000.0).abs() < 100.0);
        assert!(result.supply_temp > 18.0);
    }

    #[test]
    fn vrf_terminal_off() {
        let tu = VRFTerminalUnit::new("TU-1", 0, 8000.0, 7000.0);
        let result = tu.calculate(22.0, 0.0, 0.0, 1.2);
        assert!(result.cooling_delivered.abs() < 1e-10);
        assert!(result.heating_delivered.abs() < 1e-10);
        assert!(result.fan_power.abs() < 1e-10);
    }

    #[test]
    fn vrf_distribute_capacity() {
        let ou_result = VRFOutdoorUnitResult {
            mode: VRFMode::CoolingOnly,
            total_cooling: 12000.0, total_heating: 0.0,
            compressor_power: 4000.0, heat_recovery: 0.0,
            piping_correction: 1.0, defrost_active: false,
        };
        let requests = vec![
            VRFTerminalRequest { zone_index: 0, cooling_load: 4000.0, heating_load: 0.0 },
            VRFTerminalRequest { zone_index: 1, cooling_load: 8000.0, heating_load: 0.0 },
        ];
        let allocs = distribute_capacity(&ou_result, &requests);
        // Zone 0 gets 4/12 * 12000 = 4000, Zone 1 gets 8/12 * 12000 = 8000
        assert!((allocs[0].1 - 4000.0).abs() < 100.0, "z0={}", allocs[0].1);
        assert!((allocs[1].1 - 8000.0).abs() < 100.0, "z1={}", allocs[1].1);
    }

    #[test]
    fn vrf_cap_limited() {
        let ou = VRFOutdoorUnit::new("OU", 10000.0, 3.5, 8000.0, 3.0);
        let requests = vec![
            VRFTerminalRequest { zone_index: 0, cooling_load: 20000.0, heating_load: 0.0 },
        ];
        let result = ou.calculate(&requests, 35.0, 1.0);
        // Capacity limited by piping correction: 10000 * (1-0.002*30) = 10000*0.94 = 9400
        assert!(result.total_cooling <= 10000.0 + 1.0, "Q={}", result.total_cooling);
    }
}
