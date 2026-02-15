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

// ---------------------------------------------------------------------------
// Air Loop Simulator
// ---------------------------------------------------------------------------

/// Describes inlet air conditions at the start of the supply path.
#[derive(Debug, Clone, Copy)]
pub struct AirLoopInlet {
    pub temp: f64,
    pub humidity_ratio: f64,
    pub mass_flow_rate: f64,
    pub enthalpy: f64,
}

/// A simplified component model that transforms inlet air to outlet air.
///
/// Each component receives inlet conditions and a control signal (0-1),
/// and returns outlet conditions plus power consumed.
pub trait AirLoopComponent {
    /// Simulate this component and return outlet conditions.
    fn simulate(&self, inlet: &AirLoopInlet, control_signal: f64) -> AirLoopComponentResult;

    /// Return the component type.
    fn component_type(&self) -> SupplyComponentType;

    /// Return the component name.
    fn component_name(&self) -> &str;
}

/// Result of a single component simulation.
#[derive(Debug, Clone, Copy)]
pub struct AirLoopComponentResult {
    pub outlet_temp: f64,
    pub outlet_humidity_ratio: f64,
    pub outlet_enthalpy: f64,
    pub power: f64,
    /// Sensible capacity delivered (W), positive = heating, negative = cooling.
    pub sensible_capacity: f64,
    /// Latent capacity (W), positive = humidification.
    pub latent_capacity: f64,
}

/// Result of an air loop simulation iteration.
#[derive(Debug, Clone)]
pub struct AirLoopSimResult {
    /// Final supply air temperature (C).
    pub supply_temp: f64,
    /// Final supply humidity ratio (kg/kg).
    pub supply_humidity_ratio: f64,
    /// Supply mass flow rate (kg/s).
    pub supply_mass_flow: f64,
    /// Total fan power (W).
    pub fan_power: f64,
    /// Total cooling coil capacity (W).
    pub cooling_capacity: f64,
    /// Total heating coil capacity (W).
    pub heating_capacity: f64,
    /// Number of iterations to converge.
    pub iterations: usize,
    /// Whether the loop converged.
    pub converged: bool,
    /// Per-component results.
    pub component_results: Vec<AirLoopComponentResult>,
}

/// Simulates an air loop (AHU) by running components in sequence until convergence.
///
/// The simulator iterates through the supply path components in order,
/// passing outlet conditions of each component as inlet to the next.
/// It repeats until the supply air temperature stabilizes.
pub struct AirLoopSimulator {
    /// Convergence tolerance for supply air temperature (C).
    pub tolerance: f64,
    /// Maximum number of iterations.
    pub max_iterations: usize,
}

impl Default for AirLoopSimulator {
    fn default() -> Self {
        Self {
            tolerance: 0.01,
            max_iterations: 4,
        }
    }
}

impl AirLoopSimulator {
    pub fn new(tolerance: f64, max_iterations: usize) -> Self {
        Self { tolerance, max_iterations }
    }

    /// Simulate the air loop with given components and control signals.
    ///
    /// `inlet` — conditions at the start of the supply path (after OA mixing).
    /// `components` — ordered supply path components.
    /// `control_signals` — control signal (0-1) for each component.
    pub fn simulate(
        &self,
        inlet: &AirLoopInlet,
        components: &[Box<dyn AirLoopComponent>],
        control_signals: &[f64],
    ) -> AirLoopSimResult {
        let mut prev_supply_temp = inlet.temp;
        let mut final_result = None;

        for iteration in 0..self.max_iterations {
            let mut current = *inlet;
            let mut component_results = Vec::with_capacity(components.len());
            let mut fan_power = 0.0;
            let mut cooling_capacity = 0.0;
            let mut heating_capacity = 0.0;

            for (i, component) in components.iter().enumerate() {
                let signal = control_signals.get(i).copied().unwrap_or(1.0);
                let result = component.simulate(
                    &AirLoopInlet {
                        temp: current.temp,
                        humidity_ratio: current.humidity_ratio,
                        mass_flow_rate: current.mass_flow_rate,
                        enthalpy: current.enthalpy,
                    },
                    signal,
                );

                // Track totals by component type
                match component.component_type() {
                    SupplyComponentType::Fan => fan_power += result.power,
                    SupplyComponentType::CoolingCoil => cooling_capacity += result.sensible_capacity.min(0.0).abs(),
                    SupplyComponentType::HeatingCoil => heating_capacity += result.sensible_capacity.max(0.0),
                    _ => {}
                }

                // Update current conditions for next component
                current.temp = result.outlet_temp;
                current.humidity_ratio = result.outlet_humidity_ratio;
                current.enthalpy = result.outlet_enthalpy;

                component_results.push(result);
            }

            let converged = (current.temp - prev_supply_temp).abs() < self.tolerance;

            final_result = Some(AirLoopSimResult {
                supply_temp: current.temp,
                supply_humidity_ratio: current.humidity_ratio,
                supply_mass_flow: current.mass_flow_rate,
                fan_power,
                cooling_capacity,
                heating_capacity,
                iterations: iteration + 1,
                converged,
                component_results,
            });

            if converged {
                break;
            }
            prev_supply_temp = current.temp;
        }

        final_result.unwrap()
    }
}

// ---------------------------------------------------------------------------
// Concrete AirLoopComponent implementations
// ---------------------------------------------------------------------------

/// Fan component for air loop simulation.
pub struct FanComponent {
    pub name: String,
    pub design_power: f64,
    pub motor_efficiency: f64,
    pub motor_in_airstream_fraction: f64,
}

impl FanComponent {
    pub fn new(name: impl Into<String>, design_power: f64, motor_eff: f64) -> Self {
        Self {
            name: name.into(),
            design_power,
            motor_efficiency: motor_eff,
            motor_in_airstream_fraction: 1.0,
        }
    }
}

impl AirLoopComponent for FanComponent {
    fn simulate(&self, inlet: &AirLoopInlet, control_signal: f64) -> AirLoopComponentResult {
        let power = self.design_power * control_signal;
        let shaft_power = power * self.motor_efficiency;
        let motor_loss = power - shaft_power;
        let heat_to_air = shaft_power + motor_loss * self.motor_in_airstream_fraction;

        let cp = ep_psychrometrics::cp_air(inlet.humidity_ratio);
        let dt = if inlet.mass_flow_rate > 1e-10 {
            heat_to_air / (inlet.mass_flow_rate * cp)
        } else {
            0.0
        };

        let outlet_temp = inlet.temp + dt;
        let outlet_enthalpy = ep_psychrometrics::enthalpy(outlet_temp, inlet.humidity_ratio);

        AirLoopComponentResult {
            outlet_temp,
            outlet_humidity_ratio: inlet.humidity_ratio,
            outlet_enthalpy,
            power,
            sensible_capacity: heat_to_air,
            latent_capacity: 0.0,
        }
    }

    fn component_type(&self) -> SupplyComponentType { SupplyComponentType::Fan }
    fn component_name(&self) -> &str { &self.name }
}

/// Simple cooling coil component for air loop simulation.
///
/// Cools air towards a target temperature based on control signal.
pub struct CoolingCoilComponent {
    pub name: String,
    /// Maximum sensible cooling capacity (W).
    pub max_capacity: f64,
    /// Rated COP.
    pub cop: f64,
}

impl CoolingCoilComponent {
    pub fn new(name: impl Into<String>, max_capacity: f64, cop: f64) -> Self {
        Self { name: name.into(), max_capacity, cop }
    }
}

impl AirLoopComponent for CoolingCoilComponent {
    fn simulate(&self, inlet: &AirLoopInlet, control_signal: f64) -> AirLoopComponentResult {
        let capacity = self.max_capacity * control_signal.clamp(0.0, 1.0);
        if capacity <= 0.0 || inlet.mass_flow_rate <= 1e-10 {
            return AirLoopComponentResult {
                outlet_temp: inlet.temp,
                outlet_humidity_ratio: inlet.humidity_ratio,
                outlet_enthalpy: inlet.enthalpy,
                power: 0.0,
                sensible_capacity: 0.0,
                latent_capacity: 0.0,
            };
        }

        let cp = ep_psychrometrics::cp_air(inlet.humidity_ratio);
        let dt = capacity / (inlet.mass_flow_rate * cp);
        let outlet_temp = inlet.temp - dt;
        let outlet_enthalpy = ep_psychrometrics::enthalpy(outlet_temp, inlet.humidity_ratio);
        let power = if self.cop > 0.0 { capacity / self.cop } else { 0.0 };

        AirLoopComponentResult {
            outlet_temp,
            outlet_humidity_ratio: inlet.humidity_ratio,
            outlet_enthalpy,
            power,
            sensible_capacity: -capacity, // Negative = cooling
            latent_capacity: 0.0,
        }
    }

    fn component_type(&self) -> SupplyComponentType { SupplyComponentType::CoolingCoil }
    fn component_name(&self) -> &str { &self.name }
}

/// Simple heating coil component for air loop simulation.
pub struct HeatingCoilComponent {
    pub name: String,
    /// Maximum heating capacity (W).
    pub max_capacity: f64,
    /// Efficiency (1.0 for electric, ~0.8 for gas).
    pub efficiency: f64,
}

impl HeatingCoilComponent {
    pub fn new(name: impl Into<String>, max_capacity: f64, efficiency: f64) -> Self {
        Self { name: name.into(), max_capacity, efficiency }
    }
}

impl AirLoopComponent for HeatingCoilComponent {
    fn simulate(&self, inlet: &AirLoopInlet, control_signal: f64) -> AirLoopComponentResult {
        let capacity = self.max_capacity * control_signal.clamp(0.0, 1.0);
        if capacity <= 0.0 || inlet.mass_flow_rate <= 1e-10 {
            return AirLoopComponentResult {
                outlet_temp: inlet.temp,
                outlet_humidity_ratio: inlet.humidity_ratio,
                outlet_enthalpy: inlet.enthalpy,
                power: 0.0,
                sensible_capacity: 0.0,
                latent_capacity: 0.0,
            };
        }

        let cp = ep_psychrometrics::cp_air(inlet.humidity_ratio);
        let dt = capacity / (inlet.mass_flow_rate * cp);
        let outlet_temp = inlet.temp + dt;
        let outlet_enthalpy = ep_psychrometrics::enthalpy(outlet_temp, inlet.humidity_ratio);
        let power = if self.efficiency > 0.0 { capacity / self.efficiency } else { 0.0 };

        AirLoopComponentResult {
            outlet_temp,
            outlet_humidity_ratio: inlet.humidity_ratio,
            outlet_enthalpy,
            power,
            sensible_capacity: capacity, // Positive = heating
            latent_capacity: 0.0,
        }
    }

    fn component_type(&self) -> SupplyComponentType { SupplyComponentType::HeatingCoil }
    fn component_name(&self) -> &str { &self.name }
}

/// Calculates the required control signal for a cooling coil to achieve
/// a target supply air temperature.
///
/// Returns a value 0-1 representing the fraction of max capacity needed.
pub fn cooling_coil_control_signal(
    inlet_temp: f64,
    target_temp: f64,
    max_capacity: f64,
    mass_flow: f64,
    humidity_ratio: f64,
) -> f64 {
    if inlet_temp <= target_temp || mass_flow <= 1e-10 || max_capacity <= 0.0 {
        return 0.0;
    }
    let cp = ep_psychrometrics::cp_air(humidity_ratio);
    let needed = mass_flow * cp * (inlet_temp - target_temp);
    (needed / max_capacity).clamp(0.0, 1.0)
}

/// Calculates the required control signal for a heating coil to achieve
/// a target supply air temperature.
pub fn heating_coil_control_signal(
    inlet_temp: f64,
    target_temp: f64,
    max_capacity: f64,
    mass_flow: f64,
    humidity_ratio: f64,
) -> f64 {
    if inlet_temp >= target_temp || mass_flow <= 1e-10 || max_capacity <= 0.0 {
        return 0.0;
    }
    let cp = ep_psychrometrics::cp_air(humidity_ratio);
    let needed = mass_flow * cp * (target_temp - inlet_temp);
    (needed / max_capacity).clamp(0.0, 1.0)
}

/// Simulates a complete single-zone AHU cycle:
/// OA mixing → heating coil → cooling coil → fan → supply to zone.
///
/// Determines control signals automatically to meet the supply air temperature setpoint.
pub fn simulate_single_zone_ahu(
    return_temp: f64,
    return_w: f64,
    outdoor_temp: f64,
    outdoor_w: f64,
    oa_fraction: f64,
    supply_mass_flow: f64,
    supply_temp_setpoint: f64,
    heating_capacity: f64,
    heating_efficiency: f64,
    cooling_capacity: f64,
    cooling_cop: f64,
    fan_power: f64,
    fan_motor_eff: f64,
) -> AirLoopSimResult {
    // Step 1: OA mixing
    let mixed_temp = mixed_air_temp(return_temp, outdoor_temp, oa_fraction);
    let mixed_w = mixed_air_humidity_ratio(return_w, outdoor_w, oa_fraction);
    let mixed_h = ep_psychrometrics::enthalpy(mixed_temp, mixed_w);

    // Step 2: Estimate fan heat contribution
    let shaft_power = fan_power * fan_motor_eff;
    let motor_loss = fan_power - shaft_power;
    let fan_heat = shaft_power + motor_loss; // motor_in_airstream = 1.0
    let cp_mixed = ep_psychrometrics::cp_air(mixed_w);
    let fan_dt = if supply_mass_flow > 1e-10 { fan_heat / (supply_mass_flow * cp_mixed) } else { 0.0 };

    // Step 3: Calculate control signals to hit setpoint (accounting for fan heat)
    // For blow-through: fan → heating → cooling
    let after_fan_temp = mixed_temp + fan_dt;

    let heat_signal = heating_coil_control_signal(
        after_fan_temp, supply_temp_setpoint, heating_capacity, supply_mass_flow, mixed_w,
    );
    let after_heat_temp = after_fan_temp + heat_signal * heating_capacity / (supply_mass_flow * cp_mixed).max(1e-10);

    let cool_signal = cooling_coil_control_signal(
        after_heat_temp, supply_temp_setpoint, cooling_capacity, supply_mass_flow, mixed_w,
    );

    // Step 4: Run the simulator
    let inlet = AirLoopInlet {
        temp: mixed_temp,
        humidity_ratio: mixed_w,
        mass_flow_rate: supply_mass_flow,
        enthalpy: mixed_h,
    };

    let components: Vec<Box<dyn AirLoopComponent>> = vec![
        Box::new(FanComponent::new("Fan", fan_power, fan_motor_eff)),
        Box::new(HeatingCoilComponent::new("Htg Coil", heating_capacity, heating_efficiency)),
        Box::new(CoolingCoilComponent::new("Clg Coil", cooling_capacity, cooling_cop)),
    ];

    let signals = vec![1.0, heat_signal, cool_signal];

    let simulator = AirLoopSimulator::default();
    simulator.simulate(&inlet, &components, &signals)
}

// ---------------------------------------------------------------------------
// Water coil controller (bisection method)
// ---------------------------------------------------------------------------

/// Result of a water coil controller search.
#[derive(Debug, Clone, Copy)]
pub struct WaterCoilControlResult {
    /// Water mass flow rate that achieves target (kg/s).
    pub water_flow: f64,
    /// Achieved outlet air temperature (C).
    pub outlet_temp: f64,
    /// Number of iterations.
    pub iterations: usize,
    /// Whether the search converged.
    pub converged: bool,
}

/// Find the water mass flow rate for a water coil to achieve
/// a target leaving air temperature using bisection.
///
/// `coil_calc` is a closure that takes water_mass_flow and returns air_outlet_temp.
pub fn bisect_water_coil_flow<F>(
    target_temp: f64,
    min_flow: f64,
    max_flow: f64,
    tolerance: f64,
    max_iter: usize,
    coil_calc: F,
) -> WaterCoilControlResult
where
    F: Fn(f64) -> f64,
{
    let t_at_min = coil_calc(min_flow);
    let t_at_max = coil_calc(max_flow);

    // Check if target is achievable
    let (t_lo, t_hi) = if t_at_min < t_at_max {
        (t_at_min, t_at_max)
    } else {
        (t_at_max, t_at_min)
    };

    if target_temp < t_lo - tolerance || target_temp > t_hi + tolerance {
        // Target not achievable — return closest extreme
        let (flow, temp) = if (target_temp - t_at_min).abs() < (target_temp - t_at_max).abs() {
            (min_flow, t_at_min)
        } else {
            (max_flow, t_at_max)
        };
        return WaterCoilControlResult {
            water_flow: flow,
            outlet_temp: temp,
            iterations: 0,
            converged: false,
        };
    }

    let mut lo = min_flow;
    let mut hi = max_flow;
    let mut best_flow = (lo + hi) / 2.0;
    let mut best_temp = coil_calc(best_flow);

    for iter in 0..max_iter {
        best_flow = (lo + hi) / 2.0;
        best_temp = coil_calc(best_flow);

        if (best_temp - target_temp).abs() < tolerance {
            return WaterCoilControlResult {
                water_flow: best_flow,
                outlet_temp: best_temp,
                iterations: iter + 1,
                converged: true,
            };
        }

        // Determine which half contains the target
        let t_lo_val = coil_calc(lo);
        if (target_temp > t_lo_val) == (target_temp > best_temp) {
            lo = best_flow;
        } else {
            hi = best_flow;
        }
    }

    WaterCoilControlResult {
        water_flow: best_flow,
        outlet_temp: best_temp,
        iterations: max_iter,
        converged: (best_temp - target_temp).abs() < tolerance,
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

    // --- Air Loop Simulator Tests ---

    #[test]
    fn simulator_fan_only() {
        let inlet = AirLoopInlet {
            temp: 20.0,
            humidity_ratio: 0.008,
            mass_flow_rate: 1.2,
            enthalpy: ep_psychrometrics::enthalpy(20.0, 0.008),
        };
        let components: Vec<Box<dyn AirLoopComponent>> = vec![
            Box::new(FanComponent::new("Fan", 857.0, 0.9)),
        ];
        let sim = AirLoopSimulator::default();
        let result = sim.simulate(&inlet, &components, &[1.0]);

        assert!(result.supply_temp > 20.0, "T_sup={}", result.supply_temp);
        assert!(result.fan_power > 800.0, "P_fan={}", result.fan_power);
        assert!(result.converged);
        assert_eq!(result.iterations, 2); // 2 passes: first computes, second confirms stability
    }

    #[test]
    fn simulator_cooling_coil_reduces_temp() {
        let inlet = AirLoopInlet {
            temp: 27.0,
            humidity_ratio: 0.010,
            mass_flow_rate: 1.0,
            enthalpy: ep_psychrometrics::enthalpy(27.0, 0.010),
        };
        let components: Vec<Box<dyn AirLoopComponent>> = vec![
            Box::new(CoolingCoilComponent::new("CC", 15000.0, 3.5)),
        ];
        let sim = AirLoopSimulator::default();
        let result = sim.simulate(&inlet, &components, &[1.0]);

        assert!(result.supply_temp < 27.0, "T_sup={}", result.supply_temp);
        assert!(result.cooling_capacity > 0.0);
    }

    #[test]
    fn simulator_heating_coil_raises_temp() {
        let inlet = AirLoopInlet {
            temp: 5.0,
            humidity_ratio: 0.003,
            mass_flow_rate: 1.0,
            enthalpy: ep_psychrometrics::enthalpy(5.0, 0.003),
        };
        let components: Vec<Box<dyn AirLoopComponent>> = vec![
            Box::new(HeatingCoilComponent::new("HC", 20000.0, 0.80)),
        ];
        let sim = AirLoopSimulator::default();
        let result = sim.simulate(&inlet, &components, &[1.0]);

        assert!(result.supply_temp > 5.0, "T_sup={}", result.supply_temp);
        assert!(result.heating_capacity > 0.0);
    }

    #[test]
    fn simulator_multi_component_sequence() {
        // Fan → Cooling Coil: fan heats air, then coil cools it
        let inlet = AirLoopInlet {
            temp: 25.0,
            humidity_ratio: 0.009,
            mass_flow_rate: 1.5,
            enthalpy: ep_psychrometrics::enthalpy(25.0, 0.009),
        };
        let components: Vec<Box<dyn AirLoopComponent>> = vec![
            Box::new(FanComponent::new("Fan", 900.0, 0.9)),
            Box::new(CoolingCoilComponent::new("CC", 20000.0, 3.0)),
        ];
        let sim = AirLoopSimulator::default();
        let result = sim.simulate(&inlet, &components, &[1.0, 0.5]);

        // Fan heats then coil cools. Net effect depends on magnitudes.
        assert!(result.fan_power > 0.0);
        assert!(result.cooling_capacity > 0.0);
        assert_eq!(result.component_results.len(), 2);
    }

    #[test]
    fn simulator_convergence_immediate() {
        // Simple system that converges in 1 iteration (no feedback)
        let inlet = AirLoopInlet {
            temp: 20.0,
            humidity_ratio: 0.008,
            mass_flow_rate: 1.0,
            enthalpy: ep_psychrometrics::enthalpy(20.0, 0.008),
        };
        let components: Vec<Box<dyn AirLoopComponent>> = vec![
            Box::new(HeatingCoilComponent::new("HC", 10000.0, 1.0)),
        ];
        let sim = AirLoopSimulator::new(0.01, 4);
        let result = sim.simulate(&inlet, &components, &[0.5]);

        assert!(result.converged);
        assert_eq!(result.iterations, 2); // 2 passes: first computes, second confirms stability
    }

    #[test]
    fn simulator_zero_flow() {
        let inlet = AirLoopInlet {
            temp: 20.0,
            humidity_ratio: 0.008,
            mass_flow_rate: 0.0,
            enthalpy: ep_psychrometrics::enthalpy(20.0, 0.008),
        };
        let components: Vec<Box<dyn AirLoopComponent>> = vec![
            Box::new(FanComponent::new("Fan", 857.0, 0.9)),
            Box::new(CoolingCoilComponent::new("CC", 15000.0, 3.0)),
        ];
        let sim = AirLoopSimulator::default();
        let result = sim.simulate(&inlet, &components, &[1.0, 1.0]);

        // No flow: temperature unchanged
        assert!((result.supply_temp - 20.0).abs() < 0.1, "T={}", result.supply_temp);
    }

    #[test]
    fn cooling_control_signal_calc() {
        // inlet 30C, target 13C, capacity 20kW, 1.0 kg/s
        let signal = cooling_coil_control_signal(30.0, 13.0, 20000.0, 1.0, 0.009);
        // Need = 1.0 * 1006 * 17 ≈ 17102 W, capacity 20000 → signal ≈ 0.855
        assert!(signal > 0.8 && signal < 0.9, "signal={}", signal);
    }

    #[test]
    fn cooling_control_signal_no_cooling_needed() {
        let signal = cooling_coil_control_signal(10.0, 13.0, 20000.0, 1.0, 0.009);
        assert!(signal.abs() < 1e-10);
    }

    #[test]
    fn heating_control_signal_calc() {
        // inlet 5C, target 20C, capacity 20kW, 1.0 kg/s
        let signal = heating_coil_control_signal(5.0, 20.0, 20000.0, 1.0, 0.005);
        // Need = 1.0 * 1006 * 15 ≈ 15090 W, capacity 20000 → signal ≈ 0.755
        assert!(signal > 0.7 && signal < 0.8, "signal={}", signal);
    }

    #[test]
    fn heating_control_signal_no_heating_needed() {
        let signal = heating_coil_control_signal(25.0, 13.0, 20000.0, 1.0, 0.009);
        assert!(signal.abs() < 1e-10);
    }

    #[test]
    fn single_zone_ahu_cooling() {
        let result = simulate_single_zone_ahu(
            24.0,   // return temp
            0.009,  // return W
            35.0,   // outdoor temp
            0.015,  // outdoor W
            0.3,    // OA fraction
            1.2,    // supply mass flow
            13.0,   // supply setpoint
            15000.0, 1.0,   // heating
            25000.0, 3.5,   // cooling
            857.0, 0.9,     // fan
        );
        // Should cool to near setpoint
        assert!(result.supply_temp < 15.0, "T_sup={}", result.supply_temp);
        assert!(result.cooling_capacity > 0.0, "Q_cool={}", result.cooling_capacity);
        assert!(result.fan_power > 0.0);
    }

    #[test]
    fn single_zone_ahu_heating() {
        let result = simulate_single_zone_ahu(
            20.0,   // return temp
            0.005,  // return W
            -5.0,   // outdoor temp (cold)
            0.002,  // outdoor W
            0.3,    // OA fraction
            1.0,    // supply mass flow
            35.0,   // supply setpoint (high for heating)
            30000.0, 0.80,  // heating
            20000.0, 3.5,   // cooling
            600.0, 0.9,     // fan
        );
        // Should heat
        assert!(result.supply_temp > 20.0, "T_sup={}", result.supply_temp);
        assert!(result.heating_capacity > 0.0);
    }

    // --- Water Coil Controller (Bisection) Tests ---

    #[test]
    fn bisect_water_coil_heating() {
        // Simple linear model: T_out = T_in + flow * 100 (for test purposes)
        let result = bisect_water_coil_flow(
            30.0,   // target outlet temp
            0.0,    // min flow
            1.0,    // max flow
            0.1,    // tolerance
            30,     // max iterations
            |flow| 20.0 + flow * 20.0, // linear model: at flow=0.5 → T=30
        );
        assert!(result.converged, "converged={}", result.converged);
        assert!((result.water_flow - 0.5).abs() < 0.01, "flow={}", result.water_flow);
        assert!((result.outlet_temp - 30.0).abs() < 0.1, "T_out={}", result.outlet_temp);
    }

    #[test]
    fn bisect_water_coil_cooling() {
        // Cooling coil: more flow = colder air (inverse)
        let result = bisect_water_coil_flow(
            15.0,   // target
            0.0, 1.0, 0.1, 30,
            |flow| 25.0 - flow * 20.0, // flow=0→25C, flow=1→5C
        );
        assert!(result.converged);
        assert!((result.outlet_temp - 15.0).abs() < 0.1, "T={}", result.outlet_temp);
    }

    #[test]
    fn bisect_target_unreachable() {
        let result = bisect_water_coil_flow(
            50.0,   // target way too high
            0.0, 1.0, 0.1, 30,
            |flow| 20.0 + flow * 10.0, // max possible = 30
        );
        assert!(!result.converged);
    }
}
