//! Input parsing (IDF/JSON) and schema validation for EnergyPlus-rs.
//!
//! Supports EnergyPlus IDF format and epJSON format.

use std::collections::HashMap;
use std::path::Path;

pub mod idf;
pub mod macro_proc;
pub mod schema;
pub mod validation;

/// Parsed input model containing all objects from an IDF or epJSON file.
#[derive(Debug, Clone, Default)]
pub struct InputModel {
    /// Objects keyed by object type (case-insensitive, stored uppercase).
    objects: HashMap<String, Vec<serde_json::Value>>,
}

impl InputModel {
    /// Parse from an IDF file.
    pub fn from_idf(path: &Path) -> Result<Self, InputError> {
        let content = std::fs::read_to_string(path).map_err(|e| InputError::IoError(e.to_string()))?;
        idf::parse_idf_string(&content)
    }

    /// Parse from an IDF file with schema-aware field naming.
    pub fn from_idf_with_schema(path: &Path, schema: &schema::SchemaDb) -> Result<Self, InputError> {
        let content = std::fs::read_to_string(path).map_err(|e| InputError::IoError(e.to_string()))?;
        idf::parse_idf_with_schema(&content, schema)
    }

    /// Parse from an epJSON file.
    pub fn from_json(path: &Path) -> Result<Self, InputError> {
        let content = std::fs::read_to_string(path).map_err(|e| InputError::IoError(e.to_string()))?;
        let json: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| InputError::ParseError(format!("JSON parse error: {e}")))?;

        let mut model = InputModel::default();

        if let Some(obj) = json.as_object() {
            for (key, value) in obj {
                if let Some(instances) = value.as_object() {
                    let upper_key = key.to_uppercase();
                    for (_name, instance_data) in instances {
                        model
                            .objects
                            .entry(upper_key.clone())
                            .or_default()
                            .push(instance_data.clone());
                    }
                }
            }
        }

        Ok(model)
    }

    /// Get all objects of a given type (case-insensitive).
    pub fn get_objects(&self, object_type: &str) -> &[serde_json::Value] {
        self.objects.get(&object_type.to_uppercase()).map_or(&[], |v| v.as_slice())
    }

    /// Get mutable access to all objects of a given type (case-insensitive).
    pub fn get_objects_mut(&mut self, object_type: &str) -> &mut Vec<serde_json::Value> {
        self.objects.entry(object_type.to_uppercase()).or_default()
    }

    /// Get the number of objects of a given type.
    pub fn object_count(&self, object_type: &str) -> usize {
        self.objects.get(&object_type.to_uppercase()).map_or(0, |v| v.len())
    }

    /// Get all object types present in the model.
    pub fn object_types(&self) -> Vec<&str> {
        self.objects.keys().map(|s| s.as_str()).collect()
    }

    /// Total number of objects across all types.
    pub fn total_objects(&self) -> usize {
        self.objects.values().map(|v| v.len()).sum()
    }

    /// Insert an object into the model.
    pub fn add_object(&mut self, object_type: &str, object: serde_json::Value) {
        self.objects.entry(object_type.to_uppercase()).or_default().push(object);
    }
}

/// Inject default values from the schema into a model.
///
/// For each object in the model, if a field is missing and the schema
/// provides a default value, the default is inserted.
pub fn inject_defaults(model: &mut InputModel, schema: &schema::SchemaDb) {
    let obj_types: Vec<String> = model.object_types().iter().map(|s| s.to_string()).collect();
    for obj_type in &obj_types {
        if let Some(def) = schema.get(obj_type) {
            let objects = model.get_objects_mut(obj_type);
            for obj in objects.iter_mut() {
                if let Some(map) = obj.as_object_mut() {
                    for (i, field_def) in def.fields.iter().enumerate() {
                        let field_key = if i == 0 {
                            "name".to_string()
                        } else {
                            format!("field_{i}")
                        };
                        if !map.contains_key(&field_key) && !map.contains_key(&field_def.name) {
                            if let Some(default) = &field_def.default_value {
                                map.insert(
                                    field_def.name.clone(),
                                    serde_json::Value::String(default.clone()),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Input processing error type.
#[derive(Debug, Clone)]
pub enum InputError {
    IoError(String),
    ParseError(String),
    ValidationError(Vec<ValidationError>),
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputError::IoError(e) => write!(f, "I/O error: {e}"),
            InputError::ParseError(e) => write!(f, "Parse error: {e}"),
            InputError::ValidationError(errors) => {
                write!(f, "{} validation errors", errors.len())
            }
        }
    }
}

impl std::error::Error for InputError {}

/// A validation error found during input checking.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub object_type: String,
    pub object_name: String,
    pub field: String,
    pub message: String,
    pub severity: ValidationSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationSeverity {
    Warning,
    Error,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{:?}] {}/{}: {} - {}",
            self.severity, self.object_type, self.object_name, self.field, self.message
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_model() {
        let model = InputModel::default();
        assert_eq!(model.total_objects(), 0);
        assert!(model.get_objects("Building").is_empty());
    }

    #[test]
    fn add_and_retrieve_objects() {
        let mut model = InputModel::default();
        model.add_object("Building", serde_json::json!({"name": "TestBuilding"}));
        model.add_object("Building", serde_json::json!({"name": "TestBuilding2"}));

        assert_eq!(model.object_count("Building"), 2);
        assert_eq!(model.object_count("building"), 2); // case-insensitive
        assert_eq!(model.total_objects(), 2);
    }

    #[test]
    fn parse_json_string() {
        let json = r#"{
            "Building": {
                "MyBuilding": {
                    "north_axis": 0,
                    "terrain": "City"
                }
            },
            "Zone": {
                "Zone1": {"name": "Zone1"},
                "Zone2": {"name": "Zone2"}
            }
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let mut model = InputModel::default();

        if let Some(obj) = parsed.as_object() {
            for (key, value) in obj {
                if let Some(instances) = value.as_object() {
                    for (_name, instance_data) in instances {
                        model.add_object(key, instance_data.clone());
                    }
                }
            }
        }

        assert_eq!(model.object_count("Building"), 1);
        assert_eq!(model.object_count("Zone"), 2);
    }

    #[test]
    fn get_objects_mut_creates_entry() {
        let mut model = InputModel::default();
        let objs = model.get_objects_mut("Zone");
        assert!(objs.is_empty());
        objs.push(serde_json::json!({"name": "Z1"}));
        assert_eq!(model.object_count("Zone"), 1);
    }

    #[test]
    fn inject_defaults_fills_missing() {
        let schema = schema::SchemaDb::minimal();
        let mut model = InputModel::default();
        model.add_object("Building", serde_json::json!({"name": "TestBldg"}));
        inject_defaults(&mut model, &schema);

        let bldgs = model.get_objects("Building");
        let b = &bldgs[0];
        // North Axis should have been injected as "0"
        assert_eq!(
            b.get("North Axis").and_then(|v| v.as_str()),
            Some("0")
        );
        // Terrain should have been injected as "Suburbs"
        assert_eq!(
            b.get("Terrain").and_then(|v| v.as_str()),
            Some("Suburbs")
        );
    }

    #[test]
    fn inject_defaults_does_not_overwrite() {
        let schema = schema::SchemaDb::minimal();
        let mut model = InputModel::default();
        model.add_object(
            "Building",
            serde_json::json!({"name": "TestBldg", "Terrain": "City"}),
        );
        inject_defaults(&mut model, &schema);

        let bldgs = model.get_objects("Building");
        let b = &bldgs[0];
        // Should keep the user-specified value
        assert_eq!(b.get("Terrain").and_then(|v| v.as_str()), Some("City"));
    }
}
