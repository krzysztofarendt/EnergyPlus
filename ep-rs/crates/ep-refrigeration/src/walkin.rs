//! Walk-in cooler/freezer model.
//!
//! Models insulated walk-in boxes with door openings, lighting, fan,
//! defrost heaters, and floor heating.

/// Walk-in cooler or freezer.
#[derive(Debug, Clone)]
pub struct WalkIn {
    pub name: String,
    /// Rated cooling capacity (W).
    pub rated_capacity: f64,
    /// Operating temperature (C).
    pub operating_temp: f64,
    /// Source temperature for capacity rating (C).
    pub source_temp: f64,
    /// Rated cooling source temperature (typically suction temp).
    pub rated_source_temp: f64,
    /// Total insulated surface area (m²).
    pub insulated_area: f64,
    /// Insulated surface U-value (W/m²·K).
    pub u_value: f64,
    /// Floor area (m²).
    pub floor_area: f64,
    /// Floor U-value (W/m²·K) — includes ground coupling.
    pub floor_u_value: f64,
    /// Door area (m²).
    pub door_area: f64,
    /// Door U-value (W/m²·K).
    pub door_u_value: f64,
    /// Door open fraction (0-1).
    pub door_open_fraction: f64,
    /// Infiltration rate when door is open (m³/s per m² door area).
    pub door_flow_rate: f64,
    /// Fan power (W).
    pub fan_power: f64,
    /// Lighting power (W).
    pub lighting_power: f64,
    /// Defrost power (W).
    pub defrost_power: f64,
    /// Defrost schedule fraction (0-1).
    pub defrost_fraction: f64,
    /// Heater power to prevent floor freezing (W).
    pub floor_heater_power: f64,
    /// Miscellaneous equipment load (W).
    pub equipment_load: f64,
}

/// Walk-in calculation result.
#[derive(Debug, Clone, Copy, Default)]
pub struct WalkInResult {
    /// Total cooling load (W).
    pub total_cooling_load: f64,
    /// Transmission load through walls/ceiling (W).
    pub transmission_load: f64,
    /// Door infiltration load (W).
    pub infiltration_load: f64,
    /// Floor conduction load (W).
    pub floor_load: f64,
    /// Internal heat gains (fans + lights + equipment) (W).
    pub internal_gains: f64,
    /// Defrost heat load (W).
    pub defrost_load: f64,
    /// Fan electric power (W).
    pub fan_power: f64,
    /// Lighting electric power (W).
    pub lighting_power: f64,
}

impl WalkIn {
    pub fn new(name: impl Into<String>, rated_capacity: f64, operating_temp: f64) -> Self {
        Self {
            name: name.into(),
            rated_capacity,
            operating_temp,
            source_temp: -6.7,
            rated_source_temp: -6.7,
            insulated_area: 30.0,
            u_value: 0.235,
            floor_area: 9.3,
            floor_u_value: 0.207,
            door_area: 2.0,
            door_u_value: 0.5,
            door_open_fraction: 0.02,
            door_flow_rate: 0.3,
            fan_power: 375.0,
            lighting_power: 120.0,
            defrost_power: 5000.0,
            defrost_fraction: 0.0,
            floor_heater_power: 0.0,
            equipment_load: 0.0,
        }
    }

    /// Calculate walk-in loads.
    ///
    /// `zone_temp` — surrounding zone temperature (C).
    /// `ground_temp` — ground surface temperature (C).
    pub fn calculate(&self, zone_temp: f64, ground_temp: f64) -> WalkInResult {
        // Transmission load through walls/ceiling
        let transmission = self.insulated_area * self.u_value * (zone_temp - self.operating_temp);

        // Door conduction + infiltration
        let door_cond = self.door_area * self.door_u_value * (zone_temp - self.operating_temp);
        let dt = (zone_temp - self.operating_temp).max(0.0);
        // Simplified infiltration: sensible only, proportional to dt
        let cp_air = 1005.0; // J/(kg·K)
        let rho_air = 1.2; // kg/m³
        let infiltration = self.door_area * self.door_open_fraction
            * self.door_flow_rate * rho_air * cp_air * dt;
        let total_infiltration = door_cond + infiltration;

        // Floor load
        let floor_load = self.floor_area * self.floor_u_value
            * (ground_temp - self.operating_temp);

        // Internal heat gains
        let internal_gains = self.fan_power + self.lighting_power
            + self.equipment_load + self.floor_heater_power;

        // Defrost load
        let defrost_load = self.defrost_power * self.defrost_fraction;

        // Total cooling load
        let total = transmission + total_infiltration + floor_load
            + internal_gains + defrost_load;

        WalkInResult {
            total_cooling_load: total.max(0.0),
            transmission_load: transmission,
            infiltration_load: total_infiltration,
            floor_load,
            internal_gains,
            defrost_load,
            fan_power: self.fan_power,
            lighting_power: self.lighting_power,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walkin_basic_loads() {
        let walkin = WalkIn::new("Cooler-1", 5000.0, 2.0);
        let result = walkin.calculate(24.0, 18.0);

        assert!(result.total_cooling_load > 0.0);
        assert!(result.transmission_load > 0.0);
        assert!(result.infiltration_load > 0.0);
        assert!(result.floor_load > 0.0);
        assert!(result.internal_gains > 0.0);
    }

    #[test]
    fn walkin_freezer() {
        let walkin = WalkIn::new("Freezer-1", 10000.0, -23.0);
        let result = walkin.calculate(24.0, 18.0);

        // Freezer has much higher loads due to larger delta-T
        let cooler = WalkIn::new("Cooler-1", 5000.0, 2.0);
        let cooler_result = cooler.calculate(24.0, 18.0);
        assert!(result.transmission_load > cooler_result.transmission_load);
    }

    #[test]
    fn walkin_defrost_adds_load() {
        let mut walkin = WalkIn::new("Freezer-1", 10000.0, -23.0);
        let no_defrost = walkin.calculate(24.0, 18.0);

        walkin.defrost_fraction = 0.1;
        let with_defrost = walkin.calculate(24.0, 18.0);

        assert!(with_defrost.total_cooling_load > no_defrost.total_cooling_load);
        assert!((with_defrost.defrost_load - 500.0).abs() < 1.0); // 5000 * 0.1
    }

    #[test]
    fn walkin_no_infiltration_when_cold() {
        let walkin = WalkIn::new("Cold-Room", 5000.0, 24.0);
        // Zone temp equals operating temp → no driving force
        let result = walkin.calculate(24.0, 18.0);
        assert!(result.infiltration_load.abs() < 1.0);
    }
}
