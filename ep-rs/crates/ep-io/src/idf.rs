//! IDF (Input Data File) tokenizer and parser.
//!
//! Parses EnergyPlus IDF format: free-format, comma-delimited fields
//! with `!` line comments and `;` object terminators.

use crate::schema::SchemaDb;
use crate::{InputError, InputModel};

/// Parse an IDF string into an InputModel.
pub fn parse_idf_string(content: &str) -> Result<InputModel, InputError> {
    let tokens = tokenize(content);
    let objects = parse_objects(&tokens)?;

    let mut model = InputModel::default();
    for obj in objects {
        if obj.fields.is_empty() {
            continue;
        }
        // First field is the object type
        let object_type = obj.fields[0].clone();
        // Remaining fields as JSON object with numeric indices
        let mut json_obj = serde_json::Map::new();
        for (i, field) in obj.fields.iter().enumerate().skip(1) {
            json_obj.insert(format!("field_{}", i), serde_json::Value::String(field.clone()));
        }
        // Add name field if present
        if obj.fields.len() > 1 {
            json_obj.insert("name".to_string(), serde_json::Value::String(obj.fields[1].clone()));
        }
        model.add_object(&object_type, serde_json::Value::Object(json_obj));
    }

    Ok(model)
}

/// Parse an IDF string into an InputModel, mapping positional fields to
/// named fields using the schema. Extensible field groups are handled.
pub fn parse_idf_with_schema(content: &str, schema: &SchemaDb) -> Result<InputModel, InputError> {
    let tokens = tokenize(content);
    let objects = parse_objects(&tokens)?;

    let mut model = InputModel::default();
    for obj in objects {
        if obj.fields.is_empty() {
            continue;
        }
        let object_type = obj.fields[0].clone();
        let mut json_obj = serde_json::Map::new();

        if let Some(def) = schema.get(&object_type) {
            // Map positional fields to named fields using schema
            for (i, field_val) in obj.fields.iter().enumerate().skip(1) {
                let field_index = i - 1; // 0-based field index
                if let Some(field_def) = def.field_at(field_index) {
                    let key = field_def.name.clone();
                    json_obj.insert(key, serde_json::Value::String(field_val.clone()));
                } else {
                    // Extra field beyond schema definition
                    json_obj.insert(
                        format!("field_{i}"),
                        serde_json::Value::String(field_val.clone()),
                    );
                }
            }
            // Also set "name" key if the first schema field has reference lists
            // (convention: first field is typically the name)
            if !def.fields.is_empty() {
                if let Some(name_val) = json_obj.get(&def.fields[0].name) {
                    json_obj.insert("name".to_string(), name_val.clone());
                }
            }
        } else {
            // No schema definition: fall back to numbered fields
            for (i, field) in obj.fields.iter().enumerate().skip(1) {
                json_obj.insert(
                    format!("field_{i}"),
                    serde_json::Value::String(field.clone()),
                );
            }
            if obj.fields.len() > 1 {
                json_obj.insert(
                    "name".to_string(),
                    serde_json::Value::String(obj.fields[1].clone()),
                );
            }
        }

        model.add_object(&object_type, serde_json::Value::Object(json_obj));
    }

    Ok(model)
}

/// A parsed IDF object (list of field strings).
#[derive(Debug)]
struct IdfObject {
    fields: Vec<String>,
}

/// Token types from IDF lexer.
#[derive(Debug, Clone)]
enum Token {
    Field(String),
    Comma,
    Semicolon,
}

/// Tokenize an IDF string.
fn tokenize(content: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current_field = String::new();

    for line in content.lines() {
        // Strip comments (everything after !)
        let line = if let Some(idx) = line.find('!') {
            &line[..idx]
        } else {
            line
        };

        for ch in line.chars() {
            match ch {
                ',' => {
                    let field = current_field.trim().to_string();
                    if !field.is_empty() {
                        tokens.push(Token::Field(field));
                    }
                    tokens.push(Token::Comma);
                    current_field.clear();
                }
                ';' => {
                    let field = current_field.trim().to_string();
                    if !field.is_empty() {
                        tokens.push(Token::Field(field));
                    }
                    tokens.push(Token::Semicolon);
                    current_field.clear();
                }
                _ => {
                    current_field.push(ch);
                }
            }
        }
    }

    // Handle trailing field without terminator
    let field = current_field.trim().to_string();
    if !field.is_empty() {
        tokens.push(Token::Field(field));
    }

    tokens
}

/// Parse token stream into IDF objects.
fn parse_objects(tokens: &[Token]) -> Result<Vec<IdfObject>, InputError> {
    let mut objects = Vec::new();
    let mut current_fields = Vec::new();

    for token in tokens {
        match token {
            Token::Field(f) => {
                current_fields.push(f.clone());
            }
            Token::Comma => {
                // Field separator within an object
            }
            Token::Semicolon => {
                // Object terminator
                if !current_fields.is_empty() {
                    objects.push(IdfObject {
                        fields: std::mem::take(&mut current_fields),
                    });
                }
            }
        }
    }

    // Handle unterminated object
    if !current_fields.is_empty() {
        objects.push(IdfObject {
            fields: current_fields,
        });
    }

    Ok(objects)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_idf() {
        let idf = r#"
  Version,
    24.2;                    !- Version Identifier

  Building,
    TestBuilding,            !- Name
    0,                       !- North Axis {deg}
    City;                    !- Terrain
"#;
        let model = parse_idf_string(idf).unwrap();
        assert_eq!(model.object_count("Version"), 1);
        assert_eq!(model.object_count("Building"), 1);
    }

    #[test]
    fn parse_comments_stripped() {
        let idf = "! This is a comment\nVersion, 24.2; ! inline comment\n";
        let model = parse_idf_string(idf).unwrap();
        assert_eq!(model.object_count("Version"), 1);
    }

    #[test]
    fn parse_empty_idf() {
        let model = parse_idf_string("").unwrap();
        assert_eq!(model.total_objects(), 0);
    }

    #[test]
    fn tokenize_basic() {
        let tokens = tokenize("Building, TestName, 0;");
        // Should have: Field("Building"), Comma, Field("TestName"), Comma, Field("0"), Semicolon
        assert!(tokens.len() >= 4);
    }

    #[test]
    fn parse_with_schema_names_fields() {
        let schema = SchemaDb::minimal();
        let idf = r#"
  Building,
    TestBuilding,            !- Name
    0,                       !- North Axis {deg}
    City;                    !- Terrain
"#;
        let model = parse_idf_with_schema(idf, &schema).unwrap();
        let bldgs = model.get_objects("Building");
        assert_eq!(bldgs.len(), 1);
        let b = &bldgs[0];
        assert_eq!(b.get("Name").and_then(|v| v.as_str()), Some("TestBuilding"));
        assert_eq!(b.get("North Axis").and_then(|v| v.as_str()), Some("0"));
        assert_eq!(b.get("Terrain").and_then(|v| v.as_str()), Some("City"));
    }

    #[test]
    fn parse_with_schema_extensible_fields() {
        let schema = SchemaDb::minimal();
        let idf = r#"
  BuildingSurface:Detailed,
    Surface1,
    Wall,
    TestConst,
    Zone1,
    Outdoors,
    ,
    SunExposed,
    WindExposed,
    autocalculate,
    4,
    0, 0, 3,
    10, 0, 3,
    10, 0, 0,
    0, 0, 0;
"#;
        let model = parse_idf_with_schema(idf, &schema).unwrap();
        let surfaces = model.get_objects("BuildingSurface:Detailed");
        assert_eq!(surfaces.len(), 1);
        let s = &surfaces[0];
        assert_eq!(s.get("Name").and_then(|v| v.as_str()), Some("Surface1"));
        assert_eq!(s.get("Surface Type").and_then(|v| v.as_str()), Some("Wall"));
        // Extensible vertex group: field at index 10 = "Vertex X-coordinate" (first vertex)
        // Index 13 wraps around to "Vertex X-coordinate" (second vertex)
        assert_eq!(
            s.get("Vertex X-coordinate").and_then(|v| v.as_str()),
            Some("0")
        );
    }

    #[test]
    fn parse_with_schema_unknown_type_fallback() {
        let schema = SchemaDb::minimal();
        let idf = "CustomObject, Field1, Field2;";
        let model = parse_idf_with_schema(idf, &schema).unwrap();
        let objs = model.get_objects("CustomObject");
        assert_eq!(objs.len(), 1);
        assert_eq!(objs[0].get("name").and_then(|v| v.as_str()), Some("Field1"));
        assert_eq!(objs[0].get("field_2").and_then(|v| v.as_str()), Some("Field2"));
    }

    #[test]
    fn parse_multiline_object() {
        let idf = r#"
  Zone,
    Zone1,
    0,
    0, 0, 0,
    1,
    1,
    autocalculate,
    autocalculate;
"#;
        let model = parse_idf_string(idf).unwrap();
        assert_eq!(model.object_count("Zone"), 1);
        let zones = model.get_objects("Zone");
        assert_eq!(zones.len(), 1);
    }
}
