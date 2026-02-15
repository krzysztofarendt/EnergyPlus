//! Fluid-to-fluid heat exchanger model.
//!
//! Plate or shell-tube heat exchanger connecting two plant loops
//! using UA-effectiveness method.

/// Fluid-to-fluid heat exchanger type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FluidHXType {
    #[default]
    Plate,
    ShellTube,
}

/// Fluid-to-fluid heat exchanger.
#[derive(Debug, Clone)]
pub struct FluidToFluidHX {
    pub name: String,
    pub hx_type: FluidHXType,
    /// Overall UA-value (W/K).
    pub ua: f64,
    /// Design supply-side flow rate (kg/s).
    pub design_supply_flow: f64,
    /// Design demand-side flow rate (kg/s).
    pub design_demand_flow: f64,
}

/// Fluid HX result.
#[derive(Debug, Clone, Copy)]
pub struct FluidHXResult {
    /// Heat transfer rate (W). Positive = supply gains heat from demand.
    pub heat_rate: f64,
    /// Supply outlet temperature (C).
    pub supply_outlet_temp: f64,
    /// Demand outlet temperature (C).
    pub demand_outlet_temp: f64,
    /// Effectiveness (0-1).
    pub effectiveness: f64,
}

impl FluidToFluidHX {
    pub fn new(name: impl Into<String>, ua: f64) -> Self {
        Self {
            name: name.into(),
            hx_type: FluidHXType::Plate,
            ua,
            design_supply_flow: 5.0,
            design_demand_flow: 5.0,
        }
    }

    pub fn shell_tube(name: impl Into<String>, ua: f64) -> Self {
        let mut hx = Self::new(name, ua);
        hx.hx_type = FluidHXType::ShellTube;
        hx
    }

    /// Calculate heat exchanger performance using counterflow NTU-effectiveness.
    ///
    /// Supply side: the loop that receives or gives heat.
    /// Demand side: the other loop.
    pub fn calculate(
        &self,
        supply_inlet_temp: f64,
        supply_mass_flow: f64,
        demand_inlet_temp: f64,
        demand_mass_flow: f64,
    ) -> FluidHXResult {
        if supply_mass_flow <= 1e-10 || demand_mass_flow <= 1e-10 || self.ua <= 0.0 {
            return FluidHXResult {
                heat_rate: 0.0,
                supply_outlet_temp: supply_inlet_temp,
                demand_outlet_temp: demand_inlet_temp,
                effectiveness: 0.0,
            };
        }

        let cp_s = ep_psychrometrics::cp_water(supply_inlet_temp);
        let cp_d = ep_psychrometrics::cp_water(demand_inlet_temp);
        let c_supply = supply_mass_flow * cp_s;
        let c_demand = demand_mass_flow * cp_d;

        let c_min = c_supply.min(c_demand);
        let c_max = c_supply.max(c_demand);
        let c_ratio = c_min / c_max;

        let ntu = self.ua / c_min;

        // Counterflow effectiveness
        let effectiveness = if (c_ratio - 1.0).abs() < 1e-10 {
            ntu / (1.0 + ntu)
        } else {
            let exp_term = (-(1.0 - c_ratio) * ntu).exp();
            (1.0 - exp_term) / (1.0 - c_ratio * exp_term)
        };
        let effectiveness = effectiveness.clamp(0.0, 1.0);

        // Heat transfer: positive means supply gains heat (demand is hotter)
        let q_max = c_min * (demand_inlet_temp - supply_inlet_temp);
        let q = effectiveness * q_max;

        let supply_outlet = supply_inlet_temp + q / c_supply;
        let demand_outlet = demand_inlet_temp - q / c_demand;

        FluidHXResult {
            heat_rate: q,
            supply_outlet_temp: supply_outlet,
            demand_outlet_temp: demand_outlet,
            effectiveness,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fluid_hx_basic() {
        let hx = FluidToFluidHX::new("FHX-1", 10000.0);
        assert!((hx.ua - 10000.0).abs() < 1e-10);
    }

    #[test]
    fn fluid_hx_heating() {
        // Demand side is hot, supply side is cold
        let hx = FluidToFluidHX::new("FHX", 5000.0);
        let result = hx.calculate(30.0, 5.0, 80.0, 5.0);
        assert!(result.heat_rate > 0.0, "Q={}", result.heat_rate);
        assert!(result.supply_outlet_temp > 30.0);
        assert!(result.demand_outlet_temp < 80.0);
        assert!(result.effectiveness > 0.0 && result.effectiveness <= 1.0);
    }

    #[test]
    fn fluid_hx_cooling() {
        // Supply side is hot, demand side is cold
        let hx = FluidToFluidHX::new("FHX", 5000.0);
        let result = hx.calculate(80.0, 5.0, 30.0, 5.0);
        assert!(result.heat_rate < 0.0, "Q should be negative (supply loses heat)");
        assert!(result.supply_outlet_temp < 80.0);
        assert!(result.demand_outlet_temp > 30.0);
    }

    #[test]
    fn fluid_hx_energy_balance() {
        let hx = FluidToFluidHX::new("FHX", 8000.0);
        let result = hx.calculate(40.0, 3.0, 70.0, 5.0);
        let cp_s = ep_psychrometrics::cp_water(40.0);
        let cp_d = ep_psychrometrics::cp_water(70.0);
        let q_supply = 3.0 * cp_s * (result.supply_outlet_temp - 40.0);
        let q_demand = 5.0 * cp_d * (70.0 - result.demand_outlet_temp);
        assert!((q_supply - q_demand).abs() / q_supply.abs().max(1.0) < 0.01,
                "q_s={}, q_d={}", q_supply, q_demand);
    }

    #[test]
    fn fluid_hx_no_flow() {
        let hx = FluidToFluidHX::new("FHX", 5000.0);
        let result = hx.calculate(30.0, 0.0, 80.0, 5.0);
        assert!(result.heat_rate.abs() < 1e-10);
    }

    #[test]
    fn fluid_hx_shell_tube() {
        let hx = FluidToFluidHX::shell_tube("FHX-ST", 5000.0);
        assert_eq!(hx.hx_type, FluidHXType::ShellTube);
        let result = hx.calculate(30.0, 5.0, 80.0, 5.0);
        assert!(result.heat_rate > 0.0);
    }
}
