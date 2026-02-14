//! ESO (EnergyPlus Standard Output) file writer.
//!
//! The ESO format consists of:
//! 1. A dictionary section defining variables (report_id, name, key, units)
//! 2. "End of Data Dictionary" marker
//! 3. Data lines with timestamp records and variable values

use std::io::Write;

use crate::variables::{OutputVariable, ReportFreq, StoreType};

/// Write the variable dictionary section to ESO format.
pub fn write_dictionary(writer: &mut impl Write, variables: &[OutputVariable]) -> std::io::Result<()> {
    writeln!(writer, "Program Version,EnergyPlus-rs")?;
    for var in variables {
        let store_str = match var.store_type {
            StoreType::Average => "Avg",
            StoreType::Sum => "Sum",
        };
        writeln!(
            writer,
            "{},{},{} [{}] !{} [{}]",
            var.report_id,
            freq_code(var.freq),
            var.name,
            var.units,
            store_str,
            var.key,
        )?;
    }
    writeln!(writer, "End of Data Dictionary")?;
    Ok(())
}

/// Write a timestamp record for a reporting frequency.
pub fn write_timestamp(
    writer: &mut impl Write,
    freq: ReportFreq,
    month: u32,
    day: u32,
    hour: u32,
    minute: f64,
    dst: bool,
    day_type: &str,
) -> std::io::Result<()> {
    let dst_flag = if dst { 1 } else { 0 };
    match freq {
        ReportFreq::EachCall | ReportFreq::TimeStep => {
            writeln!(
                writer,
                "2,{},{},{},{},{:.2},{},{}",
                month, day, dst_flag, hour, minute, 0.0, day_type
            )?;
        }
        ReportFreq::Hourly => {
            writeln!(
                writer,
                "3,{},{},{},{},{:.2},{},{}",
                month, day, dst_flag, hour, minute, 0.0, day_type
            )?;
        }
        ReportFreq::Daily => {
            writeln!(writer, "4,{},{},{},{}", month, day, dst_flag, day_type)?;
        }
        ReportFreq::Monthly => {
            writeln!(writer, "5,{}", month)?;
        }
        ReportFreq::RunPeriod | ReportFreq::Annual => {
            writeln!(writer, "6")?;
        }
    }
    Ok(())
}

/// Write a data line for a single variable value.
pub fn write_data(writer: &mut impl Write, report_id: usize, value: f64) -> std::io::Result<()> {
    writeln!(writer, "{},{:.6}", report_id, value)
}

/// Map reporting frequency to ESO dictionary code.
fn freq_code(freq: ReportFreq) -> u32 {
    match freq {
        ReportFreq::EachCall | ReportFreq::TimeStep => 2,
        ReportFreq::Hourly => 1,
        ReportFreq::Daily => 7,
        ReportFreq::Monthly => 9,
        ReportFreq::RunPeriod | ReportFreq::Annual => 11,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variables::{StoreType, TimeStepType};

    #[test]
    fn dictionary_format() {
        let vars = vec![
            OutputVariable::new(1, "Zone Mean Air Temperature", "Zone1", "C", StoreType::Average, TimeStepType::Zone),
            OutputVariable::new(2, "Zone Electricity", "Zone1", "J", StoreType::Sum, TimeStepType::Zone),
        ];
        let mut buf = Vec::new();
        write_dictionary(&mut buf, &vars).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("Program Version,EnergyPlus-rs"));
        assert!(output.contains("1,1,Zone Mean Air Temperature [C] !Avg [Zone1]"));
        assert!(output.contains("2,1,Zone Electricity [J] !Sum [Zone1]"));
        assert!(output.contains("End of Data Dictionary"));
    }

    #[test]
    fn timestamp_format_timestep() {
        let mut buf = Vec::new();
        write_timestamp(&mut buf, ReportFreq::TimeStep, 1, 15, 8, 30.0, false, "Monday").unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.starts_with("2,"));
        assert!(output.contains(",1,15,0,8,30.00,"));
    }

    #[test]
    fn timestamp_format_hourly() {
        let mut buf = Vec::new();
        write_timestamp(&mut buf, ReportFreq::Hourly, 7, 21, 14, 0.0, true, "SummerDesignDay").unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.starts_with("3,"));
        assert!(output.contains(",7,21,1,14,"));
    }

    #[test]
    fn data_value_format() {
        let mut buf = Vec::new();
        write_data(&mut buf, 42, 23.456789).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert_eq!(output.trim(), "42,23.456789");
    }
}
