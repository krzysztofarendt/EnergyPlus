//! Air loop (AHU) topology and simulation.
//!
//! An air loop represents an air handling unit serving one or more zones.
//! It contains a supply path (fan, coils, heat recovery) and return path,
//! connected to zones through zone supply/return nodes.

/// Air loop operating status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AirLoopStatus {
    #[default]
    Off,
    On,
    Economizer,
}

/// Air loop flow control mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AirLoopFlowControl {
    /// Constant volume: fixed supply air flow.
    #[default]
    ConstantVolume,
    /// Variable volume: modulating supply air flow.
    VariableVolume,
}

/// Supply path component types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplyComponentType {
    Fan,
    CoolingCoil,
    HeatingCoil,
    HeatRecovery,
    Humidifier,
    Dehumidifier,
    OutsideAirMixer,
}

/// A component in the air loop supply path.
#[derive(Debug, Clone)]
pub struct SupplyComponent {
    pub name: String,
    pub component_type: SupplyComponentType,
    pub inlet_node: usize,
    pub outlet_node: usize,
}

/// Zone connection to the air loop.
#[derive(Debug, Clone)]
pub struct ZoneConnection {
    pub zone_name: String,
    pub zone_index: usize,
    pub supply_node: usize,
    pub return_node: usize,
    /// Design supply air flow rate (m3/s).
    pub design_supply_flow: f64,
    /// Current supply air flow rate fraction.
    pub current_flow_fraction: f64,
}

/// Air handling unit / air loop.
#[derive(Debug, Clone)]
pub struct AirLoop {
    pub name: String,
    pub status: AirLoopStatus,
    pub flow_control: AirLoopFlowControl,
    /// Supply path components in order.
    pub supply_components: Vec<SupplyComponent>,
    /// Zones served by this air loop.
    pub zones: Vec<ZoneConnection>,
    /// Supply air inlet node (return from zones).
    pub return_node: usize,
    /// Supply air outlet node (to zones).
    pub supply_node: usize,
    /// Mixed air node (after OA mixer).
    pub mixed_air_node: usize,
    /// Design supply air flow rate (m3/s).
    pub design_supply_flow: f64,
    /// Design supply air temperature (C).
    pub design_supply_temp: f64,
    /// Current supply air temperature setpoint (C).
    pub supply_temp_setpoint: f64,
    /// Number of HVAC iterations for convergence.
    pub max_iterations: usize,
    /// Whether the loop has converged.
    pub converged: bool,
}

impl AirLoop {
    pub fn new(
        name: impl Into<String>,
        return_node: usize,
        supply_node: usize,
        design_flow: f64,
    ) -> Self {
        Self {
            name: name.into(),
            status: AirLoopStatus::Off,
            flow_control: AirLoopFlowControl::ConstantVolume,
            supply_components: Vec::new(),
            zones: Vec::new(),
            return_node,
            supply_node,
            mixed_air_node: 0,
            design_supply_flow: design_flow,
            design_supply_temp: 12.8, // Typical cooling supply temp
            supply_temp_setpoint: 12.8,
            max_iterations: 20,
            converged: false,
        }
    }

    /// Add a supply path component.
    pub fn add_component(
        &mut self,
        name: impl Into<String>,
        component_type: SupplyComponentType,
        inlet: usize,
        outlet: usize,
    ) {
        self.supply_components.push(SupplyComponent {
            name: name.into(),
            component_type,
            inlet_node: inlet,
            outlet_node: outlet,
        });
    }

    /// Add a zone connection.
    pub fn add_zone(
        &mut self,
        zone_name: impl Into<String>,
        zone_index: usize,
        supply_node: usize,
        return_node: usize,
        design_flow: f64,
    ) {
        self.zones.push(ZoneConnection {
            zone_name: zone_name.into(),
            zone_index,
            supply_node,
            return_node,
            design_supply_flow: design_flow,
            current_flow_fraction: 1.0,
        });
    }

    /// Calculate total zone supply flow rate (m3/s).
    pub fn total_zone_flow(&self) -> f64 {
        self.zones.iter().map(|z| z.design_supply_flow * z.current_flow_fraction).sum()
    }

    /// Determine if the air loop should be on based on zone requests.
    pub fn needs_to_run(&self) -> bool {
        self.zones.iter().any(|z| z.current_flow_fraction > 0.0)
    }
}

/// Calculate mixed air temperature from return and outdoor air.
///
/// T_mix = OA_fraction * T_oa + (1 - OA_fraction) * T_return
pub fn mixed_air_temp(
    return_temp: f64,
    outdoor_temp: f64,
    oa_fraction: f64,
) -> f64 {
    let f = oa_fraction.clamp(0.0, 1.0);
    f * outdoor_temp + (1.0 - f) * return_temp
}

/// Calculate mixed air humidity ratio from return and outdoor air.
pub fn mixed_air_humidity_ratio(
    return_w: f64,
    outdoor_w: f64,
    oa_fraction: f64,
) -> f64 {
    let f = oa_fraction.clamp(0.0, 1.0);
    f * outdoor_w + (1.0 - f) * return_w
}

/// Calculate minimum outdoor air fraction from ventilation requirements.
///
/// OA_frac = V_dot_oa_min / V_dot_supply
pub fn min_oa_fraction(min_oa_flow: f64, supply_flow: f64) -> f64 {
    if supply_flow > 1e-10 {
        (min_oa_flow / supply_flow).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_loop_creation() {
        let al = AirLoop::new("AHU-1", 10, 20, 2.0);
        assert_eq!(al.status, AirLoopStatus::Off);
        assert!((al.design_supply_flow - 2.0).abs() < 1e-10);
    }

    #[test]
    fn add_components() {
        let mut al = AirLoop::new("AHU-1", 10, 20, 2.0);
        al.add_component("OA Mixer", SupplyComponentType::OutsideAirMixer, 10, 11);
        al.add_component("Cool Coil", SupplyComponentType::CoolingCoil, 11, 12);
        al.add_component("Supply Fan", SupplyComponentType::Fan, 12, 20);
        assert_eq!(al.supply_components.len(), 3);
    }

    #[test]
    fn add_zones() {
        let mut al = AirLoop::new("AHU-1", 10, 20, 2.0);
        al.add_zone("Zone 1", 0, 30, 31, 0.5);
        al.add_zone("Zone 2", 1, 32, 33, 1.0);
        assert_eq!(al.zones.len(), 2);
        assert!((al.total_zone_flow() - 1.5).abs() < 1e-10);
    }

    #[test]
    fn needs_to_run() {
        let mut al = AirLoop::new("AHU-1", 10, 20, 2.0);
        al.add_zone("Zone 1", 0, 30, 31, 0.5);
        assert!(al.needs_to_run());

        al.zones[0].current_flow_fraction = 0.0;
        assert!(!al.needs_to_run());
    }

    #[test]
    fn mixed_air_temp_calc() {
        // 30% OA at 35C, 70% return at 24C
        let t_mix = mixed_air_temp(24.0, 35.0, 0.3);
        let expected = 0.3 * 35.0 + 0.7 * 24.0; // 27.3
        assert!((t_mix - expected).abs() < 0.01, "T_mix={}", t_mix);
    }

    #[test]
    fn mixed_air_100_pct_oa() {
        let t_mix = mixed_air_temp(24.0, 35.0, 1.0);
        assert!((t_mix - 35.0).abs() < 0.01);
    }

    #[test]
    fn mixed_air_humidity_calc() {
        let w_mix = mixed_air_humidity_ratio(0.009, 0.015, 0.3);
        let expected = 0.3 * 0.015 + 0.7 * 0.009;
        assert!((w_mix - expected).abs() < 1e-6);
    }

    #[test]
    fn min_oa_fraction_calc() {
        let oa_frac = min_oa_fraction(0.5, 2.0);
        assert!((oa_frac - 0.25).abs() < 0.01);
    }

    #[test]
    fn min_oa_fraction_zero_supply() {
        let oa_frac = min_oa_fraction(0.5, 0.0);
        assert!(oa_frac.abs() < 1e-10);
    }
}
