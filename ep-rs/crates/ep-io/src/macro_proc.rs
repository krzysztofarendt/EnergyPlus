//! Macro preprocessor for EnergyPlus IDF files.
//!
//! Supports a subset of EnergyPlus macro commands:
//! - `##set1 VAR value` — define a variable
//! - `##[VAR]` — substitute a variable (inline)
//! - `##if VAR` / `##else` / `##endif` — conditional blocks
//! - `##include path` — file inclusion

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::InputError;

/// Macro preprocessor state.
pub struct MacroProcessor {
    variables: HashMap<String, String>,
    include_path: PathBuf,
}

impl MacroProcessor {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            include_path: PathBuf::from("."),
        }
    }

    /// Set the base path for `##include` directives.
    pub fn set_include_path(&mut self, path: &Path) {
        self.include_path = path.to_path_buf();
    }

    /// Define a variable.
    pub fn set_variable(&mut self, name: &str, value: &str) {
        self.variables.insert(name.to_string(), value.to_string());
    }

    /// Process macro directives in the input text, returning expanded output.
    pub fn process(&mut self, input: &str) -> Result<String, InputError> {
        self.process_inner(input, 0)
    }

    fn process_inner(&mut self, input: &str, depth: usize) -> Result<String, InputError> {
        if depth > 10 {
            return Err(InputError::ParseError(
                "Macro include depth exceeded (max 10)".to_string(),
            ));
        }

        let mut output = Vec::new();
        // Stack of (active, has_else_been_hit) for nested ##if blocks
        let mut condition_stack: Vec<(bool, bool)> = Vec::new();

        for line in input.lines() {
            let trimmed = line.trim();

            // ##set1 VAR value
            if let Some(rest) = trimmed.strip_prefix("##set1 ") {
                if is_active(&condition_stack) {
                    let rest = rest.trim();
                    if let Some(space_idx) = rest.find(|c: char| c.is_whitespace()) {
                        let var_name = &rest[..space_idx];
                        let var_value = rest[space_idx..].trim();
                        self.variables
                            .insert(var_name.to_string(), var_value.to_string());
                    } else {
                        // Variable with empty value
                        self.variables.insert(rest.to_string(), String::new());
                    }
                }
                continue;
            }

            // ##if VAR
            if let Some(rest) = trimmed.strip_prefix("##if ") {
                let var_name = rest.trim();
                let is_defined = self.variables.contains_key(var_name);
                // If we're in an inactive block, push inactive regardless
                if !is_active(&condition_stack) {
                    condition_stack.push((false, false));
                } else {
                    condition_stack.push((is_defined, false));
                }
                continue;
            }

            // ##else
            if trimmed == "##else" {
                if condition_stack.is_empty() {
                    return Err(InputError::ParseError(
                        "##else without matching ##if".to_string(),
                    ));
                }
                let len = condition_stack.len();
                let has_else = condition_stack[len - 1].1;
                if has_else {
                    return Err(InputError::ParseError(
                        "Duplicate ##else in ##if block".to_string(),
                    ));
                }
                condition_stack[len - 1].1 = true;
                // Check if parent is active
                let parent_active = if len > 1 {
                    condition_stack[..len - 1]
                        .iter()
                        .all(|(active, _)| *active)
                } else {
                    true
                };
                if parent_active {
                    condition_stack[len - 1].0 = !condition_stack[len - 1].0;
                }
                continue;
            }

            // ##endif
            if trimmed == "##endif" {
                if condition_stack.is_empty() {
                    return Err(InputError::ParseError(
                        "##endif without matching ##if".to_string(),
                    ));
                }
                condition_stack.pop();
                continue;
            }

            // ##include path
            if let Some(rest) = trimmed.strip_prefix("##include ") {
                if is_active(&condition_stack) {
                    let file_path = self.include_path.join(rest.trim());
                    let content = std::fs::read_to_string(&file_path).map_err(|e| {
                        InputError::IoError(format!("Failed to include {}: {e}", file_path.display()))
                    })?;
                    let expanded = self.process_inner(&content, depth + 1)?;
                    output.push(expanded);
                }
                continue;
            }

            // Regular line: substitute variables and emit if active
            if is_active(&condition_stack) {
                let expanded = self.substitute_variables(line);
                output.push(expanded);
            }
        }

        if !condition_stack.is_empty() {
            return Err(InputError::ParseError(format!(
                "Unterminated ##if block ({} unclosed)",
                condition_stack.len()
            )));
        }

        Ok(output.join("\n"))
    }

    /// Replace `##[VAR]` patterns in a line with their values.
    fn substitute_variables(&self, line: &str) -> String {
        let mut result = line.to_string();
        // Iterate until no more substitutions are found
        loop {
            let start = result.find("##[");
            if start.is_none() {
                break;
            }
            let start = start.unwrap();
            let rest = &result[start + 3..];
            let end = rest.find(']');
            if end.is_none() {
                break;
            }
            let end = end.unwrap();
            let var_name = &rest[..end];
            let replacement = self
                .variables
                .get(var_name)
                .cloned()
                .unwrap_or_default();
            result = format!(
                "{}{}{}",
                &result[..start],
                replacement,
                &rest[end + 1..]
            );
        }
        result
    }
}

/// Check if all conditions in the stack are active.
fn is_active(stack: &[(bool, bool)]) -> bool {
    stack.iter().all(|(active, _)| *active)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_substitute() {
        let mut proc = MacroProcessor::new();
        let input = "##set1 CITY Chicago\nBuilding, ##[CITY];";
        let output = proc.process(input).unwrap();
        assert_eq!(output, "Building, Chicago;");
    }

    #[test]
    fn substitute_multiple_vars() {
        let mut proc = MacroProcessor::new();
        let input = "##set1 A Hello\n##set1 B World\n##[A] ##[B]!";
        let output = proc.process(input).unwrap();
        assert_eq!(output, "Hello World!");
    }

    #[test]
    fn undefined_variable_substitutes_empty() {
        let mut proc = MacroProcessor::new();
        let input = "Value: ##[UNDEF]end";
        let output = proc.process(input).unwrap();
        assert_eq!(output, "Value: end");
    }

    #[test]
    fn if_true_block() {
        let mut proc = MacroProcessor::new();
        let input = "##set1 DEBUG 1\n##if DEBUG\nDEBUG_LINE\n##endif\nAFTER";
        let output = proc.process(input).unwrap();
        assert!(output.contains("DEBUG_LINE"));
        assert!(output.contains("AFTER"));
    }

    #[test]
    fn if_false_block() {
        let mut proc = MacroProcessor::new();
        let input = "##if UNDEF\nSKIPPED\n##endif\nKEPT";
        let output = proc.process(input).unwrap();
        assert!(!output.contains("SKIPPED"));
        assert!(output.contains("KEPT"));
    }

    #[test]
    fn if_else_true() {
        let mut proc = MacroProcessor::new();
        let input = "##set1 X 1\n##if X\nTRUE_BRANCH\n##else\nFALSE_BRANCH\n##endif";
        let output = proc.process(input).unwrap();
        assert!(output.contains("TRUE_BRANCH"));
        assert!(!output.contains("FALSE_BRANCH"));
    }

    #[test]
    fn if_else_false() {
        let mut proc = MacroProcessor::new();
        let input = "##if UNDEF\nTRUE_BRANCH\n##else\nFALSE_BRANCH\n##endif";
        let output = proc.process(input).unwrap();
        assert!(!output.contains("TRUE_BRANCH"));
        assert!(output.contains("FALSE_BRANCH"));
    }

    #[test]
    fn nested_if_blocks() {
        let mut proc = MacroProcessor::new();
        let input = "\
##set1 A 1
##set1 B 1
##if A
OUTER_TRUE
##if B
INNER_TRUE
##endif
##endif
END";
        let output = proc.process(input).unwrap();
        assert!(output.contains("OUTER_TRUE"));
        assert!(output.contains("INNER_TRUE"));
        assert!(output.contains("END"));
    }

    #[test]
    fn nested_if_outer_false() {
        let mut proc = MacroProcessor::new();
        let input = "\
##set1 B 1
##if UNDEF
OUTER_TRUE
##if B
INNER_TRUE
##endif
##endif
END";
        let output = proc.process(input).unwrap();
        assert!(!output.contains("OUTER_TRUE"));
        assert!(!output.contains("INNER_TRUE"));
        assert!(output.contains("END"));
    }

    #[test]
    fn unterminated_if_error() {
        let mut proc = MacroProcessor::new();
        let input = "##if X\nBODY";
        let result = proc.process(input);
        assert!(result.is_err());
    }

    #[test]
    fn else_without_if_error() {
        let mut proc = MacroProcessor::new();
        let input = "##else\nBODY";
        let result = proc.process(input);
        assert!(result.is_err());
    }

    #[test]
    fn set_variable_api() {
        let mut proc = MacroProcessor::new();
        proc.set_variable("YEAR", "2024");
        let output = proc.process("Version, ##[YEAR];").unwrap();
        assert_eq!(output, "Version, 2024;");
    }
}
