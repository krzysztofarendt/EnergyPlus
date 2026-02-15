//! Input translator: converts parsed IDF/JSON model into domain objects.
//!
//! Takes an `InputModel` (from ep-io) and a `SchemaDb`, and produces
//! a `BuildingModel` with translated zones, surfaces, constructions,
//! simulation control settings, and output requests.

use ep_io::schema::SchemaDb;
use ep_io::InputModel;
use ep_output::{OutputRequest, variables::ReportFreq};

use crate::SimulationConfig;

/// Translated building model ready for simulation.
#[derive(Debug, Clone)]
pub struct BuildingModel {
    /// Building name.
    pub name: String,
    /// North axis rotation (degrees).
    pub north_axis: f64,
    /// Terrain type.
    pub terrain: String,
    /// Zone definitions.
    pub zones: Vec<ZoneDef>,
    /// Surface definitions.
    pub surfaces: Vec<SurfaceDef>,
    /// Construction definitions.
    pub constructions: Vec<ConstructionDef>,
    /// Simulation control configuration.
    pub config: SimulationConfig,
    /// Output variable requests.
    pub output_requests: Vec<OutputRequest>,
    /// Site location.
    pub location: SiteLocation,
}

/// Site geographic location.
#[derive(Debug, Clone)]
pub struct SiteLocation {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub time_zone: f64,
    pub elevation: f64,
}

impl Default for SiteLocation {
    fn default() -> Self {
        Self {
            name: String::new(),
            latitude: 0.0,
            longitude: 0.0,
            time_zone: 0.0,
            elevation: 0.0,
        }
    }
}

/// Zone definition.
#[derive(Debug, Clone)]
pub struct ZoneDef {
    pub name: String,
    pub volume: f64,        // m³
    pub floor_area: f64,    // m²
    pub multiplier: u32,
    pub ceiling_height: f64, // m
}

/// Surface definition.
#[derive(Debug, Clone)]
pub struct SurfaceDef {
    pub name: String,
    pub surface_type: String,
    pub construction_name: String,
    pub zone_name: String,
    pub boundary_condition: String,
    pub area: f64,      // m²
    pub azimuth: f64,   // degrees
    pub tilt: f64,      // degrees
    pub vertices: Vec<[f64; 3]>,
}

/// Construction definition.
#[derive(Debug, Clone)]
pub struct ConstructionDef {
    pub name: String,
    pub layers: Vec<String>,
}

/// Translate an InputModel into a BuildingModel.
pub fn translate_input(model: &InputModel, _schema: &SchemaDb) -> Result<BuildingModel, String> {
    let mut building = BuildingModel {
        name: String::new(),
        north_axis: 0.0,
        terrain: "Suburbs".to_string(),
        zones: Vec::new(),
        surfaces: Vec::new(),
        constructions: Vec::new(),
        config: SimulationConfig::default(),
        output_requests: Vec::new(),
        location: SiteLocation::default(),
    };

    // Translate Building
    translate_building(model, &mut building);

    // Translate SimulationControl
    translate_sim_control(model, &mut building.config);

    // Translate Timestep
    translate_timestep(model, &mut building.config);

    // Translate Site:Location
    translate_location(model, &mut building.location);

    // Translate Zones
    building.zones = translate_zones(model);

    // Translate Surfaces
    building.surfaces = translate_surfaces(model);

    // Translate Constructions
    building.constructions = translate_constructions(model);

    // Translate Output requests
    building.output_requests = translate_output_requests(model);

    Ok(building)
}

fn translate_building(model: &InputModel, building: &mut BuildingModel) {
    let objs = model.get_objects("Building");
    if let Some(b) = objs.first() {
        building.name = get_str(b, "Name").unwrap_or_default();
        building.north_axis = get_f64(b, "North Axis").unwrap_or(0.0);
        building.terrain = get_str(b, "Terrain").unwrap_or_else(|| "Suburbs".to_string());
    }
}

fn translate_sim_control(model: &InputModel, config: &mut SimulationConfig) {
    let objs = model.get_objects("SimulationControl");
    if let Some(sc) = objs.first() {
        config.do_zone_sizing = is_yes(get_str(sc, "Do Zone Sizing Calculation"));
        config.do_system_sizing = is_yes(get_str(sc, "Do System Sizing Calculation"));
        config.do_plant_sizing = is_yes(get_str(sc, "Do Plant Sizing Calculation"));
        config.run_design_days = is_yes_or_default(get_str(sc, "Run Simulation for Sizing Periods"), true);
        config.run_weather_periods = is_yes_or_default(
            get_str(sc, "Run Simulation for Weather File Run Periods"), true,
        );
    }
}

fn translate_timestep(model: &InputModel, config: &mut SimulationConfig) {
    let objs = model.get_objects("Timestep");
    if let Some(ts) = objs.first() {
        if let Some(n) = get_f64(ts, "Number of Timesteps per Hour") {
            config.timesteps_per_hour = (n as u8).max(1);
        }
    }
}

fn translate_location(model: &InputModel, location: &mut SiteLocation) {
    let objs = model.get_objects("Site:Location");
    if let Some(loc) = objs.first() {
        location.name = get_str(loc, "Name").unwrap_or_default();
        location.latitude = get_f64(loc, "Latitude").unwrap_or(0.0);
        location.longitude = get_f64(loc, "Longitude").unwrap_or(0.0);
        location.time_zone = get_f64(loc, "Time Zone").unwrap_or(0.0);
        location.elevation = get_f64(loc, "Elevation").unwrap_or(0.0);
    }
}

fn translate_zones(model: &InputModel) -> Vec<ZoneDef> {
    model.get_objects("Zone").iter().map(|z| {
        let vol_str = get_str(z, "Volume").unwrap_or_default();
        let vol = if vol_str.eq_ignore_ascii_case("autocalculate") {
            0.0
        } else {
            vol_str.parse::<f64>().unwrap_or(0.0)
        };

        let ch_str = get_str(z, "Ceiling Height").unwrap_or_default();
        let ch = if ch_str.eq_ignore_ascii_case("autocalculate") {
            3.0
        } else {
            ch_str.parse::<f64>().unwrap_or(3.0)
        };

        let mult_str = get_str(z, "Multiplier").unwrap_or_else(|| "1".to_string());
        let mult = mult_str.parse::<u32>().unwrap_or(1);

        ZoneDef {
            name: get_str(z, "Name").unwrap_or_default(),
            volume: vol,
            floor_area: if vol > 0.0 && ch > 0.0 { vol / ch } else { 0.0 },
            multiplier: mult,
            ceiling_height: ch,
        }
    }).collect()
}

fn translate_surfaces(model: &InputModel) -> Vec<SurfaceDef> {
    model.get_objects("BuildingSurface:Detailed").iter().map(|s| {
        let mut vertices = Vec::new();
        // Parse vertex fields — schema-parsed fields use "Vertex X-coordinate" etc.
        if let Some(x) = get_f64(s, "Vertex X-coordinate") {
            let y = get_f64(s, "Vertex Y-coordinate").unwrap_or(0.0);
            let z = get_f64(s, "Vertex Z-coordinate").unwrap_or(0.0);
            vertices.push([x, y, z]);
        }

        let area = compute_polygon_area(&vertices);

        SurfaceDef {
            name: get_str(s, "Name").unwrap_or_default(),
            surface_type: get_str(s, "Surface Type").unwrap_or_default(),
            construction_name: get_str(s, "Construction Name").unwrap_or_default(),
            zone_name: get_str(s, "Zone Name").unwrap_or_default(),
            boundary_condition: get_str(s, "Outside Boundary Condition").unwrap_or_else(|| "Outdoors".to_string()),
            area,
            azimuth: 0.0,
            tilt: 0.0,
            vertices,
        }
    }).collect()
}

fn translate_constructions(model: &InputModel) -> Vec<ConstructionDef> {
    model.get_objects("Construction").iter().map(|c| {
        let mut layers = Vec::new();
        if let Some(l) = get_str(c, "Outside Layer") {
            layers.push(l);
        }
        // Extensible layers
        for i in 2..20 {
            let key = format!("field_{i}");
            if let Some(l) = get_str(c, &key) {
                if !l.is_empty() {
                    layers.push(l);
                }
            } else {
                break;
            }
        }
        ConstructionDef {
            name: get_str(c, "Name").unwrap_or_default(),
            layers,
        }
    }).collect()
}

fn translate_output_requests(model: &InputModel) -> Vec<OutputRequest> {
    model.get_objects("Output:Variable").iter().map(|o| {
        let freq_str = get_str(o, "Reporting Frequency").unwrap_or_else(|| "Hourly".to_string());
        let freq = ReportFreq::from_str_loose(&freq_str).unwrap_or(ReportFreq::Hourly);
        OutputRequest {
            key: get_str(o, "Key Value").unwrap_or_else(|| "*".to_string()),
            variable_name: get_str(o, "Variable Name").unwrap_or_default(),
            freq,
        }
    }).collect()
}

// --- Helpers ---

fn get_str(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
        .or_else(|| {
            // Try case-insensitive with "name" key
            if key == "Name" {
                value.get("name").and_then(|v| v.as_str()).map(|s| s.to_string())
            } else {
                None
            }
        })
}

fn get_f64(value: &serde_json::Value, key: &str) -> Option<f64> {
    value.get(key)
        .and_then(|v| {
            v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
        })
}

fn is_yes(s: Option<String>) -> bool {
    s.map_or(false, |v| v.eq_ignore_ascii_case("yes"))
}

fn is_yes_or_default(s: Option<String>, default: bool) -> bool {
    match s {
        Some(v) => v.eq_ignore_ascii_case("yes"),
        None => default,
    }
}

fn compute_polygon_area(vertices: &[[f64; 3]]) -> f64 {
    if vertices.len() < 3 {
        return 0.0;
    }
    // Newell's method for 3D polygon area
    let mut nx = 0.0_f64;
    let mut ny = 0.0_f64;
    let mut nz = 0.0_f64;
    let n = vertices.len();
    for i in 0..n {
        let j = (i + 1) % n;
        nx += (vertices[i][1] - vertices[j][1]) * (vertices[i][2] + vertices[j][2]);
        ny += (vertices[i][2] - vertices[j][2]) * (vertices[i][0] + vertices[j][0]);
        nz += (vertices[i][0] - vertices[j][0]) * (vertices[i][1] + vertices[j][1]);
    }
    0.5 * (nx * nx + ny * ny + nz * nz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_minimal_idf() -> InputModel {
        let idf = r#"
  Version, 24.2;
  Building, TestBuilding, 30, City;
  SimulationControl, Yes, No, No, Yes, No;
  Timestep, 4;
  Zone, Zone1, 0, 0, 0, 0, 1, 1, 3, 300;
"#;
        let schema = SchemaDb::minimal();
        ep_io::idf::parse_idf_with_schema(idf, &schema).unwrap()
    }

    #[test]
    fn translate_building_name() {
        let model = make_minimal_idf();
        let schema = SchemaDb::minimal();
        let bm = translate_input(&model, &schema).unwrap();
        assert_eq!(bm.name, "TestBuilding");
        assert!((bm.north_axis - 30.0).abs() < 0.1);
        assert_eq!(bm.terrain, "City");
    }

    #[test]
    fn translate_sim_control_flags() {
        let model = make_minimal_idf();
        let schema = SchemaDb::minimal();
        let bm = translate_input(&model, &schema).unwrap();
        assert!(bm.config.do_zone_sizing);
        assert!(!bm.config.do_system_sizing);
        assert!(bm.config.run_design_days);
        assert!(!bm.config.run_weather_periods);
    }

    #[test]
    fn translate_timestep() {
        let model = make_minimal_idf();
        let schema = SchemaDb::minimal();
        let bm = translate_input(&model, &schema).unwrap();
        assert_eq!(bm.config.timesteps_per_hour, 4);
    }

    #[test]
    fn translate_zones_basic() {
        let model = make_minimal_idf();
        let schema = SchemaDb::minimal();
        let bm = translate_input(&model, &schema).unwrap();
        assert_eq!(bm.zones.len(), 1);
        assert_eq!(bm.zones[0].name, "Zone1");
        assert!((bm.zones[0].volume - 300.0).abs() < 1.0);
        assert!((bm.zones[0].ceiling_height - 3.0).abs() < 0.1);
    }

    #[test]
    fn translate_output_requests() {
        let schema = SchemaDb::minimal();
        let idf = r#"
  Version, 24.2;
  Building, B1, 0, Suburbs;
  Output:Variable, *, Zone Mean Air Temperature, Hourly;
  Output:Variable, Zone1, Zone Ideal Loads Heating Rate, Timestep;
"#;
        let model = ep_io::idf::parse_idf_with_schema(idf, &schema).unwrap();
        let bm = translate_input(&model, &schema).unwrap();
        assert_eq!(bm.output_requests.len(), 2);
        assert_eq!(bm.output_requests[0].key, "*");
        assert_eq!(bm.output_requests[0].variable_name, "Zone Mean Air Temperature");
        assert_eq!(bm.output_requests[0].freq, ReportFreq::Hourly);
        assert_eq!(bm.output_requests[1].freq, ReportFreq::TimeStep);
    }

    #[test]
    fn translate_constructions() {
        let schema = SchemaDb::minimal();
        let idf = r#"
  Version, 24.2;
  Building, B1, 0, Suburbs;
  Construction, WallConst, Concrete;
"#;
        let model = ep_io::idf::parse_idf_with_schema(idf, &schema).unwrap();
        let bm = translate_input(&model, &schema).unwrap();
        assert_eq!(bm.constructions.len(), 1);
        assert_eq!(bm.constructions[0].name, "WallConst");
        assert_eq!(bm.constructions[0].layers[0], "Concrete");
    }

    #[test]
    fn polygon_area_rectangle() {
        let verts = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 0.0, 3.0],
            [0.0, 0.0, 3.0],
        ];
        let area = compute_polygon_area(&verts);
        assert!((area - 30.0).abs() < 0.1, "area={area}");
    }
}
