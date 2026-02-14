//! Regression testing framework for simulation outputs.
//!
//! Compares simulation results against reference data to detect
//! regressions in output values.

use crate::comparison::{ComparisonResult, ComparisonSummary, Tolerance, compare};

/// A reference data point for regression testing.
#[derive(Debug, Clone)]
pub struct ReferencePoint {
    pub name: String,
    pub value: f64,
    pub tolerance: Tolerance,
}

/// Regression test case.
#[derive(Debug, Clone)]
pub struct RegressionTest {
    pub name: String,
    pub description: String,
    pub reference_points: Vec<ReferencePoint>,
}

impl RegressionTest {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            reference_points: Vec::new(),
        }
    }

    /// Add a reference point.
    pub fn add_point(&mut self, name: impl Into<String>, value: f64, tolerance: Tolerance) {
        self.reference_points.push(ReferencePoint {
            name: name.into(),
            value,
            tolerance,
        });
    }

    /// Run the regression test against actual results.
    pub fn check(&self, actual_values: &[(String, f64)]) -> RegressionResult {
        let mut comparisons = Vec::new();

        for ref_point in &self.reference_points {
            if let Some((_, actual)) = actual_values.iter().find(|(name, _)| *name == ref_point.name) {
                comparisons.push(compare(&ref_point.name, ref_point.value, *actual, ref_point.tolerance));
            } else {
                // Missing result — mark as failed
                comparisons.push(ComparisonResult {
                    name: ref_point.name.clone(),
                    expected: ref_point.value,
                    actual: f64::NAN,
                    abs_diff: f64::INFINITY,
                    rel_diff: f64::INFINITY,
                    passed: false,
                });
            }
        }

        let summary = ComparisonSummary::from_results(&comparisons);

        RegressionResult {
            test_name: self.name.clone(),
            comparisons,
            summary,
        }
    }
}

/// Result of a regression test run.
#[derive(Debug, Clone)]
pub struct RegressionResult {
    pub test_name: String,
    pub comparisons: Vec<ComparisonResult>,
    pub summary: ComparisonSummary,
}

impl RegressionResult {
    pub fn passed(&self) -> bool {
        self.summary.all_passed()
    }
}

/// Regression test suite (collection of tests).
#[derive(Debug, Clone)]
pub struct RegressionSuite {
    pub name: String,
    pub tests: Vec<RegressionTest>,
}

impl RegressionSuite {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tests: Vec::new(),
        }
    }

    pub fn add_test(&mut self, test: RegressionTest) {
        self.tests.push(test);
    }

    /// Run all tests in the suite.
    pub fn run(&self, results_by_test: &[(String, Vec<(String, f64)>)]) -> SuiteResult {
        let mut test_results = Vec::new();

        for test in &self.tests {
            if let Some((_, values)) = results_by_test.iter().find(|(name, _)| *name == test.name) {
                test_results.push(test.check(values));
            }
        }

        let total = test_results.len();
        let passed = test_results.iter().filter(|r| r.passed()).count();

        SuiteResult {
            suite_name: self.name.clone(),
            test_results,
            total,
            passed,
            failed: total - passed,
        }
    }
}

/// Result of running a regression test suite.
#[derive(Debug, Clone)]
pub struct SuiteResult {
    pub suite_name: String,
    pub test_results: Vec<RegressionResult>,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
}

impl SuiteResult {
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_test_pass() {
        let mut test = RegressionTest::new("zone_temp", "Zone temperature check");
        test.add_point("zone_mean_air_temp", 22.0, Tolerance::NORMAL);
        test.add_point("zone_heating_load", 5000.0, Tolerance::NORMAL);

        let actual = vec![
            ("zone_mean_air_temp".to_string(), 22.01),
            ("zone_heating_load".to_string(), 5010.0),
        ];

        let result = test.check(&actual);
        assert!(result.passed());
        assert_eq!(result.summary.total, 2);
        assert_eq!(result.summary.passed, 2);
    }

    #[test]
    fn regression_test_fail() {
        let mut test = RegressionTest::new("energy", "Energy check");
        test.add_point("total_energy", 1000.0, Tolerance::TIGHT);

        let actual = vec![
            ("total_energy".to_string(), 1100.0), // 10% off
        ];

        let result = test.check(&actual);
        assert!(!result.passed());
    }

    #[test]
    fn regression_test_missing_value() {
        let mut test = RegressionTest::new("test", "Missing value test");
        test.add_point("exists", 100.0, Tolerance::NORMAL);
        test.add_point("missing", 200.0, Tolerance::NORMAL);

        let actual = vec![
            ("exists".to_string(), 100.0),
        ];

        let result = test.check(&actual);
        assert!(!result.passed());
        assert_eq!(result.summary.failed, 1);
    }

    #[test]
    fn regression_suite() {
        let mut suite = RegressionSuite::new("Basic Suite");

        let mut test1 = RegressionTest::new("test1", "First test");
        test1.add_point("value_a", 100.0, Tolerance::NORMAL);
        suite.add_test(test1);

        let mut test2 = RegressionTest::new("test2", "Second test");
        test2.add_point("value_b", 200.0, Tolerance::NORMAL);
        suite.add_test(test2);

        let results = vec![
            ("test1".to_string(), vec![("value_a".to_string(), 100.0)]),
            ("test2".to_string(), vec![("value_b".to_string(), 200.0)]),
        ];

        let suite_result = suite.run(&results);
        assert!(suite_result.all_passed());
        assert_eq!(suite_result.total, 2);
        assert_eq!(suite_result.passed, 2);
    }

    #[test]
    fn regression_suite_partial_fail() {
        let mut suite = RegressionSuite::new("Mixed Suite");

        let mut test1 = RegressionTest::new("pass", "Pass test");
        test1.add_point("v", 100.0, Tolerance::NORMAL);
        suite.add_test(test1);

        let mut test2 = RegressionTest::new("fail", "Fail test");
        test2.add_point("v", 100.0, Tolerance::TIGHT);
        suite.add_test(test2);

        let results = vec![
            ("pass".to_string(), vec![("v".to_string(), 100.0)]),
            ("fail".to_string(), vec![("v".to_string(), 200.0)]),
        ];

        let suite_result = suite.run(&results);
        assert!(!suite_result.all_passed());
        assert_eq!(suite_result.passed, 1);
        assert_eq!(suite_result.failed, 1);
    }
}
