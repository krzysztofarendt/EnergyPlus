//! Meter system for energy/resource tracking.
//!
//! Meters aggregate output variable values by resource type, end use,
//! and group (facility/building/zone). A hierarchical meter tree is
//! automatically constructed so that facility-level meters sum up
//! their child group and end-use meters.

use std::collections::HashMap;

use crate::accumulator::PeriodAccumulator;
use crate::variables::{ReportFreq, TimeStamp};

/// Energy/resource type tracked by a meter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resource {
    Electricity,
    NaturalGas,
    Propane,
    FuelOilNo1,
    FuelOilNo2,
    Coal,
    Diesel,
    Gasoline,
    Steam,
    Water,
    DistrictCooling,
    DistrictHeating,
    OtherFuel1,
    OtherFuel2,
}

impl Resource {
    pub fn name(&self) -> &'static str {
        match self {
            Resource::Electricity => "Electricity",
            Resource::NaturalGas => "NaturalGas",
            Resource::Propane => "Propane",
            Resource::FuelOilNo1 => "FuelOilNo1",
            Resource::FuelOilNo2 => "FuelOilNo2",
            Resource::Coal => "Coal",
            Resource::Diesel => "Diesel",
            Resource::Gasoline => "Gasoline",
            Resource::Steam => "Steam",
            Resource::Water => "Water",
            Resource::DistrictCooling => "DistrictCooling",
            Resource::DistrictHeating => "DistrictHeating",
            Resource::OtherFuel1 => "OtherFuel1",
            Resource::OtherFuel2 => "OtherFuel2",
        }
    }
}

/// End-use category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndUse {
    Heating,
    Cooling,
    InteriorLights,
    ExteriorLights,
    InteriorEquipment,
    ExteriorEquipment,
    Fans,
    Pumps,
    HeatRejection,
    Humidification,
    HeatRecovery,
    WaterSystems,
    Refrigeration,
    Generators,
}

impl EndUse {
    pub fn name(&self) -> &'static str {
        match self {
            EndUse::Heating => "Heating",
            EndUse::Cooling => "Cooling",
            EndUse::InteriorLights => "InteriorLights",
            EndUse::ExteriorLights => "ExteriorLights",
            EndUse::InteriorEquipment => "InteriorEquipment",
            EndUse::ExteriorEquipment => "ExteriorEquipment",
            EndUse::Fans => "Fans",
            EndUse::Pumps => "Pumps",
            EndUse::HeatRejection => "HeatRejection",
            EndUse::Humidification => "Humidification",
            EndUse::HeatRecovery => "HeatRecovery",
            EndUse::WaterSystems => "WaterSystems",
            EndUse::Refrigeration => "Refrigeration",
            EndUse::Generators => "Generators",
        }
    }
}

/// Grouping level for a meter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Group {
    Facility,
    Building,
    HVAC,
    Plant,
    Zone(String),
}

impl Group {
    pub fn name(&self) -> String {
        match self {
            Group::Facility => "Facility".to_string(),
            Group::Building => "Building".to_string(),
            Group::HVAC => "HVAC".to_string(),
            Group::Plant => "Plant".to_string(),
            Group::Zone(z) => format!("Zone:{z}"),
        }
    }
}

/// A meter that accumulates energy/resource values.
#[derive(Debug, Clone)]
pub struct Meter {
    pub name: String,
    pub resource: Resource,
    pub end_use: Option<EndUse>,
    pub group: Group,
    pub units: String,
    /// Accumulators for each reporting frequency.
    pub accumulators: HashMap<ReportFreq, PeriodAccumulator>,
    /// Indices into the OutputVariable vector of source variables.
    pub source_var_indices: Vec<usize>,
}

impl Meter {
    pub fn new(name: &str, resource: Resource, end_use: Option<EndUse>, group: Group) -> Self {
        let units = match resource {
            Resource::Water => "m3".to_string(),
            _ => "J".to_string(),
        };
        Self {
            name: name.to_string(),
            resource,
            end_use,
            group,
            units,
            accumulators: HashMap::new(),
            source_var_indices: Vec::new(),
        }
    }

    /// Add a value at the given frequency and timestamp.
    pub fn add_value(&mut self, freq: ReportFreq, value: f64, timestamp: &TimeStamp) {
        self.accumulators
            .entry(freq)
            .or_insert_with(PeriodAccumulator::new)
            .add(value, timestamp);
    }

    /// Report the accumulated value at a given frequency and reset.
    pub fn report_and_reset(&mut self, freq: ReportFreq) -> f64 {
        if let Some(acc) = self.accumulators.get_mut(&freq) {
            let val = acc.value_sum;
            acc.reset();
            val
        } else {
            0.0
        }
    }
}

/// Manages meters with automatic hierarchy construction.
pub struct MeterManager {
    pub meters: Vec<Meter>,
    name_index: HashMap<String, usize>,
}

impl MeterManager {
    pub fn new() -> Self {
        Self {
            meters: Vec::new(),
            name_index: HashMap::new(),
        }
    }

    /// Format a standard meter name.
    pub fn format_name(resource: Resource, end_use: Option<EndUse>, group: &Group) -> String {
        let mut parts = Vec::new();
        parts.push(resource.name().to_string());
        if let Some(eu) = end_use {
            parts.push(eu.name().to_string());
        }
        parts.push(group.name());
        parts.join(":")
    }

    /// Get or create a meter, returning its index. Idempotent.
    pub fn get_or_create(
        &mut self,
        resource: Resource,
        end_use: Option<EndUse>,
        group: Group,
    ) -> usize {
        let name = Self::format_name(resource, end_use, &group);
        let upper = name.to_uppercase();
        if let Some(&idx) = self.name_index.get(&upper) {
            return idx;
        }
        let idx = self.meters.len();
        self.meters.push(Meter::new(&name, resource, end_use, group));
        self.name_index.insert(upper, idx);
        idx
    }

    /// Build the standard hierarchy of meters for a resource + end-use + group.
    /// Creates the specific meter plus facility/group rollup meters.
    pub fn build_hierarchy(
        &mut self,
        resource: Resource,
        end_use: EndUse,
        group: Group,
    ) -> Vec<usize> {
        let mut indices = Vec::new();

        // Specific meter (e.g., Electricity:Heating:Zone:Zone1)
        indices.push(self.get_or_create(resource, Some(end_use), group.clone()));

        // End-use rollup at facility level (e.g., Electricity:Heating:Facility)
        if group != Group::Facility {
            indices.push(self.get_or_create(resource, Some(end_use), Group::Facility));
        }

        // Resource-only at facility (e.g., Electricity:Facility)
        indices.push(self.get_or_create(resource, None, Group::Facility));

        indices
    }

    /// Attach a source variable index to a meter.
    pub fn attach_variable(&mut self, meter_idx: usize, var_idx: usize) {
        if meter_idx < self.meters.len() {
            let meter = &mut self.meters[meter_idx];
            if !meter.source_var_indices.contains(&var_idx) {
                meter.source_var_indices.push(var_idx);
            }
        }
    }

    /// Update meters from source variable values at a given frequency.
    pub fn update_from_sources(
        &mut self,
        source_values: &[(usize, f64)],
        freq: ReportFreq,
        timestamp: &TimeStamp,
    ) {
        for meter in &mut self.meters {
            let mut total = 0.0;
            for &(var_idx, val) in source_values {
                if meter.source_var_indices.contains(&var_idx) {
                    total += val;
                }
            }
            if total != 0.0 {
                meter.add_value(freq, total, timestamp);
            }
        }
    }

    /// Find a meter by name (case-insensitive).
    pub fn find_by_name(&self, name: &str) -> Option<usize> {
        self.name_index.get(&name.to_uppercase()).copied()
    }

    /// Number of meters.
    pub fn len(&self) -> usize {
        self.meters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.meters.is_empty()
    }
}

impl Default for MeterManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_meter() {
        let mut mgr = MeterManager::new();
        let idx = mgr.get_or_create(Resource::Electricity, Some(EndUse::Heating), Group::Facility);
        assert_eq!(idx, 0);
        assert_eq!(mgr.meters[0].name, "Electricity:Heating:Facility");
    }

    #[test]
    fn idempotent_get_or_create() {
        let mut mgr = MeterManager::new();
        let idx1 = mgr.get_or_create(Resource::Electricity, None, Group::Facility);
        let idx2 = mgr.get_or_create(Resource::Electricity, None, Group::Facility);
        assert_eq!(idx1, idx2);
        assert_eq!(mgr.len(), 1);
    }

    #[test]
    fn hierarchy_creation() {
        let mut mgr = MeterManager::new();
        let indices = mgr.build_hierarchy(
            Resource::Electricity,
            EndUse::Heating,
            Group::Zone("Zone1".into()),
        );
        assert!(indices.len() >= 3);
        // Should have specific, end-use facility rollup, and resource-only facility
        assert!(mgr.find_by_name("Electricity:Heating:Zone:Zone1").is_some());
        assert!(mgr.find_by_name("Electricity:Heating:Facility").is_some());
        assert!(mgr.find_by_name("Electricity:Facility").is_some());
    }

    #[test]
    fn update_from_sources() {
        let mut mgr = MeterManager::new();
        let meter_idx = mgr.get_or_create(Resource::Electricity, None, Group::Facility);
        mgr.attach_variable(meter_idx, 0);
        mgr.attach_variable(meter_idx, 1);

        let ts = TimeStamp { month: 1, day: 1, hour: 1, minute: 0 };
        let source_vals = vec![(0, 100.0), (1, 200.0)];
        mgr.update_from_sources(&source_vals, ReportFreq::Hourly, &ts);

        let val = mgr.meters[meter_idx].report_and_reset(ReportFreq::Hourly);
        assert!((val - 300.0).abs() < 1e-10);
    }

    #[test]
    fn report_and_reset_meter() {
        let mut mgr = MeterManager::new();
        let idx = mgr.get_or_create(Resource::NaturalGas, Some(EndUse::Heating), Group::Building);
        let ts = TimeStamp { month: 12, day: 1, hour: 8, minute: 0 };

        mgr.meters[idx].add_value(ReportFreq::Hourly, 500.0, &ts);
        mgr.meters[idx].add_value(ReportFreq::Hourly, 300.0, &ts);

        let val = mgr.meters[idx].report_and_reset(ReportFreq::Hourly);
        assert!((val - 800.0).abs() < 1e-10);

        // After reset
        let val2 = mgr.meters[idx].report_and_reset(ReportFreq::Hourly);
        assert!((val2).abs() < 1e-10);
    }

    #[test]
    fn meter_name_format() {
        let name = MeterManager::format_name(
            Resource::Water,
            Some(EndUse::WaterSystems),
            &Group::Zone("Bathroom".into()),
        );
        assert_eq!(name, "Water:WaterSystems:Zone:Bathroom");
    }

    #[test]
    fn case_insensitive_find() {
        let mut mgr = MeterManager::new();
        mgr.get_or_create(Resource::Electricity, None, Group::Facility);
        assert!(mgr.find_by_name("electricity:facility").is_some());
        assert!(mgr.find_by_name("ELECTRICITY:FACILITY").is_some());
        assert!(mgr.find_by_name("Electricity:Facility").is_some());
        assert!(mgr.find_by_name("nonexistent").is_none());
    }
}
