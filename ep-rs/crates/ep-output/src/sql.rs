//! SQL output writer (generates SQL text, no database dependency).
//!
//! Produces CREATE TABLE and INSERT statements compatible with the
//! EnergyPlus SQL output schema.

use std::io::Write;

use crate::variables::OutputVariable;

/// Write the SQL schema (CREATE TABLE statements).
pub fn write_schema(writer: &mut impl Write) -> std::io::Result<()> {
    writeln!(
        writer,
        "CREATE TABLE IF NOT EXISTS ReportVariableDataDictionary (\n\
         \tReportVariableDataDictionaryIndex INTEGER PRIMARY KEY,\n\
         \tVariableName TEXT,\n\
         \tKeyValue TEXT,\n\
         \tUnits TEXT,\n\
         \tReportingFrequency TEXT\n\
         );"
    )?;
    writeln!(writer)?;
    writeln!(
        writer,
        "CREATE TABLE IF NOT EXISTS Time (\n\
         \tTimeIndex INTEGER PRIMARY KEY,\n\
         \tMonth INTEGER,\n\
         \tDay INTEGER,\n\
         \tHour INTEGER,\n\
         \tMinute INTEGER,\n\
         \tSimulationDays INTEGER\n\
         );"
    )?;
    writeln!(writer)?;
    writeln!(
        writer,
        "CREATE TABLE IF NOT EXISTS ReportVariableData (\n\
         \tTimeIndex INTEGER,\n\
         \tReportVariableDataDictionaryIndex INTEGER,\n\
         \tVariableValue REAL\n\
         );"
    )?;
    Ok(())
}

/// Write INSERT statements for the variable dictionary.
pub fn write_variable_dictionary(
    writer: &mut impl Write,
    variables: &[OutputVariable],
) -> std::io::Result<()> {
    for var in variables {
        writeln!(
            writer,
            "INSERT INTO ReportVariableDataDictionary VALUES({}, '{}', '{}', '{}', '{}');",
            var.report_id,
            escape_sql(&var.name),
            escape_sql(&var.key),
            escape_sql(&var.units),
            var.freq.name(),
        )?;
    }
    Ok(())
}

/// Write a time index INSERT statement.
pub fn write_time_index(
    writer: &mut impl Write,
    time_index: usize,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    simulation_days: u32,
) -> std::io::Result<()> {
    writeln!(
        writer,
        "INSERT INTO Time VALUES({time_index}, {month}, {day}, {hour}, {minute}, {simulation_days});"
    )
}

/// Write a data row INSERT statement.
pub fn write_data_row(
    writer: &mut impl Write,
    time_index: usize,
    report_id: usize,
    value: f64,
) -> std::io::Result<()> {
    writeln!(
        writer,
        "INSERT INTO ReportVariableData VALUES({time_index}, {report_id}, {value:.6});"
    )
}

/// Escape single quotes in SQL strings.
fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variables::{StoreType, TimeStepType};

    #[test]
    fn schema_ddl() {
        let mut buf = Vec::new();
        write_schema(&mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("CREATE TABLE IF NOT EXISTS ReportVariableDataDictionary"));
        assert!(output.contains("CREATE TABLE IF NOT EXISTS Time"));
        assert!(output.contains("CREATE TABLE IF NOT EXISTS ReportVariableData"));
        assert!(output.contains("VariableName TEXT"));
        assert!(output.contains("VariableValue REAL"));
    }

    #[test]
    fn dictionary_insert() {
        let vars = vec![OutputVariable::new(
            1,
            "Zone Mean Air Temperature",
            "Zone1",
            "C",
            StoreType::Average,
            TimeStepType::Zone,
        )];
        let mut buf = Vec::new();
        write_variable_dictionary(&mut buf, &vars).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("INSERT INTO ReportVariableDataDictionary VALUES(1,"));
        assert!(output.contains("'Zone Mean Air Temperature'"));
        assert!(output.contains("'Zone1'"));
        assert!(output.contains("'C'"));
    }

    #[test]
    fn data_insert() {
        let mut buf = Vec::new();
        write_data_row(&mut buf, 1, 42, 23.456).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("INSERT INTO ReportVariableData VALUES(1, 42, 23.456000)"));
    }

    #[test]
    fn time_index_insert() {
        let mut buf = Vec::new();
        write_time_index(&mut buf, 1, 7, 21, 14, 30, 100).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("INSERT INTO Time VALUES(1, 7, 21, 14, 30, 100)"));
    }

    #[test]
    fn sql_escaping() {
        assert_eq!(escape_sql("it's"), "it''s");
        assert_eq!(escape_sql("normal"), "normal");
    }
}
