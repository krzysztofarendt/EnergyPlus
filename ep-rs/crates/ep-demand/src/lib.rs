//! Demand management, utility tariffs, and life cycle cost analysis.
//!
//! Demand managers limit peak electrical demand by shedding loads.
//! Utility tariffs calculate energy costs with tiered rates, demand
//! charges, and time-of-use periods. Life cycle cost analysis computes
//! present value of building operational costs.

pub mod demand_manager;
pub mod life_cycle_cost;
pub mod tariff;
