//! Tabular report generation.
//!
//! Produces text and HTML tables for summary reports like
//! Annual Building Performance and Zone Component Loads.

/// Aggregation type for tabular cells.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AggType {
    Sum,
    Average,
    Maximum,
    Minimum,
    HoursPositive,
    HoursNonZero,
    ValueWhenMaximum,
    ValueWhenMinimum,
}

/// A cell value in a tabular report.
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Number(f64),
    Text(String),
    Empty,
}

impl CellValue {
    pub fn to_display(&self) -> String {
        match self {
            CellValue::Number(n) => format!("{n:.2}"),
            CellValue::Text(s) => s.clone(),
            CellValue::Empty => String::new(),
        }
    }
}

/// A single table with title, column/row headers, and cell grid.
#[derive(Debug, Clone)]
pub struct Table {
    pub title: String,
    pub column_headers: Vec<String>,
    pub row_headers: Vec<String>,
    /// Grid of cells [row][column].
    pub cells: Vec<Vec<CellValue>>,
}

impl Table {
    pub fn new(title: &str, column_headers: Vec<String>, row_headers: Vec<String>) -> Self {
        let num_rows = row_headers.len();
        let num_cols = column_headers.len();
        Self {
            title: title.to_string(),
            column_headers,
            row_headers,
            cells: vec![vec![CellValue::Empty; num_cols]; num_rows],
        }
    }

    /// Set a cell value by row and column index.
    pub fn set(&mut self, row: usize, col: usize, value: CellValue) {
        if row < self.cells.len() && col < self.cells[0].len() {
            self.cells[row][col] = value;
        }
    }

    /// Render as plain text with column alignment.
    pub fn to_text(&self) -> String {
        let mut lines = Vec::new();
        lines.push(self.title.clone());
        lines.push("-".repeat(self.title.len()));

        // Calculate column widths
        let mut widths = vec![0usize; self.column_headers.len() + 1];
        // Row header column
        widths[0] = self.row_headers.iter().map(|h| h.len()).max().unwrap_or(0);
        for (j, header) in self.column_headers.iter().enumerate() {
            widths[j + 1] = header.len();
            for row in &self.cells {
                if j < row.len() {
                    widths[j + 1] = widths[j + 1].max(row[j].to_display().len());
                }
            }
        }

        // Header row
        let mut header_line = format!("{:<width$}", "", width = widths[0]);
        for (j, h) in self.column_headers.iter().enumerate() {
            header_line.push_str(&format!("  {:>width$}", h, width = widths[j + 1]));
        }
        lines.push(header_line);

        // Data rows
        for (i, row_header) in self.row_headers.iter().enumerate() {
            let mut row_line = format!("{:<width$}", row_header, width = widths[0]);
            if i < self.cells.len() {
                for (j, cell) in self.cells[i].iter().enumerate() {
                    row_line.push_str(&format!(
                        "  {:>width$}",
                        cell.to_display(),
                        width = widths[j + 1]
                    ));
                }
            }
            lines.push(row_line);
        }

        lines.join("\n")
    }

    /// Render as an HTML table.
    pub fn to_html(&self) -> String {
        let mut html = String::new();
        html.push_str(&format!("<h3>{}</h3>\n", self.title));
        html.push_str("<table border=\"1\">\n<thead>\n<tr><th></th>");
        for h in &self.column_headers {
            html.push_str(&format!("<th>{h}</th>"));
        }
        html.push_str("</tr>\n</thead>\n<tbody>\n");

        for (i, row_header) in self.row_headers.iter().enumerate() {
            html.push_str(&format!("<tr><td>{row_header}</td>"));
            if i < self.cells.len() {
                for cell in &self.cells[i] {
                    html.push_str(&format!("<td>{}</td>", cell.to_display()));
                }
            }
            html.push_str("</tr>\n");
        }

        html.push_str("</tbody>\n</table>");
        html
    }
}

/// A tabular report consisting of multiple tables.
#[derive(Debug, Clone)]
pub struct TabularReport {
    pub name: String,
    pub tables: Vec<Table>,
}

impl TabularReport {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            tables: Vec::new(),
        }
    }

    pub fn add_table(&mut self, table: Table) {
        self.tables.push(table);
    }
}

/// Create an annual building performance summary report (template).
pub fn annual_building_performance() -> TabularReport {
    let mut report = TabularReport::new("AnnualBuildingUtilityPerformanceSummary");

    let table = Table::new(
        "Site and Source Energy",
        vec!["Total Energy [GJ]".into(), "Energy Per Total Building Area [MJ/m2]".into()],
        vec![
            "Total Site Energy".into(),
            "Net Site Energy".into(),
            "Total Source Energy".into(),
            "Net Source Energy".into(),
        ],
    );
    report.add_table(table);

    report
}

/// Create a zone component loads summary report (template).
pub fn zone_component_loads() -> TabularReport {
    let mut report = TabularReport::new("ZoneComponentLoadSummary");

    let table = Table::new(
        "Estimated Cooling Peak Load Components",
        vec!["Total [W]".into(), "Per Area [W/m2]".into(), "Percent [%]".into()],
        vec![
            "People".into(),
            "Lights".into(),
            "Equipment".into(),
            "Walls".into(),
            "Roofs".into(),
            "Windows".into(),
            "Infiltration".into(),
            "Total".into(),
        ],
    );
    report.add_table(table);

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_set_table() {
        let mut table = Table::new(
            "Test Table",
            vec!["Col A".into(), "Col B".into()],
            vec!["Row 1".into(), "Row 2".into()],
        );
        table.set(0, 0, CellValue::Number(1.23));
        table.set(0, 1, CellValue::Text("Hello".into()));
        table.set(1, 0, CellValue::Number(4.56));

        assert_eq!(table.cells[0][0], CellValue::Number(1.23));
        assert_eq!(table.cells[0][1], CellValue::Text("Hello".into()));
        assert_eq!(table.cells[1][1], CellValue::Empty);
    }

    #[test]
    fn text_rendering() {
        let mut table = Table::new(
            "Energy Summary",
            vec!["Value".into()],
            vec!["Heating".into(), "Cooling".into()],
        );
        table.set(0, 0, CellValue::Number(100.5));
        table.set(1, 0, CellValue::Number(200.3));

        let text = table.to_text();
        assert!(text.contains("Energy Summary"));
        assert!(text.contains("Heating"));
        assert!(text.contains("Cooling"));
        assert!(text.contains("100.50"));
        assert!(text.contains("200.30"));
    }

    #[test]
    fn html_rendering() {
        let mut table = Table::new(
            "Zone Loads",
            vec!["Total [W]".into()],
            vec!["People".into()],
        );
        table.set(0, 0, CellValue::Number(500.0));

        let html = table.to_html();
        assert!(html.contains("<h3>Zone Loads</h3>"));
        assert!(html.contains("<th>Total [W]</th>"));
        assert!(html.contains("<td>People</td>"));
        assert!(html.contains("<td>500.00</td>"));
    }

    #[test]
    fn multi_table_report() {
        let mut report = TabularReport::new("TestReport");
        report.add_table(Table::new("Table1", vec!["A".into()], vec!["R1".into()]));
        report.add_table(Table::new("Table2", vec!["B".into()], vec!["R2".into()]));

        assert_eq!(report.tables.len(), 2);
        assert_eq!(report.tables[0].title, "Table1");
        assert_eq!(report.tables[1].title, "Table2");
    }

    #[test]
    fn annual_building_performance_template() {
        let report = annual_building_performance();
        assert_eq!(report.name, "AnnualBuildingUtilityPerformanceSummary");
        assert!(!report.tables.is_empty());
        assert!(report.tables[0].row_headers.contains(&"Total Site Energy".to_string()));
    }

    #[test]
    fn cell_value_display() {
        assert_eq!(CellValue::Number(3.14159).to_display(), "3.14");
        assert_eq!(CellValue::Text("test".into()).to_display(), "test");
        assert_eq!(CellValue::Empty.to_display(), "");
    }
}
