//! ASHRAE Standard 140 (BESTEST) case definitions and acceptance ranges.
//!
//! Defines the building envelope test cases (600-series and 900-series)
//! with their expected annual heating/cooling load ranges.

use crate::comparison::ComparisonResult;

/// BESTEST case identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BestestCase {
    /// Lightweight base case: south-facing windows, no overhang.
    Case600,
    /// Case 600 + west-facing windows.
    Case610,
    /// Case 600 + horizontal overhangs.
    Case620,
    /// Case 600 + east/west windows.
    Case630,
    /// Case 600 + night setback thermostat.
    Case640,
    /// Case 600 + free-float (no HVAC).
    Case650,
    /// Heavyweight base case: south-facing windows.
    Case900,
    /// Case 900 + west-facing windows.
    Case910,
    /// Case 900 + horizontal overhangs.
    Case920,
    /// Case 900 + east/west windows.
    Case930,
    /// Case 900 + night setback thermostat.
    Case940,
    /// Case 900 + free-float (no HVAC).
    Case950,
    /// Case 600 with sunspace.
    Case960,
}

/// BESTEST case specification.
#[derive(Debug, Clone)]
pub struct BestestSpec {
    pub case: BestestCase,
    pub description: String,
    /// Construction type.
    pub construction: ConstructionType,
    /// Floor area (m²).
    pub floor_area: f64,
    /// Total window area (m²).
    pub window_area: f64,
    /// Window orientation.
    pub window_orientation: WindowOrientation,
    /// Heating setpoint (°C).
    pub heat_setpoint: f64,
    /// Cooling setpoint (°C).
    pub cool_setpoint: f64,
    /// Infiltration rate (ACH).
    pub infiltration_ach: f64,
    /// Internal gains (W).
    pub internal_gains: f64,
    /// Has thermostat setback.
    pub has_setback: bool,
    /// Is free-float (no HVAC).
    pub is_free_float: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstructionType {
    Lightweight,
    Heavyweight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowOrientation {
    South,
    EastWest,
    West,
}

/// Acceptance range for a BESTEST result.
#[derive(Debug, Clone)]
pub struct AcceptanceRange {
    pub case: BestestCase,
    pub metric: String,
    pub min: f64,
    pub max: f64,
    pub unit: String,
}

impl AcceptanceRange {
    /// Check if a value falls within the acceptance range.
    pub fn check(&self, value: f64) -> bool {
        value >= self.min && value <= self.max
    }

    /// Generate a comparison result.
    pub fn compare(&self, value: f64) -> ComparisonResult {
        let midpoint = (self.min + self.max) / 2.0;
        let passed = self.check(value);

        ComparisonResult {
            name: format!("{:?} {}", self.case, self.metric),
            expected: midpoint,
            actual: value,
            abs_diff: (value - midpoint).abs(),
            rel_diff: if midpoint.abs() > 1e-10 { (value - midpoint).abs() / midpoint.abs() } else { 0.0 },
            passed,
        }
    }
}

/// Get the BESTEST Case 600 specification.
pub fn case_600_spec() -> BestestSpec {
    BestestSpec {
        case: BestestCase::Case600,
        description: "Lightweight base case, south windows".into(),
        construction: ConstructionType::Lightweight,
        floor_area: 48.0,    // 8m × 6m
        window_area: 12.0,   // 2 windows × 3m × 2m
        window_orientation: WindowOrientation::South,
        heat_setpoint: 20.0,
        cool_setpoint: 27.0,
        infiltration_ach: 0.5,
        internal_gains: 200.0, // 200 W continuous
        has_setback: false,
        is_free_float: false,
    }
}

/// Get the BESTEST Case 900 specification.
pub fn case_900_spec() -> BestestSpec {
    BestestSpec {
        case: BestestCase::Case900,
        description: "Heavyweight base case, south windows".into(),
        construction: ConstructionType::Heavyweight,
        floor_area: 48.0,
        window_area: 12.0,
        window_orientation: WindowOrientation::South,
        heat_setpoint: 20.0,
        cool_setpoint: 27.0,
        infiltration_ach: 0.5,
        internal_gains: 200.0,
        has_setback: false,
        is_free_float: false,
    }
}

/// BESTEST acceptance ranges from ASHRAE Standard 140.
/// Values are annual heating and cooling loads in MWh.
pub fn case_600_acceptance() -> Vec<AcceptanceRange> {
    vec![
        AcceptanceRange {
            case: BestestCase::Case600,
            metric: "Annual Heating".into(),
            min: 4.296,
            max: 5.709,
            unit: "MWh".into(),
        },
        AcceptanceRange {
            case: BestestCase::Case600,
            metric: "Annual Cooling".into(),
            min: 6.137,
            max: 8.448,
            unit: "MWh".into(),
        },
        AcceptanceRange {
            case: BestestCase::Case600,
            metric: "Peak Heating".into(),
            min: 3.437,
            max: 4.354,
            unit: "kW".into(),
        },
        AcceptanceRange {
            case: BestestCase::Case600,
            metric: "Peak Cooling".into(),
            min: 5.965,
            max: 6.827,
            unit: "kW".into(),
        },
    ]
}

/// BESTEST Case 900 acceptance ranges.
pub fn case_900_acceptance() -> Vec<AcceptanceRange> {
    vec![
        AcceptanceRange {
            case: BestestCase::Case900,
            metric: "Annual Heating".into(),
            min: 1.170,
            max: 2.041,
            unit: "MWh".into(),
        },
        AcceptanceRange {
            case: BestestCase::Case900,
            metric: "Annual Cooling".into(),
            min: 2.132,
            max: 3.415,
            unit: "MWh".into(),
        },
        AcceptanceRange {
            case: BestestCase::Case900,
            metric: "Peak Heating".into(),
            min: 2.850,
            max: 3.797,
            unit: "kW".into(),
        },
        AcceptanceRange {
            case: BestestCase::Case900,
            metric: "Peak Cooling".into(),
            min: 2.888,
            max: 3.871,
            unit: "kW".into(),
        },
    ]
}

/// Free-float temperature acceptance ranges (°C).
pub fn case_650_acceptance() -> Vec<AcceptanceRange> {
    vec![
        AcceptanceRange {
            case: BestestCase::Case650,
            metric: "Min Zone Temp".into(),
            min: -18.8,
            max: -15.6,
            unit: "°C".into(),
        },
        AcceptanceRange {
            case: BestestCase::Case650,
            metric: "Max Zone Temp".into(),
            min: 64.9,
            max: 69.5,
            unit: "°C".into(),
        },
    ]
}

/// Validate a set of results against BESTEST acceptance ranges.
pub fn validate_case(
    ranges: &[AcceptanceRange],
    results: &[(String, f64)],
) -> Vec<ComparisonResult> {
    let mut comparisons = Vec::new();
    for range in ranges {
        if let Some((_, value)) = results.iter().find(|(name, _)| *name == range.metric) {
            comparisons.push(range.compare(*value));
        }
    }
    comparisons
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_600_spec_values() {
        let spec = case_600_spec();
        assert_eq!(spec.case, BestestCase::Case600);
        assert!((spec.floor_area - 48.0).abs() < 0.1);
        assert!((spec.window_area - 12.0).abs() < 0.1);
        assert_eq!(spec.construction, ConstructionType::Lightweight);
        assert!(!spec.is_free_float);
    }

    #[test]
    fn case_900_spec_values() {
        let spec = case_900_spec();
        assert_eq!(spec.case, BestestCase::Case900);
        assert_eq!(spec.construction, ConstructionType::Heavyweight);
    }

    #[test]
    fn acceptance_range_pass() {
        let range = AcceptanceRange {
            case: BestestCase::Case600,
            metric: "Annual Heating".into(),
            min: 4.0,
            max: 6.0,
            unit: "MWh".into(),
        };
        assert!(range.check(5.0));
        assert!(range.check(4.0));
        assert!(range.check(6.0));
        assert!(!range.check(3.9));
        assert!(!range.check(6.1));
    }

    #[test]
    fn acceptance_range_compare() {
        let range = AcceptanceRange {
            case: BestestCase::Case600,
            metric: "Annual Heating".into(),
            min: 4.0,
            max: 6.0,
            unit: "MWh".into(),
        };

        let result = range.compare(5.0);
        assert!(result.passed);
        assert!((result.expected - 5.0).abs() < 0.01); // midpoint

        let result_fail = range.compare(7.0);
        assert!(!result_fail.passed);
    }

    #[test]
    fn validate_case_600() {
        let ranges = case_600_acceptance();
        let results = vec![
            ("Annual Heating".to_string(), 5.0),  // within range
            ("Annual Cooling".to_string(), 7.0),  // within range
            ("Peak Heating".to_string(), 4.0),    // within range
            ("Peak Cooling".to_string(), 6.5),    // within range
        ];

        let comparisons = validate_case(&ranges, &results);
        assert_eq!(comparisons.len(), 4);
        assert!(comparisons.iter().all(|c| c.passed));
    }

    #[test]
    fn validate_case_600_failure() {
        let ranges = case_600_acceptance();
        let results = vec![
            ("Annual Heating".to_string(), 10.0), // way out of range
        ];

        let comparisons = validate_case(&ranges, &results);
        assert_eq!(comparisons.len(), 1);
        assert!(!comparisons[0].passed);
    }

    #[test]
    fn case_650_free_float() {
        let ranges = case_650_acceptance();
        assert_eq!(ranges.len(), 2);
        // Typical result should pass
        let result = ranges[0].compare(-17.0);
        assert!(result.passed);
    }
}
