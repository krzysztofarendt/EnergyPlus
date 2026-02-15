//! Predefined tabular reports, monthly/annual aggregation, and structured output.
//!
//! This module provides:
//! - Standard EnergyPlus summary reports (Input Verification, Equipment, Envelope, System)
//! - Monthly and annual value aggregation with peak tracking
//! - A `ResultsFramework` for structured JSON output

use std::collections::HashMap;

use crate::tabular::{CellValue, Table, TabularReport};

// ---------------------------------------------------------------------------
// Report Data Structs
// ---------------------------------------------------------------------------

/// Data for a single zone in the zone summary table.
#[derive(Debug, Clone)]
pub struct ZoneSummaryData {
    pub name: String,
    pub area: f64,
    pub volume: f64,
    pub multiplier: f64,
    pub above_ground_wall_area: f64,
    pub underground_wall_area: f64,
    pub window_area: f64,
    pub design_cooling_load: f64,
    pub design_heating_load: f64,
}

/// Data for a single piece of equipment in the equipment summary.
#[derive(Debug, Clone)]
pub struct EquipmentData {
    pub name: String,
    pub equipment_type: String,
    pub nominal_capacity: f64,
    pub nominal_efficiency: f64,
}

/// Data for an opaque exterior surface in the envelope summary.
#[derive(Debug, Clone)]
pub struct OpaqueSurfaceData {
    pub name: String,
    pub construction: String,
    pub u_factor: f64,
    pub gross_area: f64,
    pub azimuth: f64,
    pub tilt: f64,
}

/// Data for a fenestration surface in the envelope summary.
#[derive(Debug, Clone)]
pub struct FenestrationData {
    pub name: String,
    pub construction: String,
    pub u_factor: f64,
    pub shgc: f64,
    pub visible_transmittance: f64,
    pub area: f64,
    pub parent_surface: String,
}

// ---------------------------------------------------------------------------
// Predefined Tabular Reports
// ---------------------------------------------------------------------------

const ZONE_SUMMARY_COLUMNS: &[&str] = &[
    "Area m2",
    "Volume m3",
    "Multiplier",
    "Above Ground Walls m2",
    "Underground Walls m2",
    "Windows m2",
    "Design Cooling Load W",
    "Design Heating Load W",
];

/// Create a "Zone Summary" table from zone data.
pub fn create_zone_summary(zones: &[ZoneSummaryData]) -> Table {
    let column_headers: Vec<String> = ZONE_SUMMARY_COLUMNS.iter().map(|s| s.to_string()).collect();
    let row_headers: Vec<String> = zones.iter().map(|z| z.name.clone()).collect();

    let mut table = Table::new("Zone Summary", column_headers, row_headers);

    for (i, zone) in zones.iter().enumerate() {
        table.set(i, 0, CellValue::Number(zone.area));
        table.set(i, 1, CellValue::Number(zone.volume));
        table.set(i, 2, CellValue::Number(zone.multiplier));
        table.set(i, 3, CellValue::Number(zone.above_ground_wall_area));
        table.set(i, 4, CellValue::Number(zone.underground_wall_area));
        table.set(i, 5, CellValue::Number(zone.window_area));
        table.set(i, 6, CellValue::Number(zone.design_cooling_load));
        table.set(i, 7, CellValue::Number(zone.design_heating_load));
    }

    table
}

/// Create an `InputVerificationAndResultsSummary` report containing the zone summary.
pub fn create_input_verification_report(zones: &[ZoneSummaryData]) -> TabularReport {
    let mut report = TabularReport::new("InputVerificationAndResultsSummary");
    report.add_table(create_zone_summary(zones));
    report
}

const COOLING_EQUIPMENT_COLUMNS: &[&str] = &[
    "Type",
    "Nominal Capacity W",
    "Nominal Efficiency W/W",
];

const HEATING_EQUIPMENT_COLUMNS: &[&str] = &[
    "Type",
    "Nominal Capacity W",
    "Nominal Efficiency",
];

/// Create an `EquipmentSummary` report with cooling and heating equipment tables.
pub fn create_equipment_summary(
    cooling: &[EquipmentData],
    heating: &[EquipmentData],
) -> TabularReport {
    let mut report = TabularReport::new("EquipmentSummary");

    // Cooling Equipment table
    {
        let col_headers: Vec<String> = COOLING_EQUIPMENT_COLUMNS.iter().map(|s| s.to_string()).collect();
        let row_headers: Vec<String> = cooling.iter().map(|e| e.name.clone()).collect();
        let mut table = Table::new("Cooling Equipment", col_headers, row_headers);
        for (i, eq) in cooling.iter().enumerate() {
            table.set(i, 0, CellValue::Text(eq.equipment_type.clone()));
            table.set(i, 1, CellValue::Number(eq.nominal_capacity));
            table.set(i, 2, CellValue::Number(eq.nominal_efficiency));
        }
        report.add_table(table);
    }

    // Heating Equipment table
    {
        let col_headers: Vec<String> = HEATING_EQUIPMENT_COLUMNS.iter().map(|s| s.to_string()).collect();
        let row_headers: Vec<String> = heating.iter().map(|e| e.name.clone()).collect();
        let mut table = Table::new("Heating Equipment", col_headers, row_headers);
        for (i, eq) in heating.iter().enumerate() {
            table.set(i, 0, CellValue::Text(eq.equipment_type.clone()));
            table.set(i, 1, CellValue::Number(eq.nominal_capacity));
            table.set(i, 2, CellValue::Number(eq.nominal_efficiency));
        }
        report.add_table(table);
    }

    report
}

const OPAQUE_EXTERIOR_COLUMNS: &[&str] = &[
    "Construction",
    "U-Factor W/(m2-K)",
    "Gross Area m2",
    "Azimuth deg",
    "Tilt deg",
];

const FENESTRATION_COLUMNS: &[&str] = &[
    "Construction",
    "U-Factor",
    "SHGC",
    "Visible Transmittance",
    "Area m2",
    "Parent Surface",
];

/// Create an `EnvelopeSummary` report with opaque exterior and fenestration tables.
pub fn create_envelope_summary(
    opaque: &[OpaqueSurfaceData],
    fenestration: &[FenestrationData],
) -> TabularReport {
    let mut report = TabularReport::new("EnvelopeSummary");

    // Opaque Exterior table
    {
        let col_headers: Vec<String> = OPAQUE_EXTERIOR_COLUMNS.iter().map(|s| s.to_string()).collect();
        let row_headers: Vec<String> = opaque.iter().map(|s| s.name.clone()).collect();
        let mut table = Table::new("Opaque Exterior", col_headers, row_headers);
        for (i, surf) in opaque.iter().enumerate() {
            table.set(i, 0, CellValue::Text(surf.construction.clone()));
            table.set(i, 1, CellValue::Number(surf.u_factor));
            table.set(i, 2, CellValue::Number(surf.gross_area));
            table.set(i, 3, CellValue::Number(surf.azimuth));
            table.set(i, 4, CellValue::Number(surf.tilt));
        }
        report.add_table(table);
    }

    // Fenestration table
    {
        let col_headers: Vec<String> = FENESTRATION_COLUMNS.iter().map(|s| s.to_string()).collect();
        let row_headers: Vec<String> = fenestration.iter().map(|s| s.name.clone()).collect();
        let mut table = Table::new("Fenestration", col_headers, row_headers);
        for (i, fen) in fenestration.iter().enumerate() {
            table.set(i, 0, CellValue::Text(fen.construction.clone()));
            table.set(i, 1, CellValue::Number(fen.u_factor));
            table.set(i, 2, CellValue::Number(fen.shgc));
            table.set(i, 3, CellValue::Number(fen.visible_transmittance));
            table.set(i, 4, CellValue::Number(fen.area));
            table.set(i, 5, CellValue::Text(fen.parent_surface.clone()));
        }
        report.add_table(table);
    }

    report
}

/// Data for the "Time Setpoint Not Met" table in the System Summary.
#[derive(Debug, Clone)]
pub struct SetpointNotMetData {
    pub zone_name: String,
    pub heating_hours: f64,
    pub cooling_hours: f64,
}

/// Create a `SystemSummary` report with the "Time Setpoint Not Met" table.
pub fn create_system_summary(data: &[SetpointNotMetData]) -> TabularReport {
    let mut report = TabularReport::new("SystemSummary");

    let col_headers = vec!["Heating Hours".to_string(), "Cooling Hours".to_string()];
    let row_headers: Vec<String> = data.iter().map(|d| d.zone_name.clone()).collect();
    let mut table = Table::new("Time Setpoint Not Met", col_headers, row_headers);

    for (i, entry) in data.iter().enumerate() {
        table.set(i, 0, CellValue::Number(entry.heating_hours));
        table.set(i, 1, CellValue::Number(entry.cooling_hours));
    }

    report.add_table(table);
    report
}

// ---------------------------------------------------------------------------
// Monthly / Annual Aggregation
// ---------------------------------------------------------------------------

/// A timestamp associated with a peak value.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeOfPeak {
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub value: f64,
}

impl TimeOfPeak {
    /// Create a new `TimeOfPeak`.
    pub fn new(month: u32, day: u32, hour: u32, minute: u32, value: f64) -> Self {
        Self {
            month,
            day,
            hour,
            minute,
            value,
        }
    }
}

/// A timestamp for aggregation operations.
#[derive(Debug, Clone)]
pub struct AggTimestamp {
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

impl AggTimestamp {
    pub fn new(month: u32, day: u32, hour: u32, minute: u32) -> Self {
        Self {
            month,
            day,
            hour,
            minute,
        }
    }
}

/// Accumulator for a single month: tracks sum, count, and peak.
#[derive(Debug, Clone)]
struct MonthAccumulator {
    sum: f64,
    count: u64,
    peak: Option<TimeOfPeak>,
}

impl MonthAccumulator {
    fn new() -> Self {
        Self {
            sum: 0.0,
            count: 0,
            peak: None,
        }
    }

    fn add(&mut self, value: f64, timestamp: &AggTimestamp) {
        self.sum += value;
        self.count += 1;
        match &self.peak {
            None => {
                self.peak = Some(TimeOfPeak::new(
                    timestamp.month,
                    timestamp.day,
                    timestamp.hour,
                    timestamp.minute,
                    value,
                ));
            }
            Some(existing) => {
                if value > existing.value {
                    self.peak = Some(TimeOfPeak::new(
                        timestamp.month,
                        timestamp.day,
                        timestamp.hour,
                        timestamp.minute,
                        value,
                    ));
                }
            }
        }
    }
}

/// Aggregates values on a per-month basis, tracking sums and peaks.
#[derive(Debug, Clone)]
pub struct MonthlyAggregator {
    months: Vec<MonthAccumulator>,
}

impl MonthlyAggregator {
    /// Create a new aggregator with 12 empty month accumulators.
    pub fn new() -> Self {
        Self {
            months: (0..12).map(|_| MonthAccumulator::new()).collect(),
        }
    }

    /// Add a value for a given month (1-based: January = 1, December = 12).
    ///
    /// # Panics
    ///
    /// Panics if `month` is not in 1..=12.
    pub fn add_value(&mut self, month: u32, value: f64, timestamp: &AggTimestamp) {
        assert!(
            (1..=12).contains(&month),
            "month must be 1..=12, got {month}"
        );
        self.months[(month - 1) as usize].add(value, timestamp);
    }

    /// Get the sum of values for each month (index 0 = January).
    pub fn get_monthly_sums(&self) -> [f64; 12] {
        let mut sums = [0.0; 12];
        for (i, acc) in self.months.iter().enumerate() {
            sums[i] = acc.sum;
        }
        sums
    }

    /// Get the peak value and timestamp for each month (index 0 = January).
    ///
    /// Returns a default `TimeOfPeak` with value 0.0 for months with no data.
    pub fn get_monthly_peaks(&self) -> [TimeOfPeak; 12] {
        let mut peaks: [TimeOfPeak; 12] = std::array::from_fn(|i| {
            TimeOfPeak::new((i as u32) + 1, 1, 0, 0, 0.0)
        });
        for (i, acc) in self.months.iter().enumerate() {
            if let Some(ref peak) = acc.peak {
                peaks[i] = peak.clone();
            }
        }
        peaks
    }

    /// Get the count of values added for each month (index 0 = January).
    pub fn get_monthly_counts(&self) -> [u64; 12] {
        let mut counts = [0u64; 12];
        for (i, acc) in self.months.iter().enumerate() {
            counts[i] = acc.count;
        }
        counts
    }

    /// Get the average value for each month (NaN for months with no data).
    pub fn get_monthly_averages(&self) -> [f64; 12] {
        let mut avgs = [f64::NAN; 12];
        for (i, acc) in self.months.iter().enumerate() {
            if acc.count > 0 {
                avgs[i] = acc.sum / acc.count as f64;
            }
        }
        avgs
    }
}

impl Default for MonthlyAggregator {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregates values over the entire year, tracking total sum and peak.
#[derive(Debug, Clone)]
pub struct AnnualAggregator {
    sum: f64,
    count: u64,
    peak: Option<TimeOfPeak>,
}

impl AnnualAggregator {
    /// Create a new annual aggregator.
    pub fn new() -> Self {
        Self {
            sum: 0.0,
            count: 0,
            peak: None,
        }
    }

    /// Add a value with its associated timestamp.
    pub fn add_value(&mut self, value: f64, timestamp: &AggTimestamp) {
        self.sum += value;
        self.count += 1;
        match &self.peak {
            None => {
                self.peak = Some(TimeOfPeak::new(
                    timestamp.month,
                    timestamp.day,
                    timestamp.hour,
                    timestamp.minute,
                    value,
                ));
            }
            Some(existing) => {
                if value > existing.value {
                    self.peak = Some(TimeOfPeak::new(
                        timestamp.month,
                        timestamp.day,
                        timestamp.hour,
                        timestamp.minute,
                        value,
                    ));
                }
            }
        }
    }

    /// Get the annual sum of all added values.
    pub fn get_annual_sum(&self) -> f64 {
        self.sum
    }

    /// Get the number of values added.
    pub fn get_count(&self) -> u64 {
        self.count
    }

    /// Get the annual average (NaN if no values added).
    pub fn get_average(&self) -> f64 {
        if self.count > 0 {
            self.sum / self.count as f64
        } else {
            f64::NAN
        }
    }

    /// Get the peak value and its timestamp.
    ///
    /// Returns a default `TimeOfPeak` with value 0.0 if no data has been added.
    pub fn get_peak(&self) -> TimeOfPeak {
        self.peak
            .clone()
            .unwrap_or(TimeOfPeak::new(1, 1, 0, 0, 0.0))
    }
}

impl Default for AnnualAggregator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ResultsFramework
// ---------------------------------------------------------------------------

/// Structured results storage for JSON export.
///
/// Stores timestep-level simulation results organized by variable name,
/// along with corresponding timestamps. Results can be exported to JSON
/// for post-processing or visualization.
#[derive(Debug, Clone)]
pub struct ResultsFramework {
    /// Map from variable name to its time-series values.
    pub simulation_results: HashMap<String, Vec<f64>>,
    /// Timestamps corresponding to each timestep entry.
    pub timestamps: Vec<String>,
}

impl ResultsFramework {
    /// Create an empty results framework.
    pub fn new() -> Self {
        Self {
            simulation_results: HashMap::new(),
            timestamps: Vec::new(),
        }
    }

    /// Add a data point for a variable at a given timestamp.
    ///
    /// If this is the first value for a new timestamp, the timestamp is
    /// appended to the timestamps list. Variable time-series are padded
    /// with 0.0 for any missed timesteps to keep lengths consistent.
    pub fn add_timestep_data(&mut self, timestamp: &str, variable_name: &str, value: f64) {
        // Add timestamp if it is new (different from the last one).
        let ts_index = if self.timestamps.last().map(|s| s.as_str()) == Some(timestamp) {
            self.timestamps.len() - 1
        } else {
            self.timestamps.push(timestamp.to_string());
            self.timestamps.len() - 1
        };

        let series = self
            .simulation_results
            .entry(variable_name.to_string())
            .or_default();

        // Pad with 0.0 if this series is behind.
        while series.len() < ts_index {
            series.push(0.0);
        }

        if series.len() == ts_index {
            series.push(value);
        } else {
            // Same timestamp, overwrite.
            series[ts_index] = value;
        }
    }

    /// Export results to a JSON string.
    ///
    /// The output has the form:
    /// ```json
    /// {
    ///   "timestamps": ["01/01 01:00", ...],
    ///   "variables": {
    ///     "Zone Mean Air Temperature": [22.0, ...],
    ///     ...
    ///   }
    /// }
    /// ```
    pub fn to_json(&self) -> String {
        // Build a sorted key list for deterministic output.
        let mut keys: Vec<&String> = self.simulation_results.keys().collect();
        keys.sort();

        let mut variables_map = serde_json::Map::new();
        for key in &keys {
            let values = &self.simulation_results[*key];
            let json_values: Vec<serde_json::Value> = values
                .iter()
                .map(|v| serde_json::Value::from(*v))
                .collect();
            variables_map.insert((*key).clone(), serde_json::Value::Array(json_values));
        }

        let timestamps_json: Vec<serde_json::Value> = self
            .timestamps
            .iter()
            .map(|t| serde_json::Value::String(t.clone()))
            .collect();

        let mut root = serde_json::Map::new();
        root.insert(
            "timestamps".to_string(),
            serde_json::Value::Array(timestamps_json),
        );
        root.insert(
            "variables".to_string(),
            serde_json::Value::Object(variables_map),
        );

        serde_json::to_string_pretty(&serde_json::Value::Object(root))
            .expect("JSON serialization should not fail")
    }

    /// Number of timestep entries recorded.
    pub fn timestep_count(&self) -> usize {
        self.timestamps.len()
    }

    /// Number of distinct variables recorded.
    pub fn variable_count(&self) -> usize {
        self.simulation_results.len()
    }
}

impl Default for ResultsFramework {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Zone Summary tests
    // -----------------------------------------------------------------------

    fn sample_zones() -> Vec<ZoneSummaryData> {
        vec![
            ZoneSummaryData {
                name: "Zone1".to_string(),
                area: 50.0,
                volume: 150.0,
                multiplier: 1.0,
                above_ground_wall_area: 80.0,
                underground_wall_area: 0.0,
                window_area: 12.0,
                design_cooling_load: 5000.0,
                design_heating_load: 4000.0,
            },
            ZoneSummaryData {
                name: "Zone2".to_string(),
                area: 75.0,
                volume: 225.0,
                multiplier: 2.0,
                above_ground_wall_area: 120.0,
                underground_wall_area: 30.0,
                window_area: 18.0,
                design_cooling_load: 8000.0,
                design_heating_load: 6000.0,
            },
        ]
    }

    #[test]
    fn zone_summary_table_dimensions() {
        let zones = sample_zones();
        let table = create_zone_summary(&zones);
        assert_eq!(table.title, "Zone Summary");
        assert_eq!(table.row_headers.len(), 2);
        assert_eq!(table.column_headers.len(), 8);
        assert_eq!(table.cells.len(), 2);
        assert_eq!(table.cells[0].len(), 8);
    }

    #[test]
    fn zone_summary_row_headers() {
        let zones = sample_zones();
        let table = create_zone_summary(&zones);
        assert_eq!(table.row_headers[0], "Zone1");
        assert_eq!(table.row_headers[1], "Zone2");
    }

    #[test]
    fn zone_summary_cell_values() {
        let zones = sample_zones();
        let table = create_zone_summary(&zones);

        // Zone1 area
        assert_eq!(table.cells[0][0], CellValue::Number(50.0));
        // Zone1 volume
        assert_eq!(table.cells[0][1], CellValue::Number(150.0));
        // Zone2 multiplier
        assert_eq!(table.cells[1][2], CellValue::Number(2.0));
        // Zone2 underground wall area
        assert_eq!(table.cells[1][4], CellValue::Number(30.0));
        // Zone1 design cooling load
        assert_eq!(table.cells[0][6], CellValue::Number(5000.0));
        // Zone2 design heating load
        assert_eq!(table.cells[1][7], CellValue::Number(6000.0));
    }

    #[test]
    fn zone_summary_empty() {
        let table = create_zone_summary(&[]);
        assert_eq!(table.row_headers.len(), 0);
        assert_eq!(table.cells.len(), 0);
    }

    #[test]
    fn input_verification_report_structure() {
        let zones = sample_zones();
        let report = create_input_verification_report(&zones);
        assert_eq!(report.name, "InputVerificationAndResultsSummary");
        assert_eq!(report.tables.len(), 1);
        assert_eq!(report.tables[0].title, "Zone Summary");
    }

    // -----------------------------------------------------------------------
    // Equipment Summary tests
    // -----------------------------------------------------------------------

    fn sample_cooling() -> Vec<EquipmentData> {
        vec![
            EquipmentData {
                name: "Chiller1".to_string(),
                equipment_type: "Chiller:Electric:EIR".to_string(),
                nominal_capacity: 100000.0,
                nominal_efficiency: 3.5,
            },
            EquipmentData {
                name: "DX Coil1".to_string(),
                equipment_type: "Coil:Cooling:DX:SingleSpeed".to_string(),
                nominal_capacity: 25000.0,
                nominal_efficiency: 3.0,
            },
        ]
    }

    fn sample_heating() -> Vec<EquipmentData> {
        vec![EquipmentData {
            name: "Boiler1".to_string(),
            equipment_type: "Boiler:HotWater".to_string(),
            nominal_capacity: 80000.0,
            nominal_efficiency: 0.9,
        }]
    }

    #[test]
    fn equipment_summary_two_tables() {
        let report = create_equipment_summary(&sample_cooling(), &sample_heating());
        assert_eq!(report.name, "EquipmentSummary");
        assert_eq!(report.tables.len(), 2);
        assert_eq!(report.tables[0].title, "Cooling Equipment");
        assert_eq!(report.tables[1].title, "Heating Equipment");
    }

    #[test]
    fn equipment_summary_cooling_entries() {
        let report = create_equipment_summary(&sample_cooling(), &sample_heating());
        let cooling_table = &report.tables[0];
        assert_eq!(cooling_table.row_headers.len(), 2);
        assert_eq!(cooling_table.row_headers[0], "Chiller1");
        assert_eq!(
            cooling_table.cells[0][0],
            CellValue::Text("Chiller:Electric:EIR".to_string())
        );
        assert_eq!(cooling_table.cells[0][1], CellValue::Number(100000.0));
        assert_eq!(cooling_table.cells[0][2], CellValue::Number(3.5));
    }

    #[test]
    fn equipment_summary_heating_entries() {
        let report = create_equipment_summary(&sample_cooling(), &sample_heating());
        let heating_table = &report.tables[1];
        assert_eq!(heating_table.row_headers.len(), 1);
        assert_eq!(heating_table.row_headers[0], "Boiler1");
        assert_eq!(heating_table.cells[0][1], CellValue::Number(80000.0));
        assert_eq!(heating_table.cells[0][2], CellValue::Number(0.9));
    }

    #[test]
    fn equipment_summary_empty_lists() {
        let report = create_equipment_summary(&[], &[]);
        assert_eq!(report.tables.len(), 2);
        assert_eq!(report.tables[0].row_headers.len(), 0);
        assert_eq!(report.tables[1].row_headers.len(), 0);
    }

    // -----------------------------------------------------------------------
    // Envelope Summary tests
    // -----------------------------------------------------------------------

    fn sample_opaque() -> Vec<OpaqueSurfaceData> {
        vec![OpaqueSurfaceData {
            name: "Wall_North".to_string(),
            construction: "BrickWall".to_string(),
            u_factor: 0.45,
            gross_area: 30.0,
            azimuth: 0.0,
            tilt: 90.0,
        }]
    }

    fn sample_fenestration() -> Vec<FenestrationData> {
        vec![FenestrationData {
            name: "Window_South".to_string(),
            construction: "DoubleGlazing".to_string(),
            u_factor: 2.8,
            shgc: 0.6,
            visible_transmittance: 0.7,
            area: 4.0,
            parent_surface: "Wall_South".to_string(),
        }]
    }

    #[test]
    fn envelope_summary_opaque_table() {
        let report = create_envelope_summary(&sample_opaque(), &sample_fenestration());
        assert_eq!(report.name, "EnvelopeSummary");
        assert_eq!(report.tables.len(), 2);

        let opaque_table = &report.tables[0];
        assert_eq!(opaque_table.title, "Opaque Exterior");
        assert_eq!(opaque_table.row_headers[0], "Wall_North");
        assert_eq!(
            opaque_table.cells[0][0],
            CellValue::Text("BrickWall".to_string())
        );
        assert_eq!(opaque_table.cells[0][1], CellValue::Number(0.45));
        assert_eq!(opaque_table.cells[0][2], CellValue::Number(30.0));
        assert_eq!(opaque_table.cells[0][3], CellValue::Number(0.0));
        assert_eq!(opaque_table.cells[0][4], CellValue::Number(90.0));
    }

    #[test]
    fn envelope_summary_fenestration_table() {
        let report = create_envelope_summary(&sample_opaque(), &sample_fenestration());
        let fen_table = &report.tables[1];
        assert_eq!(fen_table.title, "Fenestration");
        assert_eq!(fen_table.row_headers[0], "Window_South");
        assert_eq!(
            fen_table.cells[0][0],
            CellValue::Text("DoubleGlazing".to_string())
        );
        assert_eq!(fen_table.cells[0][1], CellValue::Number(2.8));
        assert_eq!(fen_table.cells[0][2], CellValue::Number(0.6));
        assert_eq!(fen_table.cells[0][3], CellValue::Number(0.7));
        assert_eq!(fen_table.cells[0][4], CellValue::Number(4.0));
        assert_eq!(
            fen_table.cells[0][5],
            CellValue::Text("Wall_South".to_string())
        );
    }

    #[test]
    fn envelope_summary_empty() {
        let report = create_envelope_summary(&[], &[]);
        assert_eq!(report.tables[0].row_headers.len(), 0);
        assert_eq!(report.tables[1].row_headers.len(), 0);
    }

    // -----------------------------------------------------------------------
    // System Summary tests
    // -----------------------------------------------------------------------

    #[test]
    fn system_summary_setpoint_not_met() {
        let data = vec![
            SetpointNotMetData {
                zone_name: "Zone1".to_string(),
                heating_hours: 120.5,
                cooling_hours: 85.0,
            },
            SetpointNotMetData {
                zone_name: "Zone2".to_string(),
                heating_hours: 0.0,
                cooling_hours: 200.0,
            },
        ];
        let report = create_system_summary(&data);
        assert_eq!(report.name, "SystemSummary");
        assert_eq!(report.tables[0].title, "Time Setpoint Not Met");
        assert_eq!(report.tables[0].cells[0][0], CellValue::Number(120.5));
        assert_eq!(report.tables[0].cells[1][1], CellValue::Number(200.0));
    }

    // -----------------------------------------------------------------------
    // Monthly Aggregator tests
    // -----------------------------------------------------------------------

    #[test]
    fn monthly_aggregator_single_month_sum() {
        let mut agg = MonthlyAggregator::new();
        let ts = AggTimestamp::new(1, 15, 12, 0);
        agg.add_value(1, 100.0, &ts);
        agg.add_value(1, 200.0, &ts);
        agg.add_value(1, 300.0, &ts);

        let sums = agg.get_monthly_sums();
        assert!((sums[0] - 600.0).abs() < 1e-10);
        // Other months should be zero.
        for i in 1..12 {
            assert!((sums[i]).abs() < 1e-10);
        }
    }

    #[test]
    fn monthly_aggregator_multiple_months() {
        let mut agg = MonthlyAggregator::new();
        for month in 1..=12 {
            let ts = AggTimestamp::new(month, 1, 0, 0);
            agg.add_value(month, month as f64 * 10.0, &ts);
        }
        let sums = agg.get_monthly_sums();
        for i in 0..12 {
            let expected = (i as f64 + 1.0) * 10.0;
            assert!((sums[i] - expected).abs() < 1e-10);
        }
    }

    #[test]
    fn monthly_aggregator_peak_tracking() {
        let mut agg = MonthlyAggregator::new();

        // Add several values in January
        agg.add_value(1, 50.0, &AggTimestamp::new(1, 10, 14, 0));
        agg.add_value(1, 150.0, &AggTimestamp::new(1, 15, 15, 30));
        agg.add_value(1, 100.0, &AggTimestamp::new(1, 20, 12, 0));

        let peaks = agg.get_monthly_peaks();
        assert_eq!(peaks[0].value, 150.0);
        assert_eq!(peaks[0].month, 1);
        assert_eq!(peaks[0].day, 15);
        assert_eq!(peaks[0].hour, 15);
        assert_eq!(peaks[0].minute, 30);
    }

    #[test]
    fn monthly_aggregator_peak_update_when_new_max() {
        let mut agg = MonthlyAggregator::new();

        agg.add_value(7, 500.0, &AggTimestamp::new(7, 1, 12, 0));
        let peaks = agg.get_monthly_peaks();
        assert_eq!(peaks[6].value, 500.0);
        assert_eq!(peaks[6].day, 1);

        // Higher value should replace the peak.
        agg.add_value(7, 750.0, &AggTimestamp::new(7, 21, 14, 30));
        let peaks = agg.get_monthly_peaks();
        assert_eq!(peaks[6].value, 750.0);
        assert_eq!(peaks[6].day, 21);
        assert_eq!(peaks[6].hour, 14);
        assert_eq!(peaks[6].minute, 30);

        // Lower value should NOT replace the peak.
        agg.add_value(7, 600.0, &AggTimestamp::new(7, 25, 10, 0));
        let peaks = agg.get_monthly_peaks();
        assert_eq!(peaks[6].value, 750.0);
        assert_eq!(peaks[6].day, 21);
    }

    #[test]
    fn monthly_aggregator_empty_months_have_default_peaks() {
        let agg = MonthlyAggregator::new();
        let peaks = agg.get_monthly_peaks();
        for (i, peak) in peaks.iter().enumerate() {
            assert_eq!(peak.month, (i as u32) + 1);
            assert_eq!(peak.value, 0.0);
        }
    }

    #[test]
    fn monthly_aggregator_counts() {
        let mut agg = MonthlyAggregator::new();
        let ts = AggTimestamp::new(3, 1, 0, 0);
        agg.add_value(3, 10.0, &ts);
        agg.add_value(3, 20.0, &ts);
        agg.add_value(3, 30.0, &ts);

        let counts = agg.get_monthly_counts();
        assert_eq!(counts[2], 3);
        assert_eq!(counts[0], 0);
    }

    #[test]
    fn monthly_aggregator_averages() {
        let mut agg = MonthlyAggregator::new();
        let ts = AggTimestamp::new(6, 1, 0, 0);
        agg.add_value(6, 10.0, &ts);
        agg.add_value(6, 20.0, &ts);
        agg.add_value(6, 30.0, &ts);

        let avgs = agg.get_monthly_averages();
        assert!((avgs[5] - 20.0).abs() < 1e-10);
        // Months with no data should be NaN.
        assert!(avgs[0].is_nan());
    }

    #[test]
    #[should_panic(expected = "month must be 1..=12")]
    fn monthly_aggregator_invalid_month_zero() {
        let mut agg = MonthlyAggregator::new();
        agg.add_value(0, 1.0, &AggTimestamp::new(0, 1, 0, 0));
    }

    #[test]
    #[should_panic(expected = "month must be 1..=12")]
    fn monthly_aggregator_invalid_month_thirteen() {
        let mut agg = MonthlyAggregator::new();
        agg.add_value(13, 1.0, &AggTimestamp::new(13, 1, 0, 0));
    }

    // -----------------------------------------------------------------------
    // Annual Aggregator tests
    // -----------------------------------------------------------------------

    #[test]
    fn annual_aggregator_sum() {
        let mut agg = AnnualAggregator::new();
        agg.add_value(100.0, &AggTimestamp::new(1, 1, 0, 0));
        agg.add_value(200.0, &AggTimestamp::new(6, 15, 12, 0));
        agg.add_value(300.0, &AggTimestamp::new(12, 31, 23, 0));

        assert!((agg.get_annual_sum() - 600.0).abs() < 1e-10);
        assert_eq!(agg.get_count(), 3);
    }

    #[test]
    fn annual_aggregator_peak() {
        let mut agg = AnnualAggregator::new();
        agg.add_value(100.0, &AggTimestamp::new(1, 1, 12, 0));
        agg.add_value(500.0, &AggTimestamp::new(7, 21, 15, 30));
        agg.add_value(300.0, &AggTimestamp::new(12, 31, 23, 0));

        let peak = agg.get_peak();
        assert_eq!(peak.value, 500.0);
        assert_eq!(peak.month, 7);
        assert_eq!(peak.day, 21);
        assert_eq!(peak.hour, 15);
        assert_eq!(peak.minute, 30);
    }

    #[test]
    fn annual_aggregator_peak_updates_on_new_max() {
        let mut agg = AnnualAggregator::new();
        agg.add_value(100.0, &AggTimestamp::new(1, 1, 0, 0));
        assert_eq!(agg.get_peak().value, 100.0);

        agg.add_value(200.0, &AggTimestamp::new(2, 1, 0, 0));
        assert_eq!(agg.get_peak().value, 200.0);
        assert_eq!(agg.get_peak().month, 2);

        // Lower value should not update peak.
        agg.add_value(150.0, &AggTimestamp::new(3, 1, 0, 0));
        assert_eq!(agg.get_peak().value, 200.0);
        assert_eq!(agg.get_peak().month, 2);
    }

    #[test]
    fn annual_aggregator_empty() {
        let agg = AnnualAggregator::new();
        assert!((agg.get_annual_sum()).abs() < 1e-10);
        assert_eq!(agg.get_count(), 0);
        assert!(agg.get_average().is_nan());
        let peak = agg.get_peak();
        assert_eq!(peak.value, 0.0);
    }

    #[test]
    fn annual_aggregator_average() {
        let mut agg = AnnualAggregator::new();
        agg.add_value(10.0, &AggTimestamp::new(1, 1, 0, 0));
        agg.add_value(20.0, &AggTimestamp::new(1, 2, 0, 0));
        agg.add_value(30.0, &AggTimestamp::new(1, 3, 0, 0));
        assert!((agg.get_average() - 20.0).abs() < 1e-10);
    }

    // -----------------------------------------------------------------------
    // TimeOfPeak tests
    // -----------------------------------------------------------------------

    #[test]
    fn time_of_peak_construction() {
        let tp = TimeOfPeak::new(7, 21, 15, 30, 1234.5);
        assert_eq!(tp.month, 7);
        assert_eq!(tp.day, 21);
        assert_eq!(tp.hour, 15);
        assert_eq!(tp.minute, 30);
        assert!((tp.value - 1234.5).abs() < 1e-10);
    }

    #[test]
    fn time_of_peak_equality() {
        let a = TimeOfPeak::new(1, 1, 0, 0, 100.0);
        let b = TimeOfPeak::new(1, 1, 0, 0, 100.0);
        let c = TimeOfPeak::new(2, 1, 0, 0, 100.0);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // -----------------------------------------------------------------------
    // ResultsFramework tests
    // -----------------------------------------------------------------------

    #[test]
    fn results_framework_add_single_variable() {
        let mut rf = ResultsFramework::new();
        rf.add_timestep_data("01/01 01:00", "Temperature", 22.0);
        rf.add_timestep_data("01/01 02:00", "Temperature", 22.5);

        assert_eq!(rf.timestep_count(), 2);
        assert_eq!(rf.variable_count(), 1);
        assert_eq!(rf.simulation_results["Temperature"].len(), 2);
        assert!((rf.simulation_results["Temperature"][0] - 22.0).abs() < 1e-10);
        assert!((rf.simulation_results["Temperature"][1] - 22.5).abs() < 1e-10);
    }

    #[test]
    fn results_framework_add_multiple_variables() {
        let mut rf = ResultsFramework::new();
        rf.add_timestep_data("01/01 01:00", "Temperature", 22.0);
        rf.add_timestep_data("01/01 01:00", "Humidity", 50.0);
        rf.add_timestep_data("01/01 02:00", "Temperature", 23.0);
        rf.add_timestep_data("01/01 02:00", "Humidity", 55.0);

        assert_eq!(rf.timestep_count(), 2);
        assert_eq!(rf.variable_count(), 2);
        assert!((rf.simulation_results["Temperature"][1] - 23.0).abs() < 1e-10);
        assert!((rf.simulation_results["Humidity"][1] - 55.0).abs() < 1e-10);
    }

    #[test]
    fn results_framework_json_output_structure() {
        let mut rf = ResultsFramework::new();
        rf.add_timestep_data("01/01 01:00", "Temperature", 22.0);
        rf.add_timestep_data("01/01 02:00", "Temperature", 23.0);

        let json_str = rf.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert!(parsed["timestamps"].is_array());
        assert_eq!(parsed["timestamps"][0], "01/01 01:00");
        assert_eq!(parsed["timestamps"][1], "01/01 02:00");

        assert!(parsed["variables"].is_object());
        assert!(parsed["variables"]["Temperature"].is_array());
        assert_eq!(parsed["variables"]["Temperature"][0], 22.0);
        assert_eq!(parsed["variables"]["Temperature"][1], 23.0);
    }

    #[test]
    fn results_framework_json_multiple_variables() {
        let mut rf = ResultsFramework::new();
        rf.add_timestep_data("01/01 01:00", "Humidity", 50.0);
        rf.add_timestep_data("01/01 01:00", "Temperature", 22.0);

        let json_str = rf.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        // Variables should be in the output (sorted by key).
        assert!(parsed["variables"]["Humidity"].is_array());
        assert!(parsed["variables"]["Temperature"].is_array());
    }

    #[test]
    fn results_framework_empty() {
        let rf = ResultsFramework::new();
        assert_eq!(rf.timestep_count(), 0);
        assert_eq!(rf.variable_count(), 0);

        let json_str = rf.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["timestamps"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["variables"].as_object().unwrap().len(), 0);
    }

    #[test]
    fn results_framework_overwrite_same_timestamp() {
        let mut rf = ResultsFramework::new();
        rf.add_timestep_data("01/01 01:00", "Temp", 20.0);
        rf.add_timestep_data("01/01 01:00", "Temp", 25.0);

        // Same timestamp and variable: value should be overwritten.
        assert_eq!(rf.timestep_count(), 1);
        assert!((rf.simulation_results["Temp"][0] - 25.0).abs() < 1e-10);
    }

    // -----------------------------------------------------------------------
    // Integration-style tests
    // -----------------------------------------------------------------------

    #[test]
    fn zone_summary_text_rendering() {
        let zones = vec![ZoneSummaryData {
            name: "Office".to_string(),
            area: 100.0,
            volume: 300.0,
            multiplier: 1.0,
            above_ground_wall_area: 160.0,
            underground_wall_area: 0.0,
            window_area: 24.0,
            design_cooling_load: 10000.0,
            design_heating_load: 8000.0,
        }];
        let table = create_zone_summary(&zones);
        let text = table.to_text();
        assert!(text.contains("Zone Summary"));
        assert!(text.contains("Office"));
        assert!(text.contains("100.00"));
        assert!(text.contains("300.00"));
    }

    #[test]
    fn full_year_monthly_to_annual() {
        // Verify monthly aggregator sums match annual aggregator sum.
        let mut monthly = MonthlyAggregator::new();
        let mut annual = AnnualAggregator::new();

        for month in 1..=12 {
            let val = month as f64 * 100.0;
            let ts = AggTimestamp::new(month, 15, 12, 0);
            monthly.add_value(month, val, &ts);
            annual.add_value(val, &ts);
        }

        let monthly_total: f64 = monthly.get_monthly_sums().iter().sum();
        assert!((monthly_total - annual.get_annual_sum()).abs() < 1e-10);
        // Sum should be 100+200+...+1200 = 7800
        assert!((annual.get_annual_sum() - 7800.0).abs() < 1e-10);
    }

    #[test]
    fn annual_peak_from_multiple_months() {
        let mut annual = AnnualAggregator::new();
        annual.add_value(100.0, &AggTimestamp::new(1, 15, 14, 0));
        annual.add_value(500.0, &AggTimestamp::new(7, 21, 15, 0));
        annual.add_value(300.0, &AggTimestamp::new(12, 1, 10, 0));

        let peak = annual.get_peak();
        assert_eq!(peak.value, 500.0);
        assert_eq!(peak.month, 7);
    }

    #[test]
    fn monthly_peaks_across_all_months() {
        let mut agg = MonthlyAggregator::new();
        for month in 1..=12 {
            let ts1 = AggTimestamp::new(month, 10, 12, 0);
            let ts2 = AggTimestamp::new(month, 20, 14, 0);
            agg.add_value(month, 50.0, &ts1);
            agg.add_value(month, month as f64 * 100.0, &ts2);
        }

        let peaks = agg.get_monthly_peaks();
        for (i, peak) in peaks.iter().enumerate() {
            let expected_peak = (i as f64 + 1.0) * 100.0;
            assert!(
                (peak.value - expected_peak).abs() < 1e-10,
                "Month {}: expected peak {}, got {}",
                i + 1,
                expected_peak,
                peak.value
            );
            assert_eq!(peak.day, 20);
            assert_eq!(peak.hour, 14);
        }
    }
}
