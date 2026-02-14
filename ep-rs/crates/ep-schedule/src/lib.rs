//! Schedule types and evaluation for EnergyPlus-rs.
//!
//! Implements the EnergyPlus schedule hierarchy:
//! Year -> Week (14 day types) -> Day (timestep values)

use ep_core::time::SimulationClock;

pub mod compact;
pub mod day_type;
pub mod gains;

/// Schedule interpolation method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    /// No interpolation - step function (constant over timestep).
    #[default]
    No,
    /// Average over the timestep interval.
    Average,
    /// Linear interpolation between adjacent values.
    Linear,
}

/// Schedule type limit (for validation).
#[derive(Debug, Clone)]
pub struct ScheduleTypeLimit {
    pub name: String,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
    pub is_continuous: bool,
}

/// Handle to a schedule in the schedule manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScheduleRef(pub usize);

impl ScheduleRef {
    pub const INVALID: Self = Self(usize::MAX);

    pub fn is_valid(self) -> bool {
        self.0 != usize::MAX
    }
}

/// A schedule that can be evaluated at any simulation time.
#[derive(Debug, Clone)]
pub enum Schedule {
    /// Always returns a constant value.
    Constant(f64),

    /// Year schedule with week rules mapping date ranges to week schedules.
    Year(YearSchedule),

    /// Compact schedule with inline day schedules.
    Compact(CompactSchedule),

    /// File-based schedule loaded from CSV.
    File(FileSchedule),
}

impl Schedule {
    /// Get the schedule value for the given simulation time.
    pub fn value_at(&self, clock: &SimulationClock) -> f64 {
        match self {
            Schedule::Constant(v) => *v,
            Schedule::Year(ys) => ys.value_at(clock),
            Schedule::Compact(cs) => cs.value_at(clock),
            Schedule::File(fs) => fs.value_at(clock),
        }
    }

    /// Get the constant value, if this is a constant schedule.
    pub fn as_constant(&self) -> Option<f64> {
        match self {
            Schedule::Constant(v) => Some(*v),
            _ => None,
        }
    }

    /// Get min/max bounds for validation.
    pub fn bounds(&self) -> (f64, f64) {
        match self {
            Schedule::Constant(v) => (*v, *v),
            Schedule::Year(ys) => ys.bounds(),
            Schedule::Compact(cs) => cs.bounds(),
            Schedule::File(fs) => fs.bounds(),
        }
    }
}

/// Year schedule: maps date ranges to week schedules.
#[derive(Debug, Clone)]
pub struct YearSchedule {
    pub name: String,
    pub schedule_type: Option<ScheduleTypeLimit>,
    pub week_rules: Vec<WeekRule>,
}

/// A rule that maps a date range to a week schedule.
#[derive(Debug, Clone)]
pub struct WeekRule {
    pub start_month: u8,
    pub start_day: u8,
    pub end_month: u8,
    pub end_day: u8,
    pub week_schedule: WeekSchedule,
}

/// Week schedule: one day schedule per day type (14 types).
#[derive(Debug, Clone)]
pub struct WeekSchedule {
    pub days: [DaySchedule; 14],
}

impl WeekSchedule {
    /// Get the day schedule for a given day type.
    pub fn day_for_type(&self, day_type: day_type::DayType) -> &DaySchedule {
        &self.days[day_type as usize]
    }
}

/// Day schedule: values at each timestep within a 24-hour day.
#[derive(Debug, Clone)]
pub struct DaySchedule {
    /// Values for each timestep. Length = 24 * timesteps_per_hour.
    pub values: Vec<f64>,
    pub interpolation: Interpolation,
}

impl DaySchedule {
    /// Create a day schedule with a constant value.
    pub fn constant(value: f64, timesteps_per_hour: u8) -> Self {
        let n = 24 * timesteps_per_hour as usize;
        Self {
            values: vec![value; n],
            interpolation: Interpolation::No,
        }
    }

    /// Get the value at a specific timestep index.
    pub fn value_at_timestep(&self, hour: u8, timestep_in_hour: u8, timesteps_per_hour: u8) -> f64 {
        let idx = hour as usize * timesteps_per_hour as usize + timestep_in_hour as usize;
        if idx < self.values.len() {
            self.values[idx]
        } else if !self.values.is_empty() {
            *self.values.last().unwrap()
        } else {
            0.0
        }
    }
}

impl YearSchedule {
    pub fn value_at(&self, clock: &SimulationClock) -> f64 {
        // Find the applicable week rule
        for rule in &self.week_rules {
            if date_in_range(clock.month, clock.day_of_month, rule.start_month, rule.start_day, rule.end_month, rule.end_day) {
                let day_type = day_type::day_type_from_weekday(clock.day_of_week);
                let day_sched = rule.week_schedule.day_for_type(day_type);
                return day_sched.value_at_timestep(clock.hour_of_day, clock.timestep_in_hour, clock.timesteps_per_hour);
            }
        }
        0.0
    }

    pub fn bounds(&self) -> (f64, f64) {
        let mut min = f64::MAX;
        let mut max = f64::MIN;
        for rule in &self.week_rules {
            for day in &rule.week_schedule.days {
                for &v in &day.values {
                    min = min.min(v);
                    max = max.max(v);
                }
            }
        }
        if min > max {
            (0.0, 0.0)
        } else {
            (min, max)
        }
    }
}

/// Compact schedule (inline definition).
#[derive(Debug, Clone)]
pub struct CompactSchedule {
    pub name: String,
    pub schedule_type: Option<ScheduleTypeLimit>,
    /// For each day type range, the day schedule values.
    pub entries: Vec<CompactEntry>,
}

#[derive(Debug, Clone)]
pub struct CompactEntry {
    pub through_month: u8,
    pub through_day: u8,
    pub day_types: Vec<day_type::DayType>,
    pub until_values: Vec<(u8, u8, f64)>, // (hour, minute, value)
}

impl CompactSchedule {
    pub fn value_at(&self, clock: &SimulationClock) -> f64 {
        let day_type = day_type::day_type_from_weekday(clock.day_of_week);
        let frac_hour = clock.fractional_hour();

        for entry in &self.entries {
            // Check date range (simplified: just through date)
            if clock.month < entry.through_month
                || (clock.month == entry.through_month && clock.day_of_month <= entry.through_day)
            {
                if entry.day_types.contains(&day_type) || entry.day_types.contains(&day_type::DayType::AllDays) {
                    // Find the applicable Until time
                    for &(hour, minute, value) in &entry.until_values {
                        let until_hour = hour as f64 + minute as f64 / 60.0;
                        if frac_hour <= until_hour {
                            return value;
                        }
                    }
                    // Past all Until times, return last value
                    if let Some(&(_, _, value)) = entry.until_values.last() {
                        return value;
                    }
                }
            }
        }
        0.0
    }

    pub fn bounds(&self) -> (f64, f64) {
        let mut min = f64::MAX;
        let mut max = f64::MIN;
        for entry in &self.entries {
            for &(_, _, v) in &entry.until_values {
                min = min.min(v);
                max = max.max(v);
            }
        }
        if min > max {
            (0.0, 0.0)
        } else {
            (min, max)
        }
    }
}

/// File-based schedule loaded from CSV.
#[derive(Debug, Clone)]
pub struct FileSchedule {
    pub name: String,
    pub file_path: String,
    /// Flattened values: values[day * 24 * ts_per_hr + hour * ts_per_hr + ts]
    pub values: Vec<f64>,
    pub timesteps_per_hour: u8,
}

impl FileSchedule {
    pub fn value_at(&self, clock: &SimulationClock) -> f64 {
        let day_idx = (clock.day_of_year - 1) as usize;
        let ts_per_day = 24 * self.timesteps_per_hour as usize;
        let ts_idx = clock.hour_of_day as usize * self.timesteps_per_hour as usize + clock.timestep_in_hour as usize;
        let global_idx = day_idx * ts_per_day + ts_idx;
        self.values.get(global_idx).copied().unwrap_or(0.0)
    }

    pub fn bounds(&self) -> (f64, f64) {
        if self.values.is_empty() {
            return (0.0, 0.0);
        }
        let min = self.values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = self.values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        (min, max)
    }
}

/// Schedule manager holding all schedules.
#[derive(Debug, Default)]
pub struct ScheduleManager {
    pub schedules: Vec<Schedule>,
}

impl ScheduleManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a schedule and return its reference handle.
    pub fn add(&mut self, schedule: Schedule) -> ScheduleRef {
        let idx = self.schedules.len();
        self.schedules.push(schedule);
        ScheduleRef(idx)
    }

    /// Get a schedule value by reference.
    pub fn value(&self, sched_ref: ScheduleRef, clock: &SimulationClock) -> f64 {
        if let Some(sched) = self.schedules.get(sched_ref.0) {
            sched.value_at(clock)
        } else {
            0.0
        }
    }
}

/// Check if a month/day falls within a date range.
fn date_in_range(month: u8, day: u8, start_month: u8, start_day: u8, end_month: u8, end_day: u8) -> bool {
    let current = month as u16 * 100 + day as u16;
    let start = start_month as u16 * 100 + start_day as u16;
    let end = end_month as u16 * 100 + end_day as u16;

    if start <= end {
        current >= start && current <= end
    } else {
        // Wraps around year boundary
        current >= start || current <= end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_schedule() {
        let sched = Schedule::Constant(0.5);
        let clock = SimulationClock::new(4);
        assert_eq!(sched.value_at(&clock), 0.5);
        assert_eq!(sched.bounds(), (0.5, 0.5));
    }

    #[test]
    fn day_schedule_constant() {
        let day = DaySchedule::constant(1.0, 4);
        assert_eq!(day.values.len(), 96);
        assert_eq!(day.value_at_timestep(12, 0, 4), 1.0);
    }

    #[test]
    fn schedule_manager() {
        let mut mgr = ScheduleManager::new();
        let ref1 = mgr.add(Schedule::Constant(0.75));
        let ref2 = mgr.add(Schedule::Constant(1.0));

        let clock = SimulationClock::new(4);
        assert_eq!(mgr.value(ref1, &clock), 0.75);
        assert_eq!(mgr.value(ref2, &clock), 1.0);
    }

    #[test]
    fn date_range_check() {
        assert!(date_in_range(6, 15, 1, 1, 12, 31));
        assert!(date_in_range(1, 1, 1, 1, 12, 31));
        assert!(date_in_range(12, 31, 1, 1, 12, 31));
        assert!(!date_in_range(6, 15, 7, 1, 8, 31));
        assert!(date_in_range(7, 15, 7, 1, 8, 31));
    }

    #[test]
    fn schedule_ref_invalid() {
        assert!(!ScheduleRef::INVALID.is_valid());
        assert!(ScheduleRef(0).is_valid());
    }

    /// Helper: create a default WeekSchedule where all 14 day-type slots use the given DaySchedule.
    fn uniform_week(day: DaySchedule) -> WeekSchedule {
        WeekSchedule {
            days: std::array::from_fn(|_| day.clone()),
        }
    }

    /// Helper: build a clock set to a specific date / time.
    fn clock_at(month: u8, day_of_month: u8, day_of_year: u16, weekday: ep_core::time::Weekday, hour: u8, ts: u8) -> SimulationClock {
        let mut c = SimulationClock::new(1); // 1 ts/hr for simplicity
        c.month = month;
        c.day_of_month = day_of_month;
        c.day_of_year = day_of_year;
        c.day_of_week = weekday;
        c.hour_of_day = hour;
        c.timestep_in_hour = ts;
        c
    }

    // ----- YearSchedule tests -----

    #[test]
    fn year_schedule_single_week_rule() {
        let day = DaySchedule::constant(0.8, 1);
        let week = uniform_week(day);
        let ys = YearSchedule {
            name: "test".into(),
            schedule_type: None,
            week_rules: vec![WeekRule {
                start_month: 1,
                start_day: 1,
                end_month: 12,
                end_day: 31,
                week_schedule: week,
            }],
        };
        let sched = Schedule::Year(ys);
        let clock = clock_at(6, 15, 166, ep_core::time::Weekday::Wednesday, 10, 0);
        assert_eq!(sched.value_at(&clock), 0.8);
    }

    #[test]
    fn year_schedule_two_week_rules() {
        let day_winter = DaySchedule::constant(0.3, 1);
        let day_summer = DaySchedule::constant(0.9, 1);
        let ys = YearSchedule {
            name: "seasonal".into(),
            schedule_type: None,
            week_rules: vec![
                WeekRule {
                    start_month: 1,
                    start_day: 1,
                    end_month: 6,
                    end_day: 30,
                    week_schedule: uniform_week(day_winter),
                },
                WeekRule {
                    start_month: 7,
                    start_day: 1,
                    end_month: 12,
                    end_day: 31,
                    week_schedule: uniform_week(day_summer),
                },
            ],
        };
        let sched = Schedule::Year(ys);
        // March 15 (winter half)
        let clock_w = clock_at(3, 15, 74, ep_core::time::Weekday::Friday, 12, 0);
        assert_eq!(sched.value_at(&clock_w), 0.3);
        // August 20 (summer half)
        let clock_s = clock_at(8, 20, 232, ep_core::time::Weekday::Tuesday, 12, 0);
        assert_eq!(sched.value_at(&clock_s), 0.9);
    }

    #[test]
    fn year_schedule_weekday_dispatch() {
        // Build a WeekSchedule where Monday slot has value 1.0 and Sunday slot has value 0.2
        let day_mon = DaySchedule::constant(1.0, 1);
        let day_sun = DaySchedule::constant(0.2, 1);
        let day_default = DaySchedule::constant(0.5, 1);

        let mut days: [DaySchedule; 14] = std::array::from_fn(|_| day_default.clone());
        days[day_type::DayType::Monday as usize] = day_mon;
        days[day_type::DayType::Sunday as usize] = day_sun;

        let ys = YearSchedule {
            name: "weekday_dispatch".into(),
            schedule_type: None,
            week_rules: vec![WeekRule {
                start_month: 1,
                start_day: 1,
                end_month: 12,
                end_day: 31,
                week_schedule: WeekSchedule { days },
            }],
        };
        let sched = Schedule::Year(ys);
        // Monday
        let clock_mon = clock_at(5, 5, 125, ep_core::time::Weekday::Monday, 8, 0);
        assert_eq!(sched.value_at(&clock_mon), 1.0);
        // Sunday
        let clock_sun = clock_at(5, 4, 124, ep_core::time::Weekday::Sunday, 8, 0);
        assert_eq!(sched.value_at(&clock_sun), 0.2);
    }

    #[test]
    fn year_schedule_no_matching_rule() {
        let day = DaySchedule::constant(1.0, 1);
        let ys = YearSchedule {
            name: "july_only".into(),
            schedule_type: None,
            week_rules: vec![WeekRule {
                start_month: 7,
                start_day: 1,
                end_month: 7,
                end_day: 31,
                week_schedule: uniform_week(day),
            }],
        };
        let sched = Schedule::Year(ys);
        // Evaluate in January - no matching rule -> 0.0
        let clock = clock_at(1, 15, 15, ep_core::time::Weekday::Wednesday, 12, 0);
        assert_eq!(sched.value_at(&clock), 0.0);
    }

    #[test]
    fn year_schedule_bounds() {
        // Day schedule with values spanning [0.5, 1.0]
        let day = DaySchedule {
            values: vec![0.5, 0.7, 1.0, 0.8],
            interpolation: Interpolation::No,
        };
        let ys = YearSchedule {
            name: "bounded".into(),
            schedule_type: None,
            week_rules: vec![WeekRule {
                start_month: 1,
                start_day: 1,
                end_month: 12,
                end_day: 31,
                week_schedule: uniform_week(day),
            }],
        };
        let (lo, hi) = ys.bounds();
        assert_eq!(lo, 0.5);
        assert_eq!(hi, 1.0);
    }

    #[test]
    fn year_schedule_bounds_empty() {
        let ys = YearSchedule {
            name: "empty".into(),
            schedule_type: None,
            week_rules: vec![],
        };
        assert_eq!(ys.bounds(), (0.0, 0.0));
    }

    // ----- CompactSchedule tests -----

    #[test]
    fn compact_schedule_alldays() {
        let cs = CompactSchedule {
            name: "alldays".into(),
            schedule_type: None,
            entries: vec![CompactEntry {
                through_month: 12,
                through_day: 31,
                day_types: vec![day_type::DayType::AllDays],
                until_values: vec![
                    (8, 0, 0.0),
                    (18, 0, 1.0),
                    (24, 0, 0.5),
                ],
            }],
        };
        let sched = Schedule::Compact(cs);
        // 6 AM -> first Until (8:00) covers this -> value 0.0
        let c1 = clock_at(3, 10, 69, ep_core::time::Weekday::Monday, 6, 0);
        assert_eq!(sched.value_at(&c1), 0.0);
        // 12 PM -> second Until (18:00) covers this -> value 1.0
        let c2 = clock_at(3, 10, 69, ep_core::time::Weekday::Monday, 12, 0);
        assert_eq!(sched.value_at(&c2), 1.0);
        // 20 PM -> third Until (24:00) covers this -> value 0.5
        let c3 = clock_at(3, 10, 69, ep_core::time::Weekday::Monday, 20, 0);
        assert_eq!(sched.value_at(&c3), 0.5);
    }

    #[test]
    fn compact_schedule_weekdays_only() {
        // CompactSchedule::value_at uses contains() which only matches exact DayType
        // or AllDays. To target weekdays, list them explicitly.
        let cs = CompactSchedule {
            name: "weekdays_only".into(),
            schedule_type: None,
            entries: vec![CompactEntry {
                through_month: 12,
                through_day: 31,
                day_types: vec![
                    day_type::DayType::Monday,
                    day_type::DayType::Tuesday,
                    day_type::DayType::Wednesday,
                    day_type::DayType::Thursday,
                    day_type::DayType::Friday,
                ],
                until_values: vec![(24, 0, 1.0)],
            }],
        };
        let sched = Schedule::Compact(cs);
        // Tuesday (weekday) -> 1.0
        let c_tue = clock_at(4, 1, 91, ep_core::time::Weekday::Tuesday, 10, 0);
        assert_eq!(sched.value_at(&c_tue), 1.0);
        // Saturday (weekend) -> no match -> 0.0
        let c_sat = clock_at(4, 5, 95, ep_core::time::Weekday::Saturday, 10, 0);
        assert_eq!(sched.value_at(&c_sat), 0.0);
    }

    #[test]
    fn compact_schedule_two_through_periods() {
        let cs = CompactSchedule {
            name: "two_periods".into(),
            schedule_type: None,
            entries: vec![
                CompactEntry {
                    through_month: 6,
                    through_day: 30,
                    day_types: vec![day_type::DayType::AllDays],
                    until_values: vec![(24, 0, 1.0)],
                },
                CompactEntry {
                    through_month: 12,
                    through_day: 31,
                    day_types: vec![day_type::DayType::AllDays],
                    until_values: vec![(24, 0, 0.5)],
                },
            ],
        };
        let sched = Schedule::Compact(cs);
        // March -> first Through (6/30) -> 1.0
        let c1 = clock_at(3, 1, 60, ep_core::time::Weekday::Friday, 12, 0);
        assert_eq!(sched.value_at(&c1), 1.0);
        // September -> second Through (12/31) -> 0.5
        let c2 = clock_at(9, 1, 244, ep_core::time::Weekday::Monday, 12, 0);
        assert_eq!(sched.value_at(&c2), 0.5);
    }

    #[test]
    fn compact_schedule_bounds() {
        let cs = CompactSchedule {
            name: "bounded".into(),
            schedule_type: None,
            entries: vec![CompactEntry {
                through_month: 12,
                through_day: 31,
                day_types: vec![day_type::DayType::AllDays],
                until_values: vec![
                    (8, 0, 0.0),
                    (18, 0, 0.5),
                    (24, 0, 1.0),
                ],
            }],
        };
        assert_eq!(cs.bounds(), (0.0, 1.0));
    }

    #[test]
    fn compact_schedule_bounds_empty() {
        let cs = CompactSchedule {
            name: "empty".into(),
            schedule_type: None,
            entries: vec![],
        };
        assert_eq!(cs.bounds(), (0.0, 0.0));
    }

    // ----- FileSchedule tests -----

    #[test]
    fn file_schedule_basic_lookup() {
        // 24 values (one per hour for day 1), ts_per_hr = 1
        let values: Vec<f64> = (0..24).map(|h| h as f64 * 10.0).collect();
        let fs = FileSchedule {
            name: "file_basic".into(),
            file_path: "test.csv".into(),
            values,
            timesteps_per_hour: 1,
        };
        let sched = Schedule::File(fs);
        // Hour 5 of day 1: index = 0*24 + 5 = 5 -> value 50.0
        let c = clock_at(1, 1, 1, ep_core::time::Weekday::Monday, 5, 0);
        assert_eq!(sched.value_at(&c), 50.0);
    }

    #[test]
    fn file_schedule_mid_year() {
        // 365*24 values, one per hour, each = day_of_year as f64
        let mut values = Vec::with_capacity(365 * 24);
        for day in 0..365u16 {
            for _ in 0..24 {
                values.push((day + 1) as f64);
            }
        }
        let fs = FileSchedule {
            name: "mid_year".into(),
            file_path: "test.csv".into(),
            values,
            timesteps_per_hour: 1,
        };
        // Day 180, hour 0: index = 179*24 + 0 = 4296 -> value 180.0
        let c = clock_at(6, 29, 180, ep_core::time::Weekday::Saturday, 0, 0);
        assert_eq!(fs.value_at(&c), 180.0);
    }

    #[test]
    fn file_schedule_out_of_bounds() {
        // Only 10 values
        let fs = FileSchedule {
            name: "small".into(),
            file_path: "test.csv".into(),
            values: vec![1.0; 10],
            timesteps_per_hour: 1,
        };
        // Day 2, hour 0 -> index = 1*24 + 0 = 24 -> out of bounds -> 0.0
        let c = clock_at(1, 2, 2, ep_core::time::Weekday::Tuesday, 0, 0);
        assert_eq!(fs.value_at(&c), 0.0);
    }

    #[test]
    fn file_schedule_bounds() {
        let fs = FileSchedule {
            name: "bounds".into(),
            file_path: "test.csv".into(),
            values: vec![2.0, 5.0, 3.0, 1.0, 4.0],
            timesteps_per_hour: 1,
        };
        assert_eq!(fs.bounds(), (1.0, 5.0));
    }

    #[test]
    fn file_schedule_bounds_empty() {
        let fs = FileSchedule {
            name: "empty".into(),
            file_path: "test.csv".into(),
            values: vec![],
            timesteps_per_hour: 1,
        };
        assert_eq!(fs.bounds(), (0.0, 0.0));
    }

    // ----- ScheduleManager tests -----

    #[test]
    fn schedule_manager_invalid_ref() {
        let mgr = ScheduleManager::new();
        let clock = SimulationClock::new(1);
        assert_eq!(mgr.value(ScheduleRef::INVALID, &clock), 0.0);
    }

    #[test]
    fn schedule_manager_multiple_types() {
        let mut mgr = ScheduleManager::new();

        // Constant
        let r_const = mgr.add(Schedule::Constant(0.42));

        // Year
        let day = DaySchedule::constant(0.75, 1);
        let ys = YearSchedule {
            name: "year".into(),
            schedule_type: None,
            week_rules: vec![WeekRule {
                start_month: 1,
                start_day: 1,
                end_month: 12,
                end_day: 31,
                week_schedule: uniform_week(day),
            }],
        };
        let r_year = mgr.add(Schedule::Year(ys));

        // Compact
        let cs = CompactSchedule {
            name: "compact".into(),
            schedule_type: None,
            entries: vec![CompactEntry {
                through_month: 12,
                through_day: 31,
                day_types: vec![day_type::DayType::AllDays],
                until_values: vec![(24, 0, 0.88)],
            }],
        };
        let r_compact = mgr.add(Schedule::Compact(cs));

        // File
        let fs = FileSchedule {
            name: "file".into(),
            file_path: "test.csv".into(),
            values: vec![0.33; 365 * 24],
            timesteps_per_hour: 1,
        };
        let r_file = mgr.add(Schedule::File(fs));

        let clock = clock_at(6, 15, 166, ep_core::time::Weekday::Wednesday, 10, 0);
        assert_eq!(mgr.value(r_const, &clock), 0.42);
        assert_eq!(mgr.value(r_year, &clock), 0.75);
        assert_eq!(mgr.value(r_compact, &clock), 0.88);
        assert_eq!(mgr.value(r_file, &clock), 0.33);
    }
}
