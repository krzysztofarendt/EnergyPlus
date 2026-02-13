//! Simulation clock and timestep management.

use ep_units::Duration;

/// Day of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weekday {
    Sunday,
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
}

impl Weekday {
    /// Advance to the next day.
    pub fn next(self) -> Self {
        match self {
            Self::Sunday => Self::Monday,
            Self::Monday => Self::Tuesday,
            Self::Tuesday => Self::Wednesday,
            Self::Wednesday => Self::Thursday,
            Self::Thursday => Self::Friday,
            Self::Friday => Self::Saturday,
            Self::Saturday => Self::Sunday,
        }
    }

    /// Convert from 1-based index (1=Sunday, 7=Saturday).
    pub fn from_index(i: u8) -> Option<Self> {
        match i {
            1 => Some(Self::Sunday),
            2 => Some(Self::Monday),
            3 => Some(Self::Tuesday),
            4 => Some(Self::Wednesday),
            5 => Some(Self::Thursday),
            6 => Some(Self::Friday),
            7 => Some(Self::Saturday),
            _ => None,
        }
    }

    /// Is this a weekday (Monday-Friday)?
    pub fn is_weekday(self) -> bool {
        !matches!(self, Self::Sunday | Self::Saturday)
    }
}

/// Timestep and simulation clock.
#[derive(Debug)]
pub struct SimulationClock {
    pub current_time: chrono::NaiveDateTime,
    pub time_step: Duration,
    pub hour_of_day: u8,
    pub timestep_in_hour: u8,
    pub timesteps_per_hour: u8,
    pub day_of_year: u16,
    pub day_of_week: Weekday,
    pub month: u8,
    pub day_of_month: u8,
    pub year: i32,
    pub is_leap_year: bool,
}

impl SimulationClock {
    /// Create a new clock with default settings (4 timesteps per hour).
    pub fn new(timesteps_per_hour: u8) -> Self {
        let ts = 3600.0 / timesteps_per_hour as f64;
        Self {
            current_time: chrono::NaiveDateTime::default(),
            time_step: Duration::new(ts),
            hour_of_day: 0,
            timestep_in_hour: 0,
            timesteps_per_hour,
            day_of_year: 1,
            day_of_week: Weekday::Monday,
            month: 1,
            day_of_month: 1,
            year: 2024,
            is_leap_year: true,
        }
    }

    /// Advance the clock by one day.
    pub fn advance_day(&mut self) {
        self.day_of_year += 1;
        self.day_of_week = self.day_of_week.next();

        let days_in_month = days_in_month(self.month, self.is_leap_year);
        self.day_of_month += 1;
        if self.day_of_month > days_in_month {
            self.day_of_month = 1;
            self.month += 1;
        }
    }

    /// Fractional hour (0.0 to 24.0).
    pub fn fractional_hour(&self) -> f64 {
        self.hour_of_day as f64 + self.timestep_in_hour as f64 / self.timesteps_per_hour as f64
    }

    /// Total simulation time elapsed since midnight in seconds.
    pub fn seconds_since_midnight(&self) -> f64 {
        self.fractional_hour() * 3600.0
    }
}

/// Number of days in a given month.
pub fn days_in_month(month: u8, leap_year: bool) -> u8 {
    match month {
        1 => 31,
        2 => {
            if leap_year {
                29
            } else {
                28
            }
        }
        3 => 31,
        4 => 30,
        5 => 31,
        6 => 30,
        7 => 31,
        8 => 31,
        9 => 30,
        10 => 31,
        11 => 30,
        12 => 31,
        _ => 0,
    }
}

/// Check if a year is a leap year.
pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Day of year from month and day.
pub fn day_of_year(month: u8, day: u8, leap_year: bool) -> u16 {
    let mut doy: u16 = 0;
    for m in 1..month {
        doy += days_in_month(m, leap_year) as u16;
    }
    doy + day as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekday_cycle() {
        let mut day = Weekday::Sunday;
        for _ in 0..7 {
            day = day.next();
        }
        assert_eq!(day, Weekday::Sunday);
    }

    #[test]
    fn leap_year_check() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2023));
    }

    #[test]
    fn day_of_year_calc() {
        assert_eq!(day_of_year(1, 1, false), 1);
        assert_eq!(day_of_year(12, 31, false), 365);
        assert_eq!(day_of_year(12, 31, true), 366);
        assert_eq!(day_of_year(3, 1, false), 60);
        assert_eq!(day_of_year(3, 1, true), 61);
    }

    #[test]
    fn clock_fractional_hour() {
        let mut clock = SimulationClock::new(4);
        clock.hour_of_day = 14;
        clock.timestep_in_hour = 2;
        assert!((clock.fractional_hour() - 14.5).abs() < 1e-10);
    }
}
