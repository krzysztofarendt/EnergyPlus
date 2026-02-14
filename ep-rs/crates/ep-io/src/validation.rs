//! Schema validation framework for input models.
//!
//! Provides basic structural validation (`validate_basic`) and comprehensive
//! schema-driven validation (`validate_schema`) that checks field types,
//! numeric ranges, choice values, unique-object constraints, and
//! cross-object references.

use std::collections::{HashMap, HashSet};

use crate::schema::{Bound, FieldType, SchemaDb};
use crate::{InputModel, ValidationError, ValidationSeverity};

/// Accumulated validation results with errors and warnings.
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationError>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if there are no errors (warnings are acceptable).
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Merge another result into this one.
    pub fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }

    fn push(&mut self, err: ValidationError) {
        match err.severity {
            ValidationSeverity::Error => self.errors.push(err),
            ValidationSeverity::Warning => self.warnings.push(err),
        }
    }

    /// Total number of issues (errors + warnings).
    pub fn total(&self) -> usize {
        self.errors.len() + self.warnings.len()
    }
}

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

/// Perform full schema-driven validation on the model.
pub fn validate_schema(model: &InputModel, schema: &SchemaDb) -> ValidationResult {
    let mut result = ValidationResult::new();

    // First pass: collect all reference sets (names registered into each list)
    let mut reference_sets: HashMap<String, HashSet<String>> = HashMap::new();
    for obj_type in model.object_types() {
        if let Some(def) = schema.get(obj_type) {
            for obj in model.get_objects(obj_type) {
                for (i, field_def) in def.fields.iter().enumerate() {
                    if !field_def.reference_lists.is_empty() {
                        let key = if i == 0 { "name" } else { &format!("field_{i}") };
                        // We need to handle the borrow issue - use a separate string
                        let field_key = if i == 0 {
                            "name".to_string()
                        } else {
                            format!("field_{i}")
                        };
                        if let Some(val) = obj.get(&field_key).and_then(|v| v.as_str()) {
                            let _ = key; // suppress unused warning
                            for list in &field_def.reference_lists {
                                reference_sets
                                    .entry(list.to_uppercase())
                                    .or_default()
                                    .insert(val.to_uppercase());
                            }
                        }
                    }
                }
            }
        }
    }

    // Second pass: validate each object against its schema
    for obj_type in model.object_types() {
        let objects = model.get_objects(obj_type);

        if let Some(def) = schema.get(obj_type) {
            // Unique-object check
            if def.unique_object && objects.len() > 1 {
                result.push(ValidationError {
                    object_type: obj_type.to_string(),
                    object_name: String::new(),
                    field: String::new(),
                    message: format!(
                        "{} is a unique object but {} instances found",
                        def.name,
                        objects.len()
                    ),
                    severity: ValidationSeverity::Error,
                });
            }

            for obj in objects {
                let obj_name = obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                // Validate each defined field
                for (i, field_def) in def.fields.iter().enumerate() {
                    let field_key = if i == 0 {
                        "name".to_string()
                    } else {
                        format!("field_{i}")
                    };

                    let val = obj.get(&field_key).and_then(|v| v.as_str());

                    // Required field check
                    if field_def.required && val.map_or(true, |s| s.is_empty()) {
                        if field_def.default_value.is_none() {
                            result.push(ValidationError {
                                object_type: obj_type.to_string(),
                                object_name: obj_name.clone(),
                                field: field_def.name.clone(),
                                message: format!("Required field '{}' is missing", field_def.name),
                                severity: ValidationSeverity::Error,
                            });
                        }
                        continue;
                    }

                    if let Some(val_str) = val {
                        // Handle autosize/autocalculate
                        let val_upper = val_str.to_uppercase();
                        if val_upper == "AUTOSIZE" {
                            if !field_def.autosizable {
                                result.push(ValidationError {
                                    object_type: obj_type.to_string(),
                                    object_name: obj_name.clone(),
                                    field: field_def.name.clone(),
                                    message: format!(
                                        "Field '{}' does not allow autosize",
                                        field_def.name
                                    ),
                                    severity: ValidationSeverity::Error,
                                });
                            }
                            continue;
                        }
                        if val_upper == "AUTOCALCULATE" {
                            if !field_def.autocalculatable {
                                result.push(ValidationError {
                                    object_type: obj_type.to_string(),
                                    object_name: obj_name.clone(),
                                    field: field_def.name.clone(),
                                    message: format!(
                                        "Field '{}' does not allow autocalculate",
                                        field_def.name
                                    ),
                                    severity: ValidationSeverity::Error,
                                });
                            }
                            continue;
                        }

                        // Type-specific validation
                        match &field_def.field_type {
                            FieldType::Real | FieldType::Integer => {
                                validate_numeric(
                                    &mut result,
                                    obj_type,
                                    &obj_name,
                                    field_def,
                                    val_str,
                                );
                            }
                            FieldType::Choice(options) => {
                                validate_choice(
                                    &mut result,
                                    obj_type,
                                    &obj_name,
                                    field_def,
                                    val_str,
                                    options,
                                );
                            }
                            FieldType::ObjectList(list_name) => {
                                validate_reference(
                                    &mut result,
                                    obj_type,
                                    &obj_name,
                                    field_def,
                                    val_str,
                                    list_name,
                                    &reference_sets,
                                );
                            }
                            FieldType::Alpha | FieldType::Node | FieldType::ExternalList => {
                                // No additional validation for these types
                            }
                        }
                    }
                }
            }
        }
    }

    result
}

/// Validate a numeric field value against bounds.
fn validate_numeric(
    result: &mut ValidationResult,
    obj_type: &str,
    obj_name: &str,
    field_def: &crate::schema::FieldDef,
    val_str: &str,
) {
    let parsed: Result<f64, _> = val_str.parse();
    match parsed {
        Ok(v) => {
            if !field_def.min_bound.check_min(v) {
                let bound_desc = match field_def.min_bound {
                    Bound::Inclusive(b) => format!(">= {b}"),
                    Bound::Exclusive(b) => format!("> {b}"),
                    Bound::None => unreachable!(),
                };
                result.push(ValidationError {
                    object_type: obj_type.to_string(),
                    object_name: obj_name.to_string(),
                    field: field_def.name.clone(),
                    message: format!(
                        "Value {v} for '{}' must be {bound_desc}",
                        field_def.name
                    ),
                    severity: ValidationSeverity::Error,
                });
            }
            if !field_def.max_bound.check_max(v) {
                let bound_desc = match field_def.max_bound {
                    Bound::Inclusive(b) => format!("<= {b}"),
                    Bound::Exclusive(b) => format!("< {b}"),
                    Bound::None => unreachable!(),
                };
                result.push(ValidationError {
                    object_type: obj_type.to_string(),
                    object_name: obj_name.to_string(),
                    field: field_def.name.clone(),
                    message: format!(
                        "Value {v} for '{}' must be {bound_desc}",
                        field_def.name
                    ),
                    severity: ValidationSeverity::Error,
                });
            }
        }
        Err(_) => {
            result.push(ValidationError {
                object_type: obj_type.to_string(),
                object_name: obj_name.to_string(),
                field: field_def.name.clone(),
                message: format!(
                    "Field '{}' expected numeric value, got '{val_str}'",
                    field_def.name
                ),
                severity: ValidationSeverity::Error,
            });
        }
    }
}

/// Validate a choice field against allowed values.
fn validate_choice(
    result: &mut ValidationResult,
    obj_type: &str,
    obj_name: &str,
    field_def: &crate::schema::FieldDef,
    val_str: &str,
    options: &[String],
) {
    let val_upper = val_str.to_uppercase();
    let is_valid = options.iter().any(|o| o.to_uppercase() == val_upper);
    if !is_valid {
        result.push(ValidationError {
            object_type: obj_type.to_string(),
            object_name: obj_name.to_string(),
            field: field_def.name.clone(),
            message: format!(
                "Invalid choice '{}' for '{}'; valid options: {}",
                val_str,
                field_def.name,
                options.join(", ")
            ),
            severity: ValidationSeverity::Error,
        });
    }
}

/// Validate an object-list reference field against the collected reference sets.
fn validate_reference(
    result: &mut ValidationResult,
    obj_type: &str,
    obj_name: &str,
    field_def: &crate::schema::FieldDef,
    val_str: &str,
    list_name: &str,
    reference_sets: &HashMap<String, HashSet<String>>,
) {
    if val_str.is_empty() {
        return;
    }
    let val_upper = val_str.to_uppercase();
    let list_upper = list_name.to_uppercase();

    if let Some(set) = reference_sets.get(&list_upper) {
        if !set.contains(&val_upper) {
            result.push(ValidationError {
                object_type: obj_type.to_string(),
                object_name: obj_name.to_string(),
                field: field_def.name.clone(),
                message: format!(
                    "Reference '{}' for '{}' not found in list '{}'",
                    val_str, field_def.name, list_name
                ),
                severity: ValidationSeverity::Error,
            });
        }
    } else {
        // No objects registered in this list at all
        result.push(ValidationError {
            object_type: obj_type.to_string(),
            object_name: obj_name.to_string(),
            field: field_def.name.clone(),
            message: format!(
                "Reference '{}' for '{}' not found in list '{}'",
                val_str, field_def.name, list_name
            ),
            severity: ValidationSeverity::Error,
        });
    }
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

    #[test]
    fn schema_required_fields() {
        let schema = SchemaDb::minimal();
        let mut model = InputModel::default();
        // Material with missing required fields
        model.add_object("Material", serde_json::json!({"name": "TestMat"}));
        let result = validate_schema(&model, &schema);
        // Should have errors for missing Roughness, Thickness, Conductivity, Density, Specific Heat
        assert!(!result.is_ok());
        assert!(result.errors.len() >= 5);
    }

    #[test]
    fn schema_numeric_range_inclusive() {
        let schema = SchemaDb::minimal();
        let mut model = InputModel::default();
        // Timestep with value out of range (max 60)
        model.add_object(
            "Timestep",
            serde_json::json!({"name": "100"}),
        );
        let result = validate_schema(&model, &schema);
        // name field is field[0] = "100", which the schema treats as the first field
        // For Timestep, field[0] = "Number of Timesteps per Hour" -> 100 > 60
        let range_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.field.contains("Timesteps"))
            .collect();
        assert!(!range_errors.is_empty());
    }

    #[test]
    fn schema_numeric_range_exclusive() {
        let schema = SchemaDb::minimal();
        let mut model = InputModel::default();
        // Building with zero loads convergence (must be > 0.0, exclusive)
        model.add_object(
            "Building",
            serde_json::json!({
                "name": "TestBldg",
                "field_1": "0",
                "field_2": "City",
                "field_3": "0",  // Loads Convergence = 0, violates > 0
            }),
        );
        let result = validate_schema(&model, &schema);
        let range_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.field.contains("Loads Convergence"))
            .collect();
        assert!(!range_errors.is_empty());
    }

    #[test]
    fn schema_choice_validation() {
        let schema = SchemaDb::minimal();
        let mut model = InputModel::default();
        model.add_object(
            "Building",
            serde_json::json!({
                "name": "TestBldg",
                "field_1": "0",
                "field_2": "InvalidTerrain",
            }),
        );
        let result = validate_schema(&model, &schema);
        let choice_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.field == "Terrain")
            .collect();
        assert_eq!(choice_errors.len(), 1);
        assert!(choice_errors[0].message.contains("Invalid choice"));
    }

    #[test]
    fn schema_choice_case_insensitive() {
        let schema = SchemaDb::minimal();
        let mut model = InputModel::default();
        model.add_object(
            "Building",
            serde_json::json!({
                "name": "TestBldg",
                "field_1": "0",
                "field_2": "city",  // lowercase should be valid
            }),
        );
        let result = validate_schema(&model, &schema);
        let choice_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.field == "Terrain")
            .collect();
        assert!(choice_errors.is_empty());
    }

    #[test]
    fn schema_unique_object() {
        let schema = SchemaDb::minimal();
        let mut model = InputModel::default();
        model.add_object("Building", serde_json::json!({"name": "Bldg1"}));
        model.add_object("Building", serde_json::json!({"name": "Bldg2"}));
        let result = validate_schema(&model, &schema);
        let unique_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.message.contains("unique object"))
            .collect();
        assert_eq!(unique_errors.len(), 1);
    }

    #[test]
    fn schema_cross_references_valid() {
        let schema = SchemaDb::minimal();
        let mut model = InputModel::default();
        // Add a Material that registers in MaterialName
        model.add_object(
            "Material",
            serde_json::json!({
                "name": "TestMat",
                "field_1": "Rough",
                "field_2": "0.1",
                "field_3": "1.0",
                "field_4": "1000",
                "field_5": "1000",
            }),
        );
        // Add a Construction that references it
        model.add_object(
            "Construction",
            serde_json::json!({
                "name": "TestConst",
                "field_1": "TestMat",
            }),
        );
        let result = validate_schema(&model, &schema);
        let ref_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.field == "Outside Layer")
            .collect();
        assert!(ref_errors.is_empty(), "Valid reference should not produce errors");
    }

    #[test]
    fn schema_cross_references_broken() {
        let schema = SchemaDb::minimal();
        let mut model = InputModel::default();
        // Construction referencing a material that doesn't exist
        model.add_object(
            "Construction",
            serde_json::json!({
                "name": "TestConst",
                "field_1": "NonexistentMaterial",
            }),
        );
        let result = validate_schema(&model, &schema);
        let ref_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.field == "Outside Layer")
            .collect();
        assert_eq!(ref_errors.len(), 1);
        assert!(ref_errors[0].message.contains("not found"));
    }

    #[test]
    fn schema_autosize_allowed() {
        let schema = SchemaDb::minimal();
        let mut model = InputModel::default();
        // Zone with autocalculate ceiling height - should be fine
        model.add_object(
            "Zone",
            serde_json::json!({
                "name": "Zone1",
                "field_7": "autocalculate",
            }),
        );
        let result = validate_schema(&model, &schema);
        let auto_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.message.contains("autocalculate"))
            .collect();
        assert!(auto_errors.is_empty());
    }

    #[test]
    fn schema_autosize_disallowed() {
        let schema = SchemaDb::minimal();
        let mut model = InputModel::default();
        // Material thickness cannot be autosized
        model.add_object(
            "Material",
            serde_json::json!({
                "name": "TestMat",
                "field_1": "Rough",
                "field_2": "autosize",
                "field_3": "1.0",
                "field_4": "1000",
                "field_5": "1000",
            }),
        );
        let result = validate_schema(&model, &schema);
        let auto_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.message.contains("autosize"))
            .collect();
        assert!(!auto_errors.is_empty());
    }

    #[test]
    fn validation_result_merge() {
        let mut r1 = ValidationResult::new();
        r1.errors.push(ValidationError {
            object_type: "A".into(),
            object_name: String::new(),
            field: String::new(),
            message: "err1".into(),
            severity: ValidationSeverity::Error,
        });

        let mut r2 = ValidationResult::new();
        r2.warnings.push(ValidationError {
            object_type: "B".into(),
            object_name: String::new(),
            field: String::new(),
            message: "warn1".into(),
            severity: ValidationSeverity::Warning,
        });

        r1.merge(r2);
        assert_eq!(r1.errors.len(), 1);
        assert_eq!(r1.warnings.len(), 1);
        assert_eq!(r1.total(), 2);
        assert!(!r1.is_ok());
    }
}
