//! Utility tariff calculation.
//!
//! Supports time-of-use rate periods, tiered (block) charges,
//! demand charges, ratchets, and monthly minimum charges.

/// Rate period type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatePeriod {
    Peak,
    Shoulder,
    OffPeak,
    MidPeak,
}

/// Season type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Season {
    Winter,
    Spring,
    Summer,
    Fall,
    Annual,
}

/// Tariff meter type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterType {
    Electricity,
    NaturalGas,
    Water,
    Other,
}

/// Buy/sell mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuyOrSell {
    BuyFromUtility,
    SellToUtility,
    NetMetering,
}

/// A block (tier) in a block charge structure.
#[derive(Debug, Clone, Copy)]
pub struct ChargeBlock {
    /// Block size (kWh or kW). f64::MAX for remaining.
    pub size: f64,
    /// Cost per unit in this block.
    pub rate: f64,
}

/// A simple energy or demand charge.
#[derive(Debug, Clone)]
pub struct SimpleCharge {
    pub name: String,
    /// Rate per unit ($/kWh or $/kW).
    pub rate: f64,
    /// Season this charge applies to (None = all seasons).
    pub season: Option<Season>,
}

/// A tiered block charge structure.
#[derive(Debug, Clone)]
pub struct BlockCharge {
    pub name: String,
    pub blocks: Vec<ChargeBlock>,
    pub season: Option<Season>,
}

/// Demand ratchet — minimum demand charge based on historical peak.
#[derive(Debug, Clone)]
pub struct DemandRatchet {
    pub name: String,
    /// Fraction of baseline peak to use as minimum (0-1).
    pub multiplier: f64,
    /// Fixed offset added to ratcheted demand (kW).
    pub offset: f64,
    /// Season for the baseline peak.
    pub baseline_season: Season,
}

/// Monthly accumulated data for tariff calculation.
#[derive(Debug, Clone, Copy, Default)]
pub struct MonthlyData {
    /// Energy by period (kWh).
    pub energy: [f64; 4], // Peak, Shoulder, OffPeak, MidPeak
    /// Peak demand by period (kW).
    pub demand: [f64; 4],
    /// Season for this month.
    pub season: Season,
}

impl Default for Season {
    fn default() -> Self {
        Self::Annual
    }
}

/// Utility tariff specification.
#[derive(Debug, Clone)]
pub struct Tariff {
    pub name: String,
    pub meter_type: MeterType,
    pub buy_or_sell: BuyOrSell,
    /// Monthly service charge ($).
    pub monthly_charge: f64,
    /// Minimum monthly charge ($).
    pub min_monthly_charge: f64,
    /// Energy charges (simple rate per kWh).
    pub energy_charges: Vec<SimpleCharge>,
    /// Block energy charges (tiered rates).
    pub block_charges: Vec<BlockCharge>,
    /// Demand charges (rate per kW).
    pub demand_charges: Vec<SimpleCharge>,
    /// Demand ratchets.
    pub ratchets: Vec<DemandRatchet>,
    /// Monthly accumulated data.
    pub monthly_data: [MonthlyData; 12],
}

/// Tariff calculation result for one month.
#[derive(Debug, Clone, Copy)]
pub struct MonthlyBill {
    pub energy_charge: f64,
    pub demand_charge: f64,
    pub service_charge: f64,
    pub total: f64,
}

impl Tariff {
    pub fn new(name: impl Into<String>, meter_type: MeterType) -> Self {
        Self {
            name: name.into(),
            meter_type,
            buy_or_sell: BuyOrSell::BuyFromUtility,
            monthly_charge: 0.0,
            min_monthly_charge: 0.0,
            energy_charges: Vec::new(),
            block_charges: Vec::new(),
            demand_charges: Vec::new(),
            ratchets: Vec::new(),
            monthly_data: [MonthlyData::default(); 12],
        }
    }

    /// Add a simple energy charge.
    pub fn add_energy_charge(&mut self, name: impl Into<String>, rate: f64, season: Option<Season>) {
        self.energy_charges.push(SimpleCharge {
            name: name.into(),
            rate,
            season,
        });
    }

    /// Add a simple demand charge.
    pub fn add_demand_charge(&mut self, name: impl Into<String>, rate: f64, season: Option<Season>) {
        self.demand_charges.push(SimpleCharge {
            name: name.into(),
            rate,
            season,
        });
    }

    /// Add a block (tiered) energy charge.
    pub fn add_block_charge(&mut self, name: impl Into<String>, blocks: Vec<ChargeBlock>, season: Option<Season>) {
        self.block_charges.push(BlockCharge {
            name: name.into(),
            blocks,
            season,
        });
    }

    /// Accumulate timestep energy into monthly data.
    pub fn gather_timestep(
        &mut self,
        month: usize,
        period: RatePeriod,
        energy_kwh: f64,
        demand_kw: f64,
    ) {
        if month == 0 || month > 12 {
            return;
        }
        let m = month - 1;
        let p = period as usize;
        self.monthly_data[m].energy[p] += energy_kwh;
        if demand_kw > self.monthly_data[m].demand[p] {
            self.monthly_data[m].demand[p] = demand_kw;
        }
    }

    /// Calculate bill for a specific month.
    pub fn calculate_month(&self, month: usize) -> MonthlyBill {
        if month == 0 || month > 12 {
            return MonthlyBill { energy_charge: 0.0, demand_charge: 0.0, service_charge: 0.0, total: 0.0 };
        }
        let m = month - 1;
        let data = &self.monthly_data[m];
        let total_energy: f64 = data.energy.iter().sum();
        let peak_demand = data.demand.iter().copied().reduce(f64::max).unwrap_or(0.0);

        // Energy charges
        let mut energy_charge = 0.0;
        for charge in &self.energy_charges {
            if self.season_matches(charge.season, data.season) {
                energy_charge += total_energy * charge.rate;
            }
        }

        // Block charges
        for block_charge in &self.block_charges {
            if self.season_matches(block_charge.season, data.season) {
                energy_charge += Self::evaluate_blocks(&block_charge.blocks, total_energy);
            }
        }

        // Demand charges
        let mut demand_charge = 0.0;
        for charge in &self.demand_charges {
            if self.season_matches(charge.season, data.season) {
                demand_charge += peak_demand * charge.rate;
            }
        }

        let service_charge = self.monthly_charge;
        let mut total = energy_charge + demand_charge + service_charge;

        // Apply minimum monthly charge
        if total < self.min_monthly_charge {
            total = self.min_monthly_charge;
        }

        MonthlyBill {
            energy_charge,
            demand_charge,
            service_charge,
            total,
        }
    }

    /// Calculate annual bill (sum of all months).
    pub fn calculate_annual(&self) -> f64 {
        (1..=12).map(|m| self.calculate_month(m).total).sum()
    }

    fn season_matches(&self, charge_season: Option<Season>, month_season: Season) -> bool {
        match charge_season {
            None | Some(Season::Annual) => true,
            Some(s) => s == month_season,
        }
    }

    fn evaluate_blocks(blocks: &[ChargeBlock], mut remaining: f64) -> f64 {
        let mut cost = 0.0;
        for block in blocks {
            if remaining <= 0.0 {
                break;
            }
            let qty = remaining.min(block.size);
            cost += qty * block.rate;
            remaining -= qty;
        }
        cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_energy_charge() {
        let mut tariff = Tariff::new("Electric", MeterType::Electricity);
        tariff.add_energy_charge("Energy", 0.10, None);
        tariff.monthly_data[0].energy[0] = 1000.0; // 1000 kWh peak

        let bill = tariff.calculate_month(1);
        assert!((bill.energy_charge - 100.0).abs() < 0.01);
    }

    #[test]
    fn block_charge() {
        let mut tariff = Tariff::new("Electric", MeterType::Electricity);
        tariff.add_block_charge("Tiered", vec![
            ChargeBlock { size: 500.0, rate: 0.08 },
            ChargeBlock { size: 500.0, rate: 0.12 },
            ChargeBlock { size: f64::MAX, rate: 0.15 },
        ], None);
        tariff.monthly_data[0].energy[0] = 1200.0;

        let bill = tariff.calculate_month(1);
        // 500 * 0.08 + 500 * 0.12 + 200 * 0.15 = 40 + 60 + 30 = 130
        assert!((bill.energy_charge - 130.0).abs() < 0.01, "energy_charge={}", bill.energy_charge);
    }

    #[test]
    fn demand_charge() {
        let mut tariff = Tariff::new("Electric", MeterType::Electricity);
        tariff.add_demand_charge("Demand", 15.0, None); // $15/kW
        tariff.monthly_data[0].demand[0] = 100.0; // 100 kW peak

        let bill = tariff.calculate_month(1);
        assert!((bill.demand_charge - 1500.0).abs() < 0.01);
    }

    #[test]
    fn monthly_service_charge() {
        let mut tariff = Tariff::new("Electric", MeterType::Electricity);
        tariff.monthly_charge = 25.0;

        let bill = tariff.calculate_month(1);
        assert!((bill.service_charge - 25.0).abs() < 0.01);
        assert!((bill.total - 25.0).abs() < 0.01);
    }

    #[test]
    fn minimum_monthly_charge() {
        let mut tariff = Tariff::new("Electric", MeterType::Electricity);
        tariff.min_monthly_charge = 50.0;
        tariff.add_energy_charge("Energy", 0.10, None);
        tariff.monthly_data[0].energy[0] = 100.0; // Only $10

        let bill = tariff.calculate_month(1);
        assert!((bill.total - 50.0).abs() < 0.01); // Raised to minimum
    }

    #[test]
    fn seasonal_charge() {
        let mut tariff = Tariff::new("Electric", MeterType::Electricity);
        tariff.add_energy_charge("Summer", 0.15, Some(Season::Summer));
        tariff.add_energy_charge("Winter", 0.08, Some(Season::Winter));

        // Summer month
        tariff.monthly_data[6].season = Season::Summer;
        tariff.monthly_data[6].energy[0] = 1000.0;
        let bill = tariff.calculate_month(7);
        assert!((bill.energy_charge - 150.0).abs() < 0.01);

        // Winter month
        tariff.monthly_data[0].season = Season::Winter;
        tariff.monthly_data[0].energy[0] = 1000.0;
        let bill = tariff.calculate_month(1);
        assert!((bill.energy_charge - 80.0).abs() < 0.01);
    }

    #[test]
    fn annual_calculation() {
        let mut tariff = Tariff::new("Electric", MeterType::Electricity);
        tariff.add_energy_charge("Energy", 0.10, None);
        for m in 0..12 {
            tariff.monthly_data[m].energy[0] = 500.0; // 500 kWh/month
        }

        let annual = tariff.calculate_annual();
        assert!((annual - 600.0).abs() < 0.01); // 12 * 50
    }

    #[test]
    fn gather_timestep() {
        let mut tariff = Tariff::new("Electric", MeterType::Electricity);
        tariff.gather_timestep(1, RatePeriod::Peak, 10.0, 50.0);
        tariff.gather_timestep(1, RatePeriod::Peak, 15.0, 60.0);
        tariff.gather_timestep(1, RatePeriod::OffPeak, 5.0, 20.0);

        assert!((tariff.monthly_data[0].energy[0] - 25.0).abs() < 0.01); // Peak energy
        assert!((tariff.monthly_data[0].demand[0] - 60.0).abs() < 0.01); // Peak demand
        assert!((tariff.monthly_data[0].energy[2] - 5.0).abs() < 0.01);  // OffPeak energy
    }
}
