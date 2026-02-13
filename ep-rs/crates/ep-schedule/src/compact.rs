//! Compact schedule parsing.
//!
//! Parses EnergyPlus compact schedule format with Through/For/Until syntax.

use crate::day_type::{self, DayType};
use crate::{CompactEntry, CompactSchedule};

/// Parse a compact schedule from field strings.
///
/// Format:
/// ```text
/// Through: 12/31,
/// For: AllDays,
/// Until: 8:00, 0.0,
/// Until: 18:00, 1.0,
/// Until: 24:00, 0.0;
/// ```
pub fn parse_compact_schedule(name: &str, fields: &[&str]) -> Result<CompactSchedule, String> {
    let mut entries = Vec::new();
    let mut i = 0;

    while i < fields.len() {
        let field = fields[i].trim();

        if field.to_lowercase().starts_with("through:") {
            // Parse Through date
            let date_str = field.strip_prefix("Through:").or_else(|| field.strip_prefix("through:")).unwrap().trim();
            let (through_month, through_day) = parse_date(date_str)?;

            i += 1;

            // Parse For day types and Until values
            while i < fields.len() {
                let field = fields[i].trim();
                if field.to_lowercase().starts_with("through:") {
                    break; // New Through block
                }

                if field.to_lowercase().starts_with("for:") {
                    let day_type_str = field
                        .strip_prefix("For:")
                        .or_else(|| field.strip_prefix("for:"))
                        .unwrap()
                        .trim();
                    let day_types = parse_day_types(day_type_str)?;

                    i += 1;
                    let mut until_values = Vec::new();

                    while i < fields.len() {
                        let field = fields[i].trim();
                        if field.to_lowercase().starts_with("for:") || field.to_lowercase().starts_with("through:") {
                            break;
                        }

                        if field.to_lowercase().starts_with("until:") {
                            let time_str = field
                                .strip_prefix("Until:")
                                .or_else(|| field.strip_prefix("until:"))
                                .unwrap()
                                .trim();
                            let (hour, minute) = parse_time(time_str)?;

                            i += 1;
                            // Next field is the value
                            if i < fields.len() {
                                let value: f64 = fields[i]
                                    .trim()
                                    .trim_end_matches([',', ';'])
                                    .parse()
                                    .map_err(|e| format!("Invalid value: {e}"))?;
                                until_values.push((hour, minute, value));
                            }
                            i += 1;
                        } else {
                            i += 1;
                        }
                    }

                    entries.push(CompactEntry {
                        through_month,
                        through_day,
                        day_types,
                        until_values,
                    });
                } else {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    Ok(CompactSchedule {
        name: name.to_string(),
        schedule_type: None,
        entries,
    })
}

/// Parse a date string like "12/31" or "6/30".
fn parse_date(s: &str) -> Result<(u8, u8), String> {
    let s = s.trim().trim_end_matches(',');
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid date format: '{s}'"));
    }
    let month: u8 = parts[0].trim().parse().map_err(|_| format!("Invalid month: '{}'", parts[0]))?;
    let day: u8 = parts[1].trim().parse().map_err(|_| format!("Invalid day: '{}'", parts[1]))?;
    Ok((month, day))
}

/// Parse a time string like "8:00" or "18:30".
fn parse_time(s: &str) -> Result<(u8, u8), String> {
    let s = s.trim().trim_end_matches(',');
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid time format: '{s}'"));
    }
    let hour: u8 = parts[0].trim().parse().map_err(|_| format!("Invalid hour: '{}'", parts[0]))?;
    let minute: u8 = parts[1].trim().parse().map_err(|_| format!("Invalid minute: '{}'", parts[1]))?;
    Ok((hour, minute))
}

/// Parse day type strings (comma-separated or single).
fn parse_day_types(s: &str) -> Result<Vec<DayType>, String> {
    let s = s.trim().trim_end_matches(',');
    let mut types = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if !part.is_empty() {
            match day_type::parse_day_type(part) {
                Some(dt) => types.push(dt),
                None => return Err(format!("Unknown day type: '{part}'")),
            }
        }
    }
    if types.is_empty() {
        Err(format!("No valid day types found in: '{s}'"))
    } else {
        Ok(types)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_date_valid() {
        assert_eq!(parse_date("12/31").unwrap(), (12, 31));
        assert_eq!(parse_date(" 6/30, ").unwrap(), (6, 30));
    }

    #[test]
    fn parse_time_valid() {
        assert_eq!(parse_time("8:00").unwrap(), (8, 0));
        assert_eq!(parse_time("18:30,").unwrap(), (18, 30));
        assert_eq!(parse_time("24:00").unwrap(), (24, 0));
    }

    #[test]
    fn parse_day_types_valid() {
        let dt = parse_day_types("AllDays").unwrap();
        assert_eq!(dt, vec![DayType::AllDays]);

        let dt = parse_day_types("Weekdays").unwrap();
        assert_eq!(dt, vec![DayType::Weekdays]);
    }
}
