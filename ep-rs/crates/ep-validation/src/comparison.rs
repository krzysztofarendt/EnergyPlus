//! Numerical comparison utilities for validation.
//!
//! Provides relative, absolute, and combined tolerance checks
//! used across BESTEST and regression testing.

/// Comparison result for a single value.
#[derive(Debug, Clone)]
pub struct ComparisonResult {
    pub name: String,
    pub expected: f64,
    pub actual: f64,
    pub abs_diff: f64,
    pub rel_diff: f64,
    pub passed: bool,
}

/// Tolerance specification for numerical comparison.
#[derive(Debug, Clone, Copy)]
pub struct Tolerance {
    /// Absolute tolerance.
    pub abs_tol: f64,
    /// Relative tolerance (fraction, e.g. 0.01 = 1%).
    pub rel_tol: f64,
}

impl Tolerance {
    pub const TIGHT: Self = Self {
        abs_tol: 1e-6,
        rel_tol: 1e-6,
    };

    pub const NORMAL: Self = Self {
        abs_tol: 0.01,
        rel_tol: 0.01,
    };

    pub const LOOSE: Self = Self {
        abs_tol: 1.0,
        rel_tol: 0.05,
    };

    pub fn new(abs_tol: f64, rel_tol: f64) -> Self {
        Self { abs_tol, rel_tol }
    }

    /// Check if two values are within tolerance.
    /// Passes if EITHER absolute or relative tolerance is satisfied.
    pub fn check(&self, expected: f64, actual: f64) -> bool {
        let abs_diff = (expected - actual).abs();
        if abs_diff <= self.abs_tol {
            return true;
        }
        if expected.abs() > self.abs_tol {
            let rel_diff = abs_diff / expected.abs();
            return rel_diff <= self.rel_tol;
        }
        false
    }
}

/// Compare two values and produce a detailed result.
pub fn compare(name: impl Into<String>, expected: f64, actual: f64, tol: Tolerance) -> ComparisonResult {
    let abs_diff = (expected - actual).abs();
    let rel_diff = if expected.abs() > 1e-15 {
        abs_diff / expected.abs()
    } else {
        abs_diff
    };
    let passed = tol.check(expected, actual);

    ComparisonResult {
        name: name.into(),
        expected,
        actual,
        abs_diff,
        rel_diff,
        passed,
    }
}

/// Compare two time series and return per-point results.
pub fn compare_series(
    name: &str,
    expected: &[f64],
    actual: &[f64],
    tol: Tolerance,
) -> Vec<ComparisonResult> {
    let n = expected.len().min(actual.len());
    (0..n)
        .map(|i| compare(format!("{}[{}]", name, i), expected[i], actual[i], tol))
        .collect()
}

/// Summary statistics for a set of comparison results.
#[derive(Debug, Clone)]
pub struct ComparisonSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub max_abs_diff: f64,
    pub max_rel_diff: f64,
    pub worst_name: String,
}

impl ComparisonSummary {
    pub fn from_results(results: &[ComparisonResult]) -> Self {
        let mut summary = Self {
            total: results.len(),
            passed: 0,
            failed: 0,
            max_abs_diff: 0.0,
            max_rel_diff: 0.0,
            worst_name: String::new(),
        };

        for r in results {
            if r.passed {
                summary.passed += 1;
            } else {
                summary.failed += 1;
            }
            if r.abs_diff > summary.max_abs_diff {
                summary.max_abs_diff = r.abs_diff;
                summary.worst_name = r.name.clone();
            }
            if r.rel_diff > summary.max_rel_diff {
                summary.max_rel_diff = r.rel_diff;
            }
        }

        summary
    }

    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }
}

/// Root-mean-square error between two series.
pub fn rmse(expected: &[f64], actual: &[f64]) -> f64 {
    let n = expected.len().min(actual.len());
    if n == 0 {
        return 0.0;
    }
    let sum_sq: f64 = expected.iter().zip(actual.iter())
        .map(|(e, a)| (e - a).powi(2))
        .sum();
    (sum_sq / n as f64).sqrt()
}

/// Mean bias error between two series.
pub fn mbe(expected: &[f64], actual: &[f64]) -> f64 {
    let n = expected.len().min(actual.len());
    if n == 0 {
        return 0.0;
    }
    let sum: f64 = expected.iter().zip(actual.iter())
        .map(|(e, a)| a - e)
        .sum();
    sum / n as f64
}

/// Coefficient of variation of RMSE (CV-RMSE), as percentage.
pub fn cv_rmse(expected: &[f64], actual: &[f64]) -> f64 {
    let n = expected.len().min(actual.len());
    if n == 0 {
        return 0.0;
    }
    let mean: f64 = expected.iter().sum::<f64>() / n as f64;
    if mean.abs() < 1e-15 {
        return 0.0;
    }
    rmse(expected, actual) / mean.abs() * 100.0
}

/// Normalized mean bias error (NMBE), as percentage.
pub fn nmbe(expected: &[f64], actual: &[f64]) -> f64 {
    let n = expected.len().min(actual.len());
    if n == 0 {
        return 0.0;
    }
    let mean: f64 = expected.iter().sum::<f64>() / n as f64;
    if mean.abs() < 1e-15 {
        return 0.0;
    }
    mbe(expected, actual) / mean.abs() * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerance_absolute() {
        let tol = Tolerance::new(0.1, 0.0);
        assert!(tol.check(1.0, 1.05));
        assert!(!tol.check(1.0, 1.2));
    }

    #[test]
    fn tolerance_relative() {
        let tol = Tolerance::new(0.0, 0.05);
        assert!(tol.check(100.0, 104.0)); // 4% < 5%
        assert!(!tol.check(100.0, 110.0)); // 10% > 5%
    }

    #[test]
    fn tolerance_combined() {
        let tol = Tolerance::new(1.0, 0.01);
        // Small value — absolute tolerance applies
        assert!(tol.check(0.5, 1.0)); // abs_diff=0.5 < 1.0
        // Large value — relative tolerance applies
        assert!(tol.check(1000.0, 1009.0)); // 0.9% < 1%
        assert!(!tol.check(1000.0, 1020.0)); // 2% > 1%, abs 20 > 1
    }

    #[test]
    fn compare_values() {
        let result = compare("load", 1000.0, 1005.0, Tolerance::NORMAL);
        assert!((result.abs_diff - 5.0).abs() < 0.01);
        assert!((result.rel_diff - 0.005).abs() < 0.001);
        assert!(result.passed); // 0.5% < 1%
    }

    #[test]
    fn comparison_summary() {
        let results = vec![
            compare("a", 100.0, 100.0, Tolerance::TIGHT),
            compare("b", 100.0, 200.0, Tolerance::TIGHT),
            compare("c", 100.0, 100.0, Tolerance::TIGHT),
        ];
        let summary = ComparisonSummary::from_results(&results);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);
        assert!(!summary.all_passed());
        assert_eq!(summary.worst_name, "b");
    }

    #[test]
    fn rmse_calculation() {
        let expected = vec![1.0, 2.0, 3.0, 4.0];
        let actual = vec![1.1, 2.1, 3.1, 4.1];
        let r = rmse(&expected, &actual);
        assert!((r - 0.1).abs() < 0.01, "rmse={}", r);
    }

    #[test]
    fn mbe_calculation() {
        let expected = vec![10.0, 20.0, 30.0];
        let actual = vec![11.0, 21.0, 31.0];
        let m = mbe(&expected, &actual);
        assert!((m - 1.0).abs() < 0.01, "mbe={}", m);
    }

    #[test]
    fn cv_rmse_calculation() {
        let expected = vec![100.0, 100.0, 100.0];
        let actual = vec![110.0, 90.0, 100.0];
        let cv = cv_rmse(&expected, &actual);
        // RMSE = sqrt((100+100+0)/3) = sqrt(66.67) ≈ 8.165
        // CV-RMSE = 8.165 / 100 * 100 = 8.165%
        assert!(cv > 7.0 && cv < 9.0, "cv_rmse={}", cv);
    }

    #[test]
    fn series_comparison() {
        let expected = vec![1.0, 2.0, 3.0];
        let actual = vec![1.0, 2.0, 3.0];
        let results = compare_series("temp", &expected, &actual, Tolerance::TIGHT);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.passed));
    }
}
