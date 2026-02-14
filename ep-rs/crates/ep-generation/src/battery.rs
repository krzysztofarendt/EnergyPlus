//! Battery energy storage models.
//!
//! Simple bucket model with constant charge/discharge efficiency,
//! and kinetic battery model (KiBaM) with two-tank representation.

/// Battery model type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryModelType {
    /// Simple efficiency-based storage.
    SimpleBucket,
    /// Kinetic battery model (two-tank).
    Kinetic,
}

/// Battery storage mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMode {
    Idle,
    Charging,
    Discharging,
}

/// Simple bucket battery storage.
#[derive(Debug, Clone)]
pub struct SimpleBattery {
    pub name: String,
    /// Maximum energy capacity (J).
    pub max_capacity: f64,
    /// Maximum charge power (W).
    pub max_charge_power: f64,
    /// Maximum discharge power (W).
    pub max_discharge_power: f64,
    /// Charging efficiency (0-1).
    pub charge_efficiency: f64,
    /// Discharging efficiency (0-1).
    pub discharge_efficiency: f64,
    /// Current state of charge (J).
    pub state_of_charge: f64,
    /// Maximum SOC fraction (0-1).
    pub max_soc_fraction: f64,
    /// Minimum SOC fraction (0-1).
    pub min_soc_fraction: f64,
}

/// Battery calculation result.
#[derive(Debug, Clone, Copy)]
pub struct BatteryResult {
    /// Actual charge power drawn from source (W).
    pub charge_power: f64,
    /// Actual discharge power delivered to load (W).
    pub discharge_power: f64,
    /// Energy stored after operation (J).
    pub state_of_charge: f64,
    /// Fraction of capacity (0-1).
    pub soc_fraction: f64,
    /// Thermal losses (W).
    pub thermal_loss: f64,
    /// Storage mode.
    pub mode: StorageMode,
}

impl SimpleBattery {
    pub fn new(
        name: impl Into<String>,
        max_capacity_kwh: f64,
        max_power_kw: f64,
    ) -> Self {
        let max_capacity = max_capacity_kwh * 3_600_000.0; // kWh to J
        let max_power = max_power_kw * 1000.0; // kW to W
        Self {
            name: name.into(),
            max_capacity,
            max_charge_power: max_power,
            max_discharge_power: max_power,
            charge_efficiency: 0.95,
            discharge_efficiency: 0.95,
            state_of_charge: max_capacity * 0.5, // Start at 50%
            max_soc_fraction: 0.95,
            min_soc_fraction: 0.05,
        }
    }

    /// Calculate battery charge/discharge for the timestep.
    ///
    /// `power_request` — positive = charge, negative = discharge (W).
    /// `timestep` — duration (seconds).
    pub fn calculate(&mut self, power_request: f64, timestep: f64) -> BatteryResult {
        let max_energy = self.max_capacity * self.max_soc_fraction;
        let min_energy = self.max_capacity * self.min_soc_fraction;

        if power_request > 0.0 {
            // Charging
            let available_capacity = max_energy - self.state_of_charge;
            let max_charge_energy = self.max_charge_power * timestep;
            let requested_energy = power_request * timestep;
            let energy_to_store = requested_energy
                .min(max_charge_energy)
                .min(available_capacity / self.charge_efficiency);

            let actual_stored = energy_to_store * self.charge_efficiency;
            let thermal_loss = energy_to_store * (1.0 - self.charge_efficiency);
            self.state_of_charge += actual_stored;

            BatteryResult {
                charge_power: energy_to_store / timestep,
                discharge_power: 0.0,
                state_of_charge: self.state_of_charge,
                soc_fraction: self.state_of_charge / self.max_capacity,
                thermal_loss: thermal_loss / timestep,
                mode: StorageMode::Charging,
            }
        } else if power_request < 0.0 {
            // Discharging
            let available_energy = self.state_of_charge - min_energy;
            let max_discharge_energy = self.max_discharge_power * timestep;
            let requested_energy = (-power_request) * timestep;
            let energy_from_battery = requested_energy
                .min(max_discharge_energy / self.discharge_efficiency)
                .min(available_energy);

            let actual_delivered = energy_from_battery * self.discharge_efficiency;
            let thermal_loss = energy_from_battery * (1.0 - self.discharge_efficiency);
            self.state_of_charge -= energy_from_battery;

            BatteryResult {
                charge_power: 0.0,
                discharge_power: actual_delivered / timestep,
                state_of_charge: self.state_of_charge,
                soc_fraction: self.state_of_charge / self.max_capacity,
                thermal_loss: thermal_loss / timestep,
                mode: StorageMode::Discharging,
            }
        } else {
            BatteryResult {
                charge_power: 0.0,
                discharge_power: 0.0,
                state_of_charge: self.state_of_charge,
                soc_fraction: self.state_of_charge / self.max_capacity,
                thermal_loss: 0.0,
                mode: StorageMode::Idle,
            }
        }
    }
}

/// Kinetic battery model (KiBaM) with available and bound charge tanks.
#[derive(Debug, Clone)]
pub struct KineticBattery {
    pub name: String,
    /// Maximum Ah capacity.
    pub max_ah_capacity: f64,
    /// Available fraction (c parameter).
    pub available_fraction: f64,
    /// Charge conversion rate (k, 1/h).
    pub conversion_rate: f64,
    /// Internal resistance (ohms).
    pub internal_resistance: f64,
    /// Number of cells in series.
    pub series_cells: u32,
    /// Number of strings in parallel.
    pub parallel_strings: u32,
    /// Charged open-circuit voltage per cell (V).
    pub charged_ocv: f64,
    /// Discharged open-circuit voltage per cell (V).
    pub discharged_ocv: f64,
    /// Available charge (Ah).
    pub available: f64,
    /// Bound charge (Ah).
    pub bound: f64,
}

impl KineticBattery {
    pub fn new(
        name: impl Into<String>,
        max_ah: f64,
        available_fraction: f64,
        conversion_rate: f64,
    ) -> Self {
        let available = max_ah * available_fraction * 0.5;
        let bound = max_ah * (1.0 - available_fraction) * 0.5;
        Self {
            name: name.into(),
            max_ah_capacity: max_ah,
            available_fraction,
            conversion_rate,
            internal_resistance: 0.01,
            series_cells: 6,
            parallel_strings: 1,
            charged_ocv: 2.15,
            discharged_ocv: 1.75,
            available,
            bound,
        }
    }

    /// Total charge (Ah).
    pub fn total_charge(&self) -> f64 {
        self.available + self.bound
    }

    /// State of charge fraction.
    pub fn soc_fraction(&self) -> f64 {
        self.total_charge() / self.max_ah_capacity
    }

    /// Open-circuit voltage at current SOC.
    pub fn open_circuit_voltage(&self) -> f64 {
        let soc = self.soc_fraction();
        let v_cell = self.discharged_ocv + soc * (self.charged_ocv - self.discharged_ocv);
        v_cell * self.series_cells as f64
    }

    /// Calculate charge redistribution between tanks.
    /// `dt` — timestep in hours.
    /// `current` — positive = charging, negative = discharging (A).
    pub fn step(&mut self, current: f64, dt_hours: f64) {
        let c = self.available_fraction;
        let k = self.conversion_rate;

        // Apply current to available tank (discharge removes from available)
        let i_total = current * self.parallel_strings as f64;
        let delta_q = i_total * dt_hours;

        // Transfer between tanks: bound → available at rate k*(q2/q_total - (1-c))
        let q_total_current = self.available + self.bound;
        let eq_bound = q_total_current * (1.0 - c);
        let transfer = k * (self.bound - eq_bound) * dt_hours;

        self.available += delta_q + transfer;
        self.bound -= transfer;

        // Clamp to valid ranges
        self.available = self.available.max(0.0);
        self.bound = self.bound.max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_battery_charge() {
        let mut bat = SimpleBattery::new("Bat", 10.0, 5.0);
        bat.state_of_charge = bat.max_capacity * 0.5;
        let result = bat.calculate(3000.0, 3600.0); // 3kW for 1 hour
        assert_eq!(result.mode, StorageMode::Charging);
        assert!(result.charge_power > 0.0);
        assert!(result.soc_fraction > 0.5);
    }

    #[test]
    fn simple_battery_discharge() {
        let mut bat = SimpleBattery::new("Bat", 10.0, 5.0);
        bat.state_of_charge = bat.max_capacity * 0.5;
        let result = bat.calculate(-3000.0, 3600.0);
        assert_eq!(result.mode, StorageMode::Discharging);
        assert!(result.discharge_power > 0.0);
        assert!(result.soc_fraction < 0.5);
    }

    #[test]
    fn simple_battery_idle() {
        let mut bat = SimpleBattery::new("Bat", 10.0, 5.0);
        let result = bat.calculate(0.0, 3600.0);
        assert_eq!(result.mode, StorageMode::Idle);
    }

    #[test]
    fn simple_battery_max_charge_limit() {
        let mut bat = SimpleBattery::new("Bat", 10.0, 5.0); // 5kW max
        bat.state_of_charge = bat.max_capacity * 0.5;
        let result = bat.calculate(10000.0, 3600.0); // Request 10kW, max is 5kW
        assert!(result.charge_power <= 5001.0, "charge_power={}", result.charge_power);
    }

    #[test]
    fn simple_battery_soc_limits() {
        let mut bat = SimpleBattery::new("Bat", 10.0, 100.0);
        bat.state_of_charge = bat.max_capacity * 0.90;
        // Try to charge past max SOC
        bat.calculate(100000.0, 3600.0);
        assert!(bat.state_of_charge <= bat.max_capacity * bat.max_soc_fraction + 1.0);
    }

    #[test]
    fn simple_battery_energy_conservation() {
        let mut bat = SimpleBattery::new("Bat", 10.0, 5.0);
        let initial_soc = bat.state_of_charge;
        let result = bat.calculate(3000.0, 3600.0);
        let energy_in = result.charge_power * 3600.0;
        let energy_stored = bat.state_of_charge - initial_soc;
        // Energy stored = energy_in * efficiency
        assert!((energy_stored - energy_in * bat.charge_efficiency).abs() / energy_in.abs().max(1.0) < 0.01);
    }

    #[test]
    fn kinetic_battery_creation() {
        let bat = KineticBattery::new("KiBaM", 100.0, 0.3, 0.5);
        assert!(bat.soc_fraction() > 0.0 && bat.soc_fraction() <= 1.0);
        assert!(bat.open_circuit_voltage() > 0.0);
    }

    #[test]
    fn kinetic_battery_discharge() {
        let mut bat = KineticBattery::new("KiBaM", 100.0, 0.3, 0.5);
        let initial_total = bat.total_charge();
        bat.step(-10.0, 1.0); // Discharge 10A for 1 hour
        assert!(bat.total_charge() < initial_total);
    }
}
