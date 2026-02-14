//! Compressor model for refrigeration systems.
//!
//! Models scroll, reciprocating, and screw compressors using
//! polynomial curve fits of capacity and power vs. suction/discharge
//! temperatures (ARI Standard 540 format).

use ep_curves::Curve;

/// Compressor type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressorType {
    Reciprocating,
    Scroll,
    Screw,
    Rotary,
}

/// Single compressor model.
#[derive(Debug, Clone)]
pub struct Compressor {
    pub name: String,
    pub compressor_type: CompressorType,
    /// Rated capacity (W) at rated conditions.
    pub rated_capacity: f64,
    /// Rated power input (W).
    pub rated_power: f64,
    /// Rated suction temperature (C).
    pub rated_suction_temp: f64,
    /// Rated discharge (condensing) temperature (C).
    pub rated_discharge_temp: f64,
    /// Capacity as function of suction and discharge temps.
    /// Biquadratic: f(T_suction, T_discharge).
    pub capacity_curve: Curve,
    /// Power as function of suction and discharge temps.
    pub power_curve: Curve,
    /// Superheat (K) at compressor suction.
    pub suction_superheat: f64,
    /// Subcooling (K) at condenser exit.
    pub subcooling: f64,
}

/// Compressor operating result.
#[derive(Debug, Clone, Copy, Default)]
pub struct CompressorResult {
    /// Actual cooling capacity (W).
    pub capacity: f64,
    /// Electrical power input (W).
    pub power: f64,
    /// COP at current conditions.
    pub cop: f64,
    /// Heat rejection to condenser (W).
    pub heat_rejection: f64,
    /// Mass flow rate of refrigerant (kg/s), if available.
    pub mass_flow: f64,
}

impl Compressor {
    /// Create a compressor with linear capacity/power curves (simple model).
    pub fn new(name: impl Into<String>, rated_capacity: f64, rated_power: f64) -> Self {
        Self {
            name: name.into(),
            compressor_type: CompressorType::Scroll,
            rated_capacity,
            rated_power,
            rated_suction_temp: -6.7,
            rated_discharge_temp: 35.0,
            // Default: capacity and power scale linearly with conditions
            capacity_curve: Curve::biquadratic(1.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            power_curve: Curve::biquadratic(1.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            suction_superheat: 4.0,
            subcooling: 3.0,
        }
    }

    /// Calculate compressor performance.
    ///
    /// `suction_temp` — evaporating/suction temperature (C).
    /// `discharge_temp` — condensing/discharge temperature (C).
    /// `load_fraction` — part-load ratio (0-1).
    pub fn calculate(
        &self,
        suction_temp: f64,
        discharge_temp: f64,
        load_fraction: f64,
    ) -> CompressorResult {
        let load_fraction = load_fraction.clamp(0.0, 1.0);
        if load_fraction < 1e-10 {
            return CompressorResult::default();
        }

        // Evaluate curve corrections
        let cap_correction = self.capacity_curve.evaluate(&[suction_temp, discharge_temp]);
        let pwr_correction = self.power_curve.evaluate(&[suction_temp, discharge_temp]);

        let capacity = self.rated_capacity * cap_correction * load_fraction;
        let power = self.rated_power * pwr_correction * load_fraction;

        let cop = if power > 0.0 { capacity / power } else { 0.0 };
        let heat_rejection = capacity + power;

        CompressorResult {
            capacity,
            power,
            cop,
            heat_rejection,
            mass_flow: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressor_at_rated() {
        let comp = Compressor::new("Comp-1", 10000.0, 3000.0);
        let result = comp.calculate(-6.7, 35.0, 1.0);

        assert!((result.capacity - 10000.0).abs() < 1.0);
        assert!((result.power - 3000.0).abs() < 1.0);
        assert!((result.cop - 10000.0 / 3000.0).abs() < 0.01);
        assert!((result.heat_rejection - 13000.0).abs() < 1.0);
    }

    #[test]
    fn compressor_part_load() {
        let comp = Compressor::new("Comp-1", 10000.0, 3000.0);
        let result = comp.calculate(-6.7, 35.0, 0.5);

        assert!((result.capacity - 5000.0).abs() < 1.0);
        assert!((result.power - 1500.0).abs() < 1.0);
    }

    #[test]
    fn compressor_zero_load() {
        let comp = Compressor::new("Comp-1", 10000.0, 3000.0);
        let result = comp.calculate(-6.7, 35.0, 0.0);

        assert!(result.capacity.abs() < 1e-10);
        assert!(result.power.abs() < 1e-10);
    }

    #[test]
    fn compressor_with_biquadratic() {
        // Capacity increases with suction temp, decreases with discharge temp
        let mut comp = Compressor::new("Comp-2", 10000.0, 3000.0);
        comp.capacity_curve = Curve::biquadratic(1.0, 0.02, 0.0, -0.01, 0.0, 0.0);

        let rated = comp.calculate(-6.7, 35.0, 1.0);
        let warm_suction = comp.calculate(0.0, 35.0, 1.0);

        // Warmer suction → more capacity
        assert!(warm_suction.capacity > rated.capacity);
    }
}
