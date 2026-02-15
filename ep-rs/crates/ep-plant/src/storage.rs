//! Thermal energy storage models.
//!
//! - `StratifiedTank`: multi-node 1D water tank with buoyancy-driven mixing
//! - `IceStorage`: simple ice thermal storage with charge/discharge tracking

/// Multi-node stratified water storage tank.
///
/// Divides the tank into `num_nodes` vertical layers. Each node has its own
/// temperature. Heat loss, use-side flow, and source-side flow are computed
/// per-node. Buoyancy mixing ensures warmer nodes float above cooler ones.
#[derive(Debug, Clone)]
pub struct StratifiedTank {
    pub name: String,
    /// Total tank volume (m3).
    pub tank_volume: f64,
    /// Tank height (m).
    pub tank_height: f64,
    /// Number of nodes (vertical layers).
    pub num_nodes: usize,
    /// Node temperatures, top (0) to bottom (num_nodes-1) (C).
    pub node_temps: Vec<f64>,
    /// Setpoint temperature (C).
    pub setpoint_temp: f64,
    /// Heater capacity (W), applied at heater_node.
    pub heater_capacity: f64,
    /// Node index where heater is located (0 = top).
    pub heater_node: usize,
    /// Deadband delta-T below setpoint to activate heater (C).
    pub deadband: f64,
    /// Overall tank loss coefficient (W/K), distributed to all nodes.
    pub loss_coefficient: f64,
    /// Ambient temperature around tank (C).
    pub ambient_temp: f64,
    /// Use-side inlet node (0 = top).
    pub use_inlet_node: usize,
    /// Use-side outlet node (0 = top, typically top for hot draw).
    pub use_outlet_node: usize,
    /// Source-side inlet node (typically bottom for solar collector return).
    pub source_inlet_node: usize,
    /// Source-side outlet node (typically top for solar collector supply).
    pub source_outlet_node: usize,
}

/// Result of stratified tank calculation.
#[derive(Debug, Clone)]
pub struct StratifiedTankResult {
    /// Updated node temperatures (C).
    pub node_temps: Vec<f64>,
    /// Use-side outlet temperature (C).
    pub use_outlet_temp: f64,
    /// Source-side outlet temperature (C).
    pub source_outlet_temp: f64,
    /// Heater energy delivered (W).
    pub heater_rate: f64,
    /// Loss to ambient (W).
    pub loss_rate: f64,
    /// Energy stored in tank (J), relative to 0 C.
    pub stored_energy: f64,
}

impl StratifiedTank {
    pub fn new(
        name: impl Into<String>,
        volume: f64,
        height: f64,
        num_nodes: usize,
        initial_temp: f64,
    ) -> Self {
        let n = num_nodes.max(2);
        Self {
            name: name.into(),
            tank_volume: volume,
            tank_height: height,
            num_nodes: n,
            node_temps: vec![initial_temp; n],
            setpoint_temp: 60.0,
            heater_capacity: 0.0,
            heater_node: n / 2,
            deadband: 5.0,
            loss_coefficient: 5.0,
            ambient_temp: 20.0,
            use_inlet_node: n - 1, // cold supply enters bottom
            use_outlet_node: 0,     // hot draw from top
            source_inlet_node: n - 1,
            source_outlet_node: 0,
        }
    }

    /// Calculate one timestep of stratified tank operation.
    ///
    /// `use_inlet_temp`: temperature of incoming cold water (C)
    /// `use_mass_flow`: mass flow rate of use-side draw (kg/s)
    /// `source_inlet_temp`: temperature of source return water (C)
    /// `source_mass_flow`: source-side mass flow (kg/s)
    /// `timestep`: duration of timestep (s)
    pub fn calculate(
        &mut self,
        use_inlet_temp: f64,
        use_mass_flow: f64,
        source_inlet_temp: f64,
        source_mass_flow: f64,
        timestep: f64,
    ) -> StratifiedTankResult {
        if timestep <= 0.0 || self.tank_volume <= 0.0 || self.num_nodes < 2 {
            return StratifiedTankResult {
                node_temps: self.node_temps.clone(),
                use_outlet_temp: self.node_temps[self.use_outlet_node],
                source_outlet_temp: self.node_temps[self.source_outlet_node],
                heater_rate: 0.0,
                loss_rate: 0.0,
                stored_energy: 0.0,
            };
        }

        let n = self.num_nodes;
        let node_volume = self.tank_volume / n as f64;
        let water_density = 998.0;
        let node_mass = node_volume * water_density;
        let cp = ep_psychrometrics::cp_water(self.node_temps[0]);
        let node_capacity = node_mass * cp;

        // Loss per node
        let loss_per_node = self.loss_coefficient / n as f64;
        let mut total_loss = 0.0;

        let mut temps = self.node_temps.clone();

        // 1. Heat loss to ambient for each node
        for i in 0..n {
            let q_loss = loss_per_node * (temps[i] - self.ambient_temp);
            temps[i] -= q_loss * timestep / node_capacity;
            total_loss += q_loss;
        }

        // 2. Use-side flow (draw from tank)
        if use_mass_flow > 1e-10 {
            let flow_per_step = use_mass_flow * timestep;
            let fraction = (flow_per_step / node_mass).min(1.0);

            // Cold water enters at use_inlet_node, hot water exits at use_outlet_node
            // Simple plug-flow: shift temperatures
            let inlet = self.use_inlet_node;
            if inlet == n - 1 {
                // Bottom inlet: push cold water up from bottom
                for i in (1..n).rev() {
                    temps[i - 1] = temps[i - 1] * (1.0 - fraction) + temps[i] * fraction;
                }
                temps[n - 1] = temps[n - 1] * (1.0 - fraction) + use_inlet_temp * fraction;
            } else {
                // Top inlet: push water down
                for i in 0..n - 1 {
                    temps[i + 1] = temps[i + 1] * (1.0 - fraction) + temps[i] * fraction;
                }
                temps[0] = temps[0] * (1.0 - fraction) + use_inlet_temp * fraction;
            }
        }

        // 3. Source-side flow (e.g., solar collector heating)
        if source_mass_flow > 1e-10 {
            let flow_per_step = source_mass_flow * timestep;
            let fraction = (flow_per_step / node_mass).min(1.0);

            let inlet = self.source_inlet_node;
            if inlet == n - 1 {
                // Source enters bottom — heats from bottom up
                for i in (1..n).rev() {
                    temps[i - 1] = temps[i - 1] * (1.0 - fraction) + temps[i] * fraction;
                }
                temps[n - 1] = temps[n - 1] * (1.0 - fraction) + source_inlet_temp * fraction;
            } else {
                // Source enters top
                for i in 0..n - 1 {
                    temps[i + 1] = temps[i + 1] * (1.0 - fraction) + temps[i] * fraction;
                }
                temps[0] = temps[0] * (1.0 - fraction) + source_inlet_temp * fraction;
            }
        }

        // 4. Heater control
        let mut heater_rate = 0.0;
        let h = self.heater_node.min(n - 1);
        if self.heater_capacity > 0.0 && temps[h] < self.setpoint_temp - self.deadband {
            let needed = node_capacity * (self.setpoint_temp - temps[h]) / timestep;
            heater_rate = needed.min(self.heater_capacity);
            temps[h] += heater_rate * timestep / node_capacity;
        }

        // 5. Buoyancy mixing: merge inverted zones
        // When cold water sits above hot water, the entire inverted region
        // mixes to its mass-weighted average temperature.
        let mut resolved = true;
        while resolved {
            resolved = false;
            for i in 0..n - 1 {
                if temps[i] + 1e-10 < temps[i + 1] {
                    // Found inversion at node i. Find extent of inverted zone:
                    // extend downward while nodes are warmer than the cold top.
                    let start = i;
                    let mut sum = temps[i];
                    let mut count = 1usize;
                    let mut j = i + 1;
                    while j < n && temps[j] > temps[start] + 1e-10 {
                        sum += temps[j];
                        count += 1;
                        j += 1;
                    }
                    let avg = sum / count as f64;
                    for k in start..start + count {
                        temps[k] = avg;
                    }
                    resolved = true;
                    break; // restart scan from top
                }
            }
        }

        // Compute stored energy
        let stored_energy: f64 = temps.iter().map(|&t| node_mass * cp * t).sum();

        let use_outlet_temp = temps[self.use_outlet_node];
        let source_outlet_temp = temps[self.source_outlet_node];

        self.node_temps = temps.clone();

        StratifiedTankResult {
            node_temps: temps,
            use_outlet_temp,
            source_outlet_temp,
            heater_rate,
            loss_rate: total_loss,
            stored_energy,
        }
    }
}

/// Simple ice thermal storage.
///
/// Tracks ice fraction (0 = all water, 1 = all ice). Charging freezes
/// water, discharging melts ice to provide cooling.
#[derive(Debug, Clone)]
pub struct IceStorage {
    pub name: String,
    /// Storage capacity (J, total latent heat when fully frozen).
    pub capacity_j: f64,
    /// Maximum charge rate (W).
    pub max_charge_rate: f64,
    /// Maximum discharge rate (W).
    pub max_discharge_rate: f64,
    /// Current ice fraction (0.0 = all liquid, 1.0 = all ice).
    pub ice_fraction: f64,
    /// Freezing/melting temperature (C).
    pub freeze_temp: f64,
}

/// Ice storage calculation result.
#[derive(Debug, Clone, Copy)]
pub struct IceStorageResult {
    /// Cooling provided (W, positive = cooling to load).
    pub cooling_rate: f64,
    /// Charging rate (W, positive = ice being made).
    pub charge_rate: f64,
    /// Updated ice fraction.
    pub ice_fraction: f64,
    /// Outlet temperature (C).
    pub outlet_temp: f64,
}

impl IceStorage {
    pub fn new(
        name: impl Into<String>,
        capacity_j: f64,
        max_charge_rate: f64,
        max_discharge_rate: f64,
    ) -> Self {
        Self {
            name: name.into(),
            capacity_j,
            max_charge_rate,
            max_discharge_rate,
            ice_fraction: 0.0,
            freeze_temp: 0.0,
        }
    }

    /// Charge the ice storage (make ice).
    ///
    /// `available_cooling`: cooling capacity available from chiller (W)
    /// `timestep`: seconds
    pub fn charge(
        &mut self,
        available_cooling: f64,
        timestep: f64,
    ) -> IceStorageResult {
        if timestep <= 0.0 || self.capacity_j <= 0.0 || self.ice_fraction >= 1.0 {
            return IceStorageResult {
                cooling_rate: 0.0,
                charge_rate: 0.0,
                ice_fraction: self.ice_fraction,
                outlet_temp: self.freeze_temp,
            };
        }

        // Energy needed to fully charge
        let remaining_capacity = self.capacity_j * (1.0 - self.ice_fraction);
        let max_energy = remaining_capacity;

        // Rate-limited charging
        let charge_rate = available_cooling
            .min(self.max_charge_rate)
            .min(max_energy / timestep);

        let energy_stored = charge_rate * timestep;
        self.ice_fraction = (self.ice_fraction + energy_stored / self.capacity_j).min(1.0);

        IceStorageResult {
            cooling_rate: 0.0,
            charge_rate,
            ice_fraction: self.ice_fraction,
            outlet_temp: self.freeze_temp,
        }
    }

    /// Discharge the ice storage (provide cooling by melting ice).
    ///
    /// `cooling_load`: cooling demand (W)
    /// `inlet_temp`: return water temperature (C)
    /// `mass_flow`: water flow rate (kg/s)
    /// `timestep`: seconds
    pub fn discharge(
        &mut self,
        cooling_load: f64,
        inlet_temp: f64,
        mass_flow: f64,
        timestep: f64,
    ) -> IceStorageResult {
        if timestep <= 0.0
            || cooling_load <= 0.0
            || self.ice_fraction <= 0.0
            || mass_flow <= 1e-10
        {
            return IceStorageResult {
                cooling_rate: 0.0,
                charge_rate: 0.0,
                ice_fraction: self.ice_fraction,
                outlet_temp: inlet_temp,
            };
        }

        // Available stored energy
        let available_energy = self.capacity_j * self.ice_fraction;
        let max_energy_rate = available_energy / timestep;

        // Rate-limited discharge
        let cooling_rate = cooling_load
            .min(self.max_discharge_rate)
            .min(max_energy_rate);

        let energy_used = cooling_rate * timestep;
        self.ice_fraction = (self.ice_fraction - energy_used / self.capacity_j).max(0.0);

        // Outlet temperature
        let cp = ep_psychrometrics::cp_water(inlet_temp);
        let outlet_temp = inlet_temp - cooling_rate / (mass_flow * cp);

        IceStorageResult {
            cooling_rate,
            charge_rate: 0.0,
            ice_fraction: self.ice_fraction,
            outlet_temp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── StratifiedTank ───

    #[test]
    fn stratified_tank_initial_uniform() {
        let tank = StratifiedTank::new("Tank", 0.5, 1.5, 10, 50.0);
        assert_eq!(tank.num_nodes, 10);
        assert!(tank.node_temps.iter().all(|&t| (t - 50.0).abs() < 1e-10));
    }

    #[test]
    fn stratified_tank_standby_loss() {
        let mut tank = StratifiedTank::new("Tank", 0.5, 1.5, 6, 60.0);
        tank.ambient_temp = 20.0;
        tank.loss_coefficient = 10.0;
        tank.heater_capacity = 0.0; // no heater

        let result = tank.calculate(20.0, 0.0, 20.0, 0.0, 3600.0);

        // Tank should cool down from standby losses
        assert!(
            result.node_temps[0] < 60.0,
            "T_top={}, expected < 60",
            result.node_temps[0]
        );
        assert!(result.loss_rate > 0.0, "loss={}", result.loss_rate);
    }

    #[test]
    fn stratified_tank_hot_draw() {
        let mut tank = StratifiedTank::new("Tank", 0.5, 1.5, 6, 60.0);
        tank.heater_capacity = 0.0;
        tank.loss_coefficient = 0.0; // no losses to isolate draw effect

        // Draw hot water from top, cold inlet at bottom
        let result = tank.calculate(15.0, 0.5, 15.0, 0.0, 600.0);

        // Top should still be warm, bottom should be cooler from cold inlet
        assert!(result.use_outlet_temp > 40.0, "T_out={}", result.use_outlet_temp);
        let bottom = result.node_temps.last().unwrap();
        assert!(
            *bottom < result.node_temps[0],
            "bottom={} should < top={}",
            bottom,
            result.node_temps[0]
        );
    }

    #[test]
    fn stratified_tank_buoyancy_mixing() {
        let mut tank = StratifiedTank::new("Tank", 0.5, 1.5, 4, 40.0);
        tank.heater_capacity = 0.0;
        tank.loss_coefficient = 0.0;

        // Manually create inversion: cold on top, hot on bottom
        tank.node_temps = vec![20.0, 30.0, 50.0, 60.0];

        // No flow, no losses — only buoyancy mixing acts
        let result = tank.calculate(40.0, 0.0, 0.0, 0.0, 1.0);

        // After buoyancy mixing, temps should be non-increasing top to bottom
        for i in 0..result.node_temps.len() - 1 {
            assert!(
                result.node_temps[i] >= result.node_temps[i + 1] - 0.01,
                "Inversion at node {}: {} < {}",
                i,
                result.node_temps[i],
                result.node_temps[i + 1]
            );
        }
    }

    #[test]
    fn stratified_tank_heater_activates() {
        let mut tank = StratifiedTank::new("Tank", 0.3, 1.2, 4, 40.0);
        tank.setpoint_temp = 60.0;
        tank.deadband = 5.0;
        tank.heater_capacity = 5000.0;
        tank.heater_node = 1;
        tank.loss_coefficient = 0.0;

        // Tank at 40 C, setpoint 60 C, deadband 5 → heater should activate
        let result = tank.calculate(40.0, 0.0, 40.0, 0.0, 600.0);

        assert!(result.heater_rate > 0.0, "heater={}", result.heater_rate);
        assert!(
            result.node_temps[1] > 40.0,
            "heater node temp={}, expected > 40",
            result.node_temps[1]
        );
    }

    // ─── IceStorage ───

    #[test]
    fn ice_storage_charge() {
        let latent_heat = 334_000.0; // J/kg
        let ice_mass = 1000.0; // kg
        let capacity = latent_heat * ice_mass; // ~334 MJ

        let mut ice = IceStorage::new("Ice", capacity, 100_000.0, 100_000.0);
        assert!((ice.ice_fraction).abs() < 1e-10);

        // Charge for 1 hour at 50 kW
        let result = ice.charge(50_000.0, 3600.0);
        assert!(result.charge_rate > 0.0);
        assert!(result.ice_fraction > 0.0, "IF={}", result.ice_fraction);
        assert!(result.ice_fraction < 1.0);
    }

    #[test]
    fn ice_storage_discharge() {
        let capacity = 100_000_000.0; // 100 MJ
        let mut ice = IceStorage::new("Ice", capacity, 50_000.0, 50_000.0);
        ice.ice_fraction = 0.5; // half charged

        let result = ice.discharge(30_000.0, 12.0, 3.0, 3600.0);

        assert!(result.cooling_rate > 0.0, "Q={}", result.cooling_rate);
        assert!(result.ice_fraction < 0.5, "IF={}", result.ice_fraction);
        assert!(result.outlet_temp < 12.0, "T_out={}", result.outlet_temp);
    }

    #[test]
    fn ice_storage_energy_conservation() {
        let capacity = 50_000_000.0; // 50 MJ
        let mut ice = IceStorage::new("Ice", capacity, 100_000.0, 100_000.0);

        // Charge fully
        let timestep = 600.0; // 10 min
        let mut total_charged = 0.0;
        for _ in 0..100 {
            if ice.ice_fraction >= 1.0 {
                break;
            }
            let r = ice.charge(100_000.0, timestep);
            total_charged += r.charge_rate * timestep;
        }

        // Now discharge fully
        let mut total_discharged = 0.0;
        for _ in 0..100 {
            if ice.ice_fraction <= 0.0 {
                break;
            }
            let r = ice.discharge(100_000.0, 10.0, 5.0, timestep);
            total_discharged += r.cooling_rate * timestep;
        }

        // Energy in should approximately equal energy out
        let ratio = total_discharged / total_charged.max(1.0);
        assert!(
            (ratio - 1.0).abs() < 0.05,
            "charged={}, discharged={}, ratio={}",
            total_charged,
            total_discharged,
            ratio
        );
    }

    #[test]
    fn ice_storage_no_discharge_when_empty() {
        let mut ice = IceStorage::new("Ice", 100_000_000.0, 50_000.0, 50_000.0);
        ice.ice_fraction = 0.0;

        let result = ice.discharge(30_000.0, 12.0, 3.0, 3600.0);
        assert!(result.cooling_rate.abs() < 1e-10);
        assert!((result.outlet_temp - 12.0).abs() < 1e-10);
    }

    #[test]
    fn ice_storage_no_charge_when_full() {
        let mut ice = IceStorage::new("Ice", 100_000_000.0, 50_000.0, 50_000.0);
        ice.ice_fraction = 1.0;

        let result = ice.charge(50_000.0, 3600.0);
        assert!(result.charge_rate.abs() < 1e-10);
        assert!((result.ice_fraction - 1.0).abs() < 1e-10);
    }
}
