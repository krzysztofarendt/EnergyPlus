//! Electric chiller model using the EIR (Energy Input Ratio) method.
//!
//! Three performance curves:
//! - CapFTemp: capacity modifier = f(T_evap_leaving, T_cond_entering)
//! - EIRFTemp: EIR modifier = f(T_evap_leaving, T_cond_entering)
//! - EIRFPLR: EIR modifier = f(PLR)

use ep_curves::Curve;

/// Chiller specification.
#[derive(Debug, Clone)]
pub struct Chiller {
    pub name: String,
    /// Reference cooling capacity (W).
    pub ref_capacity: f64,
    /// Reference COP (W/W).
    pub ref_cop: f64,
    /// Reference evaporator leaving water temperature (C).
    pub ref_evap_leaving_temp: f64,
    /// Reference condenser entering water temperature (C).
    pub ref_cond_entering_temp: f64,
    /// Reference evaporator water flow rate (kg/s).
    pub ref_evap_flow: f64,
    /// Reference condenser water flow rate (kg/s).
    pub ref_cond_flow: f64,
    /// Minimum part-load ratio.
    pub min_plr: f64,
    /// Minimum unloading ratio (below this, chiller cycles).
    pub min_unload_ratio: f64,
    /// Maximum part-load ratio.
    pub max_plr: f64,
}

/// Chiller calculation result.
#[derive(Debug, Clone, Copy)]
pub struct ChillerResult {
    /// Evaporator cooling rate (W, positive = cooling water).
    pub evap_cooling_rate: f64,
    /// Compressor electrical power (W).
    pub power: f64,
    /// Condenser heat rejection rate (W).
    pub cond_heat_rate: f64,
    /// Evaporator leaving water temperature (C).
    pub evap_outlet_temp: f64,
    /// Condenser leaving water temperature (C).
    pub cond_outlet_temp: f64,
    /// Part-load ratio.
    pub part_load_ratio: f64,
    /// Cycling ratio (for operation below min unloading ratio).
    pub cycling_ratio: f64,
    /// Operating COP.
    pub cop: f64,
}

impl Chiller {
    pub fn new(
        name: impl Into<String>,
        capacity: f64,
        cop: f64,
        ref_evap_flow: f64,
        ref_cond_flow: f64,
    ) -> Self {
        Self {
            name: name.into(),
            ref_capacity: capacity,
            ref_cop: cop,
            ref_evap_leaving_temp: 6.67,
            ref_cond_entering_temp: 29.44,
            ref_evap_flow,
            ref_cond_flow,
            min_plr: 0.1,
            min_unload_ratio: 0.1,
            max_plr: 1.0,
        }
    }

    /// Calculate chiller performance at current operating conditions.
    ///
    /// `load` is the cooling load (W, positive = cooling needed).
    /// Curves modify rated capacity and EIR based on temperatures and PLR.
    pub fn calculate(
        &self,
        evap_inlet_temp: f64,
        evap_mass_flow: f64,
        cond_inlet_temp: f64,
        cond_mass_flow: f64,
        load: f64,
        cap_f_temp: &Curve,
        eir_f_temp: &Curve,
        eir_f_plr: &Curve,
    ) -> ChillerResult {
        if evap_mass_flow <= 1e-10 || load <= 0.0 || self.ref_capacity <= 0.0 {
            return ChillerResult {
                evap_cooling_rate: 0.0,
                power: 0.0,
                cond_heat_rate: 0.0,
                evap_outlet_temp: evap_inlet_temp,
                cond_outlet_temp: cond_inlet_temp,
                part_load_ratio: 0.0,
                cycling_ratio: 0.0,
                cop: 0.0,
            };
        }

        // Evaporator leaving temp target (use design as approximation)
        let evap_leaving_temp = self.ref_evap_leaving_temp;

        // Available capacity at operating temperatures
        let cap_modifier = cap_f_temp.evaluate2(evap_leaving_temp, cond_inlet_temp).max(0.0);
        let available_capacity = self.ref_capacity * cap_modifier;

        if available_capacity <= 0.0 {
            return ChillerResult {
                evap_cooling_rate: 0.0,
                power: 0.0,
                cond_heat_rate: 0.0,
                evap_outlet_temp: evap_inlet_temp,
                cond_outlet_temp: cond_inlet_temp,
                part_load_ratio: 0.0,
                cycling_ratio: 0.0,
                cop: 0.0,
            };
        }

        // Part-load ratio
        let plr = (load / available_capacity).clamp(0.0, self.max_plr);

        // Cycling ratio for low PLR operation
        let (operating_plr, cycling_ratio) = if plr < self.min_unload_ratio {
            // Below min unloading: chiller cycles
            (self.min_unload_ratio, plr / self.min_unload_ratio)
        } else {
            (plr, 1.0)
        };

        // Actual evaporator cooling
        let evap_cooling = available_capacity * plr;

        // EIR calculation
        let eir_rated = if self.ref_cop > 0.0 { 1.0 / self.ref_cop } else { 0.3 };
        let eir_temp_modifier = eir_f_temp.evaluate2(evap_leaving_temp, cond_inlet_temp).max(0.0);
        let eir_plr_modifier = eir_f_plr.evaluate1(operating_plr).max(0.0);

        // Compressor power
        let power = available_capacity * eir_rated * eir_temp_modifier * eir_plr_modifier * cycling_ratio;

        // Condenser heat rejection = evaporator cooling + compressor power
        let cond_heat = evap_cooling + power;

        // Evaporator outlet temperature
        let cp_evap = ep_psychrometrics::cp_water(evap_inlet_temp);
        let evap_outlet_temp = evap_inlet_temp - evap_cooling / (evap_mass_flow * cp_evap);

        // Condenser outlet temperature
        let cond_outlet_temp = if cond_mass_flow > 1e-10 {
            let cp_cond = ep_psychrometrics::cp_water(cond_inlet_temp);
            cond_inlet_temp + cond_heat / (cond_mass_flow * cp_cond)
        } else {
            cond_inlet_temp
        };

        let cop = if power > 0.0 { evap_cooling / power } else { 0.0 };

        ChillerResult {
            evap_cooling_rate: evap_cooling,
            power,
            cond_heat_rate: cond_heat,
            evap_outlet_temp,
            cond_outlet_temp,
            part_load_ratio: plr,
            cycling_ratio,
            cop,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_curves() -> (Curve, Curve, Curve) {
        // All modifiers = 1.0 at all conditions
        let cap_ft = Curve::biquadratic(1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let eir_ft = Curve::biquadratic(1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let eir_fplr = Curve::linear(0.0, 1.0); // EIR proportional to PLR
        (cap_ft, eir_ft, eir_fplr)
    }

    #[test]
    fn chiller_basic() {
        let ch = Chiller::new("Test Chiller", 500_000.0, 5.0, 20.0, 25.0);
        assert!((ch.ref_capacity - 500_000.0).abs() < 1e-10);
        assert!((ch.ref_cop - 5.0).abs() < 1e-10);
    }

    #[test]
    fn chiller_full_load() {
        let ch = Chiller::new("Test", 500_000.0, 5.0, 20.0, 25.0);
        let (cap_ft, eir_ft, eir_fplr) = flat_curves();

        let result = ch.calculate(
            12.0, 20.0,   // evap
            30.0, 25.0,   // cond
            500_000.0,     // full load
            &cap_ft, &eir_ft, &eir_fplr,
        );

        assert!((result.evap_cooling_rate - 500_000.0).abs() < 100.0,
                "Q={}", result.evap_cooling_rate);
        assert!((result.part_load_ratio - 1.0).abs() < 0.01);
        assert!(result.power > 0.0);
        assert!(result.evap_outlet_temp < 12.0, "T_evap_out={}", result.evap_outlet_temp);
        assert!(result.cond_outlet_temp > 30.0, "T_cond_out={}", result.cond_outlet_temp);
    }

    #[test]
    fn chiller_part_load() {
        let ch = Chiller::new("Test", 500_000.0, 5.0, 20.0, 25.0);
        let (cap_ft, eir_ft, eir_fplr) = flat_curves();

        let result = ch.calculate(
            12.0, 20.0, 30.0, 25.0,
            250_000.0, // half load
            &cap_ft, &eir_ft, &eir_fplr,
        );

        assert!((result.part_load_ratio - 0.5).abs() < 0.01, "PLR={}", result.part_load_ratio);
        assert!((result.evap_cooling_rate - 250_000.0).abs() < 100.0);
    }

    #[test]
    fn chiller_no_load() {
        let ch = Chiller::new("Test", 500_000.0, 5.0, 20.0, 25.0);
        let (cap_ft, eir_ft, eir_fplr) = flat_curves();

        let result = ch.calculate(12.0, 20.0, 30.0, 25.0, 0.0,
            &cap_ft, &eir_ft, &eir_fplr);
        assert!(result.evap_cooling_rate.abs() < 1e-10);
        assert!(result.power.abs() < 1e-10);
    }

    #[test]
    fn chiller_energy_balance() {
        let ch = Chiller::new("Test", 500_000.0, 5.0, 20.0, 25.0);
        let (cap_ft, eir_ft, eir_fplr) = flat_curves();

        let result = ch.calculate(
            12.0, 20.0, 30.0, 25.0, 500_000.0,
            &cap_ft, &eir_ft, &eir_fplr,
        );

        // Q_cond = Q_evap + W_comp
        let balance = (result.cond_heat_rate - result.evap_cooling_rate - result.power).abs();
        assert!(balance < 1.0, "balance={}", balance);
    }

    #[test]
    fn chiller_cycling_below_min_unload() {
        let mut ch = Chiller::new("Test", 500_000.0, 5.0, 20.0, 25.0);
        ch.min_unload_ratio = 0.2;
        let (cap_ft, eir_ft, eir_fplr) = flat_curves();

        // Request 5% load — should trigger cycling
        let result = ch.calculate(
            12.0, 20.0, 30.0, 25.0,
            25_000.0, // 5% of capacity
            &cap_ft, &eir_ft, &eir_fplr,
        );

        assert!(result.cycling_ratio < 1.0, "cycling={}", result.cycling_ratio);
        assert!((result.part_load_ratio - 0.05).abs() < 0.01, "PLR={}", result.part_load_ratio);
    }

    #[test]
    fn chiller_cop() {
        let ch = Chiller::new("Test", 500_000.0, 5.0, 20.0, 25.0);
        let (cap_ft, eir_ft, eir_fplr) = flat_curves();

        let result = ch.calculate(
            12.0, 20.0, 30.0, 25.0, 500_000.0,
            &cap_ft, &eir_ft, &eir_fplr,
        );

        // With flat curves and EIR_FPLR = PLR, at full load:
        // EIR = 1/5 * 1.0 * 1.0 = 0.2
        // Power = 500000 * 0.2 * 1.0 (cycling) = 100000
        // COP = 500000/100000 = 5.0
        assert!((result.cop - 5.0).abs() < 0.1, "COP={}", result.cop);
    }

    #[test]
    fn chiller_no_flow() {
        let ch = Chiller::new("Test", 500_000.0, 5.0, 20.0, 25.0);
        let (cap_ft, eir_ft, eir_fplr) = flat_curves();

        // Zero evaporator flow → early return, zero cooling and power
        let result = ch.calculate(
            12.0, 0.0, 30.0, 25.0, 500_000.0,
            &cap_ft, &eir_ft, &eir_fplr,
        );

        assert!(result.evap_cooling_rate.abs() < 1e-10, "Q={}", result.evap_cooling_rate);
        assert!(result.power.abs() < 1e-10, "P={}", result.power);
        assert!(
            (result.evap_outlet_temp - 12.0).abs() < 1e-10,
            "T_evap_out={}",
            result.evap_outlet_temp
        );
    }

    #[test]
    fn chiller_realistic_curves() {
        let ch = Chiller::new("Realistic", 500_000.0, 5.0, 20.0, 25.0);

        // Non-flat biquadratic for capacity: decreases with higher condenser temp
        // CapFTemp = 1.0 + 0.0 * T_evap + 0.0 * T_evap^2 - 0.005 * T_cond + 0.0 * T_cond^2 + 0.0 * T_evap*T_cond
        let cap_ft = Curve::biquadratic(1.0, 0.0, 0.0, -0.005, 0.0, 0.0);
        // EIRFTemp: increases with condenser temp
        let eir_ft = Curve::biquadratic(0.8, 0.0, 0.0, 0.007, 0.0, 0.0);
        let eir_fplr = Curve::linear(0.0, 1.0);

        // Case 1: reference condenser entering = 29.44 C
        let result_ref = ch.calculate(
            12.0, 20.0, 29.44, 25.0, 400_000.0,
            &cap_ft, &eir_ft, &eir_fplr,
        );

        // Case 2: higher condenser temp = 40.0 C
        let result_hot = ch.calculate(
            12.0, 20.0, 40.0, 25.0, 400_000.0,
            &cap_ft, &eir_ft, &eir_fplr,
        );

        // At higher condenser temp, capacity decreases and EIR increases → COP drops
        assert!(
            result_hot.cop < result_ref.cop,
            "COP_hot={} should be less than COP_ref={}",
            result_hot.cop,
            result_ref.cop
        );
    }

    #[test]
    fn chiller_condenser_no_flow() {
        let ch = Chiller::new("Test", 500_000.0, 5.0, 20.0, 25.0);
        let (cap_ft, eir_ft, eir_fplr) = flat_curves();

        // Zero condenser flow → condenser outlet = condenser inlet
        let result = ch.calculate(
            12.0, 20.0, 30.0, 0.0, 500_000.0,
            &cap_ft, &eir_ft, &eir_fplr,
        );

        assert!(
            (result.cond_outlet_temp - 30.0).abs() < 1e-10,
            "T_cond_out={}, expected 30.0",
            result.cond_outlet_temp
        );
    }
}
