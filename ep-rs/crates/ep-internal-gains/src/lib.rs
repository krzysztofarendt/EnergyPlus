//! Internal heat gains for EnergyPlus-rs.
//!
//! Models people, lights, electric/gas/hot water equipment, infiltration,
//! ventilation, and zone mixing. Each gain type splits into convective,
//! radiant, latent, and lost fractions.

pub mod equipment;
pub mod infiltration;
pub mod lights;
pub mod mixing;
pub mod people;
pub mod ventilation;

/// Output from an internal gain calculation: how heat is distributed.
#[derive(Debug, Clone, Copy, Default)]
pub struct GainOutput {
    /// Total power input before losses (W).
    pub total_power: f64,
    /// Convective heat to zone air (W).
    pub convective: f64,
    /// Radiant heat to zone surfaces (W).
    pub radiant: f64,
    /// Latent heat (moisture) to zone air (W).
    pub latent: f64,
    /// Visible short-wave radiation (W) — lights only.
    pub visible: f64,
    /// Heat to return air plenum (W) — lights only.
    pub return_air: f64,
    /// Lost/converted to mechanical work (W).
    pub lost: f64,
    /// CO2 generation rate (m3/s) — people and some equipment.
    pub co2_generation: f64,
}

impl GainOutput {
    /// Sum of heat entering the zone (convective + radiant + latent + visible).
    pub fn zone_total(&self) -> f64 {
        self.convective + self.radiant + self.latent + self.visible
    }

    /// Sensible heat to zone (convective + radiant + visible).
    pub fn sensible(&self) -> f64 {
        self.convective + self.radiant + self.visible
    }
}

/// Aggregate all internal gains for a zone at a given timestep.
#[derive(Debug, Clone, Default)]
pub struct ZoneInternalGains {
    /// Sum of convective gains from all sources (W).
    pub sum_convective: f64,
    /// Sum of radiant gains from all sources (W).
    pub sum_radiant: f64,
    /// Sum of latent gains from all sources (W).
    pub sum_latent: f64,
    /// Sum of visible gains from all sources (W).
    pub sum_visible: f64,
    /// Sum of return air gains from all sources (W).
    pub sum_return_air: f64,
    /// Sum of CO2 generation from all sources (m3/s).
    pub sum_co2: f64,
}

impl ZoneInternalGains {
    /// Add a gain output to the running totals.
    pub fn add(&mut self, gain: &GainOutput) {
        self.sum_convective += gain.convective;
        self.sum_radiant += gain.radiant;
        self.sum_latent += gain.latent;
        self.sum_visible += gain.visible;
        self.sum_return_air += gain.return_air;
        self.sum_co2 += gain.co2_generation;
    }

    /// Total sensible gains (convective + radiant + visible).
    pub fn total_sensible(&self) -> f64 {
        self.sum_convective + self.sum_radiant + self.sum_visible
    }

    /// Total gains entering zone air (convective + latent).
    pub fn total_to_zone_air(&self) -> f64 {
        self.sum_convective + self.sum_latent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_output_sums() {
        let g = GainOutput {
            total_power: 1000.0,
            convective: 400.0,
            radiant: 300.0,
            latent: 100.0,
            visible: 50.0,
            return_air: 50.0,
            lost: 100.0,
            co2_generation: 0.0,
        };
        assert!((g.zone_total() - 850.0).abs() < 1e-10);
        assert!((g.sensible() - 750.0).abs() < 1e-10);
    }

    #[test]
    fn zone_gains_accumulate() {
        let mut zg = ZoneInternalGains::default();
        let g1 = GainOutput { convective: 100.0, radiant: 50.0, latent: 20.0, ..Default::default() };
        let g2 = GainOutput { convective: 200.0, radiant: 100.0, latent: 30.0, ..Default::default() };
        zg.add(&g1);
        zg.add(&g2);
        assert!((zg.sum_convective - 300.0).abs() < 1e-10);
        assert!((zg.sum_radiant - 150.0).abs() < 1e-10);
        assert!((zg.total_sensible() - 450.0).abs() < 1e-10);
    }
}
