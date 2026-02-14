//! Electric load center — dispatches generators and manages storage.
//!
//! Coordinates multiple generators, inverters, and battery storage
//! to meet facility electrical demand.

/// Generator dispatch scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchScheme {
    /// Generators run at constant rated output.
    BaseLoad,
    /// Generators modulate to limit demand below a threshold.
    DemandLimit,
    /// Generators follow facility electrical demand.
    TrackElectrical,
    /// Generators follow a scheduled power profile.
    TrackSchedule,
    /// Generators follow thermal demand (CHP).
    ThermalFollow,
}

/// Electric bus type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusType {
    /// AC bus (generators produce AC directly).
    AcBus,
    /// DC bus with inverter.
    DcBusInverter,
    /// AC bus with storage.
    AcBusStorage,
}

/// A generator entry in the load center.
#[derive(Debug, Clone)]
pub struct GeneratorEntry {
    pub name: String,
    pub rated_power: f64,
    pub available: bool,
    /// Current electrical output (W).
    pub current_output: f64,
    /// Current thermal output (W).
    pub current_thermal: f64,
}

/// Electric load center managing generators and storage.
#[derive(Debug, Clone)]
pub struct LoadCenter {
    pub name: String,
    pub dispatch_scheme: DispatchScheme,
    pub bus_type: BusType,
    pub generators: Vec<GeneratorEntry>,
    /// Demand limit (W) for DemandLimit scheme.
    pub demand_limit: f64,
    /// Inverter efficiency (for DC bus).
    pub inverter_efficiency: f64,
    /// Whether storage is present.
    pub has_storage: bool,
}

/// Load center dispatch result.
#[derive(Debug, Clone, Copy)]
pub struct LoadCenterResult {
    /// Total generation (W, AC).
    pub total_generation: f64,
    /// Total thermal production (W).
    pub total_thermal: f64,
    /// Power fed into building panel (W).
    pub feed_in_power: f64,
    /// Power drawn from grid (W).
    pub draw_from_grid: f64,
    /// Surplus power (W, for storage or export).
    pub surplus_power: f64,
}

impl LoadCenter {
    pub fn new(name: impl Into<String>, scheme: DispatchScheme) -> Self {
        Self {
            name: name.into(),
            dispatch_scheme: scheme,
            bus_type: BusType::AcBus,
            generators: Vec::new(),
            demand_limit: 0.0,
            inverter_efficiency: 0.96,
            has_storage: false,
        }
    }

    pub fn add_generator(&mut self, name: impl Into<String>, rated_power: f64) {
        self.generators.push(GeneratorEntry {
            name: name.into(),
            rated_power,
            available: true,
            current_output: 0.0,
            current_thermal: 0.0,
        });
    }

    /// Dispatch generators to meet demand.
    ///
    /// `facility_demand` — total building electrical demand (W).
    /// Returns the dispatch result.
    pub fn dispatch(&mut self, facility_demand: f64) -> LoadCenterResult {
        let mut remaining_demand = facility_demand;
        let mut total_generation = 0.0;
        let total_thermal = 0.0;

        match self.dispatch_scheme {
            DispatchScheme::BaseLoad => {
                for gen in &mut self.generators {
                    if gen.available {
                        gen.current_output = gen.rated_power;
                        total_generation += gen.current_output;
                    }
                }
            }
            DispatchScheme::TrackElectrical => {
                for gen in &mut self.generators {
                    if gen.available && remaining_demand > 0.0 {
                        gen.current_output = remaining_demand.min(gen.rated_power);
                        remaining_demand -= gen.current_output;
                        total_generation += gen.current_output;
                    } else {
                        gen.current_output = 0.0;
                    }
                }
            }
            DispatchScheme::DemandLimit => {
                let excess = facility_demand - self.demand_limit;
                if excess > 0.0 {
                    let mut remaining = excess;
                    for gen in &mut self.generators {
                        if gen.available && remaining > 0.0 {
                            gen.current_output = remaining.min(gen.rated_power);
                            remaining -= gen.current_output;
                            total_generation += gen.current_output;
                        } else {
                            gen.current_output = 0.0;
                        }
                    }
                }
            }
            DispatchScheme::TrackSchedule | DispatchScheme::ThermalFollow => {
                // Simplified: same as track electrical
                for gen in &mut self.generators {
                    if gen.available && remaining_demand > 0.0 {
                        gen.current_output = remaining_demand.min(gen.rated_power);
                        remaining_demand -= gen.current_output;
                        total_generation += gen.current_output;
                    } else {
                        gen.current_output = 0.0;
                    }
                }
            }
        }

        // Apply inverter efficiency for DC bus
        if self.bus_type == BusType::DcBusInverter {
            total_generation *= self.inverter_efficiency;
        }

        let surplus = (total_generation - facility_demand).max(0.0);
        let draw_from_grid = (facility_demand - total_generation).max(0.0);

        LoadCenterResult {
            total_generation,
            total_thermal,
            feed_in_power: total_generation.min(facility_demand),
            draw_from_grid,
            surplus_power: surplus,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_load_dispatch() {
        let mut lc = LoadCenter::new("LC-1", DispatchScheme::BaseLoad);
        lc.add_generator("Gen1", 50_000.0);
        lc.add_generator("Gen2", 30_000.0);

        let result = lc.dispatch(40_000.0);
        // Base load: all generators at rated
        assert!((result.total_generation - 80_000.0).abs() < 100.0);
        assert!((result.surplus_power - 40_000.0).abs() < 100.0);
        assert!(result.draw_from_grid.abs() < 100.0);
    }

    #[test]
    fn track_electrical_dispatch() {
        let mut lc = LoadCenter::new("LC", DispatchScheme::TrackElectrical);
        lc.add_generator("Gen1", 50_000.0);
        lc.add_generator("Gen2", 50_000.0);

        let result = lc.dispatch(70_000.0);
        assert!((result.total_generation - 70_000.0).abs() < 100.0);
        assert!(result.surplus_power.abs() < 100.0);
    }

    #[test]
    fn track_electrical_insufficient() {
        let mut lc = LoadCenter::new("LC", DispatchScheme::TrackElectrical);
        lc.add_generator("Gen1", 30_000.0);

        let result = lc.dispatch(50_000.0);
        assert!((result.total_generation - 30_000.0).abs() < 100.0);
        assert!((result.draw_from_grid - 20_000.0).abs() < 100.0);
    }

    #[test]
    fn demand_limit_dispatch() {
        let mut lc = LoadCenter::new("LC", DispatchScheme::DemandLimit);
        lc.demand_limit = 100_000.0;
        lc.add_generator("Gen1", 50_000.0);

        // Below limit: no generation
        let result = lc.dispatch(80_000.0);
        assert!(result.total_generation.abs() < 100.0);

        // Above limit: generate to offset excess
        let result = lc.dispatch(130_000.0);
        assert!((result.total_generation - 30_000.0).abs() < 100.0);
    }

    #[test]
    fn dc_bus_inverter_loss() {
        let mut lc = LoadCenter::new("LC", DispatchScheme::BaseLoad);
        lc.bus_type = BusType::DcBusInverter;
        lc.inverter_efficiency = 0.96;
        lc.add_generator("PV", 10_000.0);

        let result = lc.dispatch(20_000.0);
        // DC generation with inverter loss
        assert!((result.total_generation - 9600.0).abs() < 100.0);
    }

    #[test]
    fn generator_unavailable() {
        let mut lc = LoadCenter::new("LC", DispatchScheme::TrackElectrical);
        lc.add_generator("Gen1", 50_000.0);
        lc.add_generator("Gen2", 50_000.0);
        lc.generators[1].available = false;

        let result = lc.dispatch(70_000.0);
        // Only Gen1 available
        assert!((result.total_generation - 50_000.0).abs() < 100.0);
        assert!((result.draw_from_grid - 20_000.0).abs() < 100.0);
    }
}
