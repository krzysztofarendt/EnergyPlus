//! Demand limiting and load shedding.
//!
//! Monitors facility electrical demand against a scheduled limit and
//! activates demand managers to shed loads when demand exceeds the limit.

/// Demand manager type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemandManagerType {
    Lights,
    ElectricEquipment,
    Thermostats,
    Ventilation,
    ExteriorLights,
}

/// Manager activation priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerPriority {
    /// Activate first capable manager.
    Sequential,
    /// Activate all managers simultaneously.
    All,
}

/// Load selection control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionControl {
    /// Apply to all loads of this type.
    All,
    /// Rotate through subsets of loads.
    RotateMany,
    /// Rotate one load at a time.
    RotateOne,
}

/// A single demand manager that controls a set of loads.
#[derive(Debug, Clone)]
pub struct DemandManager {
    pub name: String,
    pub manager_type: DemandManagerType,
    pub selection: SelectionControl,
    /// Lower limit for controlled variable (fraction, 0-1).
    pub lower_limit: f64,
    /// Minimum activity duration (minutes).
    pub limit_duration_min: f64,
    /// Number of loads controlled.
    pub num_loads: usize,
    /// Whether currently active.
    pub is_active: bool,
    /// Time remaining in current activation (minutes).
    pub active_time_remaining: f64,
    /// Reduction achieved (W).
    pub reduction: f64,
}

impl DemandManager {
    pub fn new(name: impl Into<String>, mgr_type: DemandManagerType, num_loads: usize) -> Self {
        Self {
            name: name.into(),
            manager_type: mgr_type,
            selection: SelectionControl::All,
            lower_limit: 0.5,
            limit_duration_min: 15.0,
            num_loads,
            is_active: false,
            active_time_remaining: 0.0,
            reduction: 0.0,
        }
    }

    /// Check if this manager can reduce load.
    pub fn can_reduce(&self) -> bool {
        !self.is_active && self.num_loads > 0
    }

    /// Activate the manager.
    pub fn activate(&mut self, potential_reduction: f64) {
        self.is_active = true;
        self.active_time_remaining = self.limit_duration_min;
        self.reduction = potential_reduction * self.lower_limit;
    }

    /// Deactivate the manager.
    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.active_time_remaining = 0.0;
        self.reduction = 0.0;
    }

    /// Update time remaining; deactivate if duration expired.
    pub fn update(&mut self, elapsed_min: f64) {
        if self.is_active {
            self.active_time_remaining -= elapsed_min;
            if self.active_time_remaining <= 0.0 {
                self.deactivate();
            }
        }
    }
}

/// Demand manager list — monitors demand and activates managers.
#[derive(Debug, Clone)]
pub struct DemandManagerList {
    pub name: String,
    pub priority: ManagerPriority,
    pub managers: Vec<DemandManager>,
    /// Demand limit (W).
    pub demand_limit: f64,
    /// Safety fraction (0-1), applied to limit.
    pub safety_fraction: f64,
    /// Averaging window (timesteps).
    pub averaging_window: usize,
    /// Rolling demand history.
    history: Vec<f64>,
    /// Peak demand this billing period (W).
    pub peak_demand: f64,
    /// Time over limit (hours).
    pub over_limit_duration: f64,
}

impl DemandManagerList {
    pub fn new(name: impl Into<String>, demand_limit: f64) -> Self {
        Self {
            name: name.into(),
            priority: ManagerPriority::Sequential,
            managers: Vec::new(),
            demand_limit,
            safety_fraction: 0.95,
            averaging_window: 4, // 1 hour at 4 ts/hr
            history: Vec::new(),
            peak_demand: 0.0,
            over_limit_duration: 0.0,
        }
    }

    /// Add a demand manager.
    pub fn add_manager(&mut self, manager: DemandManager) {
        self.managers.push(manager);
    }

    /// Effective demand limit.
    pub fn effective_limit(&self) -> f64 {
        self.demand_limit * self.safety_fraction
    }

    /// Update the rolling average and check if shedding is needed.
    ///
    /// `current_demand` — instantaneous facility demand (W).
    /// `timestep_min` — timestep duration (minutes).
    /// Returns the amount of demand over the limit (W), or 0.
    pub fn update_demand(&mut self, current_demand: f64, timestep_min: f64) -> f64 {
        // Update rolling average
        self.history.push(current_demand);
        if self.history.len() > self.averaging_window {
            self.history.remove(0);
        }
        let avg_demand: f64 = self.history.iter().sum::<f64>() / self.history.len() as f64;

        // Update peak
        if avg_demand > self.peak_demand {
            self.peak_demand = avg_demand;
        }

        let effective_limit = self.effective_limit();
        let over_limit = avg_demand - effective_limit;

        if over_limit > 0.0 {
            self.over_limit_duration += timestep_min / 60.0;
            self.activate_managers(over_limit);
        } else {
            // Update active managers (decrement timers)
            for mgr in &mut self.managers {
                mgr.update(timestep_min);
            }
        }

        over_limit.max(0.0)
    }

    /// Activate managers to shed the given amount of excess demand.
    fn activate_managers(&mut self, excess: f64) {
        let mut remaining_excess = excess;

        match self.priority {
            ManagerPriority::Sequential => {
                for mgr in &mut self.managers {
                    if remaining_excess <= 0.0 {
                        break;
                    }
                    if mgr.can_reduce() {
                        let potential = remaining_excess; // Simplified
                        mgr.activate(potential);
                        remaining_excess -= mgr.reduction;
                    }
                }
            }
            ManagerPriority::All => {
                let share = excess / self.managers.len().max(1) as f64;
                for mgr in &mut self.managers {
                    if mgr.can_reduce() {
                        mgr.activate(share);
                    }
                }
            }
        }
    }

    /// Total demand reduction from active managers (W).
    pub fn total_reduction(&self) -> f64 {
        self.managers.iter()
            .filter(|m| m.is_active)
            .map(|m| m.reduction)
            .sum()
    }

    /// Reset billing period statistics.
    pub fn reset_billing_period(&mut self) {
        self.peak_demand = 0.0;
        self.over_limit_duration = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demand_manager_lifecycle() {
        let mut mgr = DemandManager::new("Lights Mgr", DemandManagerType::Lights, 10);
        assert!(mgr.can_reduce());

        mgr.activate(5000.0);
        assert!(mgr.is_active);
        assert!(!mgr.can_reduce());
        assert!(mgr.reduction > 0.0);

        mgr.update(20.0); // Exceeds 15 min duration
        assert!(!mgr.is_active);
    }

    #[test]
    fn demand_list_below_limit() {
        let mut list = DemandManagerList::new("DM", 100_000.0);
        list.add_manager(DemandManager::new("M1", DemandManagerType::Lights, 5));

        let over = list.update_demand(80_000.0, 15.0);
        assert!(over.abs() < 1e-10);
        assert_eq!(list.total_reduction(), 0.0);
    }

    #[test]
    fn demand_list_above_limit() {
        let mut list = DemandManagerList::new("DM", 100_000.0);
        list.safety_fraction = 1.0; // No safety margin
        list.add_manager(DemandManager::new("M1", DemandManagerType::Lights, 5));

        let over = list.update_demand(120_000.0, 15.0);
        assert!(over > 0.0);
        assert!(list.total_reduction() > 0.0);
        assert!(list.managers[0].is_active);
    }

    #[test]
    fn demand_list_sequential_priority() {
        let mut list = DemandManagerList::new("DM", 100_000.0);
        list.safety_fraction = 1.0;
        list.priority = ManagerPriority::Sequential;
        list.add_manager(DemandManager::new("M1", DemandManagerType::Lights, 5));
        list.add_manager(DemandManager::new("M2", DemandManagerType::ElectricEquipment, 5));

        list.update_demand(110_000.0, 15.0);
        // Sequential: first manager should be active
        assert!(list.managers[0].is_active);
    }

    #[test]
    fn demand_list_peak_tracking() {
        let mut list = DemandManagerList::new("DM", 200_000.0);
        list.update_demand(100_000.0, 15.0);
        list.update_demand(150_000.0, 15.0);
        list.update_demand(120_000.0, 15.0);

        assert!(list.peak_demand >= 120_000.0); // Rolling average of last values
    }

    #[test]
    fn demand_list_reset() {
        let mut list = DemandManagerList::new("DM", 100_000.0);
        list.update_demand(150_000.0, 15.0);
        assert!(list.peak_demand > 0.0);

        list.reset_billing_period();
        assert!((list.peak_demand).abs() < 1e-10);
    }
}
