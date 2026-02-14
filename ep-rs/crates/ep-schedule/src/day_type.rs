//! Day type definitions for schedule evaluation.

use ep_core::time::Weekday;

/// Day types for schedule lookup (14 types matching EnergyPlus).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DayType {
    Sunday = 0,
    Monday = 1,
    Tuesday = 2,
    Wednesday = 3,
    Thursday = 4,
    Friday = 5,
    Saturday = 6,
    Holiday = 7,
    SummerDesignDay = 8,
    WinterDesignDay = 9,
    CustomDay1 = 10,
    CustomDay2 = 11,
    AllDays = 12,
    Weekdays = 13,
}

impl DayType {
    /// Check if this is a wildcard type that matches multiple days.
    pub fn is_wildcard(self) -> bool {
        matches!(self, DayType::AllDays | DayType::Weekdays)
    }

    /// Check if a specific day type is matched by this type.
    pub fn matches(self, specific: DayType) -> bool {
        match self {
            DayType::AllDays => true,
            DayType::Weekdays => matches!(
                specific,
                DayType::Monday | DayType::Tuesday | DayType::Wednesday | DayType::Thursday | DayType::Friday
            ),
            _ => self == specific,
        }
    }
}

/// Convert a weekday to the corresponding schedule day type.
pub fn day_type_from_weekday(weekday: Weekday) -> DayType {
    match weekday {
        Weekday::Sunday => DayType::Sunday,
        Weekday::Monday => DayType::Monday,
        Weekday::Tuesday => DayType::Tuesday,
        Weekday::Wednesday => DayType::Wednesday,
        Weekday::Thursday => DayType::Thursday,
        Weekday::Friday => DayType::Friday,
        Weekday::Saturday => DayType::Saturday,
    }
}

/// Parse a day type string from EnergyPlus input.
pub fn parse_day_type(s: &str) -> Option<DayType> {
    match s.trim().to_lowercase().as_str() {
        "sunday" => Some(DayType::Sunday),
        "monday" => Some(DayType::Monday),
        "tuesday" => Some(DayType::Tuesday),
        "wednesday" => Some(DayType::Wednesday),
        "thursday" => Some(DayType::Thursday),
        "friday" => Some(DayType::Friday),
        "saturday" => Some(DayType::Saturday),
        "holiday" => Some(DayType::Holiday),
        "summerdesignday" => Some(DayType::SummerDesignDay),
        "winterdesignday" => Some(DayType::WinterDesignDay),
        "customday1" => Some(DayType::CustomDay1),
        "customday2" => Some(DayType::CustomDay2),
        "alldays" => Some(DayType::AllDays),
        "weekdays" => Some(DayType::Weekdays),
        "allotherdays" => Some(DayType::AllDays),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_matching() {
        assert!(DayType::AllDays.matches(DayType::Monday));
        assert!(DayType::AllDays.matches(DayType::Sunday));
        assert!(DayType::Weekdays.matches(DayType::Monday));
        assert!(!DayType::Weekdays.matches(DayType::Sunday));
        assert!(!DayType::Weekdays.matches(DayType::Saturday));
    }

    #[test]
    fn parse_day_types() {
        assert_eq!(parse_day_type("Monday"), Some(DayType::Monday));
        assert_eq!(parse_day_type("ALLDAYS"), Some(DayType::AllDays));
        assert_eq!(parse_day_type("SummerDesignDay"), Some(DayType::SummerDesignDay));
        assert_eq!(parse_day_type("invalid"), None);
    }

    #[test]
    fn day_type_weekday_match() {
        // Weekdays should match Tue, Wed, Thu, Fri (and Monday)
        assert!(DayType::Weekdays.matches(DayType::Tuesday));
        assert!(DayType::Weekdays.matches(DayType::Wednesday));
        assert!(DayType::Weekdays.matches(DayType::Thursday));
        assert!(DayType::Weekdays.matches(DayType::Friday));
    }

    #[test]
    fn day_type_weekday_no_match_saturday() {
        assert!(!DayType::Weekdays.matches(DayType::Saturday));
    }

    #[test]
    fn day_type_exact_match() {
        assert!(DayType::Monday.matches(DayType::Monday));
        assert!(!DayType::Monday.matches(DayType::Tuesday));
        assert!(!DayType::Monday.matches(DayType::Sunday));
    }

    #[test]
    fn day_type_from_weekday_all() {
        use ep_core::time::Weekday;
        assert_eq!(day_type_from_weekday(Weekday::Sunday), DayType::Sunday);
        assert_eq!(day_type_from_weekday(Weekday::Monday), DayType::Monday);
        assert_eq!(day_type_from_weekday(Weekday::Tuesday), DayType::Tuesday);
        assert_eq!(day_type_from_weekday(Weekday::Wednesday), DayType::Wednesday);
        assert_eq!(day_type_from_weekday(Weekday::Thursday), DayType::Thursday);
        assert_eq!(day_type_from_weekday(Weekday::Friday), DayType::Friday);
        assert_eq!(day_type_from_weekday(Weekday::Saturday), DayType::Saturday);
    }
}
