//! CSV output writer.
//!
//! Produces columnar CSV with a header row of "Key:Name [Units]"
//! and data rows for each reporting timestep.

use std::io::Write;

/// Columnar CSV writer that accumulates rows.
pub struct CsvWriter {
    /// Column headers: "Key:Name [Units]"
    headers: Vec<String>,
    /// Data rows, each with one value per column.
    rows: Vec<Vec<f64>>,
}

impl CsvWriter {
    pub fn new(headers: Vec<String>) -> Self {
        Self {
            headers,
            rows: Vec::new(),
        }
    }

    /// Add a data row. `values` must match the number of columns.
    pub fn add_row(&mut self, values: Vec<f64>) {
        debug_assert_eq!(values.len(), self.headers.len());
        self.rows.push(values);
    }

    /// Write all accumulated data (header + rows) to the writer.
    pub fn write_all(&self, writer: &mut impl Write) -> std::io::Result<()> {
        // Header row
        writeln!(writer, "Date/Time,{}", self.headers.join(","))?;
        // Data rows
        for (i, row) in self.rows.iter().enumerate() {
            let vals: Vec<String> = row.iter().map(|v| format!("{v:.4}")).collect();
            writeln!(writer, "{},{}", i + 1, vals.join(","))?;
        }
        Ok(())
    }

    /// Number of data rows.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Number of columns.
    pub fn column_count(&self) -> usize {
        self.headers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_row() {
        let writer = CsvWriter::new(vec![
            "Zone1:Temperature [C]".to_string(),
            "Zone1:Humidity [%]".to_string(),
        ]);
        let mut buf = Vec::new();
        writer.write_all(&mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        let first_line = output.lines().next().unwrap();
        assert!(first_line.starts_with("Date/Time,"));
        assert!(first_line.contains("Zone1:Temperature [C]"));
        assert!(first_line.contains("Zone1:Humidity [%]"));
    }

    #[test]
    fn single_row() {
        let mut writer = CsvWriter::new(vec!["Col1".to_string(), "Col2".to_string()]);
        writer.add_row(vec![20.5, 45.3]);
        let mut buf = Vec::new();
        writer.write_all(&mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        let lines: Vec<_> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].starts_with("1,"));
        assert!(lines[1].contains("20.5000"));
        assert!(lines[1].contains("45.3000"));
    }

    #[test]
    fn multiple_rows() {
        let mut writer = CsvWriter::new(vec!["Temp".to_string()]);
        writer.add_row(vec![20.0]);
        writer.add_row(vec![21.0]);
        writer.add_row(vec![22.0]);

        assert_eq!(writer.row_count(), 3);
        assert_eq!(writer.column_count(), 1);

        let mut buf = Vec::new();
        writer.write_all(&mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let lines: Vec<_> = output.lines().collect();
        assert_eq!(lines.len(), 4); // header + 3 data rows
    }
}
