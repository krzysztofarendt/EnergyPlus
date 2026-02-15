//! Expanded airflow distribution models for duct systems, wind pressure,
//! occupant-controlled ventilation, and hybrid ventilation.
//!
//! Includes duct thermal and leakage models, Swami-Chandra wind pressure
//! coefficients, occupant window-opening probability, hybrid ventilation
//! mode selection, and additional airflow network components (coil pressure
//! drop, relief damper, zone exhaust fan).

use crate::components::{AirflowComponent, AirState, FlowResult};

// ---------------------------------------------------------------------------
// Duct Distribution System
// ---------------------------------------------------------------------------

/// A segment of ductwork with geometric and thermal properties.
#[derive(Debug, Clone)]
pub struct DuctSegment {
    /// Duct length (m).
    pub length: f64,
    /// Duct inner diameter (m).
    pub diameter: f64,
    /// Interior surface roughness (m).
    pub roughness: f64,
    /// Insulation R-value (m2*K/W).
    pub insulation_r_value: f64,
    /// Fraction of supply air lost through duct leakage (0-1).
    pub leakage_fraction: f64,
}

impl DuctSegment {
    /// Create a new duct segment with default roughness and no insulation.
    pub fn new(length: f64, diameter: f64) -> Self {
        Self {
            length,
            diameter,
            roughness: 0.0009, // galvanized steel
            insulation_r_value: 0.0,
            leakage_fraction: 0.0,
        }
    }

    /// Builder method to set insulation R-value.
    pub fn with_insulation(mut self, r_value: f64) -> Self {
        self.insulation_r_value = r_value;
        self
    }

    /// Builder method to set leakage fraction.
    pub fn with_leakage(mut self, fraction: f64) -> Self {
        self.leakage_fraction = fraction.clamp(0.0, 1.0);
        self
    }

    /// Cross-section area (m2).
    pub fn cross_section_area(&self) -> f64 {
        std::f64::consts::PI * self.diameter * self.diameter / 4.0
    }
}

/// Result of duct heat transfer calculation.
#[derive(Debug, Clone, Copy)]
pub struct DuctHeatResult {
    /// Air temperature leaving the duct (C).
    pub outlet_temp: f64,
    /// Heat loss from the air stream to the surroundings (W).
    /// Positive when inlet_temp > ambient_temp.
    pub heat_loss: f64,
}

/// Result of duct leakage calculation.
#[derive(Debug, Clone, Copy)]
pub struct LeakageResult {
    /// Air leaking out of the supply duct (kg/s).
    pub supply_leakage: f64,
    /// Air leaking into the return duct (kg/s).
    pub return_leakage: f64,
}

/// Calculate duct pressure drop using Darcy-Weisbach equation.
///
/// dP = f * (L/D) * (rho * V^2 / 2)
///
/// where `f` is the Darcy friction factor from the Swamee-Jain approximation
/// of the Colebrook-White equation for turbulent flow, or `64/Re` for laminar.
///
/// # Arguments
/// * `flow_kg_s` - Mass flow rate (kg/s)
/// * `density` - Air density (kg/m3)
/// * `viscosity` - Dynamic viscosity (Pa*s)
/// * `duct` - Duct segment properties
///
/// # Returns
/// Pressure drop in Pa. Always non-negative.
pub fn calc_duct_pressure_drop(flow_kg_s: f64, density: f64, viscosity: f64, duct: &DuctSegment) -> f64 {
    if flow_kg_s.abs() < 1e-12 || density <= 0.0 || duct.length <= 0.0 {
        return 0.0;
    }

    let area = duct.cross_section_area();
    if area <= 1e-12 {
        return 0.0;
    }

    let velocity = flow_kg_s.abs() / (density * area);
    let re = density * velocity * duct.diameter / viscosity;

    let friction_factor = if re < 2300.0 {
        if re > 1e-6 { 64.0 / re } else { 0.04 }
    } else {
        // Swamee-Jain approximation
        let e_d = duct.roughness / duct.diameter;
        let term = e_d / 3.7 + 5.74 / re.powf(0.9);
        if term > 0.0 {
            0.25 / (term.log10().powi(2))
        } else {
            0.02
        }
    };

    friction_factor * (duct.length / duct.diameter) * (density * velocity * velocity / 2.0)
}

/// Calculate duct heat transfer using effectiveness-NTU approach.
///
/// The UA value is computed from the insulation R-value and an assumed
/// outside film coefficient. The outlet temperature decays exponentially
/// toward the ambient temperature:
///
/// ```text
/// UA = pi * D * L / (R_insulation + 1/h_outside)
/// T_out = T_ambient + (T_in - T_ambient) * exp(-UA / (m_dot * cp))
/// ```
///
/// # Arguments
/// * `inlet_temp` - Inlet air temperature (C)
/// * `ambient_temp` - Surrounding air temperature (C)
/// * `flow_kg_s` - Mass flow rate (kg/s)
/// * `duct` - Duct segment properties
///
/// # Returns
/// `DuctHeatResult` with outlet temperature and heat loss.
pub fn calc_duct_heat_transfer(
    inlet_temp: f64,
    ambient_temp: f64,
    flow_kg_s: f64,
    duct: &DuctSegment,
) -> DuctHeatResult {
    let cp = 1006.0; // J/(kg*K)
    let h_outside = 10.0; // W/(m2*K), approximate outside film coefficient

    if flow_kg_s.abs() < 1e-12 {
        // No flow: duct air equilibrates to ambient
        return DuctHeatResult {
            outlet_temp: ambient_temp,
            heat_loss: 0.0,
        };
    }

    let r_total = duct.insulation_r_value + 1.0 / h_outside;
    let ua = std::f64::consts::PI * duct.diameter * duct.length / r_total;
    let ntu = ua / (flow_kg_s.abs() * cp);
    let effectiveness = 1.0 - (-ntu).exp();

    let outlet_temp = inlet_temp - effectiveness * (inlet_temp - ambient_temp);
    let heat_loss = flow_kg_s.abs() * cp * (inlet_temp - outlet_temp);

    DuctHeatResult {
        outlet_temp,
        heat_loss,
    }
}

/// Calculate duct leakage flows.
///
/// Supply leakage reduces the delivered airflow by losing air to the
/// surrounding space. Return leakage adds unconditioned air to the
/// return stream.
///
/// # Arguments
/// * `supply_flow` - Total supply mass flow rate (kg/s)
/// * `leakage_fraction` - Fraction of supply flow lost (0-1)
///
/// # Returns
/// `LeakageResult` with supply and return leakage flows.
pub fn calc_duct_leakage(supply_flow: f64, leakage_fraction: f64) -> LeakageResult {
    let frac = leakage_fraction.clamp(0.0, 1.0);
    let supply_leakage = supply_flow.abs() * frac;
    // Return leakage is typically similar in magnitude
    let return_leakage = supply_flow.abs() * frac;

    LeakageResult {
        supply_leakage,
        return_leakage,
    }
}

// ---------------------------------------------------------------------------
// Wind Pressure Coefficients
// ---------------------------------------------------------------------------

/// Terrain category for wind profile calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainCategory {
    /// Dense urban center (alpha=0.33, delta=460m).
    City,
    /// Suburban / wooded areas (alpha=0.22, delta=370m).
    Suburbs,
    /// Open terrain with few obstructions (alpha=0.14, delta=270m).
    OpenField,
    /// Flat, unobstructed coast or water (alpha=0.10, delta=210m).
    Ocean,
}

impl TerrainCategory {
    /// Power-law exponent for the terrain category.
    fn alpha(self) -> f64 {
        match self {
            TerrainCategory::City => 0.33,
            TerrainCategory::Suburbs => 0.22,
            TerrainCategory::OpenField => 0.14,
            TerrainCategory::Ocean => 0.10,
        }
    }

    /// Boundary layer thickness (m) for the terrain category.
    fn delta(self) -> f64 {
        match self {
            TerrainCategory::City => 460.0,
            TerrainCategory::Suburbs => 370.0,
            TerrainCategory::OpenField => 270.0,
            TerrainCategory::Ocean => 210.0,
        }
    }
}

/// Wind pressure coefficient model for a rectangular building.
#[derive(Debug, Clone)]
pub struct WindPressureModel {
    /// Building width perpendicular to the reference facade (m).
    pub building_width: f64,
    /// Building depth along the reference facade (m).
    pub building_depth: f64,
    /// Terrain category for local wind profile.
    pub terrain_category: TerrainCategory,
}

impl WindPressureModel {
    pub fn new(building_width: f64, building_depth: f64, terrain_category: TerrainCategory) -> Self {
        Self {
            building_width,
            building_depth,
            terrain_category,
        }
    }
}

/// Calculate wind pressure coefficient using the Swami-Chandra correlation
/// for rectangular low-rise buildings.
///
/// ```text
/// Cp = 0.6 * ln(1.248 - 0.703*sin(alpha/2)
///              - 1.175*sin^2(alpha)
///              + 0.131*sin^3(2*alpha*G)
///              + 0.769*cos(alpha/2)
///              + 0.07*G^2*sin^2(alpha/2)
///              + 0.717*cos^2(alpha/2))
/// ```
///
/// where `alpha` is the wind angle relative to the surface normal (rad)
/// and `G = ln(width/depth)`.
///
/// # Arguments
/// * `wind_direction_deg` - Wind direction in degrees from north (meteorological convention)
/// * `surface_azimuth_deg` - Surface outward normal azimuth in degrees from north
/// * `model` - Building geometry for the side-ratio parameter
///
/// # Returns
/// Dimensionless wind pressure coefficient (positive = pressure, negative = suction).
pub fn calc_wind_pressure_coefficient(
    wind_direction_deg: f64,
    surface_azimuth_deg: f64,
    model: &WindPressureModel,
) -> f64 {
    // Wind angle relative to surface normal
    let alpha_deg = wind_direction_deg - surface_azimuth_deg;
    let alpha = alpha_deg.to_radians();

    // Side ratio parameter
    let ratio = if model.building_depth > 0.0 {
        model.building_width / model.building_depth
    } else {
        1.0
    };
    let g = ratio.max(0.01).ln();

    // Swami-Chandra correlation
    let arg = 1.248
        - 0.703 * (alpha / 2.0).sin()
        - 1.175 * alpha.sin().powi(2)
        + 0.131 * (2.0 * alpha * g).sin().powi(3)
        + 0.769 * (alpha / 2.0).cos()
        + 0.07 * g.powi(2) * (alpha / 2.0).sin().powi(2)
        + 0.717 * (alpha / 2.0).cos().powi(2);

    // The argument must be positive for the logarithm
    if arg > 0.0 {
        0.6 * arg.ln()
    } else {
        -1.0 // Strong suction fallback
    }
}

/// Calculate local wind speed from meteorological station data using a
/// terrain-dependent power-law wind profile.
///
/// ```text
/// V_local = V_met * (delta_met / h_met)^alpha_met * (h_local / delta_local)^alpha_local
/// ```
///
/// The meteorological station is assumed to be at 10 m height in open terrain.
///
/// # Arguments
/// * `met_speed` - Wind speed at the meteorological station (m/s)
/// * `met_height` - Meteorological station anemometer height (m)
/// * `local_height` - Height at the building site (m)
/// * `terrain` - Local terrain category
///
/// # Returns
/// Local wind speed (m/s) at the specified height.
pub fn calc_local_wind_speed(
    met_speed: f64,
    met_height: f64,
    local_height: f64,
    terrain: TerrainCategory,
) -> f64 {
    if met_speed <= 0.0 || met_height <= 0.0 || local_height <= 0.0 {
        return 0.0;
    }

    // Meteorological station assumed open-field terrain
    let met_alpha = TerrainCategory::OpenField.alpha();
    let met_delta = TerrainCategory::OpenField.delta();

    let local_alpha = terrain.alpha();
    let local_delta = terrain.delta();

    // Two-step correction: met station → gradient height → local height
    let met_correction = (met_delta / met_height).powf(met_alpha);
    let local_correction = (local_height / local_delta).powf(local_alpha);

    met_speed * met_correction * local_correction
}

// ---------------------------------------------------------------------------
// Occupant-Controlled Ventilation
// ---------------------------------------------------------------------------

/// Probability model for occupant window opening behavior.
#[derive(Debug, Clone)]
pub enum OpeningProbabilityModel {
    /// Constant probability regardless of conditions.
    Constant(f64),
    /// Logistic function of indoor temperature.
    TemperatureDriven,
    /// Humphreys adaptive model (based on outdoor running mean).
    Humphreys,
}

/// Occupant-controlled natural ventilation parameters.
#[derive(Debug, Clone)]
pub struct OccupantVentControl {
    /// Indoor temperature above which occupants tend to open windows (C).
    pub indoor_temp_threshold: f64,
    /// Minimum outdoor temperature for window opening (C).
    pub outdoor_temp_min: f64,
    /// Maximum outdoor temperature for window opening (C).
    pub outdoor_temp_max: f64,
    /// Probability model for opening behavior.
    pub opening_probability_model: OpeningProbabilityModel,
}

impl OccupantVentControl {
    pub fn new(threshold: f64) -> Self {
        Self {
            indoor_temp_threshold: threshold,
            outdoor_temp_min: 10.0,
            outdoor_temp_max: 35.0,
            opening_probability_model: OpeningProbabilityModel::TemperatureDriven,
        }
    }
}

/// Calculate the probability that an occupant opens a window.
///
/// For `TemperatureDriven`: a logistic function centered on the threshold
/// temperature with a 2-degree spread:
///
/// ```text
/// P = 1 / (1 + exp(-(T_indoor - T_threshold) / 2))
/// ```
///
/// Returns 0.0 if outdoor temperature is outside the acceptable range.
///
/// # Arguments
/// * `indoor_temp` - Current indoor air temperature (C)
/// * `outdoor_temp` - Current outdoor air temperature (C)
/// * `control` - Occupant ventilation control parameters
///
/// # Returns
/// Opening probability in the range 0.0 to 1.0.
pub fn calc_opening_probability(
    indoor_temp: f64,
    outdoor_temp: f64,
    control: &OccupantVentControl,
) -> f64 {
    // Check outdoor temperature limits
    if outdoor_temp < control.outdoor_temp_min || outdoor_temp > control.outdoor_temp_max {
        return 0.0;
    }

    match &control.opening_probability_model {
        OpeningProbabilityModel::Constant(p) => p.clamp(0.0, 1.0),

        OpeningProbabilityModel::TemperatureDriven => {
            let x = (indoor_temp - control.indoor_temp_threshold) / 2.0;
            1.0 / (1.0 + (-x).exp())
        }

        OpeningProbabilityModel::Humphreys => {
            // Simplified Humphreys: comfort temperature = 0.534 * T_out + 11.9
            // Window opening increases as indoor temp exceeds comfort temp
            let comfort_temp = 0.534 * outdoor_temp + 11.9;
            let x = (indoor_temp - comfort_temp) / 2.0;
            1.0 / (1.0 + (-x).exp())
        }
    }
}

// ---------------------------------------------------------------------------
// Hybrid Ventilation Mode
// ---------------------------------------------------------------------------

/// Ventilation operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HybridVentMode {
    /// Windows open, mechanical systems off.
    Natural,
    /// Windows closed, mechanical HVAC active.
    Mechanical,
    /// Both natural and mechanical operating simultaneously.
    Hybrid,
}

/// Controller for hybrid ventilation switching logic.
#[derive(Debug, Clone)]
pub struct HybridVentController {
    /// Current operating mode.
    pub mode: HybridVentMode,
    /// Natural ventilation schedule value (0-1, 1 = allowed).
    pub natural_vent_schedule: f64,
    /// Whether the mechanical HVAC system is available.
    pub mechanical_system_available: bool,
}

impl HybridVentController {
    pub fn new() -> Self {
        Self {
            mode: HybridVentMode::Mechanical,
            natural_vent_schedule: 0.0,
            mechanical_system_available: true,
        }
    }
}

impl Default for HybridVentController {
    fn default() -> Self {
        Self::new()
    }
}

/// Determine the ventilation mode based on outdoor conditions, HVAC
/// availability, and the natural ventilation schedule.
///
/// Logic:
/// - If the schedule allows natural ventilation and outdoor conditions are
///   favorable (wind speed below 15 m/s, temperature between 15-30 C):
///   - If the mechanical system is also available, use `Hybrid`
///   - Otherwise use `Natural`
/// - If natural ventilation is not viable:
///   - If the mechanical system is available, use `Mechanical`
///   - If neither is available, fall back to `Natural` (passive)
///
/// # Arguments
/// * `outdoor_temp` - Outdoor dry-bulb temperature (C)
/// * `wind_speed` - Local wind speed (m/s)
/// * `hvac_available` - Whether the mechanical HVAC system is available
/// * `schedule_value` - Natural ventilation schedule (0-1, 1 = allowed)
///
/// # Returns
/// The recommended `HybridVentMode`.
pub fn determine_vent_mode(
    outdoor_temp: f64,
    wind_speed: f64,
    hvac_available: bool,
    schedule_value: f64,
) -> HybridVentMode {
    let natural_favorable = schedule_value > 0.5
        && outdoor_temp >= 15.0
        && outdoor_temp <= 30.0
        && wind_speed < 15.0;

    match (natural_favorable, hvac_available) {
        (true, true) => HybridVentMode::Hybrid,
        (true, false) => HybridVentMode::Natural,
        (false, true) => HybridVentMode::Mechanical,
        (false, false) => HybridVentMode::Natural, // passive fallback
    }
}

// ---------------------------------------------------------------------------
// Additional AirflowComponent Implementations
// ---------------------------------------------------------------------------

/// Pressure drop across a coil or heat exchanger in the duct.
///
/// Uses a quadratic relationship referenced to rated conditions:
///
/// ```text
/// dP = rated_dp * (flow / rated_flow)^2
/// ```
#[derive(Debug, Clone)]
pub struct CoilPressureDrop {
    pub name: String,
    /// Pressure drop at rated flow (Pa).
    pub rated_dp: f64,
    /// Rated mass flow rate (kg/s).
    pub rated_flow: f64,
}

impl CoilPressureDrop {
    pub fn new(name: impl Into<String>, rated_dp: f64, rated_flow: f64) -> Self {
        Self {
            name: name.into(),
            rated_dp,
            rated_flow,
        }
    }
}

impl AirflowComponent for CoilPressureDrop {
    fn calculate(&self, dp: f64, state_1: &AirState, _state_2: &AirState, laminar: bool) -> FlowResult {
        // Solve for flow given pressure drop:
        // dp = rated_dp * (flow / rated_flow)^2
        // flow = rated_flow * sqrt(|dp| / rated_dp)
        if self.rated_dp <= 0.0 || self.rated_flow <= 0.0 {
            return FlowResult::default();
        }

        if laminar || dp.abs() < 1e-10 {
            // Linear approximation: slope at rated conditions
            // d(dp)/d(flow) = 2 * rated_dp * flow / rated_flow^2 => at rated: 2*rated_dp/rated_flow
            // So d(flow)/d(dp) = rated_flow / (2 * rated_dp)
            let df = self.rated_flow / (2.0 * self.rated_dp);
            return FlowResult {
                flow: df * dp,
                df_dp: df,
                ..Default::default()
            };
        }

        let abs_dp = dp.abs();
        let sign = dp.signum();

        // Density correction relative to standard air
        let rho_corr = (state_1.density / 1.2).sqrt();
        let flow = sign * self.rated_flow * rho_corr * (abs_dp / self.rated_dp).sqrt();
        // df/d(dp) = rated_flow * rho_corr / (2 * sqrt(rated_dp * |dp|))
        let df = self.rated_flow * rho_corr / (2.0 * (self.rated_dp * abs_dp).sqrt());

        FlowResult {
            flow,
            df_dp: df,
            ..Default::default()
        }
    }
}

/// Relief damper that opens when zone pressure exceeds outdoor pressure.
///
/// Modeled as an orifice that only passes flow in one direction
/// (zone to outdoor).
#[derive(Debug, Clone)]
pub struct ReliefDamper {
    pub name: String,
    /// Free area when fully open (m2).
    pub area: f64,
    /// Discharge coefficient (dimensionless, typically 0.65).
    pub discharge_coef: f64,
}

impl ReliefDamper {
    pub fn new(name: impl Into<String>, area: f64, discharge_coef: f64) -> Self {
        Self {
            name: name.into(),
            area,
            discharge_coef,
        }
    }
}

impl AirflowComponent for ReliefDamper {
    fn calculate(&self, dp: f64, state_1: &AirState, _state_2: &AirState, laminar: bool) -> FlowResult {
        if self.area <= 0.0 {
            return FlowResult::default();
        }

        // Relief damper only opens when dp > 0 (zone pressure > outdoor)
        if dp <= 0.0 {
            // Damper closed: tiny leakage for solver stability
            let tiny_df = 1e-6;
            return FlowResult {
                flow: tiny_df * dp,
                df_dp: tiny_df,
                ..Default::default()
            };
        }

        let rho = state_1.density;

        if laminar {
            let df = self.discharge_coef * self.area * (2.0 * rho).sqrt() * 0.5;
            return FlowResult {
                flow: df * dp,
                df_dp: df,
                ..Default::default()
            };
        }

        // Orifice equation for positive dp only
        let flow = self.discharge_coef * self.area * (2.0 * rho * dp).sqrt();
        let df = self.discharge_coef * self.area * (rho / (2.0 * dp)).sqrt();

        FlowResult {
            flow,
            df_dp: df,
            ..Default::default()
        }
    }
}

/// Zone exhaust fan that provides a fixed flow rate when operating.
///
/// When scheduled on, the fan delivers a constant mass flow and creates
/// a pressure rise. When off, the fan acts as a sealed opening.
#[derive(Debug, Clone)]
pub struct ZoneExhaustFan {
    pub name: String,
    /// Maximum mass flow rate (kg/s).
    pub max_flow: f64,
    /// Fan pressure rise (Pa).
    pub pressure_rise: f64,
    /// Schedule flag (true = fan is on).
    pub schedule_on: bool,
}

impl ZoneExhaustFan {
    pub fn new(name: impl Into<String>, max_flow: f64, pressure_rise: f64) -> Self {
        Self {
            name: name.into(),
            max_flow,
            pressure_rise,
            schedule_on: false,
        }
    }
}

impl AirflowComponent for ZoneExhaustFan {
    fn calculate(&self, dp: f64, _state_1: &AirState, _state_2: &AirState, _laminar: bool) -> FlowResult {
        if !self.schedule_on || self.max_flow <= 0.0 {
            // Fan off: sealed, no flow
            return FlowResult {
                flow: 0.0,
                df_dp: 1e-8, // tiny for solver stability
                ..Default::default()
            };
        }

        // Fan on: provides fixed flow in the forward direction.
        // The effective dp includes the fan's pressure rise.
        // The flow is essentially constant at max_flow regardless of dp,
        // modeled with a very steep (stiff) linear relationship around the
        // operating point.
        let effective_dp = dp + self.pressure_rise;
        let stiffness = 100.0; // large stiffness to approximate constant flow
        let flow = self.max_flow + stiffness * effective_dp.min(0.0).max(-self.pressure_rise);

        // Clamp flow to [0, max_flow]
        let flow_clamped = flow.clamp(0.0, self.max_flow);

        FlowResult {
            flow: flow_clamped,
            df_dp: if flow_clamped > 0.0 && flow_clamped < self.max_flow { stiffness } else { 1e-8 },
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // === Duct Pressure Drop Tests ===

    #[test]
    fn duct_pressure_drop_increases_with_flow() {
        let duct = DuctSegment::new(10.0, 0.3);
        let rho = 1.2;
        let mu = 1.8e-5;

        let dp_low = calc_duct_pressure_drop(0.1, rho, mu, &duct);
        let dp_high = calc_duct_pressure_drop(0.5, rho, mu, &duct);

        assert!(dp_low > 0.0, "dp_low should be positive: {dp_low}");
        assert!(dp_high > dp_low, "dp should increase with flow: {dp_high} > {dp_low}");
    }

    #[test]
    fn duct_pressure_drop_zero_flow() {
        let duct = DuctSegment::new(10.0, 0.3);
        let dp = calc_duct_pressure_drop(0.0, 1.2, 1.8e-5, &duct);
        assert!((dp).abs() < 1e-12, "zero flow should give zero dp");
    }

    #[test]
    fn duct_pressure_drop_increases_with_roughness() {
        let mut duct_smooth = DuctSegment::new(10.0, 0.3);
        duct_smooth.roughness = 0.0001;
        let mut duct_rough = DuctSegment::new(10.0, 0.3);
        duct_rough.roughness = 0.003;

        let rho = 1.2;
        let mu = 1.8e-5;
        let flow = 0.5;

        let dp_smooth = calc_duct_pressure_drop(flow, rho, mu, &duct_smooth);
        let dp_rough = calc_duct_pressure_drop(flow, rho, mu, &duct_rough);

        assert!(dp_rough > dp_smooth, "rougher duct should have higher dp: {dp_rough} > {dp_smooth}");
    }

    #[test]
    fn duct_pressure_drop_smaller_diameter_higher_dp() {
        let duct_large = DuctSegment::new(10.0, 0.5);
        let duct_small = DuctSegment::new(10.0, 0.2);

        let rho = 1.2;
        let mu = 1.8e-5;
        let flow = 0.3;

        let dp_large = calc_duct_pressure_drop(flow, rho, mu, &duct_large);
        let dp_small = calc_duct_pressure_drop(flow, rho, mu, &duct_small);

        assert!(dp_small > dp_large, "smaller duct should have higher dp: {dp_small} > {dp_large}");
    }

    // === Duct Heat Transfer Tests ===

    #[test]
    fn duct_heat_loss_positive_when_inlet_warmer() {
        let duct = DuctSegment::new(20.0, 0.3).with_insulation(1.0);
        let result = calc_duct_heat_transfer(35.0, 20.0, 0.5, &duct);

        assert!(result.heat_loss > 0.0, "heat loss should be positive when inlet > ambient");
        assert!(result.outlet_temp < 35.0, "outlet should be cooler than inlet");
        assert!(result.outlet_temp > 20.0, "outlet should be warmer than ambient");
    }

    #[test]
    fn duct_outlet_temp_between_inlet_and_ambient() {
        let duct = DuctSegment::new(15.0, 0.25).with_insulation(0.5);
        let result = calc_duct_heat_transfer(40.0, 10.0, 0.3, &duct);

        assert!(
            result.outlet_temp > 10.0 && result.outlet_temp < 40.0,
            "outlet_temp={} should be between 10 and 40",
            result.outlet_temp
        );
    }

    #[test]
    fn duct_heat_loss_negative_when_inlet_cooler() {
        let duct = DuctSegment::new(10.0, 0.3).with_insulation(0.5);
        let result = calc_duct_heat_transfer(10.0, 25.0, 0.5, &duct);

        assert!(result.heat_loss < 0.0, "heat loss should be negative (heat gain) when inlet < ambient");
        assert!(result.outlet_temp > 10.0, "outlet should warm toward ambient");
    }

    #[test]
    fn duct_more_insulation_less_heat_loss() {
        let duct_low = DuctSegment::new(20.0, 0.3).with_insulation(0.5);
        let duct_high = DuctSegment::new(20.0, 0.3).with_insulation(3.0);

        let result_low = calc_duct_heat_transfer(40.0, 20.0, 0.5, &duct_low);
        let result_high = calc_duct_heat_transfer(40.0, 20.0, 0.5, &duct_high);

        assert!(
            result_high.heat_loss < result_low.heat_loss,
            "more insulation should reduce heat loss: {} < {}",
            result_high.heat_loss,
            result_low.heat_loss
        );
    }

    #[test]
    fn duct_zero_flow_equilibrates_to_ambient() {
        let duct = DuctSegment::new(10.0, 0.3);
        let result = calc_duct_heat_transfer(40.0, 20.0, 0.0, &duct);

        assert!(
            (result.outlet_temp - 20.0).abs() < 1e-6,
            "zero flow should equilibrate to ambient"
        );
    }

    // === Duct Leakage Tests ===

    #[test]
    fn duct_leakage_reduces_delivered_flow() {
        let supply_flow = 1.0;
        let leakage_fraction = 0.05;

        let result = calc_duct_leakage(supply_flow, leakage_fraction);
        let delivered = supply_flow - result.supply_leakage;

        assert!(delivered < supply_flow, "leakage should reduce delivered flow");
        assert!((result.supply_leakage - 0.05).abs() < 1e-10, "5% leakage of 1.0 = 0.05");
    }

    #[test]
    fn duct_leakage_zero_fraction() {
        let result = calc_duct_leakage(2.0, 0.0);
        assert!((result.supply_leakage).abs() < 1e-12);
        assert!((result.return_leakage).abs() < 1e-12);
    }

    #[test]
    fn duct_leakage_return_matches_supply() {
        let result = calc_duct_leakage(1.0, 0.1);
        assert!(
            (result.supply_leakage - result.return_leakage).abs() < 1e-12,
            "supply and return leakage should match"
        );
    }

    // === Wind Pressure Coefficient Tests ===

    #[test]
    fn wind_cp_windward_vs_leeward() {
        let model = WindPressureModel::new(20.0, 10.0, TerrainCategory::Suburbs);

        // Windward: wind hitting surface head-on (wind dir = surface azimuth)
        let cp_windward = calc_wind_pressure_coefficient(180.0, 180.0, &model);
        // Leeward: wind coming from behind (180 deg offset)
        let cp_leeward = calc_wind_pressure_coefficient(0.0, 180.0, &model);

        assert!(
            cp_windward > cp_leeward,
            "windward Cp ({cp_windward}) should be greater than leeward Cp ({cp_leeward})"
        );
    }

    #[test]
    fn wind_cp_windward_positive() {
        let model = WindPressureModel::new(15.0, 15.0, TerrainCategory::OpenField);
        let cp = calc_wind_pressure_coefficient(0.0, 0.0, &model);
        assert!(cp > 0.0, "windward Cp should be positive: {cp}");
    }

    #[test]
    fn wind_cp_side_faces_lower_than_windward() {
        let model = WindPressureModel::new(10.0, 10.0, TerrainCategory::Suburbs);

        // Windward (alpha = 0)
        let cp_front = calc_wind_pressure_coefficient(0.0, 0.0, &model);
        // Side face (alpha = 90)
        let cp_side = calc_wind_pressure_coefficient(90.0, 0.0, &model);

        assert!(
            cp_front > cp_side,
            "windward Cp ({cp_front}) should exceed side face Cp ({cp_side})"
        );
    }

    // === Local Wind Speed Tests ===

    #[test]
    fn local_wind_speed_increases_with_height() {
        let v_low = calc_local_wind_speed(5.0, 10.0, 5.0, TerrainCategory::Suburbs);
        let v_high = calc_local_wind_speed(5.0, 10.0, 30.0, TerrainCategory::Suburbs);

        assert!(
            v_high > v_low,
            "wind should be faster at height 30m ({v_high}) than 5m ({v_low})"
        );
    }

    #[test]
    fn terrain_city_slower_than_open_field() {
        let height = 10.0;
        let v_city = calc_local_wind_speed(5.0, 10.0, height, TerrainCategory::City);
        let v_open = calc_local_wind_speed(5.0, 10.0, height, TerrainCategory::OpenField);

        assert!(
            v_open > v_city,
            "open field ({v_open}) should be faster than city ({v_city}) at same height"
        );
    }

    #[test]
    fn local_wind_speed_zero_met() {
        let v = calc_local_wind_speed(0.0, 10.0, 10.0, TerrainCategory::Suburbs);
        assert!((v).abs() < 1e-12, "zero met speed should give zero local speed");
    }

    #[test]
    fn local_wind_speed_ocean_fastest() {
        let height = 10.0;
        let v_ocean = calc_local_wind_speed(5.0, 10.0, height, TerrainCategory::Ocean);
        let v_city = calc_local_wind_speed(5.0, 10.0, height, TerrainCategory::City);
        let v_suburbs = calc_local_wind_speed(5.0, 10.0, height, TerrainCategory::Suburbs);

        assert!(v_ocean > v_suburbs, "ocean ({v_ocean}) > suburbs ({v_suburbs})");
        assert!(v_suburbs > v_city, "suburbs ({v_suburbs}) > city ({v_city})");
    }

    // === Occupant Opening Probability Tests ===

    #[test]
    fn opening_probability_increases_with_indoor_temp() {
        let control = OccupantVentControl::new(24.0);
        let outdoor = 20.0;

        let p_cool = calc_opening_probability(20.0, outdoor, &control);
        let p_warm = calc_opening_probability(28.0, outdoor, &control);

        assert!(
            p_warm > p_cool,
            "probability should increase with temp: {p_warm} > {p_cool}"
        );
    }

    #[test]
    fn opening_probability_zero_when_outdoor_too_cold() {
        let control = OccupantVentControl::new(24.0);
        let p = calc_opening_probability(28.0, 5.0, &control);
        assert!((p).abs() < 1e-12, "should be zero when outdoor below min");
    }

    #[test]
    fn opening_probability_at_threshold() {
        let control = OccupantVentControl::new(24.0);
        let p = calc_opening_probability(24.0, 20.0, &control);
        // At threshold, logistic gives 0.5
        assert!(
            (p - 0.5).abs() < 0.01,
            "probability at threshold should be ~0.5: {p}"
        );
    }

    #[test]
    fn opening_probability_constant_model() {
        let mut control = OccupantVentControl::new(24.0);
        control.opening_probability_model = OpeningProbabilityModel::Constant(0.3);

        let p = calc_opening_probability(20.0, 20.0, &control);
        assert!((p - 0.3).abs() < 1e-10, "constant model should return 0.3: {p}");
    }

    #[test]
    fn opening_probability_humphreys() {
        let mut control = OccupantVentControl::new(24.0);
        control.opening_probability_model = OpeningProbabilityModel::Humphreys;

        // At outdoor 20C, comfort ~ 0.534*20+11.9 = 22.6C
        // Indoor at 26C is above comfort, so probability should be high
        let p = calc_opening_probability(26.0, 20.0, &control);
        assert!(p > 0.5, "indoor above Humphreys comfort should give p > 0.5: {p}");
    }

    // === Hybrid Ventilation Mode Tests ===

    #[test]
    fn hybrid_vent_natural_when_favorable_no_hvac() {
        let mode = determine_vent_mode(22.0, 3.0, false, 1.0);
        assert_eq!(mode, HybridVentMode::Natural);
    }

    #[test]
    fn hybrid_vent_mechanical_when_unfavorable() {
        let mode = determine_vent_mode(5.0, 3.0, true, 1.0);
        assert_eq!(mode, HybridVentMode::Mechanical);
    }

    #[test]
    fn hybrid_vent_hybrid_when_both_available() {
        let mode = determine_vent_mode(22.0, 3.0, true, 1.0);
        assert_eq!(mode, HybridVentMode::Hybrid);
    }

    #[test]
    fn hybrid_vent_mechanical_when_schedule_off() {
        let mode = determine_vent_mode(22.0, 3.0, true, 0.0);
        assert_eq!(mode, HybridVentMode::Mechanical);
    }

    #[test]
    fn hybrid_vent_natural_fallback_when_nothing_available() {
        // Unfavorable outdoor but no HVAC
        let mode = determine_vent_mode(5.0, 3.0, false, 0.0);
        assert_eq!(mode, HybridVentMode::Natural);
    }

    // === Coil Pressure Drop Tests ===

    #[test]
    fn coil_dp_quadratic_relationship() {
        let coil = CoilPressureDrop::new("Cooling Coil", 150.0, 1.0);
        let state = AirState::standard();

        // At rated flow, dp should be rated_dp
        let result_rated = coil.calculate(150.0, &state, &state, false);
        // At half dp (150/4 = 37.5 Pa), flow should be half rated
        let result_half = coil.calculate(37.5, &state, &state, false);

        let ratio = result_rated.flow / result_half.flow;
        assert!(
            (ratio - 2.0).abs() < 0.1,
            "flow should double when dp quadruples: ratio={ratio}"
        );
    }

    #[test]
    fn coil_dp_zero_returns_zero() {
        let coil = CoilPressureDrop::new("Coil", 100.0, 0.5);
        let state = AirState::standard();
        let result = coil.calculate(0.0, &state, &state, false);
        assert!((result.flow).abs() < 1e-6);
    }

    // === Relief Damper Tests ===

    #[test]
    fn relief_damper_forward_flow() {
        let damper = ReliefDamper::new("Relief", 0.5, 0.65);
        let state = AirState::standard();

        // Positive dp = zone > outdoor → damper opens
        let result = damper.calculate(10.0, &state, &state, false);
        assert!(result.flow > 0.0, "relief damper should open with positive dp");
    }

    #[test]
    fn relief_damper_no_reverse_flow() {
        let damper = ReliefDamper::new("Relief", 0.5, 0.65);
        let state = AirState::standard();

        // Negative dp = outdoor > zone → damper stays closed
        let result = damper.calculate(-10.0, &state, &state, false);
        assert!(
            result.flow <= 0.0,
            "relief damper should not allow reverse flow: {}",
            result.flow
        );
    }

    #[test]
    fn relief_damper_flow_increases_with_dp() {
        let damper = ReliefDamper::new("Relief", 0.5, 0.65);
        let state = AirState::standard();

        let result_low = damper.calculate(5.0, &state, &state, false);
        let result_high = damper.calculate(20.0, &state, &state, false);

        assert!(
            result_high.flow > result_low.flow,
            "higher dp should give more flow: {} > {}",
            result_high.flow,
            result_low.flow
        );
    }

    // === Zone Exhaust Fan Tests ===

    #[test]
    fn exhaust_fan_fixed_flow_when_on() {
        let mut fan = ZoneExhaustFan::new("Exhaust", 0.5, 250.0);
        fan.schedule_on = true;
        let state = AirState::standard();

        let result = fan.calculate(0.0, &state, &state, false);
        assert!(
            (result.flow - 0.5).abs() < 0.01,
            "fan should deliver rated flow: {}",
            result.flow
        );
    }

    #[test]
    fn exhaust_fan_no_flow_when_off() {
        let fan = ZoneExhaustFan::new("Exhaust", 0.5, 250.0);
        let state = AirState::standard();

        let result = fan.calculate(10.0, &state, &state, false);
        assert!(
            result.flow.abs() < 1e-6,
            "fan off should give no flow: {}",
            result.flow
        );
    }

    #[test]
    fn exhaust_fan_flow_nonnegative() {
        let mut fan = ZoneExhaustFan::new("Exhaust", 0.5, 250.0);
        fan.schedule_on = true;
        let state = AirState::standard();

        // Even with adverse pressure, flow should not go negative
        let result = fan.calculate(-500.0, &state, &state, false);
        assert!(result.flow >= 0.0, "exhaust fan flow should not be negative: {}", result.flow);
    }
}
