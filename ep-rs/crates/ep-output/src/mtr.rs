//! MTR (Meter) file writer.
//!
//! The MTR format mirrors ESO but contains only meter data.

use std::io::Write;

use crate::meters::Meter;

/// Write the meter dictionary section.
pub fn write_meter_dictionary(writer: &mut impl Write, meters: &[Meter]) -> std::io::Result<()> {
    writeln!(writer, "Program Version,EnergyPlus-rs")?;
    for (i, meter) in meters.iter().enumerate() {
        writeln!(
            writer,
            "{},{},Cumulative {} [{}]",
            i + 1,
            1,
            meter.name,
            meter.units,
        )?;
    }
    writeln!(writer, "End of Data Dictionary")?;
    Ok(())
}

/// Write a meter data line.
pub fn write_meter_data(
    writer: &mut impl Write,
    index: usize,
    value: f64,
) -> std::io::Result<()> {
    writeln!(writer, "{},{:.6}", index + 1, value)
}

/// Write a cumulative meter data line.
pub fn write_cumulative_data(
    writer: &mut impl Write,
    index: usize,
    cumulative: f64,
) -> std::io::Result<()> {
    writeln!(writer, "{},{:.6}", index + 1, cumulative)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meters::{Group, Meter, Resource};

    #[test]
    fn dictionary_format() {
        let meters = vec![
            Meter::new("Electricity:Facility", Resource::Electricity, None, Group::Facility),
            Meter::new("NaturalGas:Facility", Resource::NaturalGas, None, Group::Facility),
        ];
        let mut buf = Vec::new();
        write_meter_dictionary(&mut buf, &meters).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("Program Version"));
        assert!(output.contains("1,1,Cumulative Electricity:Facility [J]"));
        assert!(output.contains("2,1,Cumulative NaturalGas:Facility [J]"));
        assert!(output.contains("End of Data Dictionary"));
    }

    #[test]
    fn data_line_format() {
        let mut buf = Vec::new();
        write_meter_data(&mut buf, 0, 12345.6789).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert_eq!(output.trim(), "1,12345.678900");
    }
}
