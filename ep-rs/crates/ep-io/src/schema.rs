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

    /// Build a comprehensive schema with ~55 object types covering
    /// site, geometry, materials, schedules, gains, sizing, HVAC, plant,
    /// setpoints, and output objects.
    pub fn standard() -> Self {
        let mut db = Self::minimal();

        // --- Site ---
        db.insert(
            ObjectDef::new("Site:Location")
                .unique()
                .memo("Site geographic location")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::real("Latitude").default_value("0").units("deg")
                    .min_inclusive(-90.0).max_inclusive(90.0))
                .field(FieldDef::real("Longitude").default_value("0").units("deg")
                    .min_inclusive(-180.0).max_inclusive(180.0))
                .field(FieldDef::real("Time Zone").default_value("0").units("hr")
                    .min_inclusive(-12.0).max_inclusive(14.0))
                .field(FieldDef::real("Elevation").default_value("0").units("m")
                    .min_inclusive(-1000.0).max_inclusive(9999.0)),
        );

        db.insert(
            ObjectDef::new("Site:GroundTemperature:BuildingSurface")
                .unique()
                .memo("Monthly ground temperatures for building surface heat transfer")
                .field(FieldDef::real("January Ground Temperature").default_value("18").units("C"))
                .field(FieldDef::real("February Ground Temperature").default_value("18").units("C"))
                .field(FieldDef::real("March Ground Temperature").default_value("18").units("C"))
                .field(FieldDef::real("April Ground Temperature").default_value("18").units("C"))
                .field(FieldDef::real("May Ground Temperature").default_value("18").units("C"))
                .field(FieldDef::real("June Ground Temperature").default_value("18").units("C"))
                .field(FieldDef::real("July Ground Temperature").default_value("18").units("C"))
                .field(FieldDef::real("August Ground Temperature").default_value("18").units("C"))
                .field(FieldDef::real("September Ground Temperature").default_value("18").units("C"))
                .field(FieldDef::real("October Ground Temperature").default_value("18").units("C"))
                .field(FieldDef::real("November Ground Temperature").default_value("18").units("C"))
                .field(FieldDef::real("December Ground Temperature").default_value("18").units("C")),
        );

        // --- Geometry ---
        db.insert(
            ObjectDef::new("FenestrationSurface:Detailed")
                .memo("Window or door subsurface with vertices")
                .min_fields(10)
                .extensible(3)
                .field(FieldDef::alpha("Name").required().references(&["SubSurfaceNames"]))
                .field(FieldDef::choice("Surface Type", &["Window", "Door", "GlassDoor", "TubularDaylightDome", "TubularDaylightDiffuser"]).required())
                .field(FieldDef::object_list("Construction Name", "ConstructionNames").required())
                .field(FieldDef::object_list("Building Surface Name", "SurfaceNames").required())
                .field(FieldDef::alpha("Outside Boundary Condition Object"))
                .field(FieldDef::real("View Factor to Ground").autocalculatable().default_value("autocalculate"))
                .field(FieldDef::alpha("Frame and Divider Name"))
                .field(FieldDef::real("Multiplier").default_value("1").min_inclusive(1.0))
                .field(FieldDef::real("Number of Vertices").autocalculatable().default_value("autocalculate"))
                .field(FieldDef::real("Vertex X-coordinate").required().units("m"))
                .field(FieldDef::real("Vertex Y-coordinate").required().units("m"))
                .field(FieldDef::real("Vertex Z-coordinate").required().units("m")),
        );

        db.insert(
            ObjectDef::new("Shading:Site:Detailed")
                .memo("Site shading surface with vertices")
                .extensible(3)
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::alpha("Transmittance Schedule Name"))
                .field(FieldDef::real("Number of Vertices").autocalculatable().default_value("autocalculate"))
                .field(FieldDef::real("Vertex X-coordinate").required().units("m"))
                .field(FieldDef::real("Vertex Y-coordinate").required().units("m"))
                .field(FieldDef::real("Vertex Z-coordinate").required().units("m")),
        );

        db.insert(
            ObjectDef::new("Shading:Building:Detailed")
                .memo("Building shading surface with vertices")
                .extensible(3)
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::alpha("Transmittance Schedule Name"))
                .field(FieldDef::real("Number of Vertices").autocalculatable().default_value("autocalculate"))
                .field(FieldDef::real("Vertex X-coordinate").required().units("m"))
                .field(FieldDef::real("Vertex Y-coordinate").required().units("m"))
                .field(FieldDef::real("Vertex Z-coordinate").required().units("m")),
        );

        // --- Materials ---
        db.insert(
            ObjectDef::new("Material:NoMass")
                .memo("No-mass material layer (R-value only)")
                .min_fields(3)
                .field(FieldDef::alpha("Name").required().references(&["MaterialName"]))
                .field(FieldDef::choice("Roughness", &["VeryRough", "Rough", "MediumRough", "MediumSmooth", "Smooth", "VerySmooth"]).required())
                .field(FieldDef::real("Thermal Resistance").required().min_exclusive(0.0).units("m2-K/W")),
        );

        db.insert(
            ObjectDef::new("Material:AirGap")
                .memo("Air gap material layer")
                .field(FieldDef::alpha("Name").required().references(&["MaterialName"]))
                .field(FieldDef::real("Thermal Resistance").required().min_exclusive(0.0).units("m2-K/W")),
        );

        db.insert(
            ObjectDef::new("WindowMaterial:Glazing")
                .memo("Glass layer for windows")
                .min_fields(9)
                .field(FieldDef::alpha("Name").required().references(&["MaterialName"]))
                .field(FieldDef::choice("Optical Data Type", &["SpectralAverage", "Spectral"]).default_value("SpectralAverage"))
                .field(FieldDef::alpha("Window Glass Spectral Data Set Name"))
                .field(FieldDef::real("Thickness").required().min_exclusive(0.0).units("m"))
                .field(FieldDef::real("Solar Transmittance at Normal Incidence").min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Front Side Solar Reflectance at Normal Incidence").min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Back Side Solar Reflectance at Normal Incidence").min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Visible Transmittance at Normal Incidence").min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Front Side Visible Reflectance at Normal Incidence").min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Back Side Visible Reflectance at Normal Incidence").min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Infrared Transmittance at Normal Incidence").default_value("0").min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Front Side Infrared Hemispherical Emissivity").default_value("0.84").min_exclusive(0.0).max_exclusive(1.0))
                .field(FieldDef::real("Back Side Infrared Hemispherical Emissivity").default_value("0.84").min_exclusive(0.0).max_exclusive(1.0))
                .field(FieldDef::real("Conductivity").default_value("0.9").min_exclusive(0.0).units("W/m-K")),
        );

        db.insert(
            ObjectDef::new("WindowMaterial:Gas")
                .memo("Gas fill for window gaps")
                .field(FieldDef::alpha("Name").required().references(&["MaterialName"]))
                .field(FieldDef::choice("Gas Type", &["Air", "Argon", "Krypton", "Xenon", "Custom"]).default_value("Air"))
                .field(FieldDef::real("Thickness").required().min_exclusive(0.0).units("m")),
        );

        // --- Schedules ---
        db.insert(
            ObjectDef::new("Schedule:Constant")
                .memo("Constant value schedule")
                .field(FieldDef::alpha("Name").required().references(&["ScheduleNames"]))
                .field(FieldDef::object_list("Schedule Type Limits Name", "ScheduleTypeLimitsNames"))
                .field(FieldDef::real("Hourly Value").default_value("0")),
        );

        // --- Internal Gains ---
        db.insert(
            ObjectDef::new("People")
                .memo("People internal gains")
                .min_fields(5)
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::object_list("Zone Name", "ZoneNames").required())
                .field(FieldDef::object_list("Number of People Schedule Name", "ScheduleNames").required())
                .field(FieldDef::choice("Number of People Calculation Method", &["People", "People/Area", "Area/Person"]).default_value("People"))
                .field(FieldDef::real("Number of People"))
                .field(FieldDef::real("People per Floor Area").units("person/m2"))
                .field(FieldDef::real("Floor Area per Person").units("m2/person"))
                .field(FieldDef::real("Fraction Radiant").default_value("0.3").min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::alpha("Activity Level Schedule Name")),
        );

        db.insert(
            ObjectDef::new("Lights")
                .memo("Lighting internal gains")
                .min_fields(4)
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::object_list("Zone Name", "ZoneNames").required())
                .field(FieldDef::object_list("Schedule Name", "ScheduleNames").required())
                .field(FieldDef::choice("Design Level Calculation Method", &["LightingLevel", "Watts/Area", "Watts/Person"]).default_value("LightingLevel"))
                .field(FieldDef::real("Lighting Level").units("W"))
                .field(FieldDef::real("Watts per Floor Area").units("W/m2"))
                .field(FieldDef::real("Watts per Person").units("W/person"))
                .field(FieldDef::real("Fraction Radiant").default_value("0.72").min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Fraction Visible").default_value("0.18").min_inclusive(0.0).max_inclusive(1.0)),
        );

        db.insert(
            ObjectDef::new("ElectricEquipment")
                .memo("Electric equipment internal gains")
                .min_fields(4)
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::object_list("Zone Name", "ZoneNames").required())
                .field(FieldDef::object_list("Schedule Name", "ScheduleNames").required())
                .field(FieldDef::choice("Design Level Calculation Method", &["EquipmentLevel", "Watts/Area", "Watts/Person"]).default_value("EquipmentLevel"))
                .field(FieldDef::real("Design Level").units("W"))
                .field(FieldDef::real("Watts per Floor Area").units("W/m2"))
                .field(FieldDef::real("Watts per Person").units("W/person"))
                .field(FieldDef::real("Fraction Latent").default_value("0").min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Fraction Radiant").default_value("0.3").min_inclusive(0.0).max_inclusive(1.0)),
        );

        db.insert(
            ObjectDef::new("ZoneInfiltration:DesignFlowRate")
                .memo("Zone air infiltration")
                .min_fields(4)
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::object_list("Zone Name", "ZoneNames").required())
                .field(FieldDef::object_list("Schedule Name", "ScheduleNames").required())
                .field(FieldDef::choice("Design Flow Rate Calculation Method",
                    &["Flow/Zone", "Flow/Area", "Flow/ExteriorArea", "Flow/ExteriorWallArea", "AirChanges/Hour"])
                    .default_value("Flow/Zone"))
                .field(FieldDef::real("Design Flow Rate").units("m3/s"))
                .field(FieldDef::real("Flow Rate per Floor Area").units("m3/s-m2"))
                .field(FieldDef::real("Flow Rate per Exterior Surface Area").units("m3/s-m2"))
                .field(FieldDef::real("Air Changes per Hour").units("1/hr"))
                .field(FieldDef::real("Constant Term Coefficient").default_value("1"))
                .field(FieldDef::real("Temperature Term Coefficient").default_value("0"))
                .field(FieldDef::real("Velocity Term Coefficient").default_value("0"))
                .field(FieldDef::real("Velocity Squared Term Coefficient").default_value("0")),
        );

        // --- Sizing ---
        db.insert(
            ObjectDef::new("Sizing:Zone")
                .memo("Zone sizing parameters")
                .field(FieldDef::object_list("Zone Name", "ZoneNames").required())
                .field(FieldDef::choice("Zone Cooling Design Supply Air Temperature Input Method",
                    &["SupplyAirTemperature", "TemperatureDifference"]).default_value("SupplyAirTemperature"))
                .field(FieldDef::real("Zone Cooling Design Supply Air Temperature").default_value("14").units("C"))
                .field(FieldDef::real("Zone Heating Design Supply Air Temperature").default_value("40").units("C"))
                .field(FieldDef::real("Zone Cooling Design Supply Air Humidity Ratio").default_value("0.008").units("kgWater/kgDryAir"))
                .field(FieldDef::real("Zone Heating Design Supply Air Humidity Ratio").default_value("0.008").units("kgWater/kgDryAir"))
                .field(FieldDef::real("Zone Heating Sizing Factor").default_value("1.25").min_inclusive(0.0))
                .field(FieldDef::real("Zone Cooling Sizing Factor").default_value("1.15").min_inclusive(0.0)),
        );

        db.insert(
            ObjectDef::new("Sizing:System")
                .memo("System sizing parameters")
                .field(FieldDef::object_list("AirLoop Name", "AirLoopNames").required())
                .field(FieldDef::choice("Type of Load to Size On", &["Sensible", "Total", "VentilationRequirement"]).default_value("Sensible"))
                .field(FieldDef::real("Design Outdoor Air Flow Rate").autosizable().default_value("autosize").units("m3/s"))
                .field(FieldDef::real("Central Heating Design Supply Air Temperature").default_value("16.7").units("C"))
                .field(FieldDef::real("Central Cooling Design Supply Air Temperature").default_value("12.8").units("C"))
                .field(FieldDef::real("Sizing Option").default_value("NonCoincident")),
        );

        db.insert(
            ObjectDef::new("Sizing:Plant")
                .memo("Plant loop sizing parameters")
                .field(FieldDef::alpha("Plant or Condenser Loop Name").required())
                .field(FieldDef::choice("Loop Type", &["Heating", "Cooling", "Condenser", "Steam"]).required())
                .field(FieldDef::real("Design Loop Exit Temperature").required().units("C"))
                .field(FieldDef::real("Loop Design Temperature Difference").required().units("deltaC").min_exclusive(0.0)),
        );

        // --- HVAC ---
        db.insert(
            ObjectDef::new("AirLoopHVAC")
                .memo("Air loop definition")
                .field(FieldDef::alpha("Name").required().references(&["AirLoopNames"]))
                .field(FieldDef::alpha("Controller List Name"))
                .field(FieldDef::alpha("Availability Manager List Name"))
                .field(FieldDef::real("Design Supply Air Flow Rate").autosizable().default_value("autosize").units("m3/s"))
                .field(FieldDef::alpha("Branch List Name"))
                .field(FieldDef::alpha("Connector List Name"))
                .field(FieldDef::node("Supply Side Inlet Node Name"))
                .field(FieldDef::node("Demand Side Outlet Node Name"))
                .field(FieldDef::node("Demand Side Inlet Node Names"))
                .field(FieldDef::node("Supply Side Outlet Node Names")),
        );

        db.insert(
            ObjectDef::new("Fan:ConstantVolume")
                .memo("Constant volume fan")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::object_list("Availability Schedule Name", "ScheduleNames"))
                .field(FieldDef::real("Fan Total Efficiency").default_value("0.7").min_exclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Pressure Rise").required().units("Pa").min_inclusive(0.0))
                .field(FieldDef::real("Maximum Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::real("Motor Efficiency").default_value("0.9").min_exclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Motor In Airstream Fraction").default_value("1.0").min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::node("Air Inlet Node Name"))
                .field(FieldDef::node("Air Outlet Node Name")),
        );

        db.insert(
            ObjectDef::new("Fan:VariableVolume")
                .memo("Variable volume fan")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::object_list("Availability Schedule Name", "ScheduleNames"))
                .field(FieldDef::real("Fan Total Efficiency").default_value("0.7").min_exclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Pressure Rise").required().units("Pa").min_inclusive(0.0))
                .field(FieldDef::real("Maximum Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::choice("Fan Power Minimum Flow Rate Input Method", &["Fraction", "FixedFlowRate"]).default_value("Fraction"))
                .field(FieldDef::real("Fan Power Minimum Flow Fraction").default_value("0.25").min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Fan Power Minimum Air Flow Rate").units("m3/s"))
                .field(FieldDef::real("Motor Efficiency").default_value("0.9").min_exclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Motor In Airstream Fraction").default_value("1.0").min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Fan Power Coefficient 1"))
                .field(FieldDef::real("Fan Power Coefficient 2"))
                .field(FieldDef::real("Fan Power Coefficient 3"))
                .field(FieldDef::real("Fan Power Coefficient 4"))
                .field(FieldDef::real("Fan Power Coefficient 5"))
                .field(FieldDef::node("Air Inlet Node Name"))
                .field(FieldDef::node("Air Outlet Node Name")),
        );

        db.insert(
            ObjectDef::new("Coil:Cooling:DX:SingleSpeed")
                .memo("Single speed DX cooling coil")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::object_list("Availability Schedule Name", "ScheduleNames"))
                .field(FieldDef::real("Gross Rated Total Cooling Capacity").autosizable().units("W"))
                .field(FieldDef::real("Gross Rated Sensible Heat Ratio").autosizable().min_inclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Gross Rated Cooling COP").default_value("3.0").min_exclusive(0.0).units("W/W"))
                .field(FieldDef::real("Rated Air Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::node("Air Inlet Node Name"))
                .field(FieldDef::node("Air Outlet Node Name")),
        );

        db.insert(
            ObjectDef::new("Coil:Heating:Fuel")
                .memo("Fuel-fired heating coil")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::object_list("Availability Schedule Name", "ScheduleNames"))
                .field(FieldDef::choice("Fuel Type", &["NaturalGas", "Propane", "Diesel", "FuelOilNo1", "FuelOilNo2"]).default_value("NaturalGas"))
                .field(FieldDef::real("Burner Efficiency").default_value("0.8").min_exclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Nominal Capacity").autosizable().units("W"))
                .field(FieldDef::node("Air Inlet Node Name"))
                .field(FieldDef::node("Air Outlet Node Name")),
        );

        db.insert(
            ObjectDef::new("Coil:Cooling:Water")
                .memo("Chilled water cooling coil")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::object_list("Availability Schedule Name", "ScheduleNames"))
                .field(FieldDef::real("Design Water Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::real("Design Air Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::real("Design Inlet Water Temperature").autosizable().units("C"))
                .field(FieldDef::real("Design Inlet Air Temperature").autosizable().units("C"))
                .field(FieldDef::real("Design Outlet Air Temperature").autosizable().units("C"))
                .field(FieldDef::node("Water Inlet Node Name"))
                .field(FieldDef::node("Water Outlet Node Name"))
                .field(FieldDef::node("Air Inlet Node Name"))
                .field(FieldDef::node("Air Outlet Node Name")),
        );

        db.insert(
            ObjectDef::new("Coil:Heating:Water")
                .memo("Hot water heating coil")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::object_list("Availability Schedule Name", "ScheduleNames"))
                .field(FieldDef::real("U-Factor Times Area Value").autosizable().units("W/K"))
                .field(FieldDef::real("Maximum Water Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::node("Water Inlet Node Name"))
                .field(FieldDef::node("Water Outlet Node Name"))
                .field(FieldDef::node("Air Inlet Node Name"))
                .field(FieldDef::node("Air Outlet Node Name")),
        );

        db.insert(
            ObjectDef::new("AirTerminal:SingleDuct:ConstantVolume:NoReheat")
                .memo("Constant volume single-duct terminal with no reheat")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::object_list("Availability Schedule Name", "ScheduleNames"))
                .field(FieldDef::node("Air Inlet Node Name").required())
                .field(FieldDef::node("Air Outlet Node Name").required())
                .field(FieldDef::real("Maximum Air Flow Rate").autosizable().units("m3/s")),
        );

        db.insert(
            ObjectDef::new("AirLoopHVAC:OutdoorAirSystem")
                .memo("Outdoor air system for air loop")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::alpha("Controller List Name"))
                .field(FieldDef::alpha("Outdoor Air Equipment List Name")),
        );

        db.insert(
            ObjectDef::new("Controller:WaterCoil")
                .memo("Water coil controller")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::choice("Control Variable", &["Temperature", "HumidityRatio", "TemperatureAndHumidityRatio"]).default_value("Temperature"))
                .field(FieldDef::choice("Action", &["Normal", "Reverse"]).default_value("Normal"))
                .field(FieldDef::node("Actuator Node Name"))
                .field(FieldDef::node("Sensor Node Name"))
                .field(FieldDef::real("Controller Convergence Tolerance").default_value("0.001"))
                .field(FieldDef::real("Maximum Actuated Flow").autosizable().units("m3/s"))
                .field(FieldDef::real("Minimum Actuated Flow").default_value("0").units("m3/s")),
        );

        // --- Plant ---
        db.insert(
            ObjectDef::new("PlantLoop")
                .memo("Plant loop definition")
                .field(FieldDef::alpha("Name").required().references(&["PlantLoopNames"]))
                .field(FieldDef::choice("Fluid Type", &["Water", "Steam"]).default_value("Water"))
                .field(FieldDef::real("Maximum Loop Temperature").default_value("100").units("C"))
                .field(FieldDef::real("Minimum Loop Temperature").default_value("0").units("C"))
                .field(FieldDef::real("Maximum Loop Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::real("Minimum Loop Flow Rate").default_value("0").units("m3/s"))
                .field(FieldDef::real("Plant Loop Volume").autocalculatable().default_value("autocalculate").units("m3"))
                .field(FieldDef::node("Plant Side Inlet Node Name"))
                .field(FieldDef::node("Plant Side Outlet Node Name"))
                .field(FieldDef::node("Demand Side Inlet Node Name"))
                .field(FieldDef::node("Demand Side Outlet Node Name")),
        );

        db.insert(
            ObjectDef::new("Boiler:HotWater")
                .memo("Hot water boiler")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::choice("Fuel Type", &["NaturalGas", "Propane", "Electricity", "FuelOilNo1", "FuelOilNo2"]).default_value("NaturalGas"))
                .field(FieldDef::real("Nominal Capacity").autosizable().units("W"))
                .field(FieldDef::real("Nominal Thermal Efficiency").required().min_exclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Design Water Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::real("Minimum Part Load Ratio").default_value("0").min_inclusive(0.0))
                .field(FieldDef::real("Maximum Part Load Ratio").default_value("1.1").min_inclusive(0.0))
                .field(FieldDef::real("Optimum Part Load Ratio").default_value("1.0").min_inclusive(0.0))
                .field(FieldDef::node("Boiler Water Inlet Node Name"))
                .field(FieldDef::node("Boiler Water Outlet Node Name")),
        );

        db.insert(
            ObjectDef::new("Chiller:Electric:EIR")
                .memo("Electric EIR chiller")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::real("Reference Capacity").autosizable().units("W"))
                .field(FieldDef::real("Reference COP").required().min_exclusive(0.0).units("W/W"))
                .field(FieldDef::real("Reference Leaving Chilled Water Temperature").default_value("6.67").units("C"))
                .field(FieldDef::real("Reference Entering Condenser Fluid Temperature").default_value("29.44").units("C"))
                .field(FieldDef::real("Reference Chilled Water Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::real("Reference Condenser Fluid Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::node("Chilled Water Inlet Node Name"))
                .field(FieldDef::node("Chilled Water Outlet Node Name"))
                .field(FieldDef::node("Condenser Inlet Node Name"))
                .field(FieldDef::node("Condenser Outlet Node Name")),
        );

        db.insert(
            ObjectDef::new("CoolingTower:SingleSpeed")
                .memo("Single-speed cooling tower")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::node("Water Inlet Node Name"))
                .field(FieldDef::node("Water Outlet Node Name"))
                .field(FieldDef::real("Design Water Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::real("Design Air Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::real("Design Fan Power").autosizable().units("W"))
                .field(FieldDef::real("Design U-Factor Times Area Value").autosizable().units("W/K")),
        );

        db.insert(
            ObjectDef::new("Pump:VariableSpeed")
                .memo("Variable speed pump")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::node("Inlet Node Name"))
                .field(FieldDef::node("Outlet Node Name"))
                .field(FieldDef::real("Design Maximum Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::real("Design Pump Head").default_value("179352").units("Pa"))
                .field(FieldDef::real("Design Power Consumption").autosizable().units("W"))
                .field(FieldDef::real("Motor Efficiency").default_value("0.9").min_exclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Fraction of Motor Inefficiencies to Fluid Stream").default_value("0").min_inclusive(0.0).max_inclusive(1.0)),
        );

        db.insert(
            ObjectDef::new("Pump:ConstantSpeed")
                .memo("Constant speed pump")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::node("Inlet Node Name"))
                .field(FieldDef::node("Outlet Node Name"))
                .field(FieldDef::real("Design Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::real("Design Pump Head").default_value("179352").units("Pa"))
                .field(FieldDef::real("Design Power Consumption").autosizable().units("W"))
                .field(FieldDef::real("Motor Efficiency").default_value("0.9").min_exclusive(0.0).max_inclusive(1.0))
                .field(FieldDef::real("Fraction of Motor Inefficiencies to Fluid Stream").default_value("0").min_inclusive(0.0).max_inclusive(1.0)),
        );

        db.insert(
            ObjectDef::new("Pipe:Adiabatic")
                .memo("Adiabatic pipe (no heat transfer)")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::node("Inlet Node Name"))
                .field(FieldDef::node("Outlet Node Name")),
        );

        db.insert(
            ObjectDef::new("CondenserLoop")
                .memo("Condenser loop definition")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::choice("Fluid Type", &["Water"]).default_value("Water"))
                .field(FieldDef::real("Maximum Loop Temperature").default_value("80").units("C"))
                .field(FieldDef::real("Minimum Loop Temperature").default_value("5").units("C"))
                .field(FieldDef::real("Maximum Loop Flow Rate").autosizable().units("m3/s"))
                .field(FieldDef::real("Minimum Loop Flow Rate").default_value("0").units("m3/s"))
                .field(FieldDef::node("Condenser Side Inlet Node Name"))
                .field(FieldDef::node("Condenser Side Outlet Node Name"))
                .field(FieldDef::node("Demand Side Inlet Node Name"))
                .field(FieldDef::node("Demand Side Outlet Node Name")),
        );

        // --- Setpoints ---
        db.insert(
            ObjectDef::new("SetpointManager:Scheduled")
                .memo("Scheduled setpoint manager")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::choice("Control Variable", &["Temperature", "HumidityRatio", "MassFlowRate"]).default_value("Temperature"))
                .field(FieldDef::object_list("Schedule Name", "ScheduleNames").required())
                .field(FieldDef::node("Setpoint Node or NodeList Name").required()),
        );

        db.insert(
            ObjectDef::new("SetpointManager:OutdoorAirReset")
                .memo("Outdoor air reset setpoint manager")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::choice("Control Variable", &["Temperature"]).default_value("Temperature"))
                .field(FieldDef::real("Setpoint at Outdoor Low Temperature").required().units("C"))
                .field(FieldDef::real("Outdoor Low Temperature").required().units("C"))
                .field(FieldDef::real("Setpoint at Outdoor High Temperature").required().units("C"))
                .field(FieldDef::real("Outdoor High Temperature").required().units("C"))
                .field(FieldDef::node("Setpoint Node or NodeList Name").required()),
        );

        db.insert(
            ObjectDef::new("SetpointManager:MixedAir")
                .memo("Mixed air setpoint manager")
                .field(FieldDef::alpha("Name").required())
                .field(FieldDef::choice("Control Variable", &["Temperature"]).default_value("Temperature"))
                .field(FieldDef::node("Reference Setpoint Node Name").required())
                .field(FieldDef::node("Fan Inlet Node Name").required())
                .field(FieldDef::node("Fan Outlet Node Name").required())
                .field(FieldDef::node("Setpoint Node or NodeList Name").required()),
        );

        // --- Output ---
        db.insert(
            ObjectDef::new("OutputControl:Table:Style")
                .unique()
                .memo("Controls tabular output format")
                .field(FieldDef::choice("Column Separator", &["Comma", "Tab", "Fixed", "HTML", "SQLite", "All"]).default_value("Comma")),
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

    #[test]
    fn standard_schema_completeness() {
        let db = SchemaDb::standard();
        // Should have all minimal objects plus new ones
        assert!(db.len() >= 54, "expected >= 54, got {}", db.len());

        let expected_new = [
            "Site:Location", "Site:GroundTemperature:BuildingSurface",
            "FenestrationSurface:Detailed", "Shading:Site:Detailed", "Shading:Building:Detailed",
            "Material:NoMass", "Material:AirGap", "WindowMaterial:Glazing", "WindowMaterial:Gas",
            "Schedule:Constant",
            "People", "Lights", "ElectricEquipment", "ZoneInfiltration:DesignFlowRate",
            "Sizing:Zone", "Sizing:System", "Sizing:Plant",
            "AirLoopHVAC", "Fan:ConstantVolume", "Fan:VariableVolume",
            "Coil:Cooling:DX:SingleSpeed", "Coil:Heating:Fuel",
            "Coil:Cooling:Water", "Coil:Heating:Water",
            "AirTerminal:SingleDuct:ConstantVolume:NoReheat",
            "AirLoopHVAC:OutdoorAirSystem", "Controller:WaterCoil",
            "PlantLoop", "Boiler:HotWater", "Chiller:Electric:EIR",
            "CoolingTower:SingleSpeed", "Pump:VariableSpeed", "Pump:ConstantSpeed",
            "Pipe:Adiabatic", "CondenserLoop",
            "SetpointManager:Scheduled", "SetpointManager:OutdoorAirReset", "SetpointManager:MixedAir",
            "OutputControl:Table:Style",
        ];
        for name in &expected_new {
            assert!(db.get(name).is_some(), "Missing definition for {name}");
        }
    }

    #[test]
    fn standard_schema_site_location_fields() {
        let db = SchemaDb::standard();
        let loc = db.get("Site:Location").unwrap();
        assert!(loc.unique_object);
        assert_eq!(loc.fields.len(), 5);
        assert_eq!(loc.fields[1].name, "Latitude");
        assert_eq!(loc.fields[1].min_bound, Bound::Inclusive(-90.0));
        assert_eq!(loc.fields[1].max_bound, Bound::Inclusive(90.0));
    }

    #[test]
    fn standard_schema_people_fields() {
        let db = SchemaDb::standard();
        let people = db.get("People").unwrap();
        assert_eq!(people.fields[0].name, "Name");
        assert!(people.fields[0].required);
        // Check zone name is object_list reference
        assert!(matches!(people.fields[1].field_type, FieldType::ObjectList(_)));
    }

    #[test]
    fn standard_schema_sizing_zone() {
        let db = SchemaDb::standard();
        let sz = db.get("Sizing:Zone").unwrap();
        assert_eq!(sz.fields[2].name, "Zone Cooling Design Supply Air Temperature");
        assert_eq!(sz.fields[2].default_value.as_deref(), Some("14"));
    }

    #[test]
    fn standard_schema_air_loop() {
        let db = SchemaDb::standard();
        let al = db.get("AirLoopHVAC").unwrap();
        assert_eq!(al.fields[0].name, "Name");
        assert!(al.fields[0].reference_lists.contains(&"AirLoopNames".to_string()));
        // Design flow rate should be autosizable
        assert!(al.fields[3].autosizable);
    }

    #[test]
    fn standard_schema_plant_loop() {
        let db = SchemaDb::standard();
        let pl = db.get("PlantLoop").unwrap();
        assert_eq!(pl.fields[1].name, "Fluid Type");
        if let FieldType::Choice(opts) = &pl.fields[1].field_type {
            assert!(opts.contains(&"Water".to_string()));
        } else {
            panic!("Fluid Type should be Choice");
        }
    }

    #[test]
    fn standard_schema_fenestration_extensible() {
        let db = SchemaDb::standard();
        let fen = db.get("FenestrationSurface:Detailed").unwrap();
        assert_eq!(fen.extensible_count, 3);
        // Extensible fields: vertex x, y, z
        let last3_start = fen.fields.len() - 3;
        assert_eq!(fen.fields[last3_start].name, "Vertex X-coordinate");
        assert_eq!(fen.fields[last3_start + 1].name, "Vertex Y-coordinate");
        assert_eq!(fen.fields[last3_start + 2].name, "Vertex Z-coordinate");
    }

    #[test]
    fn standard_schema_setpoint_managers() {
        let db = SchemaDb::standard();
        let sched = db.get("SetpointManager:Scheduled").unwrap();
        assert!(sched.fields[2].required); // Schedule Name required
        let oar = db.get("SetpointManager:OutdoorAirReset").unwrap();
        assert_eq!(oar.fields.len(), 7);
        let mixed = db.get("SetpointManager:MixedAir").unwrap();
        assert_eq!(mixed.fields.len(), 6);
    }
}
