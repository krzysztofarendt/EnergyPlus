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
}
