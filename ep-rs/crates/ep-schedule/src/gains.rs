//! Internal heat gain definitions.

use crate::ScheduleRef;
use ep_units::*;

/// Internal heat gain definition for a zone.
#[derive(Debug, Clone)]
pub struct InternalGain {
    pub name: String,
    pub gain_type: GainType,
    pub schedule: ScheduleRef,
    pub design_level: Power,
    pub radiant_fraction: f64,
    pub convective_fraction: f64,
    pub latent_fraction: f64,
    pub return_air_fraction: f64,
    pub carbon_dioxide_rate: f64,
}

/// Type of internal gain.
#[derive(Debug, Clone)]
pub enum GainType {
    People {
        activity_schedule: ScheduleRef,
        count: f64,
    },
    Lights,
    ElectricEquipment,
    GasEquipment,
    HotWaterEquipment,
    SteamEquipment,
    OtherEquipment,
    InfiltrationDesignFlowRate {
        flow_rate: VolumeFlowRate,
        /// Coefficients: A + B*|Tz-To| + C*WindSpeed + D*WindSpeed^2
        coefficients: [f64; 4],
    },
}
