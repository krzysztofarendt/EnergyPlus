//! Sizing calculations for zones, systems, and plants.
//!
//! Collects peak loads and flows during sizing design days to
//! determine equipment capacities.

/// Zone sizing data collected during sizing design days.
#[derive(Debug, Clone)]
pub struct ZoneSizingData {
    pub zone_name: String,
    /// Design cooling load (W).
    pub design_cool_load: f64,
    /// Design heating load (W).
    pub design_heat_load: f64,
    /// Design cooling airflow (m³/s).
    pub design_cool_airflow: f64,
    /// Design heating airflow (m³/s).
    pub design_heat_airflow: f64,
    /// Peak cooling supply air temperature (C).
    pub cool_supply_temp: f64,
    /// Peak heating supply air temperature (C).
    pub heat_supply_temp: f64,
    /// Peak cooling outdoor temp (C).
    pub cool_outdoor_temp: f64,
    /// Peak heating outdoor temp (C).
    pub heat_outdoor_temp: f64,
    /// Time of cooling peak.
    pub cool_peak_hour: u8,
    pub cool_peak_day: u8,
    pub cool_peak_month: u8,
    /// Time of heating peak.
    pub heat_peak_hour: u8,
    pub heat_peak_day: u8,
    pub heat_peak_month: u8,
    /// Sizing method: DesignDay, DesignDayWithLimit, SensibleLoad, Latent
    pub sizing_method: SizingMethod,
    /// Safety factor applied to design loads.
    pub sizing_factor: f64,
}

/// Sizing method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizingMethod {
    /// Size based on design day conditions.
    DesignDay,
    /// Size on design day with max flow limit.
    DesignDayWithLimit,
    /// Size on sensible load only.
    SensibleLoad,
    /// Size on ventilation requirement.
    VentilationRequirement,
}

impl ZoneSizingData {
    pub fn new(zone_name: impl Into<String>) -> Self {
        Self {
            zone_name: zone_name.into(),
            design_cool_load: 0.0,
            design_heat_load: 0.0,
            design_cool_airflow: 0.0,
            design_heat_airflow: 0.0,
            cool_supply_temp: 14.0,
            heat_supply_temp: 40.0,
            cool_outdoor_temp: 0.0,
            heat_outdoor_temp: 0.0,
            cool_peak_hour: 0,
            cool_peak_day: 0,
            cool_peak_month: 0,
            heat_peak_hour: 0,
            heat_peak_day: 0,
            heat_peak_month: 0,
            sizing_method: SizingMethod::DesignDay,
            sizing_factor: 1.0,
        }
    }

    /// Update with a cooling load reading (keep the peak).
    pub fn update_cooling(
        &mut self,
        load: f64,
        airflow: f64,
        outdoor_temp: f64,
        month: u8,
        day: u8,
        hour: u8,
    ) {
        if load > self.design_cool_load {
            self.design_cool_load = load;
            self.design_cool_airflow = airflow;
            self.cool_outdoor_temp = outdoor_temp;
            self.cool_peak_month = month;
            self.cool_peak_day = day;
            self.cool_peak_hour = hour;
        }
    }

    /// Update with a heating load reading.
    pub fn update_heating(
        &mut self,
        load: f64,
        airflow: f64,
        outdoor_temp: f64,
        month: u8,
        day: u8,
        hour: u8,
    ) {
        if load > self.design_heat_load {
            self.design_heat_load = load;
            self.design_heat_airflow = airflow;
            self.heat_outdoor_temp = outdoor_temp;
            self.heat_peak_month = month;
            self.heat_peak_day = day;
            self.heat_peak_hour = hour;
        }
    }

    /// Final design loads with sizing factor applied.
    pub fn final_cool_load(&self) -> f64 {
        self.design_cool_load * self.sizing_factor
    }

    pub fn final_heat_load(&self) -> f64 {
        self.design_heat_load * self.sizing_factor
    }

    pub fn final_cool_airflow(&self) -> f64 {
        self.design_cool_airflow * self.sizing_factor
    }

    pub fn final_heat_airflow(&self) -> f64 {
        self.design_heat_airflow * self.sizing_factor
    }
}

/// System (air loop) sizing data.
#[derive(Debug, Clone)]
pub struct SystemSizingData {
    pub system_name: String,
    /// Design cooling capacity (W).
    pub design_cool_capacity: f64,
    /// Design heating capacity (W).
    pub design_heat_capacity: f64,
    /// Design cooling airflow (m³/s).
    pub design_cool_airflow: f64,
    /// Design heating airflow (m³/s).
    pub design_heat_airflow: f64,
    /// Design outside air fraction.
    pub design_oa_fraction: f64,
    /// Sizing factor.
    pub sizing_factor: f64,
}

impl SystemSizingData {
    pub fn new(system_name: impl Into<String>) -> Self {
        Self {
            system_name: system_name.into(),
            design_cool_capacity: 0.0,
            design_heat_capacity: 0.0,
            design_cool_airflow: 0.0,
            design_heat_airflow: 0.0,
            design_oa_fraction: 0.0,
            sizing_factor: 1.0,
        }
    }

    /// Size from zone sizing data (sum of zones served).
    pub fn size_from_zones(&mut self, zones: &[ZoneSizingData]) {
        self.design_cool_capacity = zones.iter().map(|z| z.final_cool_load()).sum();
        self.design_heat_capacity = zones.iter().map(|z| z.final_heat_load()).sum();
        self.design_cool_airflow = zones.iter().map(|z| z.final_cool_airflow()).sum();
        self.design_heat_airflow = zones.iter().map(|z| z.final_heat_airflow()).sum();
    }

    pub fn final_cool_capacity(&self) -> f64 {
        self.design_cool_capacity * self.sizing_factor
    }

    pub fn final_heat_capacity(&self) -> f64 {
        self.design_heat_capacity * self.sizing_factor
    }
}

/// Plant loop sizing data.
#[derive(Debug, Clone)]
pub struct PlantSizingData {
    pub plant_name: String,
    /// Loop type.
    pub loop_type: PlantLoopType,
    /// Design loop delta T (C).
    pub design_delta_t: f64,
    /// Design flow rate (m³/s).
    pub design_flow_rate: f64,
    /// Design exit temperature (C).
    pub design_exit_temp: f64,
    /// Sizing factor.
    pub sizing_factor: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlantLoopType {
    Heating,
    Cooling,
    Condenser,
    Steam,
}

impl PlantSizingData {
    pub fn new(plant_name: impl Into<String>, loop_type: PlantLoopType) -> Self {
        Self {
            plant_name: plant_name.into(),
            loop_type,
            design_delta_t: match loop_type {
                PlantLoopType::Heating => 11.0,  // 11°C typical
                PlantLoopType::Cooling => 5.0,   // 5°C typical
                PlantLoopType::Condenser => 5.5, // 5.5°C typical
                PlantLoopType::Steam => 100.0,   // placeholder
            },
            design_flow_rate: 0.0,
            design_exit_temp: match loop_type {
                PlantLoopType::Heating => 82.0,  // 82°C typical
                PlantLoopType::Cooling => 7.0,   // 7°C typical
                PlantLoopType::Condenser => 29.0, // 29°C typical
                PlantLoopType::Steam => 100.0,
            },
            sizing_factor: 1.0,
        }
    }

    /// Calculate design flow from total capacity and delta T.
    /// Q = m_dot * cp * delta_T → m_dot = Q / (cp * delta_T)
    /// Then convert to volumetric: V_dot = m_dot / rho
    pub fn calculate_flow_from_capacity(&mut self, total_capacity: f64) {
        let cp = 4186.0; // J/(kg·K) water
        let rho = 998.0; // kg/m³ water
        if self.design_delta_t.abs() > 0.1 {
            let mass_flow = total_capacity / (cp * self.design_delta_t);
            self.design_flow_rate = mass_flow / rho;
        }
    }

    pub fn final_flow_rate(&self) -> f64 {
        self.design_flow_rate * self.sizing_factor
    }
}

/// Sentinel value indicating a field should be autosized.
pub const AUTOSIZE: f64 = -99999.0;

/// Check if a value is the autosize sentinel.
pub fn is_autosized(value: f64) -> bool {
    (value - AUTOSIZE).abs() < 1.0
}

/// Calculate design airflow from a peak sensible load.
///
/// V_dot = Q / (rho * Cp * (T_supply - T_zone))
///
/// # Arguments
/// * `peak_load` - Peak sensible load (W), positive
/// * `supply_temp` - Design supply air temperature (C)
/// * `zone_temp` - Zone design temperature (C)
/// * `humidity_ratio` - Design humidity ratio (kg/kg)
pub fn design_airflow_from_load(
    peak_load: f64,
    supply_temp: f64,
    zone_temp: f64,
    humidity_ratio: f64,
) -> f64 {
    let rho = 1.204; // kg/m³ at standard conditions
    let cp = 1006.0 + 1860.0 * humidity_ratio; // J/(kg·K) moist air
    let dt = (supply_temp - zone_temp).abs();
    if dt < 0.1 {
        return 0.0;
    }
    peak_load.abs() / (rho * cp * dt)
}

/// Resolve zone sizes by applying sizing factors and computing design airflows.
pub fn resolve_zone_sizes(zones: &mut [ZoneSizingData]) {
    for zone in zones.iter_mut() {
        // Compute airflow from load if not already set
        if zone.design_cool_airflow < 1e-10 && zone.design_cool_load > 0.0 {
            zone.design_cool_airflow = design_airflow_from_load(
                zone.design_cool_load,
                zone.cool_supply_temp,
                26.0, // assume cooling zone temp
                0.008,
            );
        }
        if zone.design_heat_airflow < 1e-10 && zone.design_heat_load > 0.0 {
            zone.design_heat_airflow = design_airflow_from_load(
                zone.design_heat_load,
                zone.heat_supply_temp,
                20.0, // assume heating zone temp
                0.008,
            );
        }
    }
}

/// Aggregate sizing result from all sizing data.
#[derive(Debug, Clone, Default)]
pub struct SizingResult {
    pub zone_sizes: Vec<ZoneSizingData>,
    pub system_sizes: Vec<SystemSizingData>,
    pub plant_sizes: Vec<PlantSizingData>,
    /// Total design cooling capacity across all zones (W).
    pub total_cool_capacity: f64,
    /// Total design heating capacity across all zones (W).
    pub total_heat_capacity: f64,
}

impl SizingResult {
    /// Build from collected sizing data.
    pub fn from_zones(zones: Vec<ZoneSizingData>) -> Self {
        let total_cool = zones.iter().map(|z| z.final_cool_load()).sum();
        let total_heat = zones.iter().map(|z| z.final_heat_load()).sum();
        Self {
            zone_sizes: zones,
            system_sizes: Vec::new(),
            plant_sizes: Vec::new(),
            total_cool_capacity: total_cool,
            total_heat_capacity: total_heat,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_sizing_peak_tracking() {
        let mut zs = ZoneSizingData::new("Zone1");
        zs.update_cooling(5000.0, 0.5, 35.0, 7, 21, 14);
        zs.update_cooling(6000.0, 0.6, 36.0, 7, 21, 15);
        zs.update_cooling(4000.0, 0.4, 34.0, 7, 21, 16);

        assert!((zs.design_cool_load - 6000.0).abs() < 1.0);
        assert_eq!(zs.cool_peak_hour, 15);
    }

    #[test]
    fn zone_sizing_factor() {
        let mut zs = ZoneSizingData::new("Zone1");
        zs.design_cool_load = 10000.0;
        zs.design_cool_airflow = 1.0;
        zs.sizing_factor = 1.15;

        assert!((zs.final_cool_load() - 11500.0).abs() < 1.0);
        assert!((zs.final_cool_airflow() - 1.15).abs() < 0.01);
    }

    #[test]
    fn system_sizing_from_zones() {
        let mut z1 = ZoneSizingData::new("Z1");
        z1.design_cool_load = 5000.0;
        z1.design_cool_airflow = 0.5;
        z1.design_heat_load = 3000.0;
        z1.design_heat_airflow = 0.3;

        let mut z2 = ZoneSizingData::new("Z2");
        z2.design_cool_load = 7000.0;
        z2.design_cool_airflow = 0.7;
        z2.design_heat_load = 4000.0;
        z2.design_heat_airflow = 0.4;

        let mut sys = SystemSizingData::new("AHU-1");
        sys.size_from_zones(&[z1, z2]);

        assert!((sys.design_cool_capacity - 12000.0).abs() < 1.0);
        assert!((sys.design_heat_capacity - 7000.0).abs() < 1.0);
        assert!((sys.design_cool_airflow - 1.2).abs() < 0.01);
    }

    #[test]
    fn plant_sizing_flow_calculation() {
        let mut ps = PlantSizingData::new("CW Loop", PlantLoopType::Cooling);
        ps.design_delta_t = 5.0;
        ps.calculate_flow_from_capacity(100_000.0); // 100 kW

        // V_dot = Q / (cp * dT * rho) = 100000 / (4186 * 5 * 998) ≈ 0.00478 m³/s
        assert!(ps.design_flow_rate > 0.004 && ps.design_flow_rate < 0.006,
                "flow={}", ps.design_flow_rate);
    }

    #[test]
    fn plant_sizing_defaults() {
        let heating = PlantSizingData::new("HW", PlantLoopType::Heating);
        assert!((heating.design_delta_t - 11.0).abs() < 0.1);
        assert!((heating.design_exit_temp - 82.0).abs() < 0.1);

        let cooling = PlantSizingData::new("CW", PlantLoopType::Cooling);
        assert!((cooling.design_delta_t - 5.0).abs() < 0.1);
        assert!((cooling.design_exit_temp - 7.0).abs() < 0.1);
    }

    #[test]
    fn zone_sizing_heating_peak() {
        let mut zs = ZoneSizingData::new("Zone1");
        zs.update_heating(3000.0, 0.3, -5.0, 1, 15, 8);
        zs.update_heating(5000.0, 0.5, -10.0, 1, 21, 7);
        zs.update_heating(4000.0, 0.4, -8.0, 1, 21, 9);

        // Only the peak (5000 W) should be retained
        assert!((zs.design_heat_load - 5000.0).abs() < 1.0);
        assert!((zs.design_heat_airflow - 0.5).abs() < 0.01);
        assert!((zs.heat_outdoor_temp - (-10.0)).abs() < 0.1);
        assert_eq!(zs.heat_peak_hour, 7);
        assert_eq!(zs.heat_peak_day, 21);
        assert_eq!(zs.heat_peak_month, 1);
    }

    #[test]
    fn system_sizing_factor() {
        let mut z1 = ZoneSizingData::new("Z1");
        z1.design_cool_load = 10000.0;
        z1.design_heat_load = 8000.0;
        z1.design_cool_airflow = 1.0;
        z1.design_heat_airflow = 0.8;

        let mut sys = SystemSizingData::new("AHU-1");
        sys.sizing_factor = 1.1;
        sys.size_from_zones(&[z1]);

        // final_cool_capacity = design_cool_capacity * sizing_factor = 10000 * 1.1 = 11000
        assert!((sys.final_cool_capacity() - 11000.0).abs() < 1.0);
        assert!((sys.final_heat_capacity() - 8800.0).abs() < 1.0);
    }

    #[test]
    fn plant_sizing_zero_delta_t() {
        let mut ps = PlantSizingData::new("CW Loop", PlantLoopType::Cooling);
        ps.design_delta_t = 0.0;
        ps.calculate_flow_from_capacity(100_000.0);

        // delta_t < 0.1, so flow should remain 0
        assert!((ps.design_flow_rate - 0.0).abs() < 1e-10);
    }

    #[test]
    fn autosize_sentinel() {
        assert!(is_autosized(AUTOSIZE));
        assert!(!is_autosized(0.0));
        assert!(!is_autosized(1000.0));
        assert!(!is_autosized(-1.0));
    }

    #[test]
    fn design_airflow_from_cooling_load() {
        // 10 kW cooling, supply at 14C, zone at 24C
        let flow = design_airflow_from_load(10000.0, 14.0, 24.0, 0.008);
        // V_dot = 10000 / (1.204 * ~1021 * 10) ≈ 0.813 m³/s
        assert!(flow > 0.7 && flow < 0.9, "flow={flow}");
    }

    #[test]
    fn design_airflow_zero_dt() {
        let flow = design_airflow_from_load(10000.0, 22.0, 22.0, 0.008);
        assert!((flow).abs() < 1e-10);
    }

    #[test]
    fn resolve_zone_sizes_computes_airflow() {
        let mut zones = vec![
            ZoneSizingData::new("Z1"),
        ];
        zones[0].design_cool_load = 10000.0;
        zones[0].cool_supply_temp = 14.0;
        zones[0].design_heat_load = 8000.0;
        zones[0].heat_supply_temp = 40.0;

        resolve_zone_sizes(&mut zones);

        assert!(zones[0].design_cool_airflow > 0.0,
            "cool_flow={}", zones[0].design_cool_airflow);
        assert!(zones[0].design_heat_airflow > 0.0,
            "heat_flow={}", zones[0].design_heat_airflow);
    }

    #[test]
    fn sizing_result_from_zones() {
        let mut z1 = ZoneSizingData::new("Z1");
        z1.design_cool_load = 10000.0;
        z1.design_heat_load = 8000.0;
        z1.sizing_factor = 1.1;

        let mut z2 = ZoneSizingData::new("Z2");
        z2.design_cool_load = 5000.0;
        z2.design_heat_load = 3000.0;

        let result = SizingResult::from_zones(vec![z1, z2]);
        // 10000*1.1 + 5000*1.0 = 16000
        assert!((result.total_cool_capacity - 16000.0).abs() < 1.0);
        // 8000*1.1 + 3000*1.0 = 11800
        assert!((result.total_heat_capacity - 11800.0).abs() < 1.0);
    }
}
