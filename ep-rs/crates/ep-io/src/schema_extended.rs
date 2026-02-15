//! Phase 12 extended IDF object definitions.
//!
//! Adds object definitions for thermal comfort, room air models,
//! conduction finite difference, advanced fenestration, ground heat
//! transfer, airflow network extensions, and output extensions.

use crate::schema::{FieldDef, ObjectDef, SchemaDb};

/// Number of object definitions added by [`add_extended_definitions`].
pub const EXTENDED_DEF_COUNT: usize = 22;

/// Add Phase 12 extended object definitions to a schema database.
pub fn add_extended_definitions(db: &mut SchemaDb) {
    // ---------------------------------------------------------------
    // 1. Thermal Comfort
    // ---------------------------------------------------------------

    db.insert(
        ObjectDef::new("ThermalComfort:Fanger")
            .unique()
            .memo("Fanger thermal comfort model reporting settings")
            .field(
                FieldDef::choice(
                    "Mean Radiant Temperature Calculation Type",
                    &["ZoneAveraged", "SurfaceWeighted", "AngleFactor"],
                )
                .default_value("ZoneAveraged"),
            )
            .field(
                FieldDef::choice("Report Frequency", &["Timestep", "Hourly", "Daily"])
                    .default_value("Hourly"),
            ),
    );

    db.insert(
        ObjectDef::new("ThermalComfort:AdaptiveASH55")
            .unique()
            .memo("ASHRAE Standard 55 adaptive comfort model")
            .field(
                FieldDef::choice(
                    "Averaging Time Constant",
                    &["Monthly", "RunningAverage"],
                )
                .default_value("Monthly"),
            )
            .field(
                FieldDef::choice("Acceptability Limits", &["80PercentAcceptable", "90PercentAcceptable"])
                    .default_value("80PercentAcceptable"),
            ),
    );

    // ---------------------------------------------------------------
    // 2. Room Air Models
    // ---------------------------------------------------------------

    db.insert(
        ObjectDef::new("RoomAirSettings:UnderFloorAirDistributionInterior")
            .memo("Under-floor air distribution model for interior zones")
            .min_fields(3)
            .field(FieldDef::alpha("Name").required())
            .field(FieldDef::object_list("Zone Name", "ZoneNames").required())
            .field(
                FieldDef::integer("Number of Diffusers")
                    .default_value("1")
                    .min_inclusive(1.0),
            )
            .field(
                FieldDef::real("Power per Plume")
                    .default_value("0")
                    .units("W")
                    .min_inclusive(0.0),
            )
            .field(
                FieldDef::real("Design Effective Area of Diffuser")
                    .default_value("0.0075")
                    .units("m2")
                    .min_exclusive(0.0),
            )
            .field(
                FieldDef::real("Diffuser Slot Angle from Vertical")
                    .default_value("45")
                    .units("deg")
                    .min_inclusive(0.0)
                    .max_inclusive(90.0),
            )
            .field(
                FieldDef::real("Transition Height")
                    .default_value("1.7")
                    .units("m")
                    .min_exclusive(0.0),
            ),
    );

    db.insert(
        ObjectDef::new("RoomAirSettings:CrossVentilation")
            .memo("Cross ventilation room air model")
            .min_fields(2)
            .field(FieldDef::alpha("Name").required())
            .field(FieldDef::object_list("Zone Name", "ZoneNames").required())
            .field(FieldDef::object_list("Gain Schedule Name", "ScheduleNames")),
    );

    db.insert(
        ObjectDef::new("RoomAir:TemperaturePattern:UserDefined")
            .memo("User-defined room air temperature pattern")
            .min_fields(4)
            .extensible(2)
            .field(FieldDef::alpha("Name").required())
            .field(FieldDef::object_list("Zone Name", "ZoneNames").required())
            .field(
                FieldDef::choice(
                    "Pattern Type",
                    &[
                        "ConstantGradient",
                        "TwoGradientInterp",
                        "NonDimensionalHeight",
                        "SurfaceMapping",
                    ],
                )
                .required(),
            )
            .field(
                FieldDef::real("Return Air Temperature Offset")
                    .default_value("0")
                    .units("deltaC"),
            )
            .field(FieldDef::integer("Number of Pairs").min_inclusive(0.0))
            // extensible pair group
            .field(FieldDef::real("Height").units("m").min_inclusive(0.0))
            .field(FieldDef::real("Temperature Offset").units("deltaC")),
    );

    // ---------------------------------------------------------------
    // 3. Conduction Finite Difference (CondFD)
    // ---------------------------------------------------------------

    db.insert(
        ObjectDef::new("HeatBalanceAlgorithm")
            .unique()
            .memo("Selects the heat balance algorithm for surfaces")
            .field(
                FieldDef::choice(
                    "Algorithm",
                    &[
                        "ConductionTransferFunction",
                        "MoisturePenetrationDepthConductionTransferFunction",
                        "ConductionFiniteDifference",
                    ],
                )
                .default_value("ConductionTransferFunction"),
            )
            .field(
                FieldDef::real("Surface Temperature Upper Limit")
                    .default_value("200")
                    .units("C")
                    .min_inclusive(200.0),
            )
            .field(
                FieldDef::real("Minimum Surface Convection Heat Transfer Coefficient Value")
                    .default_value("0.1")
                    .units("W/m2-K")
                    .min_exclusive(0.0),
            )
            .field(
                FieldDef::integer("Maximum Allowable Deltemp")
                    .default_value("3")
                    .min_exclusive(0.0),
            ),
    );

    db.insert(
        ObjectDef::new("MaterialProperty:PhaseChange")
            .memo("Phase change material enthalpy-temperature data")
            .min_fields(3)
            .extensible(2)
            .field(FieldDef::alpha("Name").required())
            .field(
                FieldDef::real("Temperature Coefficient for Thermal Conductivity")
                    .default_value("0")
                    .units("W/m-K2"),
            )
            // extensible pairs
            .field(FieldDef::real("Temperature").required().units("C"))
            .field(FieldDef::real("Enthalpy").required().units("J/kg").min_inclusive(0.0)),
    );

    db.insert(
        ObjectDef::new("MaterialProperty:VariableThermalConductivity")
            .memo("Temperature-dependent thermal conductivity data")
            .min_fields(3)
            .extensible(2)
            .field(FieldDef::alpha("Name").required())
            // extensible pairs
            .field(FieldDef::real("Temperature").required().units("C"))
            .field(
                FieldDef::real("Thermal Conductivity")
                    .required()
                    .units("W/m-K")
                    .min_exclusive(0.0),
            ),
    );

    // ---------------------------------------------------------------
    // 4. Advanced Fenestration
    // ---------------------------------------------------------------

    db.insert(
        ObjectDef::new("WindowMaterial:ComplexShade")
            .memo("Complex shading layer for BSDF windows")
            .min_fields(4)
            .field(FieldDef::alpha("Name").required().references(&["MaterialName"]))
            .field(
                FieldDef::choice(
                    "Shading Type",
                    &["OtherShadingType", "Venetian", "Woven", "Perforated"],
                )
                .required(),
            )
            .field(
                FieldDef::real("Front Side Beam-Beam Solar Transmittance")
                    .default_value("0")
                    .min_inclusive(0.0)
                    .max_inclusive(1.0),
            )
            .field(
                FieldDef::real("Back Side Beam-Beam Solar Transmittance")
                    .default_value("0")
                    .min_inclusive(0.0)
                    .max_inclusive(1.0),
            )
            .field(
                FieldDef::real("Front Side Beam-Beam Solar Reflectance")
                    .default_value("0")
                    .min_inclusive(0.0)
                    .max_inclusive(1.0),
            )
            .field(
                FieldDef::real("Back Side Beam-Beam Solar Reflectance")
                    .default_value("0")
                    .min_inclusive(0.0)
                    .max_inclusive(1.0),
            ),
    );

    db.insert(
        ObjectDef::new("Construction:ComplexFenestrationState")
            .memo("BSDF-based fenestration construction")
            .min_fields(4)
            .field(FieldDef::alpha("Name").required().references(&["ConstructionNames"]))
            .field(
                FieldDef::choice("Basis Type", &["LBNLWINDOW", "UserDefined"])
                    .default_value("LBNLWINDOW"),
            )
            .field(
                FieldDef::choice("Basis Symmetry Type", &["Axisymmetric", "None"])
                    .default_value("None"),
            )
            .field(
                FieldDef::choice(
                    "Window Thermal Model",
                    &["ISO15099", "ScaledCavityWidth"],
                )
                .default_value("ISO15099"),
            )
            .field(FieldDef::alpha("Basis Matrix Name"))
            .field(FieldDef::alpha("Solar Optical Complex Front Transmittance Matrix Name"))
            .field(FieldDef::alpha("Solar Optical Complex Back Reflectance Matrix Name")),
    );

    db.insert(
        ObjectDef::new("WindowProperty:ShadingControl")
            .memo("Window shading control strategy")
            .min_fields(3)
            .field(FieldDef::alpha("Name").required())
            .field(
                FieldDef::choice(
                    "Shading Type",
                    &[
                        "InteriorShade",
                        "ExteriorShade",
                        "ExteriorScreen",
                        "InteriorBlind",
                        "ExteriorBlind",
                        "BetweenGlassShade",
                        "BetweenGlassBlind",
                        "SwitchableGlazing",
                    ],
                )
                .required(),
            )
            .field(
                FieldDef::choice(
                    "Shading Control Type",
                    &[
                        "AlwaysOn",
                        "AlwaysOff",
                        "OnIfScheduleAllows",
                        "OnIfHighSolarOnWindow",
                        "OnIfHighHorizontalSolar",
                        "OnIfHighOutdoorAirTemperature",
                        "OnIfHighZoneAirTemperature",
                        "OnIfHighZoneCooling",
                        "OnNightIfLowOutdoorTempAndOffDay",
                        "MeetDaylightIlluminanceSetpoint",
                    ],
                )
                .required(),
            )
            .field(FieldDef::object_list("Schedule Name", "ScheduleNames"))
            .field(FieldDef::real("Setpoint").units("W/m2"))
            .field(
                FieldDef::choice(
                    "Glare Control Is Active",
                    &["Yes", "No"],
                )
                .default_value("No"),
            ),
    );

    db.insert(
        ObjectDef::new("WindowMaterial:GlazingGroup:Thermochromic")
            .memo("Thermochromic glazing group with temperature-dependent glass layers")
            .min_fields(3)
            .extensible(2)
            .field(FieldDef::alpha("Name").required().references(&["MaterialName"]))
            // extensible pairs
            .field(FieldDef::real("Optical Data Temperature").units("C"))
            .field(FieldDef::object_list("Window Material Glazing Name", "MaterialName")),
    );

    // ---------------------------------------------------------------
    // 5. Ground Heat Transfer
    // ---------------------------------------------------------------

    db.insert(
        ObjectDef::new("Foundation:Kiva")
            .memo("Kiva foundation heat transfer model definition")
            .min_fields(2)
            .field(FieldDef::alpha("Name").required())
            .field(
                FieldDef::real("Interior Horizontal Insulation Material Name")
                    .default_value("")
            )
            .field(
                FieldDef::real("Interior Horizontal Insulation Depth")
                    .default_value("0")
                    .units("m")
                    .min_inclusive(0.0),
            )
            .field(
                FieldDef::real("Interior Vertical Insulation Depth")
                    .default_value("0")
                    .units("m")
                    .min_inclusive(0.0),
            )
            .field(
                FieldDef::real("Exterior Horizontal Insulation Depth")
                    .default_value("0")
                    .units("m")
                    .min_inclusive(0.0),
            )
            .field(
                FieldDef::real("Exterior Vertical Insulation Depth")
                    .default_value("0")
                    .units("m")
                    .min_inclusive(0.0),
            )
            .field(
                FieldDef::real("Wall Height Above Grade")
                    .default_value("0.2")
                    .units("m")
                    .min_inclusive(0.0),
            )
            .field(
                FieldDef::real("Wall Depth Below Slab")
                    .default_value("0")
                    .units("m")
                    .min_inclusive(0.0),
            )
            .field(
                FieldDef::real("Interior Horizontal Insulation Width")
                    .default_value("0")
                    .units("m")
                    .min_inclusive(0.0),
            )
            .field(
                FieldDef::real("Exterior Horizontal Insulation Width")
                    .default_value("0")
                    .units("m")
                    .min_inclusive(0.0),
            ),
    );

    db.insert(
        ObjectDef::new("Foundation:Kiva:Settings")
            .unique()
            .memo("Global settings for Kiva foundation heat transfer")
            .field(
                FieldDef::real("Soil Conductivity")
                    .default_value("1.73")
                    .units("W/m-K")
                    .min_exclusive(0.0),
            )
            .field(
                FieldDef::real("Soil Density")
                    .default_value("1842")
                    .units("kg/m3")
                    .min_exclusive(0.0),
            )
            .field(
                FieldDef::real("Soil Specific Heat")
                    .default_value("419")
                    .units("J/kg-K")
                    .min_exclusive(0.0),
            )
            .field(
                FieldDef::real("Ground Solar Absorptivity")
                    .default_value("0.9")
                    .min_inclusive(0.0)
                    .max_inclusive(1.0),
            )
            .field(
                FieldDef::real("Ground Thermal Absorptivity")
                    .default_value("0.9")
                    .min_inclusive(0.0)
                    .max_inclusive(1.0),
            )
            .field(
                FieldDef::real("Ground Surface Roughness")
                    .default_value("0.03")
                    .units("m")
                    .min_exclusive(0.0),
            ),
    );

    db.insert(
        ObjectDef::new("SurfaceProperty:ExposedFoundationPerimeter")
            .memo("Defines exposed perimeter for foundation heat transfer")
            .min_fields(2)
            .field(FieldDef::object_list("Surface Name", "SurfaceNames").required())
            .field(
                FieldDef::choice(
                    "Exposed Perimeter Calculation Method",
                    &["TotalExposedPerimeter", "ExposedPerimeterFraction", "BySegment"],
                )
                .default_value("TotalExposedPerimeter"),
            )
            .field(
                FieldDef::real("Total Exposed Perimeter")
                    .units("m")
                    .min_inclusive(0.0),
            )
            .field(
                FieldDef::real("Exposed Perimeter Fraction")
                    .default_value("1.0")
                    .min_inclusive(0.0)
                    .max_inclusive(1.0),
            ),
    );

    // ---------------------------------------------------------------
    // 6. Airflow Network Extended
    // ---------------------------------------------------------------

    db.insert(
        ObjectDef::new("AirflowNetwork:SimulationControl")
            .unique()
            .memo("Airflow network simulation control parameters")
            .min_fields(1)
            .field(FieldDef::alpha("Name").required())
            .field(
                FieldDef::choice(
                    "AirflowNetwork Control",
                    &[
                        "MultizoneWithDistribution",
                        "MultizoneWithoutDistribution",
                        "MultizoneWithDistributionOnlyDuringFanOperation",
                        "NoMultizoneOrDistribution",
                    ],
                )
                .default_value("MultizoneWithDistribution"),
            )
            .field(
                FieldDef::choice(
                    "Wind Pressure Coefficient Type",
                    &["Input", "SurfaceAverageCalculation"],
                )
                .default_value("SurfaceAverageCalculation"),
            )
            .field(
                FieldDef::choice(
                    "Building Type",
                    &["LowRise", "HighRise"],
                )
                .default_value("LowRise"),
            )
            .field(
                FieldDef::choice(
                    "Height Selection for Local Wind Pressure Calculation",
                    &["ExternalNode", "OpeningHeight"],
                )
                .default_value("OpeningHeight"),
            )
            .field(
                FieldDef::integer("Maximum Number of Iterations")
                    .default_value("500")
                    .min_inclusive(10.0)
                    .max_inclusive(30000.0),
            )
            .field(
                FieldDef::real("Convergence Tolerance")
                    .default_value("0.0001")
                    .min_exclusive(0.0),
            ),
    );

    db.insert(
        ObjectDef::new("AirflowNetwork:Distribution:DuctLeakage")
            .memo("Duct leakage for airflow network distribution")
            .min_fields(3)
            .field(FieldDef::alpha("Name").required())
            .field(
                FieldDef::real("Air Mass Flow Coefficient")
                    .required()
                    .units("kg/s")
                    .min_exclusive(0.0),
            )
            .field(
                FieldDef::real("Air Mass Flow Exponent")
                    .default_value("0.65")
                    .min_inclusive(0.5)
                    .max_inclusive(1.0),
            ),
    );

    db.insert(
        ObjectDef::new("AirflowNetwork:OccupantVentilationControl")
            .memo("Occupant-based ventilation control for airflow network")
            .min_fields(1)
            .field(FieldDef::alpha("Name").required())
            .field(
                FieldDef::real("Minimum Opening Time")
                    .default_value("300")
                    .units("s")
                    .min_inclusive(0.0),
            )
            .field(
                FieldDef::real("Minimum Closing Time")
                    .default_value("300")
                    .units("s")
                    .min_inclusive(0.0),
            )
            .field(
                FieldDef::real("Thermal Comfort Low Temperature Boundary")
                    .default_value("10")
                    .units("C"),
            )
            .field(
                FieldDef::real("Thermal Comfort High Temperature Boundary")
                    .default_value("33")
                    .units("C"),
            ),
    );

    // ---------------------------------------------------------------
    // 7. Output Extensions
    // ---------------------------------------------------------------

    db.insert(
        ObjectDef::new("Output:Table:SummaryReports")
            .unique()
            .memo("Requests predefined summary report tables")
            .extensible(1)
            .min_fields(1)
            .field(
                FieldDef::choice(
                    "Report Name",
                    &[
                        "AllSummary",
                        "AllMonthly",
                        "AllSummaryAndMonthly",
                        "AllSummaryAndSizingPeriod",
                        "AnnualBuildingUtilityPerformanceSummary",
                        "DemandEndUseComponentsSummary",
                        "EnvelopeSummary",
                        "InputVerificationandResultsSummary",
                        "ZoneComponentLoadSummary",
                        "SensibleHeatGainSummary",
                    ],
                )
                .required(),
            ),
    );

    db.insert(
        ObjectDef::new("Output:Table:Monthly")
            .memo("Custom monthly tabular report")
            .min_fields(3)
            .extensible(2)
            .field(FieldDef::alpha("Name").required())
            .field(
                FieldDef::integer("Digits After Decimal")
                    .default_value("2")
                    .min_inclusive(0.0)
                    .max_inclusive(10.0),
            )
            // extensible pair group
            .field(FieldDef::alpha("Variable or Meter Name").required())
            .field(
                FieldDef::choice(
                    "Aggregation Type",
                    &[
                        "SumOrAverage",
                        "Maximum",
                        "Minimum",
                        "ValueWhenMaximumOrMinimum",
                        "HoursNonZero",
                        "HoursPositive",
                        "HoursNegative",
                        "SumOrAverageDuringHoursShown",
                        "MaximumDuringHoursShown",
                        "MinimumDuringHoursShown",
                    ],
                )
                .default_value("SumOrAverage"),
            ),
    );

    db.insert(
        ObjectDef::new("OutputControl:ReportingTolerances")
            .unique()
            .memo("Tolerances for time not comfortable and time setpoint not met reporting")
            .field(
                FieldDef::real("Tolerance for Time Heating Setpoint Not Met")
                    .default_value("0.2")
                    .units("deltaC")
                    .min_inclusive(0.0)
                    .max_inclusive(10.0),
            )
            .field(
                FieldDef::real("Tolerance for Time Cooling Setpoint Not Met")
                    .default_value("0.2")
                    .units("deltaC")
                    .min_inclusive(0.0)
                    .max_inclusive(10.0),
            ),
    );

    db.insert(
        ObjectDef::new("Output:Diagnostics")
            .unique()
            .memo("Controls diagnostic output messages")
            .extensible(1)
            .field(
                FieldDef::choice(
                    "Key",
                    &[
                        "DisplayAllWarnings",
                        "DisplayExtraWarnings",
                        "DisplayUnusedSchedules",
                        "DisplayUnusedObjects",
                        "DisplayAdvancedReportVariables",
                        "DisplayZoneAirHeatBalanceOffBalance",
                        "DoNotMirrorDetachedShading",
                        "DisplayWeatherMissingDataWarnings",
                        "ReportDuringWarmup",
                        "ReportDetailedWarmupConvergence",
                    ],
                )
                .required(),
            ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Bound, FieldType};

    /// Helper: create a standard schema extended with Phase 12 definitions.
    fn make_extended_db() -> SchemaDb {
        let mut db = SchemaDb::standard();
        add_extended_definitions(&mut db);
        db
    }

    #[test]
    fn extended_schema_creation_no_panic() {
        let db = make_extended_db();
        assert!(db.len() > 0);
    }

    #[test]
    fn total_added_definitions() {
        let base = SchemaDb::standard();
        let extended = make_extended_db();
        let added = extended.len() - base.len();
        assert_eq!(
            added, EXTENDED_DEF_COUNT,
            "expected {EXTENDED_DEF_COUNT} new definitions, got {added}"
        );
    }

    #[test]
    fn thermal_comfort_objects_present() {
        let db = make_extended_db();
        assert!(db.get("ThermalComfort:Fanger").is_some());
        assert!(db.get("ThermalComfort:AdaptiveASH55").is_some());
    }

    #[test]
    fn room_air_model_objects_present() {
        let db = make_extended_db();
        assert!(db.get("RoomAirSettings:UnderFloorAirDistributionInterior").is_some());
        assert!(db.get("RoomAirSettings:CrossVentilation").is_some());
        assert!(db.get("RoomAir:TemperaturePattern:UserDefined").is_some());
    }

    #[test]
    fn condfd_objects_present() {
        let db = make_extended_db();
        assert!(db.get("HeatBalanceAlgorithm").is_some());
        assert!(db.get("MaterialProperty:PhaseChange").is_some());
        assert!(db.get("MaterialProperty:VariableThermalConductivity").is_some());
    }

    #[test]
    fn advanced_fenestration_objects_present() {
        let db = make_extended_db();
        assert!(db.get("WindowMaterial:ComplexShade").is_some());
        assert!(db.get("Construction:ComplexFenestrationState").is_some());
        assert!(db.get("WindowProperty:ShadingControl").is_some());
        assert!(db.get("WindowMaterial:GlazingGroup:Thermochromic").is_some());
    }

    #[test]
    fn ground_heat_transfer_objects_present() {
        let db = make_extended_db();
        assert!(db.get("Foundation:Kiva").is_some());
        assert!(db.get("Foundation:Kiva:Settings").is_some());
        assert!(db.get("SurfaceProperty:ExposedFoundationPerimeter").is_some());
    }

    #[test]
    fn airflow_network_objects_present() {
        let db = make_extended_db();
        assert!(db.get("AirflowNetwork:SimulationControl").is_some());
        assert!(db.get("AirflowNetwork:Distribution:DuctLeakage").is_some());
        assert!(db.get("AirflowNetwork:OccupantVentilationControl").is_some());
    }

    #[test]
    fn output_extension_objects_present() {
        let db = make_extended_db();
        assert!(db.get("Output:Table:SummaryReports").is_some());
        assert!(db.get("Output:Table:Monthly").is_some());
        assert!(db.get("OutputControl:ReportingTolerances").is_some());
        assert!(db.get("Output:Diagnostics").is_some());
    }

    #[test]
    fn unique_objects_marked_correctly() {
        let db = make_extended_db();

        // These should be unique
        let unique_names = [
            "ThermalComfort:Fanger",
            "ThermalComfort:AdaptiveASH55",
            "HeatBalanceAlgorithm",
            "Foundation:Kiva:Settings",
            "AirflowNetwork:SimulationControl",
            "Output:Table:SummaryReports",
            "OutputControl:ReportingTolerances",
            "Output:Diagnostics",
        ];
        for name in &unique_names {
            let obj = db.get(name).unwrap();
            assert!(obj.unique_object, "{name} should be unique");
        }

        // These should NOT be unique
        let non_unique = [
            "RoomAirSettings:UnderFloorAirDistributionInterior",
            "Foundation:Kiva",
            "AirflowNetwork:Distribution:DuctLeakage",
            "Output:Table:Monthly",
        ];
        for name in &non_unique {
            let obj = db.get(name).unwrap();
            assert!(!obj.unique_object, "{name} should not be unique");
        }
    }

    #[test]
    fn extensible_objects_sizes() {
        let db = make_extended_db();

        // extensible(2) objects
        let ext2 = [
            "RoomAir:TemperaturePattern:UserDefined",
            "MaterialProperty:PhaseChange",
            "MaterialProperty:VariableThermalConductivity",
            "WindowMaterial:GlazingGroup:Thermochromic",
            "Output:Table:Monthly",
        ];
        for name in &ext2 {
            let obj = db.get(name).unwrap();
            assert_eq!(obj.extensible_count, 2, "{name} extensible_count should be 2");
        }

        // extensible(1) objects
        let ext1 = ["Output:Table:SummaryReports", "Output:Diagnostics"];
        for name in &ext1 {
            let obj = db.get(name).unwrap();
            assert_eq!(obj.extensible_count, 1, "{name} extensible_count should be 1");
        }
    }

    #[test]
    fn field_counts_key_objects() {
        let db = make_extended_db();

        assert_eq!(db.get("ThermalComfort:Fanger").unwrap().fields.len(), 2);
        assert_eq!(
            db.get("RoomAirSettings:UnderFloorAirDistributionInterior")
                .unwrap()
                .fields
                .len(),
            7
        );
        assert_eq!(db.get("HeatBalanceAlgorithm").unwrap().fields.len(), 4);
        assert_eq!(
            db.get("WindowProperty:ShadingControl").unwrap().fields.len(),
            6
        );
        assert_eq!(db.get("Foundation:Kiva:Settings").unwrap().fields.len(), 6);
        assert_eq!(
            db.get("AirflowNetwork:SimulationControl").unwrap().fields.len(),
            7
        );
        assert_eq!(
            db.get("OutputControl:ReportingTolerances").unwrap().fields.len(),
            2
        );
    }

    #[test]
    fn choice_fields_contain_expected_options() {
        let db = make_extended_db();

        // HeatBalanceAlgorithm algorithm choices
        let hba = db.get("HeatBalanceAlgorithm").unwrap();
        if let FieldType::Choice(opts) = &hba.fields[0].field_type {
            assert!(opts.contains(&"ConductionTransferFunction".to_string()));
            assert!(opts.contains(&"ConductionFiniteDifference".to_string()));
            assert!(opts.contains(
                &"MoisturePenetrationDepthConductionTransferFunction".to_string()
            ));
            assert_eq!(opts.len(), 3);
        } else {
            panic!("HeatBalanceAlgorithm Algorithm field should be Choice type");
        }

        // WindowProperty:ShadingControl shading types
        let wsc = db.get("WindowProperty:ShadingControl").unwrap();
        if let FieldType::Choice(opts) = &wsc.fields[1].field_type {
            assert!(opts.contains(&"InteriorShade".to_string()));
            assert!(opts.contains(&"SwitchableGlazing".to_string()));
            assert_eq!(opts.len(), 8);
        } else {
            panic!("ShadingControl Shading Type should be Choice");
        }

        // AirflowNetwork control choices
        let afn = db.get("AirflowNetwork:SimulationControl").unwrap();
        if let FieldType::Choice(opts) = &afn.fields[1].field_type {
            assert!(opts.contains(&"MultizoneWithDistribution".to_string()));
            assert!(opts.contains(&"NoMultizoneOrDistribution".to_string()));
            assert_eq!(opts.len(), 4);
        } else {
            panic!("AFN Control should be Choice");
        }
    }

    #[test]
    fn numeric_bounds_on_key_fields() {
        let db = make_extended_db();

        // Kiva Settings soil conductivity > 0
        let kiva = db.get("Foundation:Kiva:Settings").unwrap();
        assert_eq!(kiva.fields[0].min_bound, Bound::Exclusive(0.0));

        // ReportingTolerances 0..10
        let tol = db.get("OutputControl:ReportingTolerances").unwrap();
        assert_eq!(tol.fields[0].min_bound, Bound::Inclusive(0.0));
        assert_eq!(tol.fields[0].max_bound, Bound::Inclusive(10.0));

        // DuctLeakage exponent 0.5..1.0
        let duct = db.get("AirflowNetwork:Distribution:DuctLeakage").unwrap();
        assert_eq!(duct.fields[2].min_bound, Bound::Inclusive(0.5));
        assert_eq!(duct.fields[2].max_bound, Bound::Inclusive(1.0));
    }

    #[test]
    fn extensible_field_resolution_phase_change() {
        let db = make_extended_db();
        let pc = db.get("MaterialProperty:PhaseChange").unwrap();
        assert_eq!(pc.extensible_count, 2);

        // Base fields: Name, TemperatureCoefficient, Temperature, Enthalpy
        // Extensible pair starts at index 2 (Temperature, Enthalpy)
        let f2 = pc.field_at(2).unwrap();
        assert_eq!(f2.name, "Temperature");

        let f3 = pc.field_at(3).unwrap();
        assert_eq!(f3.name, "Enthalpy");

        // Index 4 should wrap back to Temperature
        let f4 = pc.field_at(4).unwrap();
        assert_eq!(f4.name, "Temperature");

        // Index 5 should wrap back to Enthalpy
        let f5 = pc.field_at(5).unwrap();
        assert_eq!(f5.name, "Enthalpy");
    }
}
