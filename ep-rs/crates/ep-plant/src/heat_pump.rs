//! Heat pump models for plant loops.
//!
//! - `WaterToWaterHeatPump`: equation fit model with 5-coefficient equations
//!   for capacity and power as functions of source/load temps and flow
//! - `EirHeatPump`: EIR-based model similar to EIR chiller but for heating mode

use ep_curves::Curve;

/// Water-to-water heat pump using the equation fit method.
///
/// Capacity and power are expressed as linear functions of source/load
/// temperatures and volume flow rates:
///   Q = c1 + c2*T_load_in + c3*T_source_in + c4*V_load + c5*V_source
///   P = c1 + c2*T_load_in + c3*T_source_in + c4*V_load + c5*V_source
#[derive(Debug, Clone)]
pub struct WaterToWaterHeatPump {
    pub name: String,
    /// Reference heating capacity (W).
    pub ref_capacity: f64,
    /// Reference COP (W/W).
    pub ref_cop: f64,
    /// Reference load-side flow (kg/s).
    pub ref_load_flow: f64,
    /// Reference source-side flow (kg/s).
    pub ref_source_flow: f64,
    /// Capacity coefficients [c1..c5].
    pub cap_coefficients: [f64; 5],
    /// Power coefficients [c1..c5].
    pub power_coefficients: [f64; 5],
    /// Companion COP (for reference).
    pub ref_source_temp: f64,
    /// Reference load inlet temp (C).
    pub ref_load_temp: f64,
}

/// Heat pump calculation result.
#[derive(Debug, Clone, Copy)]
pub struct HeatPumpResult {
    /// Heating (or cooling) capacity (W).
    pub capacity: f64,
    /// Compressor power (W).
    pub power: f64,
    /// Source-side heat transfer (W, positive = heat extracted from source).
    pub source_heat_rate: f64,
    /// Load-side outlet temperature (C).
    pub load_outlet_temp: f64,
    /// Source-side outlet temperature (C).
    pub source_outlet_temp: f64,
    /// COP at operating conditions.
    pub cop: f64,
}

impl WaterToWaterHeatPump {
    pub fn new(
        name: impl Into<String>,
        ref_capacity: f64,
        ref_cop: f64,
        ref_load_flow: f64,
        ref_source_flow: f64,
    ) -> Self {
        Self {
            name: name.into(),
            ref_capacity,
            ref_cop,
            ref_load_flow,
            ref_source_flow,
            // Default: capacity and power equal to reference (flat)
            cap_coefficients: [1.0, 0.0, 0.0, 0.0, 0.0],
            power_coefficients: [1.0, 0.0, 0.0, 0.0, 0.0],
            ref_source_temp: 10.0,
            ref_load_temp: 40.0,
        }
    }

    /// Calculate heat pump performance in heating mode.
    ///
    /// `load` is the heating demand (W, positive = heating needed).
    pub fn calculate(
        &self,
        load_inlet_temp: f64,
        load_mass_flow: f64,
        source_inlet_temp: f64,
        source_mass_flow: f64,
        load: f64,
    ) -> HeatPumpResult {
        let zero = HeatPumpResult {
            capacity: 0.0,
            power: 0.0,
            source_heat_rate: 0.0,
            load_outlet_temp: load_inlet_temp,
            source_outlet_temp: source_inlet_temp,
            cop: 0.0,
        };

        if load_mass_flow <= 1e-10 || load <= 0.0 || self.ref_capacity <= 0.0 {
            return zero;
        }

        // Normalized temperatures and flows
        let t_load_norm = load_inlet_temp / self.ref_load_temp;
        let t_source_norm = source_inlet_temp / self.ref_source_temp.max(1.0);
        let v_load_norm = load_mass_flow / self.ref_load_flow.max(1e-10);
        let v_source_norm = source_mass_flow / self.ref_source_flow.max(1e-10);

        // Available capacity
        let c = &self.cap_coefficients;
        let cap_modifier = (c[0] + c[1] * t_load_norm + c[2] * t_source_norm
            + c[3] * v_load_norm
            + c[4] * v_source_norm)
            .max(0.0);
        let available_capacity = self.ref_capacity * cap_modifier;

        if available_capacity <= 0.0 {
            return zero;
        }

        // Part-load
        let actual_capacity = load.min(available_capacity);

        // Power at operating conditions
        let p = &self.power_coefficients;
        let power_modifier = (p[0] + p[1] * t_load_norm + p[2] * t_source_norm
            + p[3] * v_load_norm
            + p[4] * v_source_norm)
            .max(0.0);
        let ref_power = self.ref_capacity / self.ref_cop.max(0.01);
        let available_power = ref_power * power_modifier;
        let plr = actual_capacity / available_capacity;
        let power = available_power * plr;

        // Source-side heat = capacity - power (heating mode: Q_source = Q_load - W)
        let source_heat = actual_capacity - power;

        // Outlet temperatures
        let cp_load = ep_psychrometrics::cp_water(load_inlet_temp);
        let load_outlet_temp = load_inlet_temp + actual_capacity / (load_mass_flow * cp_load);

        let source_outlet_temp = if source_mass_flow > 1e-10 {
            let cp_source = ep_psychrometrics::cp_water(source_inlet_temp);
            source_inlet_temp - source_heat / (source_mass_flow * cp_source)
        } else {
            source_inlet_temp
        };

        let cop = if power > 0.0 {
            actual_capacity / power
        } else {
            0.0
        };

        HeatPumpResult {
            capacity: actual_capacity,
            power,
            source_heat_rate: source_heat,
            load_outlet_temp,
            source_outlet_temp,
            cop,
        }
    }
}

/// Plant loop EIR heat pump — EIR-based model similar to chiller.
///
/// Uses three performance curves like the EIR chiller, but operates in
/// heating mode. Can be reversed for cooling by using cooling reference values.
#[derive(Debug, Clone)]
pub struct EirHeatPump {
    pub name: String,
    /// Reference heating capacity (W).
    pub ref_capacity: f64,
    /// Reference COP (W/W).
    pub ref_cop: f64,
    /// Reference load-side leaving temp (C).
    pub ref_load_leaving_temp: f64,
    /// Reference source-side entering temp (C).
    pub ref_source_entering_temp: f64,
    /// Reference load flow (kg/s).
    pub ref_load_flow: f64,
    /// Reference source flow (kg/s).
    pub ref_source_flow: f64,
    /// Minimum part-load ratio.
    pub min_plr: f64,
}

impl EirHeatPump {
    pub fn new(
        name: impl Into<String>,
        capacity: f64,
        cop: f64,
        ref_load_flow: f64,
        ref_source_flow: f64,
    ) -> Self {
        Self {
            name: name.into(),
            ref_capacity: capacity,
            ref_cop: cop,
            ref_load_leaving_temp: 45.0,
            ref_source_entering_temp: 10.0,
            ref_load_flow,
            ref_source_flow,
            min_plr: 0.1,
        }
    }

    /// Calculate EIR heat pump performance.
    ///
    /// Uses the same 3-curve approach as the EIR chiller:
    ///   CapFTemp = f(T_load_leaving, T_source_entering)
    ///   EIRFTemp = f(T_load_leaving, T_source_entering)
    ///   EIRFPLR = f(PLR)
    pub fn calculate(
        &self,
        load_inlet_temp: f64,
        load_mass_flow: f64,
        source_inlet_temp: f64,
        source_mass_flow: f64,
        load: f64,
        cap_f_temp: &Curve,
        eir_f_temp: &Curve,
        eir_f_plr: &Curve,
    ) -> HeatPumpResult {
        let zero = HeatPumpResult {
            capacity: 0.0,
            power: 0.0,
            source_heat_rate: 0.0,
            load_outlet_temp: load_inlet_temp,
            source_outlet_temp: source_inlet_temp,
            cop: 0.0,
        };

        if load_mass_flow <= 1e-10 || load <= 0.0 || self.ref_capacity <= 0.0 {
            return zero;
        }

        let load_leaving = self.ref_load_leaving_temp;

        let cap_modifier = cap_f_temp
            .evaluate2(load_leaving, source_inlet_temp)
            .max(0.0);
        let available_capacity = self.ref_capacity * cap_modifier;

        if available_capacity <= 0.0 {
            return zero;
        }

        let plr = (load / available_capacity).clamp(0.0, 1.0);
        let actual_capacity = available_capacity * plr;

        let eir_rated = if self.ref_cop > 0.0 {
            1.0 / self.ref_cop
        } else {
            0.3
        };
        let eir_temp_modifier = eir_f_temp
            .evaluate2(load_leaving, source_inlet_temp)
            .max(0.0);
        let operating_plr = plr.max(self.min_plr);
        let eir_plr_modifier = eir_f_plr.evaluate1(operating_plr).max(0.0);

        let power = available_capacity * eir_rated * eir_temp_modifier * eir_plr_modifier
            * if plr < self.min_plr {
                plr / self.min_plr
            } else {
                1.0
            };

        // Source heat = capacity - power (energy balance for heating mode)
        let source_heat = actual_capacity - power;

        let cp_load = ep_psychrometrics::cp_water(load_inlet_temp);
        let load_outlet_temp = load_inlet_temp + actual_capacity / (load_mass_flow * cp_load);

        let source_outlet_temp = if source_mass_flow > 1e-10 {
            let cp_source = ep_psychrometrics::cp_water(source_inlet_temp);
            source_inlet_temp - source_heat / (source_mass_flow * cp_source)
        } else {
            source_inlet_temp
        };

        let cop = if power > 0.0 {
            actual_capacity / power
        } else {
            0.0
        };

        HeatPumpResult {
            capacity: actual_capacity,
            power,
            source_heat_rate: source_heat,
            load_outlet_temp,
            source_outlet_temp,
            cop,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── WaterToWaterHeatPump ───

    #[test]
    fn wwhp_basic() {
        let hp = WaterToWaterHeatPump::new("TestHP", 50_000.0, 4.0, 2.0, 3.0);
        assert!((hp.ref_capacity - 50_000.0).abs() < 1e-10);
        assert!((hp.ref_cop - 4.0).abs() < 1e-10);
    }

    #[test]
    fn wwhp_full_load() {
        let hp = WaterToWaterHeatPump::new("HP1", 50_000.0, 4.0, 2.0, 3.0);
        let result = hp.calculate(35.0, 2.0, 10.0, 3.0, 50_000.0);

        assert!((result.capacity - 50_000.0).abs() < 100.0, "Q={}", result.capacity);
        assert!(result.power > 0.0);
        assert!(result.load_outlet_temp > 35.0, "T_load_out={}", result.load_outlet_temp);
        assert!(result.cop > 0.0);
    }

    #[test]
    fn wwhp_energy_balance() {
        let hp = WaterToWaterHeatPump::new("HP", 50_000.0, 4.0, 2.0, 3.0);
        let result = hp.calculate(35.0, 2.0, 10.0, 3.0, 50_000.0);

        // Q_load = Q_source + W (heating mode energy balance)
        let balance = (result.capacity - result.source_heat_rate - result.power).abs();
        assert!(balance < 1.0, "balance={}", balance);
    }

    #[test]
    fn wwhp_part_load() {
        let hp = WaterToWaterHeatPump::new("HP", 50_000.0, 4.0, 2.0, 3.0);
        let result = hp.calculate(35.0, 2.0, 10.0, 3.0, 25_000.0);

        assert!((result.capacity - 25_000.0).abs() < 100.0, "Q={}", result.capacity);
        assert!(result.power < 50_000.0 / 4.0, "P should be less than full load");
    }

    #[test]
    fn wwhp_no_load() {
        let hp = WaterToWaterHeatPump::new("HP", 50_000.0, 4.0, 2.0, 3.0);
        let result = hp.calculate(35.0, 2.0, 10.0, 3.0, 0.0);
        assert!(result.capacity.abs() < 1e-10);
        assert!(result.power.abs() < 1e-10);
    }

    #[test]
    fn wwhp_no_flow() {
        let hp = WaterToWaterHeatPump::new("HP", 50_000.0, 4.0, 2.0, 3.0);
        let result = hp.calculate(35.0, 0.0, 10.0, 3.0, 50_000.0);
        assert!(result.capacity.abs() < 1e-10);
    }

    #[test]
    fn wwhp_source_cools() {
        let hp = WaterToWaterHeatPump::new("HP", 50_000.0, 4.0, 2.0, 3.0);
        let result = hp.calculate(35.0, 2.0, 10.0, 3.0, 50_000.0);

        // Source outlet should be colder (heat extracted from source)
        assert!(
            result.source_outlet_temp < 10.0,
            "T_source_out={}, expected < 10.0",
            result.source_outlet_temp
        );
    }

    // ─── EirHeatPump ───

    fn flat_curves() -> (Curve, Curve, Curve) {
        let cap_ft = Curve::biquadratic(1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let eir_ft = Curve::biquadratic(1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let eir_fplr = Curve::linear(0.0, 1.0);
        (cap_ft, eir_ft, eir_fplr)
    }

    #[test]
    fn eir_hp_full_load() {
        let hp = EirHeatPump::new("EIR-HP", 100_000.0, 3.5, 5.0, 5.0);
        let (cap_ft, eir_ft, eir_fplr) = flat_curves();

        let result = hp.calculate(40.0, 5.0, 10.0, 5.0, 100_000.0, &cap_ft, &eir_ft, &eir_fplr);

        assert!((result.capacity - 100_000.0).abs() < 100.0, "Q={}", result.capacity);
        assert!((result.cop - 3.5).abs() < 0.1, "COP={}", result.cop);
    }

    #[test]
    fn eir_hp_energy_balance() {
        let hp = EirHeatPump::new("EIR-HP", 100_000.0, 3.5, 5.0, 5.0);
        let (cap_ft, eir_ft, eir_fplr) = flat_curves();

        let result = hp.calculate(40.0, 5.0, 10.0, 5.0, 80_000.0, &cap_ft, &eir_ft, &eir_fplr);

        let balance = (result.capacity - result.source_heat_rate - result.power).abs();
        assert!(balance < 1.0, "balance={}", balance);
    }

    #[test]
    fn eir_hp_cop_varies_with_source_temp() {
        let hp = EirHeatPump::new("EIR-HP", 100_000.0, 3.5, 5.0, 5.0);
        // EIR increases with lower source temp
        let eir_ft = Curve::biquadratic(1.0, 0.0, 0.0, -0.01, 0.0, 0.0);
        let cap_ft = Curve::biquadratic(1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let eir_fplr = Curve::linear(0.0, 1.0);

        let warm = hp.calculate(40.0, 5.0, 15.0, 5.0, 80_000.0, &cap_ft, &eir_ft, &eir_fplr);
        let cold = hp.calculate(40.0, 5.0, 0.0, 5.0, 80_000.0, &cap_ft, &eir_ft, &eir_fplr);

        // Lower source temp → higher EIR → lower COP
        assert!(
            cold.cop < warm.cop,
            "COP_cold={} should < COP_warm={}",
            cold.cop,
            warm.cop
        );
    }
}
