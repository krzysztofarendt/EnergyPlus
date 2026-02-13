//! Non-fatal diagnostic message accumulator with rate-limiting.

use std::collections::HashMap;

/// Severity level for diagnostic messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Severe,
}

/// A single diagnostic message.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub category: &'static str,
    pub message: String,
    pub location: Option<SourceLocation>,
    pub count: u32,
}

/// Source location for traceability.
#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub file: &'static str,
    pub line: u32,
    pub module: &'static str,
}

/// Accumulates diagnostic messages and rate-limits repeated warnings.
pub struct DiagnosticCollector {
    diagnostics: Vec<Diagnostic>,
    recurring: HashMap<String, u32>,
    max_recurring: u32,
}

impl DiagnosticCollector {
    pub fn new(max_recurring: u32) -> Self {
        Self {
            diagnostics: Vec::new(),
            recurring: HashMap::new(),
            max_recurring,
        }
    }

    pub fn info(&mut self, category: &'static str, message: impl Into<String>) {
        self.add(DiagnosticLevel::Info, category, message.into());
    }

    pub fn warn(&mut self, category: &'static str, message: impl Into<String>) {
        self.add(DiagnosticLevel::Warning, category, message.into());
    }

    pub fn severe(&mut self, category: &'static str, message: impl Into<String>) {
        let msg = message.into();
        self.diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Severe,
            category,
            message: msg,
            location: None,
            count: 1,
        });
    }

    fn add(&mut self, level: DiagnosticLevel, category: &'static str, message: String) {
        let count = self.recurring.entry(message.clone()).or_insert(0);
        *count += 1;
        if *count <= self.max_recurring {
            self.diagnostics.push(Diagnostic {
                level,
                category,
                message,
                location: None,
                count: *count,
            });
        }
    }

    /// Get all collected diagnostics.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Count of severe-level messages.
    pub fn severe_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Severe)
            .count()
    }

    /// Count of warning-level messages.
    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Warning)
            .count()
    }

    /// Check if any severe errors have been recorded.
    pub fn has_severe_errors(&self) -> bool {
        self.severe_count() > 0
    }

    /// Clear all diagnostics.
    pub fn clear(&mut self) {
        self.diagnostics.clear();
        self.recurring.clear();
    }
}

impl Default for DiagnosticCollector {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiting() {
        let mut dc = DiagnosticCollector::new(3);
        for _ in 0..10 {
            dc.warn("test", "repeated warning");
        }
        assert_eq!(dc.warning_count(), 3);
    }

    #[test]
    fn severe_always_recorded() {
        let mut dc = DiagnosticCollector::new(0);
        dc.severe("test", "bad thing happened");
        assert_eq!(dc.severe_count(), 1);
        assert!(dc.has_severe_errors());
    }
}
