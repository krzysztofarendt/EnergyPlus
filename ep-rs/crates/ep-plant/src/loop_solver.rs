//! Plant loop solver — iterates between supply and demand half-loops
//! to converge flow rates and temperatures.
//!
//! The solver follows the EnergyPlus half-loop iteration scheme:
//! 1. Demand side: coils request flow based on load
//! 2. Supply side: pump delivers flow, equipment meets demand
//! 3. Convergence check: supply outlet temp and flow stable
//! 4. FlowLockState progression: PumpQuery → Unlocked → Locked

use crate::loop_topology::{FlowLockState, PlantLoop};

/// Trait for plant loop components.
///
/// Each component reads its inlet conditions, computes performance,
/// and produces outlet conditions. The solver calls `simulate()` for
/// each component in sequence.
pub trait PlantComponent {
    /// Component name.
    fn name(&self) -> &str;

    /// Component type string (e.g., "Boiler:HotWater").
    fn component_type(&self) -> &str;

    /// Inlet node index.
    fn inlet_node(&self) -> usize;

    /// Outlet node index.
    fn outlet_node(&self) -> usize;

    /// Simulate this component at current conditions.
    ///
    /// `inlet_temp`: water temperature entering the component (C)
    /// `mass_flow`: water mass flow rate (kg/s)
    /// `load`: heating/cooling load assigned (W, positive = heating for boilers,
    ///         positive = cooling for chillers)
    /// `flow_lock`: current flow lock state
    ///
    /// Returns the component result.
    fn simulate(
        &mut self,
        inlet_temp: f64,
        mass_flow: f64,
        load: f64,
        flow_lock: FlowLockState,
    ) -> PlantComponentResult;
}

/// Result from a plant component simulation.
#[derive(Debug, Clone, Copy)]
pub struct PlantComponentResult {
    /// Outlet water temperature (C).
    pub outlet_temp: f64,
    /// Mass flow rate through the component (kg/s).
    pub mass_flow: f64,
    /// Capacity delivered (W, positive = heating, negative = cooling).
    pub capacity: f64,
    /// Power consumption (W).
    pub power: f64,
    /// Requested flow rate (kg/s) — may differ from delivered in PumpQuery.
    pub requested_flow: f64,
}

/// Plant loop solver configuration and state.
#[derive(Debug, Clone)]
pub struct PlantLoopSolver {
    /// Maximum number of half-loop iterations.
    pub max_iterations: usize,
    /// Temperature convergence tolerance (C).
    pub temp_tolerance: f64,
    /// Flow convergence tolerance (kg/s).
    pub flow_tolerance: f64,
}

/// Result of a plant loop solve iteration.
#[derive(Debug, Clone, Copy)]
pub struct PlantConvergenceResult {
    /// Number of iterations performed.
    pub iterations: usize,
    /// Whether temperature converged.
    pub temp_converged: bool,
    /// Whether flow converged.
    pub flow_converged: bool,
    /// Final supply outlet temperature (C).
    pub supply_outlet_temp: f64,
    /// Final demand outlet temperature (C).
    pub demand_outlet_temp: f64,
    /// Final loop flow rate (kg/s).
    pub loop_flow_rate: f64,
}

impl Default for PlantLoopSolver {
    fn default() -> Self {
        Self {
            max_iterations: 8,
            temp_tolerance: 0.1,
            flow_tolerance: 0.001,
        }
    }
}

impl PlantLoopSolver {
    pub fn new(max_iterations: usize, temp_tolerance: f64, flow_tolerance: f64) -> Self {
        Self {
            max_iterations,
            temp_tolerance,
            flow_tolerance,
        }
    }

    /// Simulate one side of the plant loop.
    ///
    /// Runs each component in sequence, passing outlet of one as inlet of next.
    /// Returns the final outlet temperature and total flow request.
    fn simulate_half_loop(
        components: &mut [Box<dyn PlantComponent>],
        inlet_temp: f64,
        mass_flow: f64,
        load: f64,
        flow_lock: FlowLockState,
    ) -> HalfLoopResult {
        if components.is_empty() {
            return HalfLoopResult {
                outlet_temp: inlet_temp,
                total_flow_request: mass_flow,
                total_capacity: 0.0,
                total_power: 0.0,
            };
        }

        let mut current_temp = inlet_temp;
        let mut total_capacity = 0.0;
        let mut total_power = 0.0;
        let mut max_flow_request = 0.0_f64;

        // Distribute load equally among active components (simple approach)
        let active_count = components.len().max(1) as f64;
        let load_per_component = load / active_count;

        for component in components.iter_mut() {
            let result = component.simulate(current_temp, mass_flow, load_per_component, flow_lock);
            current_temp = result.outlet_temp;
            total_capacity += result.capacity;
            total_power += result.power;
            max_flow_request = max_flow_request.max(result.requested_flow);
        }

        HalfLoopResult {
            outlet_temp: current_temp,
            total_flow_request: if max_flow_request > 1e-10 {
                max_flow_request
            } else {
                mass_flow
            },
            total_capacity,
            total_power,
        }
    }

    /// Simulate a complete plant loop to convergence.
    ///
    /// Iterates between demand and supply sides until temperatures and
    /// flows stabilize or max iterations reached.
    ///
    /// `plant_loop`: the loop topology with setpoint and flow limits
    /// `demand_components`: components on the demand side (e.g., coils)
    /// `supply_components`: components on the supply side (e.g., pump, chiller)
    /// `demand_load`: total load on demand side (W)
    /// `supply_load`: total load for supply equipment to meet (W)
    pub fn simulate(
        &self,
        plant_loop: &mut PlantLoop,
        demand_components: &mut [Box<dyn PlantComponent>],
        supply_components: &mut [Box<dyn PlantComponent>],
        demand_load: f64,
        supply_load: f64,
    ) -> PlantConvergenceResult {
        let mut prev_supply_outlet = plant_loop.loop_temp;
        let mut prev_flow = plant_loop.design_flow_rate;
        let mut supply_outlet_temp = plant_loop.loop_temp;
        let mut demand_outlet_temp = plant_loop.loop_temp;
        let mut loop_flow = plant_loop.design_flow_rate;

        // Supply inlet temp = demand outlet (updated each iteration)

        let mut converged_iter = 0;

        for iter in 0..self.max_iterations {
            converged_iter = iter + 1;

            // 1. Demand side: coils process load, request flow
            let flow_lock = if iter == 0 {
                FlowLockState::Unlocked
            } else {
                FlowLockState::Locked
            };

            let demand_result = Self::simulate_half_loop(
                demand_components,
                supply_outlet_temp, // demand inlet = supply outlet
                loop_flow,
                demand_load,
                flow_lock,
            );
            demand_outlet_temp = demand_result.outlet_temp;

            // Update flow from demand requests (if unlocked)
            if flow_lock == FlowLockState::Unlocked {
                loop_flow = demand_result
                    .total_flow_request
                    .clamp(plant_loop.min_flow_rate, plant_loop.max_flow_rate);
            }

            // 2. Supply side: equipment meets demand
            let supply_result = Self::simulate_half_loop(
                supply_components,
                demand_outlet_temp, // supply inlet = demand outlet
                loop_flow,
                supply_load,
                flow_lock,
            );
            supply_outlet_temp = supply_result.outlet_temp;

            // 3. Convergence check
            let temp_delta = (supply_outlet_temp - prev_supply_outlet).abs();
            let flow_delta = (loop_flow - prev_flow).abs();

            let temp_ok = temp_delta < self.temp_tolerance;
            let flow_ok = flow_delta < self.flow_tolerance;

            prev_supply_outlet = supply_outlet_temp;
            prev_flow = loop_flow;

            if temp_ok && flow_ok && iter > 0 {
                plant_loop.temp_converged = true;
                plant_loop.flow_converged = true;
                break;
            }
        }

        // Update loop state
        plant_loop.loop_temp = supply_outlet_temp;

        PlantConvergenceResult {
            iterations: converged_iter,
            temp_converged: plant_loop.temp_converged,
            flow_converged: plant_loop.flow_converged,
            supply_outlet_temp,
            demand_outlet_temp,
            loop_flow_rate: loop_flow,
        }
    }
}

/// Internal result from simulating one half-loop.
#[derive(Debug)]
#[allow(dead_code)]
struct HalfLoopResult {
    outlet_temp: f64,
    total_flow_request: f64,
    total_capacity: f64,
    total_power: f64,
}

/// Equipment operation scheme for load dispatch.
#[derive(Debug, Clone)]
pub struct EquipmentOperation {
    /// Equipment entries in dispatch priority order.
    pub equipment: Vec<EquipmentEntry>,
}

/// An entry in the equipment dispatch list.
#[derive(Debug, Clone)]
pub struct EquipmentEntry {
    /// Equipment name.
    pub name: String,
    /// Component index (into the component list).
    pub component_index: usize,
    /// Lower load range (W) — equipment is available above this.
    pub lower_limit: f64,
    /// Upper load range (W) — equipment is available below this.
    pub upper_limit: f64,
}

impl EquipmentOperation {
    pub fn new() -> Self {
        Self {
            equipment: Vec::new(),
        }
    }

    /// Add equipment to the dispatch list.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        component_index: usize,
        lower_limit: f64,
        upper_limit: f64,
    ) {
        self.equipment.push(EquipmentEntry {
            name: name.into(),
            component_index,
            lower_limit,
            upper_limit,
        });
    }

    /// Dispatch load to equipment based on load range.
    ///
    /// Returns a vector of (component_index, assigned_load) pairs.
    pub fn dispatch(&self, total_load: f64) -> Vec<(usize, f64)> {
        let mut remaining = total_load;
        let mut assignments = Vec::new();

        for entry in &self.equipment {
            if remaining <= 0.0 {
                break;
            }

            if total_load >= entry.lower_limit && total_load <= entry.upper_limit {
                let assigned = remaining.min(entry.upper_limit - entry.lower_limit);
                assignments.push((entry.component_index, assigned));
                remaining -= assigned;
            }
        }

        // If nothing was dispatched by range, assign to first equipment
        if assignments.is_empty() && !self.equipment.is_empty() && total_load > 0.0 {
            assignments.push((self.equipment[0].component_index, total_load));
        }

        assignments
    }
}

/// Mix outlet temperatures from multiple branches (mass-weighted average).
pub fn mix_branch_outlets(branch_temps: &[f64], branch_flows: &[f64]) -> f64 {
    let total_flow: f64 = branch_flows.iter().sum();
    if total_flow <= 1e-10 || branch_temps.is_empty() {
        return if branch_temps.is_empty() {
            20.0
        } else {
            branch_temps[0]
        };
    }

    let weighted_sum: f64 = branch_temps
        .iter()
        .zip(branch_flows.iter())
        .map(|(&t, &f)| t * f)
        .sum();

    weighted_sum / total_flow
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Mock plant components for testing ───

    /// Simple heater: raises water temp by delivering heat.
    struct MockHeater {
        name: String,
        capacity: f64, // W
        inlet: usize,
        outlet: usize,
    }

    impl MockHeater {
        fn new(name: &str, capacity: f64, inlet: usize, outlet: usize) -> Self {
            Self {
                name: name.to_string(),
                capacity,
                inlet,
                outlet,
            }
        }
    }

    impl PlantComponent for MockHeater {
        fn name(&self) -> &str {
            &self.name
        }
        fn component_type(&self) -> &str {
            "MockHeater"
        }
        fn inlet_node(&self) -> usize {
            self.inlet
        }
        fn outlet_node(&self) -> usize {
            self.outlet
        }
        fn simulate(
            &mut self,
            inlet_temp: f64,
            mass_flow: f64,
            load: f64,
            _flow_lock: FlowLockState,
        ) -> PlantComponentResult {
            if mass_flow <= 1e-10 || load <= 0.0 {
                return PlantComponentResult {
                    outlet_temp: inlet_temp,
                    mass_flow,
                    capacity: 0.0,
                    power: 0.0,
                    requested_flow: mass_flow,
                };
            }
            let actual_load = load.min(self.capacity);
            let cp = ep_psychrometrics::cp_water(inlet_temp);
            let dt = actual_load / (mass_flow * cp);
            PlantComponentResult {
                outlet_temp: inlet_temp + dt,
                mass_flow,
                capacity: actual_load,
                power: actual_load * 0.1, // 10% parasitic
                requested_flow: mass_flow,
            }
        }
    }

    /// Simple cooler: lowers water temp (like a coil extracting heat).
    struct MockCooler {
        name: String,
        capacity: f64,
        inlet: usize,
        outlet: usize,
    }

    impl MockCooler {
        fn new(name: &str, capacity: f64, inlet: usize, outlet: usize) -> Self {
            Self {
                name: name.to_string(),
                capacity,
                inlet,
                outlet,
            }
        }
    }

    impl PlantComponent for MockCooler {
        fn name(&self) -> &str {
            &self.name
        }
        fn component_type(&self) -> &str {
            "MockCooler"
        }
        fn inlet_node(&self) -> usize {
            self.inlet
        }
        fn outlet_node(&self) -> usize {
            self.outlet
        }
        fn simulate(
            &mut self,
            inlet_temp: f64,
            mass_flow: f64,
            load: f64,
            _flow_lock: FlowLockState,
        ) -> PlantComponentResult {
            if mass_flow <= 1e-10 || load <= 0.0 {
                return PlantComponentResult {
                    outlet_temp: inlet_temp,
                    mass_flow,
                    capacity: 0.0,
                    power: 0.0,
                    requested_flow: mass_flow,
                };
            }
            let actual_load = load.min(self.capacity);
            let cp = ep_psychrometrics::cp_water(inlet_temp);
            let dt = actual_load / (mass_flow * cp);
            PlantComponentResult {
                outlet_temp: inlet_temp - dt, // cooling
                mass_flow,
                capacity: -actual_load,
                power: actual_load * 0.2,
                requested_flow: mass_flow,
            }
        }
    }

    /// Pass-through component (like an adiabatic pipe or pump with no temp rise).
    struct MockPassthrough {
        name: String,
        inlet: usize,
        outlet: usize,
    }

    impl PlantComponent for MockPassthrough {
        fn name(&self) -> &str {
            &self.name
        }
        fn component_type(&self) -> &str {
            "MockPassthrough"
        }
        fn inlet_node(&self) -> usize {
            self.inlet
        }
        fn outlet_node(&self) -> usize {
            self.outlet
        }
        fn simulate(
            &mut self,
            inlet_temp: f64,
            mass_flow: f64,
            _load: f64,
            _flow_lock: FlowLockState,
        ) -> PlantComponentResult {
            PlantComponentResult {
                outlet_temp: inlet_temp,
                mass_flow,
                capacity: 0.0,
                power: 0.0,
                requested_flow: mass_flow,
            }
        }
    }

    // ─── PlantComponent trait ───

    #[test]
    fn mock_heater_heats_water() {
        let mut heater = MockHeater::new("Boiler", 50_000.0, 0, 1);
        let result = heater.simulate(40.0, 2.0, 50_000.0, FlowLockState::Unlocked);

        assert!(result.outlet_temp > 40.0, "T_out={}", result.outlet_temp);
        assert!(result.capacity > 0.0);
    }

    #[test]
    fn mock_cooler_cools_water() {
        let mut cooler = MockCooler::new("Coil", 30_000.0, 2, 3);
        let result = cooler.simulate(12.0, 2.0, 30_000.0, FlowLockState::Unlocked);

        assert!(result.outlet_temp < 12.0, "T_out={}", result.outlet_temp);
        assert!(result.capacity < 0.0);
    }

    // ─── Half-loop simulation ───

    #[test]
    fn half_loop_single_heater() {
        let mut components: Vec<Box<dyn PlantComponent>> =
            vec![Box::new(MockHeater::new("Boiler", 100_000.0, 0, 1))];

        let result = PlantLoopSolver::simulate_half_loop(
            &mut components,
            60.0,
            3.0,
            80_000.0,
            FlowLockState::Unlocked,
        );

        assert!(result.outlet_temp > 60.0, "T_out={}", result.outlet_temp);
        assert!(result.total_capacity > 0.0);
    }

    #[test]
    fn half_loop_series_components() {
        let mut components: Vec<Box<dyn PlantComponent>> = vec![
            Box::new(MockPassthrough {
                name: "Pump".into(),
                inlet: 0,
                outlet: 1,
            }),
            Box::new(MockHeater::new("Boiler", 100_000.0, 1, 2)),
        ];

        let result = PlantLoopSolver::simulate_half_loop(
            &mut components,
            50.0,
            2.0,
            60_000.0,
            FlowLockState::Unlocked,
        );

        // Pump passes through, boiler heats
        assert!(result.outlet_temp > 50.0, "T_out={}", result.outlet_temp);
    }

    #[test]
    fn half_loop_empty() {
        let mut components: Vec<Box<dyn PlantComponent>> = vec![];

        let result = PlantLoopSolver::simulate_half_loop(
            &mut components,
            45.0,
            1.0,
            0.0,
            FlowLockState::Unlocked,
        );

        assert!((result.outlet_temp - 45.0).abs() < 1e-10);
    }

    // ─── Full loop simulation ───

    #[test]
    fn loop_solver_converges_hot_water() {
        // Hot water loop: boiler (supply) heats water, coil (demand) cools it
        let mut plant_loop = PlantLoop::new("HW Loop", 0, 1, 2, 3);
        plant_loop.design_flow_rate = 2.0;
        plant_loop.max_flow_rate = 3.0;
        plant_loop.min_flow_rate = 0.1;
        plant_loop.loop_temp = 60.0;
        plant_loop.setpoint_temp = 80.0;

        let mut demand: Vec<Box<dyn PlantComponent>> =
            vec![Box::new(MockCooler::new("HW Coil", 40_000.0, 2, 3))];

        let mut supply: Vec<Box<dyn PlantComponent>> =
            vec![Box::new(MockHeater::new("Boiler", 50_000.0, 0, 1))];

        let solver = PlantLoopSolver::default();
        let result = solver.simulate(
            &mut plant_loop,
            &mut demand,
            &mut supply,
            40_000.0, // demand cooling load
            40_000.0, // supply heating load
        );

        assert!(result.iterations > 0, "iter={}", result.iterations);
        assert!(
            result.iterations <= 8,
            "too many iterations: {}",
            result.iterations
        );
        // Supply should heat water
        assert!(
            result.supply_outlet_temp > result.demand_outlet_temp,
            "T_supply={} should > T_demand={}",
            result.supply_outlet_temp,
            result.demand_outlet_temp
        );
    }

    #[test]
    fn loop_solver_converges_chilled_water() {
        // Chilled water loop: chiller (supply) cools water, coil (demand) heats it
        let mut plant_loop = PlantLoop::new("CHW Loop", 0, 1, 2, 3);
        plant_loop.design_flow_rate = 3.0;
        plant_loop.max_flow_rate = 4.0;
        plant_loop.min_flow_rate = 0.1;
        plant_loop.loop_temp = 7.0;
        plant_loop.setpoint_temp = 6.67;

        // On demand side, the coil heats the water (extracts cooling from building)
        let mut demand: Vec<Box<dyn PlantComponent>> =
            vec![Box::new(MockHeater::new("CW Coil", 60_000.0, 2, 3))];

        // On supply side, chiller cools the water back down
        let mut supply: Vec<Box<dyn PlantComponent>> =
            vec![Box::new(MockCooler::new("Chiller", 80_000.0, 0, 1))];

        let solver = PlantLoopSolver::default();
        let result = solver.simulate(
            &mut plant_loop,
            &mut demand,
            &mut supply,
            60_000.0, // demand load (heating water = extracting building cooling)
            60_000.0, // supply load (chiller provides cooling)
        );

        assert!(result.iterations <= 8, "iter={}", result.iterations);
        // Supply (chiller) outlet should be cooler than demand outlet
        assert!(
            result.supply_outlet_temp < result.demand_outlet_temp,
            "T_supply_out={} should < T_demand_out={}",
            result.supply_outlet_temp,
            result.demand_outlet_temp
        );
    }

    #[test]
    fn loop_solver_no_load() {
        let mut plant_loop = PlantLoop::new("Idle", 0, 1, 2, 3);
        plant_loop.design_flow_rate = 2.0;
        plant_loop.max_flow_rate = 3.0;
        plant_loop.min_flow_rate = 0.0;
        plant_loop.loop_temp = 40.0;

        let mut demand: Vec<Box<dyn PlantComponent>> =
            vec![Box::new(MockCooler::new("Coil", 50_000.0, 2, 3))];
        let mut supply: Vec<Box<dyn PlantComponent>> =
            vec![Box::new(MockHeater::new("Boiler", 50_000.0, 0, 1))];

        let solver = PlantLoopSolver::default();
        let result = solver.simulate(&mut plant_loop, &mut demand, &mut supply, 0.0, 0.0);

        // No load → temperatures should stay near initial
        assert!(
            (result.supply_outlet_temp - 40.0).abs() < 1.0,
            "T_supply={}",
            result.supply_outlet_temp
        );
    }

    #[test]
    fn loop_solver_respects_max_iterations() {
        let mut plant_loop = PlantLoop::new("Test", 0, 1, 2, 3);
        plant_loop.design_flow_rate = 2.0;
        plant_loop.max_flow_rate = 3.0;
        plant_loop.min_flow_rate = 0.1;
        plant_loop.loop_temp = 50.0;

        let mut demand: Vec<Box<dyn PlantComponent>> =
            vec![Box::new(MockCooler::new("Coil", 100_000.0, 2, 3))];
        let mut supply: Vec<Box<dyn PlantComponent>> =
            vec![Box::new(MockHeater::new("Boiler", 100_000.0, 0, 1))];

        let solver = PlantLoopSolver::new(3, 0.001, 0.0001);
        let result = solver.simulate(
            &mut plant_loop,
            &mut demand,
            &mut supply,
            80_000.0,
            80_000.0,
        );

        assert!(result.iterations <= 3, "iter={}", result.iterations);
    }

    // ─── Equipment dispatch ───

    #[test]
    fn equipment_dispatch_single() {
        let mut ops = EquipmentOperation::new();
        ops.add("Boiler-1", 0, 0.0, 500_000.0);

        let assignments = ops.dispatch(200_000.0);
        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[0].0, 0);
        assert!((assignments[0].1 - 200_000.0).abs() < 1.0);
    }

    #[test]
    fn equipment_dispatch_staging() {
        let mut ops = EquipmentOperation::new();
        ops.add("Chiller-1", 0, 0.0, 300_000.0);
        ops.add("Chiller-2", 1, 300_000.0, 600_000.0);

        // Small load → only chiller 1
        let a1 = ops.dispatch(200_000.0);
        assert_eq!(a1.len(), 1);
        assert_eq!(a1[0].0, 0);

        // Large load → chiller 2 range
        let a2 = ops.dispatch(400_000.0);
        assert!(!a2.is_empty());
    }

    #[test]
    fn equipment_dispatch_zero_load() {
        let mut ops = EquipmentOperation::new();
        ops.add("Boiler", 0, 0.0, 500_000.0);

        let assignments = ops.dispatch(0.0);
        assert!(assignments.is_empty());
    }

    // ─── Branch mixing ───

    #[test]
    fn mix_branch_outlets_equal_flow() {
        let temps = vec![60.0, 80.0];
        let flows = vec![1.0, 1.0];
        let mixed = mix_branch_outlets(&temps, &flows);
        assert!((mixed - 70.0).abs() < 0.01, "mixed={}", mixed);
    }

    #[test]
    fn mix_branch_outlets_unequal_flow() {
        let temps = vec![40.0, 80.0];
        let flows = vec![3.0, 1.0];
        let mixed = mix_branch_outlets(&temps, &flows);
        // (40*3 + 80*1) / 4 = 200/4 = 50.0
        assert!((mixed - 50.0).abs() < 0.01, "mixed={}", mixed);
    }

    #[test]
    fn mix_branch_outlets_zero_flow() {
        let temps = vec![60.0, 80.0];
        let flows = vec![0.0, 0.0];
        let mixed = mix_branch_outlets(&temps, &flows);
        // Zero total flow → returns first temp
        assert!((mixed - 60.0).abs() < 0.01, "mixed={}", mixed);
    }

    #[test]
    fn mix_branch_outlets_single() {
        let temps = vec![55.0];
        let flows = vec![2.0];
        let mixed = mix_branch_outlets(&temps, &flows);
        assert!((mixed - 55.0).abs() < 0.01, "mixed={}", mixed);
    }
}
