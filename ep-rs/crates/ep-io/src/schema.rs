//! IDD schema representation for field-level validation.
//!
//! Provides types to describe EnergyPlus object definitions (field types,
//! numeric bounds, defaults, references) and a `SchemaDb` that holds a
//! collection of such definitions with case-insensitive lookup.

use std::collections::HashMap;

/// The data type of an IDF/epJSON field.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    Alpha,
    Real,
    Integer,
    Choice(Vec<String>),
    /// Reference to objects registered in the named list.
    ObjectList(String),
    Node,
    ExternalList,
}

/// Inclusive or exclusive numeric bound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bound {
    Inclusive(f64),
    Exclusive(f64),
    None,
}

impl Bound {
    /// Check whether `value` satisfies this lower bound.
    pub fn check_min(&self, value: f64) -> bool {
        match self {
            Bound::Inclusive(b) => value >= *b,
            Bound::Exclusive(b) => value > *b,
            Bound::None => true,
        }
    }

    /// Check whether `value` satisfies this upper bound.
    pub fn check_max(&self, value: f64) -> bool {
        match self {
            Bound::Inclusive(b) => value <= *b,
            Bound::Exclusive(b) => value < *b,
            Bound::None => true,
        }
    }
}

/// Definition of a single field within an IDD object.
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub field_type: FieldType,
    pub required: bool,
    pub default_value: Option<String>,
    pub min_bound: Bound,
    pub max_bound: Bound,
    pub units: Option<String>,
    pub autosizable: bool,
    pub autocalculatable: bool,
    /// Lists this field registers names into (for cross-referencing).
    pub reference_lists: Vec<String>,
    /// Lists this field references from (for cross-referencing).
    pub object_lists: Vec<String>,
    pub retain_case: bool,
}

impl FieldDef {
    fn new(name: &str, field_type: FieldType) -> Self {
        Self {
            name: name.to_string(),
            field_type,
            required: false,
            default_value: None,
            min_bound: Bound::None,
            max_bound: Bound::None,
            units: None,
            autosizable: false,
            autocalculatable: false,
            reference_lists: Vec::new(),
            object_lists: Vec::new(),
            retain_case: false,
        }
    }

    /// Create an Alpha (string) field.
    pub fn alpha(name: &str) -> Self {
        Self::new(name, FieldType::Alpha)
    }

    /// Create a Real (floating-point) field.
    pub fn real(name: &str) -> Self {
        Self::new(name, FieldType::Real)
    }

    /// Create an Integer field.
    pub fn integer(name: &str) -> Self {
        Self::new(name, FieldType::Integer)
    }

    /// Create a Choice field with allowed values.
    pub fn choice(name: &str, options: &[&str]) -> Self {
        Self::new(
            name,
            FieldType::Choice(options.iter().map(|s| s.to_string()).collect()),
        )
    }

    /// Create an ObjectList reference field.
    pub fn object_list(name: &str, list: &str) -> Self {
        let mut fd = Self::new(name, FieldType::ObjectList(list.to_string()));
        fd.object_lists.push(list.to_string());
        fd
    }

    /// Create a Node field.
    pub fn node(name: &str) -> Self {
        Self::new(name, FieldType::Node)
    }

    // Builder methods

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn default_value(mut self, val: &str) -> Self {
        self.default_value = Some(val.to_string());
        self
    }

    pub fn min_inclusive(mut self, val: f64) -> Self {
        self.min_bound = Bound::Inclusive(val);
        self
    }

    pub fn min_exclusive(mut self, val: f64) -> Self {
        self.min_bound = Bound::Exclusive(val);
        self
    }

    pub fn max_inclusive(mut self, val: f64) -> Self {
        self.max_bound = Bound::Inclusive(val);
        self
    }

    pub fn max_exclusive(mut self, val: f64) -> Self {
        self.max_bound = Bound::Exclusive(val);
        self
    }

    pub fn units(mut self, u: &str) -> Self {
        self.units = Some(u.to_string());
        self
    }

    pub fn autosizable(mut self) -> Self {
        self.autosizable = true;
        self
    }

    pub fn autocalculatable(mut self) -> Self {
        self.autocalculatable = true;
        self
    }

    pub fn references(mut self, lists: &[&str]) -> Self {
        self.reference_lists = lists.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn retain_case(mut self) -> Self {
        self.retain_case = true;
        self
    }
}

/// Definition of an IDD object type.
#[derive(Debug, Clone)]
pub struct ObjectDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
    /// Number of extensible fields at the end that repeat as a group.
    pub extensible_count: usize,
    pub min_fields: usize,
    pub unique_object: bool,
    pub memo: String,
}

impl ObjectDef {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            fields: Vec::new(),
            extensible_count: 0,
            min_fields: 0,
            unique_object: false,
            memo: String::new(),
        }
    }

    pub fn field(mut self, f: FieldDef) -> Self {
        self.fields.push(f);
        self
    }

    pub fn extensible(mut self, count: usize) -> Self {
        self.extensible_count = count;
        self
    }

    pub fn min_fields(mut self, n: usize) -> Self {
        self.min_fields = n;
        self
    }

    pub fn unique(mut self) -> Self {
        self.unique_object = true;
        self
    }

    pub fn memo(mut self, m: &str) -> Self {
        self.memo = m.to_string();
        self
    }

    /// Resolve the field definition for position `index` (0-based), handling
    /// extensible repeats.
    pub fn field_at(&self, index: usize) -> Option<&FieldDef> {
        if index < self.fields.len() {
            Some(&self.fields[index])
        } else if self.extensible_count > 0 {
            let base = self.fields.len() - self.extensible_count;
            let offset = (index - self.fields.len()) % self.extensible_count;
            Some(&self.fields[base + offset])
        } else {
            None
        }
    }
}

/// Collection of object definitions with case-insensitive lookup.
#[derive(Debug, Clone, Default)]
pub struct SchemaDb {
    defs: HashMap<String, ObjectDef>,
}

impl SchemaDb {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an object definition.
    pub fn insert(&mut self, def: ObjectDef) {
        self.defs.insert(def.name.to_uppercase(), def);
    }

    /// Look up an object definition (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&ObjectDef> {
        self.defs.get(&name.to_uppercase())
    }

    /// Iterate over all definitions.
    pub fn iter(&self) -> impl Iterator<Item = &ObjectDef> {
        self.defs.values()
    }

    /// Number of defined object types.
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// Build a minimal schema with ~15 core EnergyPlus object types.
    pub fn minimal() -> Self {
        let mut db = Self::new();

        // Version
        db.insert(
            ObjectDef::new("Version")
                .unique()
                .memo("Specifies the EnergyPlus version")
                .field(FieldDef::alpha("Version Identifier").default_value("24.2")),
        );

        // Building
        db.insert(
            ObjectDef::new("Building")
                .unique()
                .memo("Describes building parameters")
                .min_fields(1)
                .field(FieldDef::alpha("Name").required().references(&["BuildingNames"]))
                .field(FieldDef::real("North Axis").default_value("0").units("deg"))
                .field(
                    FieldDef::choice(
                        "Terrain",
                        &["Country", "Suburbs", "City", "Ocean", "Urban"],
                    )
                    .default_value("Suburbs"),
                )
                .field(
                    FieldDef::real("Loads Convergence Tolerance Value")
                        .default_value("0.04")
                        .min_exclusive(0.0)
                        .max_inclusive(0.5),
                )
                .field(
                    FieldDef::real("Temperature Convergence Tolerance Value")
                        .default_value("0.4")
                        .min_exclusive(0.0)
                        .max_inclusive(0.5)
                        .units("deltaC"),
                )
                .field(
                    FieldDef::choice(
                        "Solar Distribution",
                        &[
                            "MinimalShadowing",
                            "FullExterior",
                            "FullInteriorAndExterior",
                            "FullExteriorWithReflections",
                            "FullInteriorAndExteriorWithReflections",
                        ],
                    )
                    .default_value("FullExterior"),
                )
                .field(
                    FieldDef::integer("Maximum Number of Warmup Days")
                        .default_value("25")
                        .min_inclusive(1.0),
                )
                .field(
                    FieldDef::integer("Minimum Number of Warmup Days")
                        .default_value("1")
                        .min_inclusive(1.0),
                ),
        );

        // Zone
        db.insert(
            ObjectDef::new("Zone")
                .memo("Defines a thermal zone")
                .min_fields(1)
                .field(FieldDef::alpha("Name").required().references(&["ZoneNames"]))
                .field(FieldDef::real("Direction of Relative North").default_value("0").units("deg"))
                .field(FieldDef::real("X Origin").default_value("0").units("m"))
                .field(FieldDef::real("Y Origin").default_value("0").units("m"))
                .field(FieldDef::real("Z Origin").default_value("0").units("m"))
                .field(FieldDef::integer("Type").default_value("1").min_inclusive(1.0).max_inclusive(1.0))
                .field(FieldDef::integer("Multiplier").default_value("1").min_inclusive(1.0))
                .field(FieldDef::real("Ceiling Height").autocalculatable().default_value("autocalculate").units("m"))
                .field(FieldDef::real("Volume").autocalculatable().default_value("autocalculate").units("m3")),
        );

        // SimulationControl
        db.insert(
            ObjectDef::new("SimulationControl")
                .unique()
                .memo("Simulation control flags")
                .field(FieldDef::choice("Do Zone Sizing Calculation", &["Yes", "No"]).default_value("No"))
                .field(FieldDef::choice("Do System Sizing Calculation", &["Yes", "No"]).default_value("No"))
                .field(FieldDef::choice("Do Plant Sizing Calculation", &["Yes", "No"]).default_value("No"))
                .field(FieldDef::choice("Run Simulation for Sizing Periods", &["Yes", "No"]).default_value("Yes"))
                .field(FieldDef::choice("Run Simulation for Weather File Run Periods", &["Yes", "No"]).default_value("Yes")),
        );

        // Timestep
        db.insert(
            ObjectDef::new("Timestep")
                .unique()
                .memo("Number of timesteps per hour")
                .field(
                    FieldDef::integer("Number of Timesteps per Hour")
                        .default_value("6")
                        .min_inclusive(1.0)
                        .max_inclusive(60.0),
                ),
        );

        // RunPeriod
        db.insert(
            ObjectDef::new("RunPeriod")
                .memo("Specifies simulation run period")
                .min_fields(5)
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::integer("Begin Month").required().min_inclusive(1.0).max_inclusive(12.0))
                .field(FieldDef::integer("Begin Day of Month").required().min_inclusive(1.0).max_inclusive(31.0))
                .field(FieldDef::integer("End Month").required().min_inclusive(1.0).max_inclusive(12.0))
                .field(FieldDef::integer("End Day of Month").required().min_inclusive(1.0).max_inclusive(31.0)),
        );

        // Material
        db.insert(
            ObjectDef::new("Material")
                .memo("Regular material layer")
                .min_fields(6)
                .field(FieldDef::alpha("Name").required().references(&["MaterialName"]))
                .field(
                    FieldDef::choice("Roughness", &["VeryRough", "Rough", "MediumRough", "MediumSmooth", "Smooth", "VerySmooth"])
                        .required(),
                )
                .field(FieldDef::real("Thickness").required().min_exclusive(0.0).max_inclusive(3.0).units("m"))
                .field(FieldDef::real("Conductivity").required().min_exclusive(0.0).units("W/m-K"))
                .field(FieldDef::real("Density").required().min_exclusive(0.0).units("kg/m3"))
                .field(FieldDef::real("Specific Heat").required().min_inclusive(100.0).units("J/kg-K")),
        );

        // Construction
        db.insert(
            ObjectDef::new("Construction")
                .memo("Describes construction layers")
                .min_fields(2)
                .extensible(1)
                .field(FieldDef::alpha("Name").required().references(&["ConstructionNames"]))
                .field(FieldDef::object_list("Outside Layer", "MaterialName").required()),
        );

        // GlobalGeometryRules
        db.insert(
            ObjectDef::new("GlobalGeometryRules")
                .unique()
                .memo("Global geometry conventions")
                .min_fields(3)
                .field(
                    FieldDef::choice(
                        "Starting Vertex Position",
                        &["UpperLeftCorner", "LowerLeftCorner", "UpperRightCorner", "LowerRightCorner"],
                    )
                    .required(),
                )
                .field(
                    FieldDef::choice("Vertex Entry Direction", &["Counterclockwise", "Clockwise"]).required(),
                )
                .field(
                    FieldDef::choice(
                        "Coordinate System",
                        &["Relative", "World", "Absolute"],
                    )
                    .required(),
                ),
        );

        // ScheduleTypeLimits
        db.insert(
            ObjectDef::new("ScheduleTypeLimits")
                .memo("Defines limits for schedule values")
                .field(FieldDef::alpha("Name").required().references(&["ScheduleTypeLimitsNames"]))
                .field(FieldDef::real("Lower Limit Value"))
                .field(FieldDef::real("Upper Limit Value"))
                .field(
                    FieldDef::choice("Numeric Type", &["Continuous", "Discrete"]).default_value("Continuous"),
                )
                .field(FieldDef::choice("Unit Type", &["Dimensionless", "Temperature", "DeltaTemperature", "Availability"]).default_value("Dimensionless")),
        );

        // Schedule:Compact
        db.insert(
            ObjectDef::new("Schedule:Compact")
                .memo("Compact schedule definition")
                .extensible(1)
                .min_fields(3)
                .field(FieldDef::alpha("Name").required().references(&["ScheduleNames"]))
                .field(FieldDef::object_list("Schedule Type Limits Name", "ScheduleTypeLimitsNames"))
                .field(FieldDef::alpha("Field 1").required()),
        );

        // Output:Variable
        db.insert(
            ObjectDef::new("Output:Variable")
                .memo("Requests output variable reporting")
                .min_fields(3)
                .field(FieldDef::alpha("Key Value").required().default_value("*"))
                .field(FieldDef::alpha("Variable Name").required())
                .field(
                    FieldDef::choice(
                        "Reporting Frequency",
                        &["Detailed", "Timestep", "Hourly", "Daily", "Monthly", "RunPeriod", "Annual"],
                    )
                    .default_value("Hourly"),
                ),
        );

        // Output:Meter
        db.insert(
            ObjectDef::new("Output:Meter")
                .memo("Requests meter output reporting")
                .min_fields(2)
                .field(FieldDef::alpha("Key Name").required())
                .field(
                    FieldDef::choice(
                        "Reporting Frequency",
                        &["Timestep", "Hourly", "Daily", "Monthly", "RunPeriod", "Annual"],
                    )
                    .default_value("Hourly"),
                ),
        );

        // SizingPeriod:DesignDay
        db.insert(
            ObjectDef::new("SizingPeriod:DesignDay")
                .memo("Design day specification for sizing")
                .min_fields(4)
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::integer("Month").required().min_inclusive(1.0).max_inclusive(12.0))
                .field(FieldDef::integer("Day of Month").required().min_inclusive(1.0).max_inclusive(31.0))
                .field(
                    FieldDef::choice("Day Type", &[
                        "SummerDesignDay", "WinterDesignDay",
                        "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
                    ]).required(),
                )
                .field(FieldDef::real("Maximum Dry-Bulb Temperature").required().units("C"))
                .field(FieldDef::real("Daily Dry-Bulb Temperature Range").default_value("0").min_inclusive(0.0).units("deltaC")),
        );

        // BuildingSurface:Detailed
        db.insert(
            ObjectDef::new("BuildingSurface:Detailed")
                .memo("Detailed building surface with vertices")
                .min_fields(10)
                .extensible(3)
                .field(FieldDef::alpha("Name").required().references(&["SurfaceNames"]))
                .field(
                    FieldDef::choice("Surface Type", &["Floor", "Wall", "Ceiling", "Roof"]).required(),
                )
                .field(FieldDef::object_list("Construction Name", "ConstructionNames").required())
                .field(FieldDef::object_list("Zone Name", "ZoneNames").required())
                .field(
                    FieldDef::choice(
                        "Outside Boundary Condition",
                        &["Outdoors", "Ground", "Zone", "Surface", "Adiabatic"],
                    )
                    .required(),
                )
                .field(FieldDef::alpha("Outside Boundary Condition Object"))
                .field(
                    FieldDef::choice("Sun Exposure", &["SunExposed", "NoSun"]).default_value("SunExposed"),
                )
                .field(
                    FieldDef::choice("Wind Exposure", &["WindExposed", "NoWind"]).default_value("WindExposed"),
                )
                .field(FieldDef::real("View Factor to Ground").autocalculatable().default_value("autocalculate"))
                .field(FieldDef::real("Number of Vertices").autocalculatable().default_value("autocalculate"))
                // extensible vertex group (x, y, z)
                .field(FieldDef::real("Vertex X-coordinate").required().units("m"))
                .field(FieldDef::real("Vertex Y-coordinate").required().units("m"))
                .field(FieldDef::real("Vertex Z-coordinate").required().units("m")),
        );

        db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_insensitive_lookup() {
        let db = SchemaDb::minimal();
        assert!(db.get("Building").is_some());
        assert!(db.get("building").is_some());
        assert!(db.get("BUILDING").is_some());
        assert!(db.get("bUiLdInG").is_some());
        assert!(db.get("nonexistent").is_none());
    }

    #[test]
    fn builder_ergonomics() {
        let field = FieldDef::real("Temperature")
            .required()
            .min_inclusive(-100.0)
            .max_exclusive(200.0)
            .units("C")
            .default_value("20.0");

        assert_eq!(field.name, "Temperature");
        assert!(field.required);
        assert_eq!(field.min_bound, Bound::Inclusive(-100.0));
        assert_eq!(field.max_bound, Bound::Exclusive(200.0));
        assert_eq!(field.units.as_deref(), Some("C"));
        assert_eq!(field.default_value.as_deref(), Some("20.0"));
        assert!(matches!(field.field_type, FieldType::Real));
    }

    #[test]
    fn minimal_schema_completeness() {
        let db = SchemaDb::minimal();
        assert!(db.len() >= 15);

        // Check all 15 core types exist
        let expected = [
            "Version", "Building", "Zone", "SimulationControl", "Timestep",
            "RunPeriod", "Material", "Construction", "GlobalGeometryRules",
            "ScheduleTypeLimits", "Schedule:Compact", "Output:Variable",
            "Output:Meter", "SizingPeriod:DesignDay", "BuildingSurface:Detailed",
        ];
        for name in &expected {
            assert!(db.get(name).is_some(), "Missing definition for {name}");
        }
    }

    #[test]
    fn bound_checking() {
        assert!(Bound::Inclusive(0.0).check_min(0.0));
        assert!(!Bound::Exclusive(0.0).check_min(0.0));
        assert!(Bound::Exclusive(0.0).check_min(0.001));
        assert!(Bound::None.check_min(f64::NEG_INFINITY));

        assert!(Bound::Inclusive(100.0).check_max(100.0));
        assert!(!Bound::Exclusive(100.0).check_max(100.0));
        assert!(Bound::Exclusive(100.0).check_max(99.999));
        assert!(Bound::None.check_max(f64::INFINITY));
    }

    #[test]
    fn unique_object_flag() {
        let db = SchemaDb::minimal();
        assert!(db.get("Building").unwrap().unique_object);
        assert!(db.get("Version").unwrap().unique_object);
        assert!(!db.get("Zone").unwrap().unique_object);
        assert!(!db.get("Material").unwrap().unique_object);
    }

    #[test]
    fn extensible_field_resolution() {
        let db = SchemaDb::minimal();
        let surface = db.get("BuildingSurface:Detailed").unwrap();
        assert_eq!(surface.extensible_count, 3);

        // Vertex fields are the last 3 in the definition (indices 10, 11, 12)
        // Accessing index 13 should wrap to the extensible group
        let f10 = surface.field_at(10).unwrap();
        assert_eq!(f10.name, "Vertex X-coordinate");
        let f13 = surface.field_at(13).unwrap();
        assert_eq!(f13.name, "Vertex X-coordinate");
        let f14 = surface.field_at(14).unwrap();
        assert_eq!(f14.name, "Vertex Y-coordinate");
    }

    #[test]
    fn field_def_choice_options() {
        let db = SchemaDb::minimal();
        let building = db.get("Building").unwrap();
        let terrain_field = &building.fields[2];
        assert_eq!(terrain_field.name, "Terrain");
        if let FieldType::Choice(opts) = &terrain_field.field_type {
            assert!(opts.contains(&"City".to_string()));
            assert!(opts.contains(&"Suburbs".to_string()));
        } else {
            panic!("Terrain field should be Choice type");
        }
    }

    #[test]
    fn field_def_references() {
        let db = SchemaDb::minimal();
        let zone = db.get("Zone").unwrap();
        let name_field = &zone.fields[0];
        assert!(name_field.reference_lists.contains(&"ZoneNames".to_string()));
    }

    #[test]
    fn autocalculatable_fields() {
        let db = SchemaDb::minimal();
        let zone = db.get("Zone").unwrap();
        let ceiling = &zone.fields[7]; // Ceiling Height
        assert!(ceiling.autocalculatable);
        assert_eq!(ceiling.default_value.as_deref(), Some("autocalculate"));
    }
}
