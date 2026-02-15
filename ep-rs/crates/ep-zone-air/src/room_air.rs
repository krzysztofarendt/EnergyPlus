//! Room air distribution models for non-uniform zone temperature fields.
//!
//! Standard zone models assume a well-mixed single air node. These models
//! capture spatial temperature variation within a zone for:
//!
//! - **UFAD** (Underfloor Air Distribution): 3-zone vertical stratification
//! - **Displacement Ventilation**: 2-zone vertical stratification (Mundt model)
//! - **Cross Ventilation**: 2-node jet/recirculation pattern
//! - **User-Defined Pattern**: arbitrary temperature offsets by height

use ep_psychrometrics::cp_air;

// ---------------------------------------------------------------------------
// UFAD (Underfloor Air Distribution) model
// ---------------------------------------------------------------------------

/// Underfloor Air Distribution (UFAD) model result.
///
/// Contains temperatures at three vertical zones: lower occupied, upper
/// occupied, and upper room (above occupied zone to ceiling).
#[derive(Debug, Clone)]
pub struct UfadResult {
    /// Temperature in the lower occupied zone (C).
    pub temp_lower: f64,
    /// Temperature in the upper occupied zone (C).
    pub temp_upper_occupied: f64,
    /// Temperature in the upper room zone above occupied (C).
    pub temp_upper_room: f64,
    /// Temperature gradient in the stratified region (K/m).
    pub gradient: f64,
}

/// UFAD (Underfloor Air Distribution) model.
///
/// Models 3-zone vertical stratification produced by floor-level supply
/// diffusers. The lower occupied zone is close to supply temperature,
/// while upper zones are stratified by convective heat plumes rising
/// from occupants and equipment.
///
/// The three zones are:
/// 1. **Lower occupied** (floor to transition height): well-mixed near supply
/// 2. **Upper occupied** (transition height to head height ~1.8m): stratified
/// 3. **Upper room** (head height to ceiling): fully stratified
///
/// Temperature profile:
///   T_lower = T_supply + offset (offset from floor diffuser mixing)
///   T(h) = T_lower + gamma * (h - h_transition)  for h > h_transition
///
/// Gamma (gradient, K/m) depends on internal load density and supply airflow.
#[derive(Debug, Clone)]
pub struct UfadModel {
    /// Supply air temperature from floor diffusers (C).
    pub supply_temp: f64,
    /// Total room height (m).
    pub room_height: f64,
    /// Floor area (m2).
    pub floor_area: f64,
    /// Transition height between lower and upper zones (m).
    /// Typically 0.8-1.2m for UFAD systems.
    pub transition_height: f64,
    /// Total convective power per plume source (W).
    /// Represents convective gains from occupants, equipment, etc.
    pub power_per_plume: f64,
    /// Number of plume sources in the zone.
    pub num_plumes: usize,
    /// Supply airflow rate (kg/s).
    pub supply_flow: f64,
    /// Humidity ratio of supply air (kg/kg), used for Cp calculation.
    pub humidity_ratio: f64,
}

impl UfadModel {
    /// Create a new UFAD model with the given parameters.
    pub fn new(
        supply_temp: f64,
        room_height: f64,
        floor_area: f64,
        transition_height: f64,
        power_per_plume: f64,
        num_plumes: usize,
        supply_flow: f64,
    ) -> Self {
        Self {
            supply_temp,
            room_height,
            floor_area,
            transition_height,
            power_per_plume,
            num_plumes,
            supply_flow,
            humidity_ratio: 0.008,
        }
    }

    /// Calculate the temperature gradient (K/m) in the stratified region.
    ///
    /// Based on the relationship between convective plume power, supply
    /// airflow capacity, and room geometry. The gradient increases with
    /// higher internal loads and decreases with higher supply flow.
    ///
    /// gamma = Q_total / (m_dot * Cp * (H - h_transition))
    ///
    /// This is a simplified Mundt/Linden plume model where the total
    /// convective gain must be removed over the stratified height.
    pub fn calculate_gradient(&self) -> f64 {
        let cp = cp_air(self.humidity_ratio);
        let total_convective_power = self.power_per_plume * self.num_plumes as f64;
        let stratified_height = self.room_height - self.transition_height;

        if self.supply_flow < 1e-10 || stratified_height < 1e-10 || cp < 1e-10 {
            return 0.0;
        }

        total_convective_power / (self.supply_flow * cp * stratified_height)
    }

    /// Calculate zone temperatures at the three UFAD stratification levels.
    ///
    /// Returns temperatures for lower occupied, upper occupied, and upper room
    /// zones based on the convective plume stratification model.
    pub fn calculate(&self) -> UfadResult {
        let gamma = self.calculate_gradient();

        // Lower zone: supply temp + small mixing offset from floor diffuser
        // The offset accounts for induction mixing at the diffuser outlet.
        // Typically 1-2 K above supply temp.
        let cp = cp_air(self.humidity_ratio);
        let mixing_offset = if self.supply_flow > 1e-10 && cp > 1e-10 {
            // A fraction of the plume heat warms the lower zone via recirculation
            let q_total = self.power_per_plume * self.num_plumes as f64;
            let recirculation_fraction = 0.1; // ~10% of gains recirculate to lower zone
            q_total * recirculation_fraction / (self.supply_flow * cp)
        } else {
            0.0
        };
        let temp_lower = self.supply_temp + mixing_offset;

        // Upper occupied zone: evaluate at representative height (1.4m, seated head)
        let upper_occupied_height = 1.4_f64.min(self.room_height);
        let temp_upper_occupied = if upper_occupied_height > self.transition_height {
            temp_lower + gamma * (upper_occupied_height - self.transition_height)
        } else {
            temp_lower
        };

        // Upper room zone: evaluate at ceiling
        let temp_upper_room = temp_lower + gamma * (self.room_height - self.transition_height);

        UfadResult {
            temp_lower,
            temp_upper_occupied,
            temp_upper_room,
            gradient: gamma,
        }
    }

    /// Get the temperature at an arbitrary height using the stratification profile.
    ///
    /// Below transition height returns the lower zone temperature.
    /// Above transition height applies the linear gradient.
    pub fn temperature_at_height(&self, height: f64) -> f64 {
        let result = self.calculate();
        if height <= self.transition_height {
            result.temp_lower
        } else {
            let clamped = height.min(self.room_height);
            result.temp_lower + result.gradient * (clamped - self.transition_height)
        }
    }
}

// ---------------------------------------------------------------------------
// Displacement Ventilation model
// ---------------------------------------------------------------------------

/// Displacement Ventilation model result.
///
/// Two-zone vertical stratification with a distinct interface height.
#[derive(Debug, Clone)]
pub struct DisplacementVentResult {
    /// Temperature in the lower zone (C).
    pub temp_lower: f64,
    /// Temperature in the upper zone (C).
    pub temp_upper: f64,
    /// Interface height between lower and upper zones (m).
    pub interface_height: f64,
    /// Average zone temperature (C) for overall energy balance.
    pub temp_average: f64,
}

/// Displacement Ventilation model.
///
/// Models 2-zone vertical stratification produced by low-velocity floor-level
/// supply and ceiling-level return. Cool air pools at the floor and is pushed
/// upward by convective plumes, creating a distinct interface between the
/// lower cool zone and upper warm zone.
///
/// Based on the Mundt model:
/// - T_lower = T_supply (fresh air lake at floor level)
/// - T_upper = T_lower + Q_cooling / (m_dot * Cp) via energy balance
/// - Interface height depends on supply flow and plume buoyancy
///
/// The interface height (h_i) is determined by the balance between supply
/// volume flow and plume entrainment. Higher supply flow raises the
/// interface; stronger plumes lower it.
#[derive(Debug, Clone)]
pub struct DisplacementVentModel {
    /// Supply air temperature at floor level (C).
    pub supply_temp: f64,
    /// Supply airflow rate (kg/s).
    pub supply_flow: f64,
    /// Total room height (m).
    pub room_height: f64,
    /// Total convective cooling load in the zone (W).
    /// This is the convective heat gain from internal sources.
    pub cooling_load: f64,
    /// Number of plume sources (affects interface height).
    pub num_plumes: usize,
    /// Humidity ratio (kg/kg) for Cp calculation.
    pub humidity_ratio: f64,
}

impl DisplacementVentModel {
    /// Create a new displacement ventilation model.
    pub fn new(
        supply_temp: f64,
        supply_flow: f64,
        room_height: f64,
        cooling_load: f64,
    ) -> Self {
        Self {
            supply_temp,
            supply_flow,
            room_height,
            cooling_load,
            num_plumes: 1,
            humidity_ratio: 0.008,
        }
    }

    /// Set the number of plume sources.
    pub fn with_plumes(mut self, n: usize) -> Self {
        self.num_plumes = n.max(1);
        self
    }

    /// Calculate the interface height between lower and upper zones.
    ///
    /// Based on a simplified Mundt/Linden model where the interface height
    /// is proportional to (supply_flow / plume_strength)^(3/5) scaled by
    /// room height. More flow pushes the interface up; stronger plumes
    /// push it down (more entrainment).
    ///
    /// h_i = C * H * (Q_supply / Q_plume)^(3/5)
    ///
    /// where C is an empirical constant (~0.7 for single plume in a room)
    /// and Q_plume is the buoyancy flux.
    fn calculate_interface_height(&self) -> f64 {
        if self.supply_flow < 1e-10 || self.cooling_load < 1e-10 {
            // No stratification without both supply and load
            return self.room_height;
        }

        let cp = cp_air(self.humidity_ratio);
        // Temperature difference the supply must absorb
        let delta_t = self.cooling_load / (self.supply_flow * cp).max(1e-10);
        // Buoyancy parameter: g * beta * Q / (rho * Cp)
        // beta ~ 1/T_abs for ideal gas, simplified as delta_t / T_reference
        let t_ref = self.supply_temp + 273.15;
        let buoyancy_per_plume = 9.81 * delta_t / (t_ref * self.num_plumes as f64);

        if buoyancy_per_plume < 1e-10 {
            return self.room_height;
        }

        // Linden filling-box: h_i/H = C * (Q_vol / (B^(1/3) * H^(5/3)))^(3/5)
        // Simplified to a ratio that gives reasonable results:
        // When supply is strong relative to buoyancy, interface is high.
        let air_density = 1.2; // kg/m3 nominal
        let supply_volume = self.supply_flow / air_density;
        let plume_strength = buoyancy_per_plume.powf(1.0 / 3.0) * self.room_height.powf(5.0 / 3.0);

        let ratio = if plume_strength > 1e-10 {
            (supply_volume / plume_strength).powf(0.6)
        } else {
            1.0
        };

        // Scale factor: interface typically at 0.4-0.8 of room height
        let c_empirical = 1.5;
        let h_i = c_empirical * ratio * self.room_height;

        // Clamp to physical range: at least 0.1*H, at most H
        h_i.clamp(0.1 * self.room_height, self.room_height)
    }

    /// Calculate the displacement ventilation zone temperatures.
    pub fn calculate(&self) -> DisplacementVentResult {
        let interface_height = self.calculate_interface_height();

        // Lower zone temperature = supply temperature (fresh air lake)
        let temp_lower = self.supply_temp;

        // Upper zone temperature from energy balance:
        // Q_cooling = m_dot * Cp * (T_upper - T_supply)
        let cp = cp_air(self.humidity_ratio);
        let temp_upper = if self.supply_flow > 1e-10 && cp > 1e-10 {
            self.supply_temp + self.cooling_load / (self.supply_flow * cp)
        } else {
            self.supply_temp
        };

        // Volume-weighted average temperature
        let lower_fraction = interface_height / self.room_height;
        let upper_fraction = 1.0 - lower_fraction;
        let temp_average = temp_lower * lower_fraction + temp_upper * upper_fraction;

        DisplacementVentResult {
            temp_lower,
            temp_upper,
            interface_height,
            temp_average,
        }
    }
}

// ---------------------------------------------------------------------------
// Cross-Ventilation model
// ---------------------------------------------------------------------------

/// Cross-Ventilation model result.
///
/// Two-node model: a jet zone near the inlet and a recirculation zone
/// filling the remainder of the room.
#[derive(Debug, Clone)]
pub struct CrossVentResult {
    /// Temperature of the jet zone near the inlet (C).
    pub temp_jet: f64,
    /// Temperature of the recirculation zone (C).
    pub temp_recirculation: f64,
    /// Average zone temperature (C).
    pub temp_average: f64,
    /// Jet zone volume fraction (0 to 1).
    pub jet_fraction: f64,
}

/// Cross-Ventilation model.
///
/// Models wind-driven cross-ventilation through a room with inlet and
/// outlet openings on opposite walls. Creates two distinct zones:
///
/// 1. **Jet zone**: the air stream from inlet to outlet, carrying outdoor
///    air at roughly outdoor temperature
/// 2. **Recirculation zone**: the rest of the room where air mixes with
///    the jet boundary and internal gains accumulate
///
/// The jet zone fraction depends on inlet geometry and room depth.
/// The recirculation zone temperature depends on mixing with the jet
/// and internal heat gains.
#[derive(Debug, Clone)]
pub struct CrossVentModel {
    /// Inlet opening area (m2).
    pub inlet_area: f64,
    /// Outlet opening area (m2).
    pub outlet_area: f64,
    /// Outdoor air temperature (C).
    pub outdoor_temp: f64,
    /// Wind speed at building height (m/s).
    pub wind_speed: f64,
    /// Wind direction relative to inlet normal (degrees).
    /// 0 = perpendicular to inlet, 90 = parallel (no ventilation).
    pub wind_direction: f64,
    /// Room depth from inlet to outlet wall (m).
    pub room_depth: f64,
    /// Room width perpendicular to flow (m).
    pub room_width: f64,
    /// Room height (m).
    pub room_height: f64,
    /// Internal convective heat gain (W).
    pub internal_gain: f64,
    /// Humidity ratio (kg/kg) for Cp calculation.
    pub humidity_ratio: f64,
}

impl CrossVentModel {
    /// Create a new cross-ventilation model.
    pub fn new(
        inlet_area: f64,
        outlet_area: f64,
        outdoor_temp: f64,
        wind_speed: f64,
        wind_direction: f64,
        room_depth: f64,
        room_width: f64,
        room_height: f64,
        internal_gain: f64,
    ) -> Self {
        Self {
            inlet_area,
            outlet_area,
            outdoor_temp,
            wind_speed,
            wind_direction,
            room_depth,
            room_width,
            room_height,
            internal_gain,
            humidity_ratio: 0.008,
        }
    }

    /// Calculate the effective ventilation mass flow rate through the room.
    ///
    /// Uses a simplified orifice model with wind pressure coefficient:
    /// m_dot = Cd * A_eff * rho * V_wind * cos(theta)
    ///
    /// where A_eff accounts for both inlet and outlet areas in series.
    fn ventilation_flow(&self) -> f64 {
        let cd = 0.65; // discharge coefficient
        let rho = 1.2; // air density kg/m3

        // Effective area for two openings in series:
        // 1/A_eff^2 = 1/A_in^2 + 1/A_out^2
        if self.inlet_area < 1e-10 || self.outlet_area < 1e-10 {
            return 0.0;
        }
        let a_eff_sq = 1.0 / (1.0 / (self.inlet_area * self.inlet_area)
            + 1.0 / (self.outlet_area * self.outlet_area));
        let a_eff = a_eff_sq.sqrt();

        // Wind pressure component (cos of angle, clamped to non-negative)
        let angle_rad = self.wind_direction.to_radians();
        let cos_theta = angle_rad.cos().max(0.0);

        cd * a_eff * rho * self.wind_speed * cos_theta
    }

    /// Calculate the jet zone volume fraction.
    ///
    /// The jet expands from the inlet with a half-angle of about 12-15 degrees.
    /// The jet fraction is the ratio of jet volume to total room volume.
    fn jet_volume_fraction(&self) -> f64 {
        let room_volume = self.room_depth * self.room_width * self.room_height;
        if room_volume < 1e-10 || self.inlet_area < 1e-10 {
            return 0.0;
        }

        // Jet half-angle ~13 degrees -> expansion rate tan(13) ~ 0.23
        let expansion_rate = 0.23;
        // Jet width at room depth (starting from inlet hydraulic diameter)
        let inlet_diameter = (4.0 * self.inlet_area / std::f64::consts::PI).sqrt();
        let jet_width_at_exit = inlet_diameter + 2.0 * expansion_rate * self.room_depth;
        let jet_height_at_exit = inlet_diameter + 2.0 * expansion_rate * self.room_depth;

        // Average jet cross-section (trapezoidal expansion)
        let avg_width = (inlet_diameter + jet_width_at_exit) / 2.0;
        let avg_height = (inlet_diameter + jet_height_at_exit) / 2.0;
        let jet_volume = avg_width * avg_height * self.room_depth;

        (jet_volume / room_volume).clamp(0.0, 0.95)
    }

    /// Calculate cross-ventilation zone temperatures.
    pub fn calculate(&self) -> CrossVentResult {
        let m_dot = self.ventilation_flow();
        let jet_fraction = self.jet_volume_fraction();

        // Jet zone: outdoor air temperature (the jet carries outdoor air)
        let temp_jet = self.outdoor_temp;

        // Recirculation zone: mixing of jet entrainment + internal gains
        let cp = cp_air(self.humidity_ratio);
        let temp_recirculation = if m_dot > 1e-10 && cp > 1e-10 {
            // The recirculation zone receives heat from internal gains
            // and mixes with jet air. The mixing rate is proportional to
            // the entrainment from the jet boundary.
            //
            // Energy balance on recirculation zone:
            // m_dot_entrain * Cp * (T_recirc - T_jet) = Q_internal * (1 - jet_fraction)
            //
            // Entrainment flow ~ 0.5 * m_dot for typical jet expansion
            let entrainment_fraction = 0.5;
            let m_dot_entrain = m_dot * entrainment_fraction;
            let q_recirc = self.internal_gain * (1.0 - jet_fraction);

            if m_dot_entrain > 1e-10 {
                self.outdoor_temp + q_recirc / (m_dot_entrain * cp)
            } else {
                self.outdoor_temp
            }
        } else {
            // No ventilation: room heats up without limit (return outdoor as baseline)
            // In practice the zone solver handles this case.
            self.outdoor_temp
        };

        // Volume-weighted average
        let temp_average = temp_jet * jet_fraction + temp_recirculation * (1.0 - jet_fraction);

        CrossVentResult {
            temp_jet,
            temp_recirculation,
            temp_average,
            jet_fraction,
        }
    }
}

// ---------------------------------------------------------------------------
// User-Defined Pattern
// ---------------------------------------------------------------------------

/// User-defined temperature pattern result at a queried height.
#[derive(Debug, Clone)]
pub struct PatternResult {
    /// Temperature offset at the queried height (K or C-delta).
    pub offset: f64,
    /// Absolute temperature at the queried height (C), if a base temp was given.
    pub temperature: f64,
}

/// User-defined room air temperature pattern.
///
/// Stores a set of (height, temperature_offset) pairs that define how the
/// air temperature varies with height in the zone. Offsets are relative to
/// the zone mean air temperature.
///
/// Points are sorted by height on construction. Linear interpolation is used
/// between specified points. Below the lowest point, the lowest offset is
/// used; above the highest point, the highest offset is used.
#[derive(Debug, Clone)]
pub struct UserDefinedPattern {
    /// Sorted (height_m, offset_K) pairs defining the temperature profile.
    /// Heights in meters from floor, offsets in K (or C-delta) from zone mean.
    points: Vec<(f64, f64)>,
}

impl UserDefinedPattern {
    /// Create a new user-defined pattern from (height, offset) pairs.
    ///
    /// Points are sorted by height. Duplicate heights are preserved (the
    /// first encountered will be used for interpolation).
    ///
    /// # Arguments
    /// * `points` - Vec of (height_m, offset_K) tuples.
    ///   Height in meters from floor level, offset in K from zone mean.
    pub fn new(mut points: Vec<(f64, f64)>) -> Self {
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        Self { points }
    }

    /// Return the number of defined points.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Return true if no points are defined.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Get the stored points (sorted by height).
    pub fn points(&self) -> &[(f64, f64)] {
        &self.points
    }

    /// Interpolate the temperature offset at a given height.
    ///
    /// Uses linear interpolation between bracketing points.
    /// Extrapolates as constant beyond the defined range.
    ///
    /// Returns 0.0 if no points are defined.
    pub fn offset_at_height(&self, height: f64) -> f64 {
        if self.points.is_empty() {
            return 0.0;
        }

        // Below lowest point
        if height <= self.points[0].0 {
            return self.points[0].1;
        }

        // Above highest point
        let last = self.points.len() - 1;
        if height >= self.points[last].0 {
            return self.points[last].1;
        }

        // Find bracketing interval
        for i in 0..last {
            let (h0, t0) = self.points[i];
            let (h1, t1) = self.points[i + 1];
            if height >= h0 && height <= h1 {
                let dh = h1 - h0;
                if dh.abs() < 1e-15 {
                    return t0;
                }
                let frac = (height - h0) / dh;
                return t0 + frac * (t1 - t0);
            }
        }

        // Fallback (should not reach here)
        self.points[last].1
    }

    /// Get absolute temperature at a height given a zone mean temperature.
    ///
    /// # Arguments
    /// * `height` - Height from floor (m)
    /// * `zone_mean_temp` - Zone mean air temperature (C)
    pub fn temperature_at_height(&self, height: f64, zone_mean_temp: f64) -> PatternResult {
        let offset = self.offset_at_height(height);
        PatternResult {
            offset,
            temperature: zone_mean_temp + offset,
        }
    }

    /// Verify that the pattern is energy-consistent: the height-averaged
    /// offset should be approximately zero (offsets balanced around the mean).
    ///
    /// Returns the integrated average offset. Values near zero indicate
    /// consistency with the zone mean temperature assumption.
    pub fn average_offset(&self) -> f64 {
        if self.points.len() < 2 {
            return if self.points.is_empty() {
                0.0
            } else {
                self.points[0].1
            };
        }

        // Trapezoidal integration of offset over height
        let mut integral = 0.0;
        for i in 0..self.points.len() - 1 {
            let (h0, t0) = self.points[i];
            let (h1, t1) = self.points[i + 1];
            integral += 0.5 * (t0 + t1) * (h1 - h0);
        }

        let total_height = self.points.last().unwrap().0 - self.points[0].0;
        if total_height.abs() < 1e-15 {
            return 0.0;
        }

        integral / total_height
    }
}

// ---------------------------------------------------------------------------
// Room air model enum for dispatching
// ---------------------------------------------------------------------------

/// Room air model type selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoomAirModelType {
    /// Well-mixed (standard single-node assumption).
    #[default]
    WellMixed,
    /// UFAD (underfloor air distribution) 3-zone model.
    Ufad,
    /// Displacement ventilation 2-zone model.
    DisplacementVentilation,
    /// Cross-ventilation 2-node (jet + recirculation) model.
    CrossVentilation,
    /// User-defined temperature pattern.
    UserDefined,
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // UFAD model tests
    // -----------------------------------------------------------------------

    #[test]
    fn ufad_basic_stratification() {
        let model = UfadModel::new(
            16.0, // supply temp
            3.0,  // room height
            100.0, // floor area
            1.0,  // transition height
            200.0, // power per plume
            5,    // 5 plumes = 1000W total
            0.5,  // supply flow kg/s
        );
        let result = model.calculate();

        // Lower zone should be near supply temp (with small offset)
        assert!(result.temp_lower > 16.0, "T_lower={}", result.temp_lower);
        assert!(result.temp_lower < 18.0, "T_lower too high: {}", result.temp_lower);

        // Upper zones should be warmer
        assert!(result.temp_upper_occupied > result.temp_lower,
            "T_upper_occ={} should > T_lower={}", result.temp_upper_occupied, result.temp_lower);
        assert!(result.temp_upper_room >= result.temp_upper_occupied,
            "T_upper_room={} should >= T_upper_occ={}", result.temp_upper_room, result.temp_upper_occupied);
    }

    #[test]
    fn ufad_gradient_increases_with_load() {
        let low_load = UfadModel::new(16.0, 3.0, 100.0, 1.0, 100.0, 4, 0.5);
        let high_load = UfadModel::new(16.0, 3.0, 100.0, 1.0, 400.0, 4, 0.5);

        let gamma_low = low_load.calculate_gradient();
        let gamma_high = high_load.calculate_gradient();

        assert!(gamma_high > gamma_low,
            "High load gradient {} should exceed low load {}", gamma_high, gamma_low);
    }

    #[test]
    fn ufad_gradient_decreases_with_flow() {
        let low_flow = UfadModel::new(16.0, 3.0, 100.0, 1.0, 200.0, 5, 0.3);
        let high_flow = UfadModel::new(16.0, 3.0, 100.0, 1.0, 200.0, 5, 1.0);

        let gamma_low = low_flow.calculate_gradient();
        let gamma_high = high_flow.calculate_gradient();

        assert!(gamma_low > gamma_high,
            "Low flow gradient {} should exceed high flow {}", gamma_low, gamma_high);
    }

    #[test]
    fn ufad_zero_flow_no_stratification() {
        let model = UfadModel::new(16.0, 3.0, 100.0, 1.0, 200.0, 5, 0.0);
        let result = model.calculate();

        assert!((result.gradient).abs() < 1e-10, "gradient={}", result.gradient);
        assert!((result.temp_lower - result.temp_upper_room).abs() < 1e-10);
    }

    #[test]
    fn ufad_temperature_at_height_below_transition() {
        let model = UfadModel::new(16.0, 3.0, 100.0, 1.0, 200.0, 5, 0.5);
        let t_floor = model.temperature_at_height(0.0);
        let t_mid_lower = model.temperature_at_height(0.5);
        let result = model.calculate();

        // Both below transition should return lower zone temp
        assert!((t_floor - result.temp_lower).abs() < 1e-10);
        assert!((t_mid_lower - result.temp_lower).abs() < 1e-10);
    }

    #[test]
    fn ufad_temperature_at_height_above_transition() {
        let model = UfadModel::new(16.0, 3.0, 100.0, 1.0, 200.0, 5, 0.5);
        let result = model.calculate();

        let t_above = model.temperature_at_height(2.0);
        let expected = result.temp_lower + result.gradient * (2.0 - 1.0);
        assert!((t_above - expected).abs() < 1e-10,
            "t_above={}, expected={}", t_above, expected);
    }

    #[test]
    fn ufad_temperature_clamped_at_ceiling() {
        let model = UfadModel::new(16.0, 3.0, 100.0, 1.0, 200.0, 5, 0.5);
        let result = model.calculate();

        // Height above room height should be clamped
        let t_above_ceiling = model.temperature_at_height(5.0);
        assert!((t_above_ceiling - result.temp_upper_room).abs() < 1e-10);
    }

    #[test]
    fn ufad_energy_balance_approximate() {
        // The total heat removed by supply air should approximately equal the gains
        let model = UfadModel::new(16.0, 3.0, 100.0, 1.0, 200.0, 5, 0.5);
        let result = model.calculate();
        let cp = cp_air(0.008);

        // Q = m_dot * Cp * (T_return - T_supply), where T_return ~ T_upper_room
        let q_removed = model.supply_flow * cp * (result.temp_upper_room - model.supply_temp);
        let q_total = 200.0 * 5.0; // total internal gains

        // They should be of the same order of magnitude
        assert!(q_removed > 0.0, "q_removed={}", q_removed);
        // The return temp accounts for all gains, so q_removed should approach q_total
        // allowing for the recirculation fraction going to lower zone
        assert!((q_removed - q_total).abs() < q_total * 0.5,
            "q_removed={}, q_total={}", q_removed, q_total);
    }

    // -----------------------------------------------------------------------
    // Displacement Ventilation model tests
    // -----------------------------------------------------------------------

    #[test]
    fn displacement_basic_stratification() {
        let model = DisplacementVentModel::new(
            16.0, // supply temp
            0.5,  // supply flow kg/s
            3.0,  // room height
            2000.0, // cooling load W
        );
        let result = model.calculate();

        // Lower zone should be at supply temperature
        assert!((result.temp_lower - 16.0).abs() < 1e-10,
            "T_lower={}", result.temp_lower);

        // Upper zone should be warmer than lower
        assert!(result.temp_upper > result.temp_lower,
            "T_upper={} should > T_lower={}", result.temp_upper, result.temp_lower);

        // Interface height should be within room
        assert!(result.interface_height > 0.0 && result.interface_height <= 3.0,
            "h_i={}", result.interface_height);
    }

    #[test]
    fn displacement_upper_temp_energy_balance() {
        let model = DisplacementVentModel::new(16.0, 0.5, 3.0, 2000.0);
        let result = model.calculate();
        let cp = cp_air(0.008);

        // T_upper = T_supply + Q / (m_dot * Cp)
        let expected_upper = 16.0 + 2000.0 / (0.5 * cp);
        assert!((result.temp_upper - expected_upper).abs() < 0.01,
            "T_upper={}, expected={}", result.temp_upper, expected_upper);
    }

    #[test]
    fn displacement_zero_load() {
        let model = DisplacementVentModel::new(16.0, 0.5, 3.0, 0.0);
        let result = model.calculate();

        // With no cooling load, upper = lower = supply
        assert!((result.temp_upper - result.temp_lower).abs() < 1e-10);
        assert!((result.temp_average - 16.0).abs() < 1e-10);
    }

    #[test]
    fn displacement_zero_flow() {
        let model = DisplacementVentModel::new(16.0, 0.0, 3.0, 2000.0);
        let result = model.calculate();

        // No flow: temps stay at supply, interface at full height
        assert!((result.temp_upper - 16.0).abs() < 1e-10);
        assert!((result.interface_height - 3.0).abs() < 1e-10);
    }

    #[test]
    fn displacement_average_between_zones() {
        let model = DisplacementVentModel::new(16.0, 0.5, 3.0, 2000.0);
        let result = model.calculate();

        // Average should be between lower and upper
        assert!(result.temp_average >= result.temp_lower,
            "avg={} < lower={}", result.temp_average, result.temp_lower);
        assert!(result.temp_average <= result.temp_upper,
            "avg={} > upper={}", result.temp_average, result.temp_upper);
    }

    #[test]
    fn displacement_higher_load_higher_upper_temp() {
        let low = DisplacementVentModel::new(16.0, 0.5, 3.0, 1000.0);
        let high = DisplacementVentModel::new(16.0, 0.5, 3.0, 3000.0);

        let r_low = low.calculate();
        let r_high = high.calculate();

        assert!(r_high.temp_upper > r_low.temp_upper,
            "High load T_upper={} should > low load T_upper={}",
            r_high.temp_upper, r_low.temp_upper);
    }

    #[test]
    fn displacement_higher_flow_raises_interface() {
        let low_flow = DisplacementVentModel::new(16.0, 0.2, 3.0, 2000.0);
        let high_flow = DisplacementVentModel::new(16.0, 1.0, 3.0, 2000.0);

        let r_low = low_flow.calculate();
        let r_high = high_flow.calculate();

        assert!(r_high.interface_height >= r_low.interface_height,
            "High flow h_i={} should >= low flow h_i={}",
            r_high.interface_height, r_low.interface_height);
    }

    #[test]
    fn displacement_with_plumes() {
        let single = DisplacementVentModel::new(16.0, 0.5, 3.0, 2000.0);
        let multi = DisplacementVentModel::new(16.0, 0.5, 3.0, 2000.0).with_plumes(4);

        let r_single = single.calculate();
        let r_multi = multi.calculate();

        // Same total load but distributed across more plumes: upper temp should be the same
        // (energy balance unchanged), but interface height may differ
        assert!((r_single.temp_upper - r_multi.temp_upper).abs() < 0.01,
            "Same load should give same upper temp: single={}, multi={}",
            r_single.temp_upper, r_multi.temp_upper);
    }

    // -----------------------------------------------------------------------
    // Cross-Ventilation model tests
    // -----------------------------------------------------------------------

    #[test]
    fn cross_vent_basic() {
        let model = CrossVentModel::new(
            1.0,   // inlet area m2
            1.0,   // outlet area m2
            25.0,  // outdoor temp C
            3.0,   // wind speed m/s
            0.0,   // wind perpendicular to inlet
            6.0,   // room depth m
            4.0,   // room width m
            3.0,   // room height m
            1000.0, // internal gain W
        );
        let result = model.calculate();

        // Jet zone should be at outdoor temp
        assert!((result.temp_jet - 25.0).abs() < 1e-10);

        // Recirculation should be warmer than jet (due to internal gains)
        assert!(result.temp_recirculation >= result.temp_jet,
            "T_recirc={} should >= T_jet={}", result.temp_recirculation, result.temp_jet);

        // Jet fraction should be reasonable
        assert!(result.jet_fraction > 0.0 && result.jet_fraction < 1.0,
            "jet_fraction={}", result.jet_fraction);
    }

    #[test]
    fn cross_vent_parallel_wind_no_flow() {
        let model = CrossVentModel::new(
            1.0, 1.0, 25.0,
            3.0,   // wind speed
            90.0,  // parallel to inlet -> cos(90) = 0
            6.0, 4.0, 3.0,
            1000.0,
        );
        let result = model.calculate();

        // With wind parallel to inlet, no ventilation flow
        // Recirculation temp should equal outdoor (our fallback)
        assert!((result.temp_recirculation - 25.0).abs() < 1e-10);
    }

    #[test]
    fn cross_vent_zero_wind() {
        let model = CrossVentModel::new(
            1.0, 1.0, 25.0,
            0.0,  // no wind
            0.0,
            6.0, 4.0, 3.0,
            1000.0,
        );
        let result = model.calculate();

        // No wind = no flow = fallback temps
        assert!((result.temp_jet - 25.0).abs() < 1e-10);
        assert!((result.temp_recirculation - 25.0).abs() < 1e-10);
    }

    #[test]
    fn cross_vent_higher_gain_warmer_recirc() {
        let low_gain = CrossVentModel::new(
            1.0, 1.0, 25.0, 3.0, 0.0, 6.0, 4.0, 3.0, 500.0,
        );
        let high_gain = CrossVentModel::new(
            1.0, 1.0, 25.0, 3.0, 0.0, 6.0, 4.0, 3.0, 3000.0,
        );

        let r_low = low_gain.calculate();
        let r_high = high_gain.calculate();

        assert!(r_high.temp_recirculation > r_low.temp_recirculation,
            "High gain T_recirc={} should > low gain {}",
            r_high.temp_recirculation, r_low.temp_recirculation);
    }

    #[test]
    fn cross_vent_larger_inlet_more_jet() {
        let small_inlet = CrossVentModel::new(
            0.3, 1.0, 25.0, 3.0, 0.0, 6.0, 4.0, 3.0, 1000.0,
        );
        let large_inlet = CrossVentModel::new(
            2.0, 2.0, 25.0, 3.0, 0.0, 6.0, 4.0, 3.0, 1000.0,
        );

        let r_small = small_inlet.calculate();
        let r_large = large_inlet.calculate();

        assert!(r_large.jet_fraction > r_small.jet_fraction,
            "Larger inlet jet_fraction={} should > small inlet {}",
            r_large.jet_fraction, r_small.jet_fraction);
    }

    #[test]
    fn cross_vent_average_between_zones() {
        let model = CrossVentModel::new(
            1.0, 1.0, 25.0, 3.0, 0.0, 6.0, 4.0, 3.0, 1000.0,
        );
        let result = model.calculate();

        // Average should be between jet and recirculation
        let t_min = result.temp_jet.min(result.temp_recirculation);
        let t_max = result.temp_jet.max(result.temp_recirculation);
        assert!(result.temp_average >= t_min - 1e-10 && result.temp_average <= t_max + 1e-10,
            "avg={} not between jet={} and recirc={}",
            result.temp_average, result.temp_jet, result.temp_recirculation);
    }

    #[test]
    fn cross_vent_ventilation_flow_scales_with_wind() {
        let slow = CrossVentModel::new(
            1.0, 1.0, 25.0, 1.0, 0.0, 6.0, 4.0, 3.0, 0.0,
        );
        let fast = CrossVentModel::new(
            1.0, 1.0, 25.0, 5.0, 0.0, 6.0, 4.0, 3.0, 0.0,
        );

        let flow_slow = slow.ventilation_flow();
        let flow_fast = fast.ventilation_flow();

        assert!(flow_fast > flow_slow,
            "Higher wind flow {} should > lower {}", flow_fast, flow_slow);
        // Linear relationship: 5x wind = 5x flow
        assert!((flow_fast / flow_slow - 5.0).abs() < 0.01);
    }

    #[test]
    fn cross_vent_zero_inlet_area() {
        let model = CrossVentModel::new(
            0.0, 1.0, 25.0, 3.0, 0.0, 6.0, 4.0, 3.0, 1000.0,
        );
        let flow = model.ventilation_flow();
        assert!(flow.abs() < 1e-10, "Zero inlet should give zero flow");

        let result = model.calculate();
        assert!(result.jet_fraction.abs() < 1e-10, "Zero inlet: no jet");
    }

    // -----------------------------------------------------------------------
    // UserDefinedPattern tests
    // -----------------------------------------------------------------------

    #[test]
    fn pattern_empty() {
        let pattern = UserDefinedPattern::new(vec![]);
        assert!(pattern.is_empty());
        assert_eq!(pattern.len(), 0);
        assert!((pattern.offset_at_height(1.0)).abs() < 1e-10);
    }

    #[test]
    fn pattern_single_point() {
        let pattern = UserDefinedPattern::new(vec![(1.5, 2.0)]);
        assert_eq!(pattern.len(), 1);

        // Any height returns the single offset
        assert!((pattern.offset_at_height(0.0) - 2.0).abs() < 1e-10);
        assert!((pattern.offset_at_height(1.5) - 2.0).abs() < 1e-10);
        assert!((pattern.offset_at_height(3.0) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn pattern_linear_interpolation() {
        let pattern = UserDefinedPattern::new(vec![
            (0.0, -1.0),
            (3.0, 2.0),
        ]);

        // At midpoint: -1.0 + 0.5 * (2.0 - (-1.0)) = 0.5
        let offset = pattern.offset_at_height(1.5);
        assert!((offset - 0.5).abs() < 1e-10, "offset={}", offset);

        // At 1/3: -1.0 + 1/3 * 3.0 = 0.0
        let offset_third = pattern.offset_at_height(1.0);
        assert!((offset_third - 0.0).abs() < 1e-10, "offset={}", offset_third);
    }

    #[test]
    fn pattern_extrapolation_constant() {
        let pattern = UserDefinedPattern::new(vec![
            (1.0, -0.5),
            (2.0, 1.5),
        ]);

        // Below range: use lowest point's offset
        assert!((pattern.offset_at_height(0.0) - (-0.5)).abs() < 1e-10);
        // Above range: use highest point's offset
        assert!((pattern.offset_at_height(5.0) - 1.5).abs() < 1e-10);
    }

    #[test]
    fn pattern_sorted_on_construction() {
        // Points given out of order
        let pattern = UserDefinedPattern::new(vec![
            (3.0, 3.0),
            (0.0, -1.0),
            (1.5, 1.0),
        ]);

        let points = pattern.points();
        assert!((points[0].0 - 0.0).abs() < 1e-10);
        assert!((points[1].0 - 1.5).abs() < 1e-10);
        assert!((points[2].0 - 3.0).abs() < 1e-10);
    }

    #[test]
    fn pattern_multi_segment_interpolation() {
        let pattern = UserDefinedPattern::new(vec![
            (0.0, -2.0),
            (1.0, -1.0),
            (2.0, 0.0),
            (3.0, 2.0),
        ]);

        // First segment: 0.5m -> -2.0 + 0.5*(-1.0 - (-2.0)) = -1.5
        assert!((pattern.offset_at_height(0.5) - (-1.5)).abs() < 1e-10);

        // Second segment: 1.5m -> -1.0 + 0.5*(0.0 - (-1.0)) = -0.5
        assert!((pattern.offset_at_height(1.5) - (-0.5)).abs() < 1e-10);

        // Third segment: 2.5m -> 0.0 + 0.5*(2.0 - 0.0) = 1.0
        assert!((pattern.offset_at_height(2.5) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn pattern_temperature_at_height() {
        let pattern = UserDefinedPattern::new(vec![
            (0.0, -1.0),
            (3.0, 2.0),
        ]);

        let zone_mean = 22.0;
        let result = pattern.temperature_at_height(1.5, zone_mean);

        // Offset at 1.5m = 0.5 (midpoint interpolation)
        assert!((result.offset - 0.5).abs() < 1e-10);
        assert!((result.temperature - 22.5).abs() < 1e-10);
    }

    #[test]
    fn pattern_average_offset_balanced() {
        // Symmetric pattern: offsets should average to zero
        let pattern = UserDefinedPattern::new(vec![
            (0.0, -2.0),
            (3.0, 2.0),
        ]);

        let avg = pattern.average_offset();
        assert!(avg.abs() < 1e-10, "avg={}", avg);
    }

    #[test]
    fn pattern_average_offset_unbalanced() {
        // All positive offsets: average should be positive
        let pattern = UserDefinedPattern::new(vec![
            (0.0, 1.0),
            (3.0, 3.0),
        ]);

        let avg = pattern.average_offset();
        // Trapezoidal: 0.5 * (1.0 + 3.0) * 3.0 / 3.0 = 2.0
        assert!((avg - 2.0).abs() < 1e-10, "avg={}", avg);
    }

    #[test]
    fn pattern_exact_at_defined_points() {
        let pattern = UserDefinedPattern::new(vec![
            (0.0, -1.0),
            (1.0, 0.0),
            (2.0, 1.5),
            (3.0, 2.0),
        ]);

        assert!((pattern.offset_at_height(0.0) - (-1.0)).abs() < 1e-10);
        assert!((pattern.offset_at_height(1.0) - 0.0).abs() < 1e-10);
        assert!((pattern.offset_at_height(2.0) - 1.5).abs() < 1e-10);
        assert!((pattern.offset_at_height(3.0) - 2.0).abs() < 1e-10);
    }

    // -----------------------------------------------------------------------
    // RoomAirModelType tests
    // -----------------------------------------------------------------------

    #[test]
    fn room_air_model_type_default() {
        let model: RoomAirModelType = Default::default();
        assert_eq!(model, RoomAirModelType::WellMixed);
    }

    #[test]
    fn room_air_model_type_variants() {
        // Verify all variants exist and are distinct
        let types = [
            RoomAirModelType::WellMixed,
            RoomAirModelType::Ufad,
            RoomAirModelType::DisplacementVentilation,
            RoomAirModelType::CrossVentilation,
            RoomAirModelType::UserDefined,
        ];
        for i in 0..types.len() {
            for j in (i + 1)..types.len() {
                assert_ne!(types[i], types[j]);
            }
        }
    }
}
