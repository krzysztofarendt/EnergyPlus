//! Life cycle cost analysis.
//!
//! Calculates present value of building costs over a study period
//! using constant-dollar or current-dollar discounting, with support
//! for energy price escalation and MACRS depreciation.

/// Discounting convention (when within year PV is computed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscountConvention {
    /// PV factor at year start: 1/(1+r)^(year-1).
    BeginningOfYear,
    /// PV factor at mid-year: 1/(1+r)^(year-0.5).
    MidYear,
    /// PV factor at year end: 1/(1+r)^year.
    EndOfYear,
}

/// Inflation approach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InflationApproach {
    /// Real discount rate (excludes inflation).
    ConstantDollar,
    /// Nominal discount rate (includes inflation).
    CurrentDollar,
}

/// Cost category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostCategory {
    Construction,
    Energy,
    Maintenance,
    Repair,
    Replacement,
    Operation,
    Salvage,
    OtherCapital,
    OtherOperational,
}

/// A recurring annual cost.
#[derive(Debug, Clone)]
pub struct RecurringCost {
    pub name: String,
    pub category: CostCategory,
    /// Annual cost amount ($).
    pub annual_cost: f64,
    /// Annual escalation rate (e.g., 0.02 for 2%).
    pub escalation_rate: f64,
    /// First year (1-based, relative to service date).
    pub start_year: usize,
}

/// A one-time non-recurring cost.
#[derive(Debug, Clone)]
pub struct NonrecurringCost {
    pub name: String,
    pub category: CostCategory,
    /// Cost amount ($).
    pub cost: f64,
    /// Year of occurrence (1-based).
    pub year: usize,
}

/// Energy price escalation factors by resource type.
#[derive(Debug, Clone)]
pub struct EnergyEscalation {
    pub resource_name: String,
    /// Annual escalation factors (index 0 = year 1).
    pub factors: Vec<f64>,
}

/// Life cycle cost analysis parameters and calculation.
#[derive(Debug, Clone)]
pub struct LifeCycleCost {
    pub study_years: usize,
    pub discount_convention: DiscountConvention,
    pub inflation_approach: InflationApproach,
    /// Real discount rate (for constant dollar).
    pub real_discount_rate: f64,
    /// Nominal discount rate (for current dollar).
    pub nominal_discount_rate: f64,
    /// Annual inflation rate.
    pub inflation_rate: f64,
    /// Recurring costs.
    pub recurring_costs: Vec<RecurringCost>,
    /// Non-recurring costs.
    pub nonrecurring_costs: Vec<NonrecurringCost>,
    /// Annual energy costs by resource (index 0 = year 1).
    pub annual_energy_costs: Vec<f64>,
    /// Energy price escalation.
    pub energy_escalation: Vec<EnergyEscalation>,
}

/// Life cycle cost result.
#[derive(Debug, Clone)]
pub struct LccResult {
    /// Present value by year.
    pub yearly_pv: Vec<f64>,
    /// Total present value of all costs.
    pub total_pv: f64,
    /// Present value of energy costs.
    pub energy_pv: f64,
    /// Present value of non-energy costs.
    pub non_energy_pv: f64,
}

impl LifeCycleCost {
    pub fn new(study_years: usize, real_discount_rate: f64) -> Self {
        Self {
            study_years,
            discount_convention: DiscountConvention::EndOfYear,
            inflation_approach: InflationApproach::ConstantDollar,
            real_discount_rate,
            nominal_discount_rate: 0.0,
            inflation_rate: 0.0,
            recurring_costs: Vec::new(),
            nonrecurring_costs: Vec::new(),
            annual_energy_costs: Vec::new(),
            energy_escalation: Vec::new(),
        }
    }

    /// Single present value factor for a given year.
    pub fn spv(&self, year: usize) -> f64 {
        let rate = match self.inflation_approach {
            InflationApproach::ConstantDollar => self.real_discount_rate,
            InflationApproach::CurrentDollar => self.nominal_discount_rate,
        };

        let effective_year = match self.discount_convention {
            DiscountConvention::BeginningOfYear => (year as f64) - 1.0,
            DiscountConvention::MidYear => (year as f64) - 0.5,
            DiscountConvention::EndOfYear => year as f64,
        };

        if effective_year <= 0.0 {
            1.0
        } else {
            1.0 / (1.0 + rate).powf(effective_year)
        }
    }

    /// Calculate the full life cycle cost analysis.
    pub fn calculate(&self) -> LccResult {
        let mut yearly_pv = vec![0.0; self.study_years];
        let mut energy_pv = 0.0;
        let mut non_energy_pv = 0.0;

        // Energy costs
        for (i, &cost) in self.annual_energy_costs.iter().enumerate() {
            if i >= self.study_years {
                break;
            }
            let escalation = self.get_energy_escalation(i);
            let escalated_cost = cost * escalation;
            let pv = escalated_cost * self.spv(i + 1);
            yearly_pv[i] += pv;
            energy_pv += pv;
        }

        // Recurring costs
        for rc in &self.recurring_costs {
            for year in rc.start_year..=self.study_years {
                let escalation = (1.0 + rc.escalation_rate).powi((year - rc.start_year) as i32);
                let cost = rc.annual_cost * escalation;
                let pv = cost * self.spv(year);
                if year > 0 && year <= self.study_years {
                    yearly_pv[year - 1] += pv;
                    non_energy_pv += pv;
                }
            }
        }

        // Non-recurring costs
        for nrc in &self.nonrecurring_costs {
            if nrc.year > 0 && nrc.year <= self.study_years {
                let pv = nrc.cost * self.spv(nrc.year);
                yearly_pv[nrc.year - 1] += pv;
                non_energy_pv += pv;
            }
        }

        let total_pv = energy_pv + non_energy_pv;

        LccResult {
            yearly_pv,
            total_pv,
            energy_pv,
            non_energy_pv,
        }
    }

    fn get_energy_escalation(&self, year_index: usize) -> f64 {
        for esc in &self.energy_escalation {
            if year_index < esc.factors.len() {
                return esc.factors[year_index];
            }
        }
        1.0 // No escalation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spv_end_of_year() {
        let lcc = LifeCycleCost::new(25, 0.03);
        // Year 1: 1/(1.03)^1 ≈ 0.9709
        let pv = lcc.spv(1);
        assert!((pv - 0.97087).abs() < 0.001, "spv={}", pv);
        // Year 10: 1/(1.03)^10 ≈ 0.7441
        let pv10 = lcc.spv(10);
        assert!((pv10 - 0.7441).abs() < 0.001, "spv10={}", pv10);
    }

    #[test]
    fn spv_beginning_of_year() {
        let mut lcc = LifeCycleCost::new(25, 0.03);
        lcc.discount_convention = DiscountConvention::BeginningOfYear;
        // Year 1: 1/(1.03)^0 = 1.0
        assert!((lcc.spv(1) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn simple_energy_pv() {
        let mut lcc = LifeCycleCost::new(10, 0.05);
        lcc.annual_energy_costs = vec![10_000.0; 10]; // $10k/year for 10 years

        let result = lcc.calculate();
        assert!(result.energy_pv > 0.0);
        // PV of annuity: $10k * (1 - 1.05^-10) / 0.05 ≈ $77,217
        assert!((result.energy_pv - 77_217.0).abs() < 100.0,
                "energy_pv={}", result.energy_pv);
    }

    #[test]
    fn recurring_cost() {
        let mut lcc = LifeCycleCost::new(5, 0.05);
        lcc.recurring_costs.push(RecurringCost {
            name: "Maintenance".into(),
            category: CostCategory::Maintenance,
            annual_cost: 5_000.0,
            escalation_rate: 0.0,
            start_year: 1,
        });

        let result = lcc.calculate();
        assert!(result.non_energy_pv > 0.0);
        // PV of $5k/yr for 5 years at 5%
        assert!((result.non_energy_pv - 21_647.0).abs() < 100.0,
                "non_energy_pv={}", result.non_energy_pv);
    }

    #[test]
    fn nonrecurring_cost() {
        let mut lcc = LifeCycleCost::new(25, 0.03);
        lcc.nonrecurring_costs.push(NonrecurringCost {
            name: "Roof Replacement".into(),
            category: CostCategory::Replacement,
            cost: 50_000.0,
            year: 15,
        });

        let result = lcc.calculate();
        // PV = 50000 / 1.03^15 ≈ 32,087
        assert!((result.non_energy_pv - 32_087.0).abs() < 200.0,
                "pv={}", result.non_energy_pv);
    }

    #[test]
    fn energy_escalation() {
        let mut lcc = LifeCycleCost::new(3, 0.05);
        lcc.annual_energy_costs = vec![10_000.0, 10_000.0, 10_000.0];
        lcc.energy_escalation.push(EnergyEscalation {
            resource_name: "Electricity".into(),
            factors: vec![1.0, 1.03, 1.06], // 3% annual escalation
        });

        let result = lcc.calculate();
        // Year 1: 10000 * 1.0 / 1.05^1
        // Year 2: 10000 * 1.03 / 1.05^2
        // Year 3: 10000 * 1.06 / 1.05^3
        assert!(result.energy_pv > 0.0);
    }

    #[test]
    fn total_pv_is_sum() {
        let mut lcc = LifeCycleCost::new(5, 0.03);
        lcc.annual_energy_costs = vec![5_000.0; 5];
        lcc.recurring_costs.push(RecurringCost {
            name: "Maint".into(),
            category: CostCategory::Maintenance,
            annual_cost: 2_000.0,
            escalation_rate: 0.0,
            start_year: 1,
        });

        let result = lcc.calculate();
        assert!((result.total_pv - result.energy_pv - result.non_energy_pv).abs() < 0.01);
    }
}
