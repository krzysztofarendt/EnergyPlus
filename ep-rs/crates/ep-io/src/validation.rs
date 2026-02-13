//! Schema validation framework for input models.

use crate::{InputModel, ValidationError, ValidationSeverity};

/// Validate an input model against basic rules.
pub fn validate_basic(model: &InputModel) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Check for required Version object
    if model.object_count("Version") == 0 {
        errors.push(ValidationError {
            object_type: "Version".to_string(),
            object_name: String::new(),
            field: String::new(),
            message: "No Version object found in input".to_string(),
            severity: ValidationSeverity::Warning,
        });
    }

    // Check for required Building object
    if model.object_count("Building") == 0 {
        errors.push(ValidationError {
            object_type: "Building".to_string(),
            object_name: String::new(),
            field: String::new(),
            message: "No Building object found in input".to_string(),
            severity: ValidationSeverity::Error,
        });
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_empty_model() {
        let model = InputModel::default();
        let errors = validate_basic(&model);
        assert!(errors.len() >= 1);
    }

    #[test]
    fn validate_complete_model() {
        let mut model = InputModel::default();
        model.add_object("Version", serde_json::json!({"field_1": "24.2"}));
        model.add_object("Building", serde_json::json!({"name": "Test"}));
        let errors = validate_basic(&model);
        assert!(errors.is_empty());
    }
}
