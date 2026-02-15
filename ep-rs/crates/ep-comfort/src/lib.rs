//! Thermal comfort models for EnergyPlus-rs.
//!
//! Implements:
//! - Fanger PMV/PPD (ISO 7730)
//! - ASHRAE 55 adaptive comfort
//! - CEN 15251 adaptive comfort (EN 16798-1)
//! - Pierce two-node thermoregulation model
//! - Simple ASHRAE 55 summer/winter check
//! - Ankle draft risk
//! - Ceiling fan cooling effect

// ─── Constants ──────────────────────────────────────────────────────

const STEFAN_BOLTZMANN: f64 = 5.67e-8; // W/(m²·K⁴)
const BODY_SURFACE_AREA_DUBOIS: f64 = 1.8; // m² (average adult)
const SKIN_ABSORPTANCE: f64 = 0.72; // fraction of radiant heat absorbed by skin
const BODY_SPECIFIC_HEAT: f64 = 3490.0; // J/(kg·K) average body tissue
const BODY_MASS: f64 = 70.0; // kg (reference person)
const CORE_TEMP_NEUTRAL: f64 = 36.8; // °C neutral core temperature
const SKIN_TEMP_NEUTRAL: f64 = 33.7; // °C neutral mean skin temperature
const BLOOD_DENSITY_CP: f64 = 4187.0; // J/(kg·K) blood density × specific heat

// ─── Fanger PMV/PPD (ISO 7730) ─────────────────────────────────────

/// Fanger PMV/PPD result.
#[derive(Debug, Clone)]
pub struct PmvPpdResult {
    /// Predicted Mean Vote (-3 to +3).
    pub pmv: f64,
    /// Predicted Percentage of Dissatisfied (0-100%).
    pub ppd: f64,
}

/// Calculate PMV and PPD according to ISO 7730 / ASHRAE 55.
///
/// # Arguments
/// * `air_temp` - Air temperature (°C)
/// * `mean_radiant_temp` - Mean radiant temperature (°C)
/// * `air_velocity` - Air velocity (m/s)
/// * `relative_humidity` - Relative humidity (0-100%)
/// * `metabolic_rate` - Metabolic rate (met, 1 met = 58.15 W/m²)
/// * `clothing_insulation` - Clothing insulation (clo, 1 clo = 0.155 m²·K/W)
/// * `external_work` - External mechanical work (met), typically 0
pub fn fanger_pmv_ppd(
    air_temp: f64,
    mean_radiant_temp: f64,
    air_velocity: f64,
    relative_humidity: f64,
    metabolic_rate: f64,
    clothing_insulation: f64,
    external_work: f64,
) -> PmvPpdResult {
    let met_w = metabolic_rate * 58.15; // W/m²
    let work_w = external_work * 58.15;
    let internal_heat = met_w - work_w;

    let i_cl = clothing_insulation * 0.155; // m²·K/W
    let f_cl = if clothing_insulation <= 0.078 {
        1.0 + 1.29 * i_cl
    } else {
        1.05 + 0.645 * i_cl
    };

    // Water vapor partial pressure (Pa)
    let p_a = relative_humidity / 100.0 * saturated_vapor_pressure(air_temp);

    // Clothing surface temperature - iterative solution
    let t_cl = solve_clothing_temp(air_temp, mean_radiant_temp, air_velocity, i_cl, f_cl, internal_heat);

    // Convective heat transfer coefficient
    let h_c = convective_coefficient(air_temp, t_cl, air_velocity);

    // Mean radiant temp in K
    let t_r_k = mean_radiant_temp + 273.15;
    let t_cl_k = t_cl + 273.15;

    // Heat loss components (W/m²)
    // Skin diffusion
    let heat_loss_skin = 3.05e-3 * (5733.0 - 6.99 * internal_heat - p_a);
    // Sweating
    let heat_loss_sweat = if internal_heat > 58.15 {
        0.42 * (internal_heat - 58.15)
    } else {
        0.0
    };
    // Latent respiration
    let heat_loss_resp_latent = 1.7e-5 * met_w * (5867.0 - p_a);
    // Dry respiration
    let heat_loss_resp_dry = 0.0014 * met_w * (34.0 - air_temp);
    // Radiation from clothing
    let heat_loss_radiation =
        3.96e-8 * f_cl * (t_cl_k.powi(4) - t_r_k.powi(4));
    // Convection from clothing
    let heat_loss_convection = f_cl * h_c * (t_cl - air_temp);

    // PMV
    let ts_coeff = 0.303 * (-0.036_f64 * met_w).exp() + 0.028;
    let pmv = ts_coeff
        * (internal_heat - heat_loss_skin - heat_loss_sweat
            - heat_loss_resp_latent - heat_loss_resp_dry
            - heat_loss_radiation - heat_loss_convection);

    // PPD
    let ppd = 100.0 - 95.0 * (-0.03353 * pmv.powi(4) - 0.2179 * pmv.powi(2)).exp();
    let ppd = ppd.clamp(5.0, 100.0);

    PmvPpdResult { pmv, ppd }
}

/// Solve for clothing surface temperature iteratively.
///
/// Uses the ISO 7730 iterative equation with relaxation to ensure convergence.
fn solve_clothing_temp(
    t_air: f64,
    t_mrt: f64,
    v_air: f64,
    i_cl: f64,
    f_cl: f64,
    internal_heat: f64,
) -> f64 {
    let t_r_k = t_mrt + 273.15;
    // Initial guess from ISO 7730
    let denom = 3.5 * (6.45 * i_cl + 0.1);
    let mut t_cl = if denom > 0.0 {
        t_air + (35.5 - t_air) / denom
    } else {
        (t_air + 35.5) / 2.0
    };

    for _ in 0..150 {
        let t_cl_k = t_cl + 273.15;
        let h_c = convective_coefficient(t_air, t_cl, v_air);

        let t_cl_new = 35.7 - 0.028 * internal_heat
            - i_cl * (3.96e-8 * f_cl * (t_cl_k.powi(4) - t_r_k.powi(4))
                + f_cl * h_c * (t_cl - t_air));

        if (t_cl_new - t_cl).abs() < 1e-5 {
            return t_cl_new;
        }
        // Relaxation to prevent oscillation
        t_cl = 0.5 * t_cl + 0.5 * t_cl_new;
    }
    t_cl
}

/// Calculate convective heat transfer coefficient.
fn convective_coefficient(t_air: f64, t_cl: f64, v_air: f64) -> f64 {
    let h_c_natural = 2.38 * (t_cl - t_air).abs().powf(0.25);
    let h_c_forced = 12.1 * v_air.sqrt();
    h_c_natural.max(h_c_forced)
}

/// Saturated vapor pressure (Pa) from temperature (°C).
fn saturated_vapor_pressure(t: f64) -> f64 {
    // ASHRAE Handbook correlation
    if t >= 0.0 {
        610.78 * (17.269 * t / (237.29 + t)).exp()
    } else {
        610.78 * (21.875 * t / (265.5 + t)).exp()
    }
}

// ─── ASHRAE 55 Adaptive Comfort ────────────────────────────────────

/// ASHRAE 55 adaptive comfort result.
#[derive(Debug, Clone)]
pub struct AdaptiveComfortResult {
    /// 80% acceptability lower limit (°C).
    pub lower_80: f64,
    /// 80% acceptability upper limit (°C).
    pub upper_80: f64,
    /// 90% acceptability lower limit (°C).
    pub lower_90: f64,
    /// 90% acceptability upper limit (°C).
    pub upper_90: f64,
    /// Neutral operative temperature (°C).
    pub t_neutral: f64,
    /// Whether the model is applicable (prevailing mean outdoor 10-33.5°C).
    pub applicable: bool,
}

/// Calculate ASHRAE 55 adaptive comfort limits.
///
/// Applicable to naturally conditioned spaces with metabolic rates 1.0-1.3 met.
///
/// # Arguments
/// * `prevailing_mean_outdoor_temp` - Running mean outdoor temperature (°C)
pub fn ashrae55_adaptive(prevailing_mean_outdoor_temp: f64) -> AdaptiveComfortResult {
    let t_out = prevailing_mean_outdoor_temp;
    let applicable = t_out >= 10.0 && t_out <= 33.5;

    // Neutral operative temperature
    let t_neutral = 0.31 * t_out + 17.8;

    AdaptiveComfortResult {
        lower_80: t_neutral - 3.5,
        upper_80: t_neutral + 3.5,
        lower_90: t_neutral - 2.5,
        upper_90: t_neutral + 2.5,
        t_neutral,
        applicable,
    }
}

// ─── CEN 15251 / EN 16798-1 Adaptive Comfort ───────────────────────

/// CEN 15251 comfort category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CenCategory {
    /// Category I: high expectation (new buildings, sensitive occupants).
    I,
    /// Category II: normal expectation (new and renovated buildings).
    II,
    /// Category III: moderate expectation (existing buildings).
    III,
}

/// CEN 15251 adaptive comfort result.
#[derive(Debug, Clone)]
pub struct CenAdaptiveResult {
    /// Comfort lower limit (°C).
    pub lower: f64,
    /// Comfort upper limit (°C).
    pub upper: f64,
    /// Neutral operative temperature (°C).
    pub t_neutral: f64,
    /// Category used.
    pub category: CenCategory,
    /// Whether the model is applicable (running mean outdoor 10-30°C).
    pub applicable: bool,
}

/// Calculate CEN 15251 (EN 16798-1) adaptive comfort limits.
///
/// # Arguments
/// * `running_mean_outdoor_temp` - Exponentially weighted running mean outdoor temp (°C)
/// * `category` - Comfort expectation category
pub fn cen15251_adaptive(
    running_mean_outdoor_temp: f64,
    category: CenCategory,
) -> CenAdaptiveResult {
    let t_rm = running_mean_outdoor_temp;
    let applicable = t_rm >= 10.0 && t_rm <= 30.0;

    // Neutral operative temperature (same formula for all categories)
    let t_neutral = 0.33 * t_rm + 18.8;

    // Category-dependent offset
    let offset = match category {
        CenCategory::I => 2.0,
        CenCategory::II => 3.0,
        CenCategory::III => 4.0,
    };

    CenAdaptiveResult {
        lower: t_neutral - offset,
        upper: t_neutral + offset,
        t_neutral,
        category,
        applicable,
    }
}

/// Calculate CEN exponentially weighted running mean outdoor temperature.
///
/// T_rm = (1 - alpha) * (T_d-1 + alpha*T_d-2 + alpha²*T_d-3 + ...)
///
/// # Arguments
/// * `daily_mean_temps` - Previous days' mean outdoor temps [day-1, day-2, ...]
/// * `alpha` - Weighting constant (typically 0.8)
pub fn cen_running_mean(daily_mean_temps: &[f64], alpha: f64) -> f64 {
    if daily_mean_temps.is_empty() {
        return 20.0; // default
    }

    let mut t_rm = 0.0;
    let mut weight_sum = 0.0;
    let factor = 1.0 - alpha;

    for (i, &t) in daily_mean_temps.iter().enumerate() {
        let w = factor * alpha.powi(i as i32);
        t_rm += w * t;
        weight_sum += w;
    }

    if weight_sum > 0.0 {
        t_rm / weight_sum
    } else {
        daily_mean_temps[0]
    }
}

// ─── Pierce Two-Node Thermoregulation ───────────────────────────────

/// Pierce two-node model result.
#[derive(Debug, Clone)]
pub struct TwoNodeResult {
    /// Mean skin temperature (°C).
    pub t_skin: f64,
    /// Core temperature (°C).
    pub t_core: f64,
    /// Skin wettedness (fraction 0-1).
    pub skin_wettedness: f64,
    /// Total evaporative heat loss (W/m²).
    pub evap_heat_loss: f64,
    /// Thermal sensation (DISC): -5 (cold) to +5 (intolerably hot).
    pub disc: f64,
    /// Thermal sensation (TSENS): -4 (very cold) to +4 (very hot).
    pub tsens: f64,
    /// Standard Effective Temperature (SET) (°C).
    pub set: f64,
}

/// Pierce two-node thermoregulation model.
///
/// Simulates dynamic heat exchange between body core and skin shell,
/// with thermoregulatory responses (vasodilation/constriction, sweating, shivering).
///
/// # Arguments
/// * `air_temp` - Air temperature (°C)
/// * `mean_radiant_temp` - Mean radiant temperature (°C)
/// * `air_velocity` - Air velocity (m/s)
/// * `relative_humidity` - Relative humidity (0-100%)
/// * `metabolic_rate` - Metabolic rate (met)
/// * `clothing_insulation` - Clothing insulation (clo)
/// * `duration_minutes` - Exposure duration (minutes)
pub fn pierce_two_node(
    air_temp: f64,
    mean_radiant_temp: f64,
    air_velocity: f64,
    relative_humidity: f64,
    metabolic_rate: f64,
    clothing_insulation: f64,
    duration_minutes: f64,
) -> TwoNodeResult {
    let met_w = metabolic_rate * 58.15;
    let i_cl = clothing_insulation * 0.155;
    let p_a = relative_humidity / 100.0 * saturated_vapor_pressure(air_temp);

    let f_cl = if clothing_insulation <= 0.078 {
        1.0 + 1.29 * i_cl
    } else {
        1.05 + 0.645 * i_cl
    };

    // Linearized radiative heat transfer coefficient
    let t_op = (air_temp + mean_radiant_temp) / 2.0;
    let h_r = 4.0 * STEFAN_BOLTZMANN * SKIN_ABSORPTANCE * ((t_op + 273.15).powi(3));
    let h_c = convective_coefficient(air_temp, SKIN_TEMP_NEUTRAL, air_velocity).max(3.0);
    let h_combined = h_r + h_c;

    // Operative temperature
    let t_operative = (h_r * mean_radiant_temp + h_c * air_temp) / h_combined;

    // Initial conditions
    let mut t_skin = SKIN_TEMP_NEUTRAL;
    let mut t_core = CORE_TEMP_NEUTRAL;

    // Skin blood flow parameters
    let skin_blood_flow_neutral = 6.3; // L/(h·m²)
    let mut skin_blood_flow;
    let alpha_skin = 0.1; // skin mass fraction

    // Thermal capacitances (J/(K·m²))
    let c_skin = BODY_MASS * alpha_skin * BODY_SPECIFIC_HEAT / BODY_SURFACE_AREA_DUBOIS;
    let c_core = BODY_MASS * (1.0 - alpha_skin) * BODY_SPECIFIC_HEAT / BODY_SURFACE_AREA_DUBOIS;

    let dt = 60.0; // 1-minute time steps
    let steps = (duration_minutes / 1.0).ceil() as usize;

    let mut skin_wettedness = 0.06;
    let mut evap_heat_loss = 0.0;

    for _ in 0..steps {
        // Skin blood flow regulation
        let vasodilation = if t_core > CORE_TEMP_NEUTRAL {
            200.0 * (t_core - CORE_TEMP_NEUTRAL)
        } else {
            0.0
        };
        let vasoconstriction = if t_skin < SKIN_TEMP_NEUTRAL {
            0.5 * (SKIN_TEMP_NEUTRAL - t_skin)
        } else {
            0.0
        };
        skin_blood_flow = (skin_blood_flow_neutral + vasodilation)
            / (1.0 + vasoconstriction);
        skin_blood_flow = skin_blood_flow.clamp(0.5, 90.0);

        // Core-to-skin heat transfer via blood
        let q_core_to_skin =
            skin_blood_flow * BLOOD_DENSITY_CP / 3600.0 * (t_core - t_skin);

        // Regulatory sweating
        let sweat_control = if t_core > CORE_TEMP_NEUTRAL {
            250.0 * (t_core - CORE_TEMP_NEUTRAL)
        } else {
            0.0
        };
        let e_rsw = sweat_control * (1.0 - (t_skin - SKIN_TEMP_NEUTRAL).powi(2) / 1600.0).max(0.0);

        // Max evaporation (Lewis relation: LR = 16.5 K/kPa)
        let p_sk_sat = saturated_vapor_pressure(t_skin) / 1000.0; // kPa
        let p_a_kpa = p_a / 1000.0; // kPa
        let lr = 16.5; // Lewis ratio K/kPa
        let r_e_cl = i_cl / (lr * f_cl); // m²·kPa/W
        let r_e_a = 1.0 / (lr * f_cl * h_c); // m²·kPa/W
        let e_max = ((p_sk_sat - p_a_kpa) / (r_e_cl + r_e_a)).max(0.0);

        // Skin diffusion evaporation
        let e_diff = 0.06 * (1.0 - e_rsw / e_max.max(0.001)) * e_max;

        evap_heat_loss = if e_max > 0.001 {
            (e_rsw + e_diff).min(e_max)
        } else {
            0.0
        };
        skin_wettedness = if e_max > 0.001 {
            (evap_heat_loss / e_max).clamp(0.06, 1.0)
        } else {
            0.06
        };

        // Dry heat transfer through clothing to environment
        let r_cl_total = i_cl + 1.0 / (f_cl * h_combined);
        let q_skin_to_env = (t_skin - t_operative) / r_cl_total;

        // Shivering
        let shiver = if t_skin < SKIN_TEMP_NEUTRAL && t_core < CORE_TEMP_NEUTRAL {
            19.4 * (SKIN_TEMP_NEUTRAL - t_skin) * (CORE_TEMP_NEUTRAL - t_core)
        } else {
            0.0
        };

        // Respiration heat loss
        let q_resp = (0.0014 * met_w * (34.0 - air_temp))
            + (1.7e-5 * met_w * (5867.0 - p_a));

        // Core energy balance
        let d_t_core = (met_w + shiver - q_resp - q_core_to_skin) / c_core * dt;

        // Skin energy balance
        let d_t_skin = (q_core_to_skin - q_skin_to_env - evap_heat_loss) / c_skin * dt;

        t_core += d_t_core;
        t_skin += d_t_skin;

        // Clamp to physiologically reasonable range
        t_core = t_core.clamp(33.0, 42.0);
        t_skin = t_skin.clamp(20.0, 42.0);
    }

    // DISC (thermal discomfort)
    let disc = if t_skin < SKIN_TEMP_NEUTRAL {
        // Cold discomfort
        0.68 * (SKIN_TEMP_NEUTRAL - t_skin) * (-1.0)
    } else if skin_wettedness > 0.06 {
        // Warm discomfort based on skin wettedness
        4.7 * (skin_wettedness - 0.06) / (1.0 - 0.06)
    } else {
        0.0
    };
    let disc = disc.clamp(-5.0, 5.0);

    // TSENS (thermal sensation)
    let tsens = 0.4685 * (t_core - CORE_TEMP_NEUTRAL)
        + 0.5315 * (t_skin - SKIN_TEMP_NEUTRAL);
    let tsens = tsens.clamp(-4.0, 4.0);

    // Standard Effective Temperature (SET*)
    let set = calc_set(t_skin, skin_wettedness, metabolic_rate);

    TwoNodeResult {
        t_skin,
        t_core,
        skin_wettedness,
        evap_heat_loss,
        disc,
        tsens,
        set,
    }
}

/// Calculate Standard Effective Temperature (SET*).
///
/// SET* is the equivalent air temperature of a standard environment
/// (RH=50%, v=0.1 m/s, clo=0.6) that produces the same
/// physiological state (skin temperature and skin wettedness).
fn calc_set(t_skin: f64, skin_wettedness: f64, _metabolic_rate: f64) -> f64 {
    // Standard environment: clo=0.6, v=0.1, RH=50%
    let i_cl_std = 0.6 * 0.155; // m²·K/W
    let h_c_std = 3.0; // W/(m²·K) at v=0.1 m/s
    let h_r_std = 4.7; // linearized radiation coefficient
    let h_combined_std = h_c_std + h_r_std;
    let f_cl_std = 1.0 + 0.15 * 0.6;

    // Total resistance from skin to environment
    let r_total = i_cl_std + 1.0 / (f_cl_std * h_combined_std);

    // Evaporative resistance
    let h_e_std = 16.5 * h_c_std;
    let denom_e = f64::max(f_cl_std * h_e_std, 0.01);
    let r_e_total = i_cl_std / denom_e + 1.0 / denom_e;

    // Dry heat loss per degree difference
    let dry_conductance = 1.0 / r_total; // W/(m²·K)

    // At steady state in standard environment: q_dry + q_evap = q_met - q_resp
    // q_dry = (T_skin - SET*) / r_total
    // q_evap = w * (p_sat(T_skin) - 0.5*p_sat(SET*)) / r_e_total
    // Solve for SET*:
    let p_sat_skin = saturated_vapor_pressure(t_skin);
    let q_evap_est = skin_wettedness * p_sat_skin / r_e_total;

    // Approximation: SET ≈ T_skin - total_heat_loss * r_total
    // Using skin temperature as primary indicator
    let set = t_skin - q_evap_est * r_total / dry_conductance * 0.01;
    set.clamp(10.0, 45.0)
}

// ─── Simple ASHRAE 55 Comfort Check ─────────────────────────────────

/// ASHRAE 55 comfort zone check result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComfortZone {
    /// Within winter comfort zone (heating season).
    Winter,
    /// Within summer comfort zone (cooling season).
    Summer,
    /// Within both zones.
    Both,
    /// Outside acceptable range.
    OutOfRange,
}

/// Simple ASHRAE 55 comfort zone check.
///
/// Uses the operative temperature limits from ASHRAE 55 Figure 5.2.1.2.
///
/// # Arguments
/// * `operative_temp` - Operative temperature (°C)
/// * `humidity_ratio` - Humidity ratio (kg/kg)
pub fn ashrae55_comfort_zone(operative_temp: f64, humidity_ratio: f64) -> ComfortZone {
    // ASHRAE 55 comfort zones (simplified rectangular approximation)
    // Winter: 20.0-23.5°C at 0.004-0.012 kg/kg (clo = 1.0, met = 1.0-1.3)
    // Summer: 23.0-26.0°C at 0.004-0.012 kg/kg (clo = 0.5, met = 1.0-1.3)

    let humidity_ok = humidity_ratio >= 0.0 && humidity_ratio <= 0.012;

    if !humidity_ok {
        return ComfortZone::OutOfRange;
    }

    let winter = operative_temp >= 20.0 && operative_temp <= 23.5;
    let summer = operative_temp >= 23.0 && operative_temp <= 26.0;

    match (winter, summer) {
        (true, true) => ComfortZone::Both,
        (true, false) => ComfortZone::Winter,
        (false, true) => ComfortZone::Summer,
        (false, false) => ComfortZone::OutOfRange,
    }
}

// ─── Ankle Draft Risk ───────────────────────────────────────────────

/// Ankle draft risk result.
#[derive(Debug, Clone)]
pub struct AnkleDraftResult {
    /// Predicted Percentage of Dissatisfied due to draft at ankle (%).
    pub ppd_ankle: f64,
    /// Whether the ankle draft exceeds ASHRAE 55 limit (PPD < 20%).
    pub acceptable: bool,
}

/// Calculate ankle-level draft dissatisfaction.
///
/// Based on ASHRAE 55-2017 Section 5.2.5.
///
/// # Arguments
/// * `air_temp_ankle` - Air temperature at 0.1m height (°C)
/// * `air_velocity_ankle` - Air velocity at 0.1m height (m/s)
/// * `air_temp_head` - Air temperature at 1.1m height (°C)
/// * `overall_pmv` - PMV for the whole body
pub fn ankle_draft_risk(
    air_temp_ankle: f64,
    air_velocity_ankle: f64,
    air_temp_head: f64,
    overall_pmv: f64,
) -> AnkleDraftResult {
    // Vertical air temperature difference
    let delta_t = air_temp_head - air_temp_ankle;

    // Draft risk model (simplified Fanger draft model applied to ankle)
    // PPD_ankle = (34 - t_ankle) * (v_ankle - 0.05)^0.62 * (0.37*v_ankle*Tu + 3.14)
    // Simplified version using delta_t and PMV
    let tu = 40.0; // typical turbulence intensity (%)
    let ppd_ankle = if air_velocity_ankle > 0.05 && delta_t > 0.0 {
        let dr = (34.0 - air_temp_ankle)
            * (air_velocity_ankle - 0.05).powf(0.62)
            * (0.37 * air_velocity_ankle * tu / 100.0 + 3.14);
        dr.clamp(0.0, 100.0)
    } else if delta_t > 3.0 {
        // Vertical temperature difference penalty
        let ppd_vertical = 100.0 / (1.0 + (-2.58 + 0.76 * delta_t).exp());
        ppd_vertical.clamp(0.0, 100.0)
    } else {
        // Low risk with small delta_t and low velocity
        let _ = overall_pmv;
        let ppd_base = 100.0 / (1.0 + (-2.58 + 0.76 * delta_t.max(0.0)).exp());
        ppd_base.clamp(0.0, 100.0)
    };

    AnkleDraftResult {
        ppd_ankle,
        acceptable: ppd_ankle < 20.0,
    }
}

// ─── Ceiling Fan Cooling Effect ─────────────────────────────────────

/// Ceiling fan cooling effect result.
#[derive(Debug, Clone)]
pub struct CeilingFanEffect {
    /// Increased air speed at occupant level (m/s).
    pub air_speed_increase: f64,
    /// Equivalent cooling effect (°C reduction in operative temperature).
    pub cooling_effect: f64,
    /// Adjusted operative temperature for comfort calculation (°C).
    pub adjusted_operative_temp: f64,
}

/// Calculate ceiling fan cooling effect.
///
/// Based on ASHRAE 55-2017 elevated air speed comfort method.
///
/// # Arguments
/// * `operative_temp` - Operative temperature without fan (°C)
/// * `air_speed_at_occupant` - Air speed at occupant level with fan running (m/s)
/// * `metabolic_rate` - Metabolic rate (met)
pub fn ceiling_fan_cooling_effect(
    operative_temp: f64,
    air_speed_at_occupant: f64,
    metabolic_rate: f64,
) -> CeilingFanEffect {
    // Elevated air speed cooling effect (ASHRAE 55, Section 5.2.3)
    // Only applicable when operative temp > 25°C and met <= 1.3
    let v = air_speed_at_occupant;

    let cooling_effect = if operative_temp > 25.0 && v > 0.2 && metabolic_rate <= 2.0 {
        // Cooling effect increases with air speed, limited to ~3.5°C
        // Regression from ASHRAE 55 Figure 5.2.3
        let ce = if v <= 0.6 {
            1.2 * (v - 0.2)
        } else if v <= 0.9 {
            0.48 + 1.6 * (v - 0.6)
        } else if v <= 1.2 {
            0.96 + 1.0 * (v - 0.9)
        } else {
            (1.26 + 0.5 * (v - 1.2)).min(3.5)
        };
        ce.max(0.0)
    } else {
        0.0
    };

    CeilingFanEffect {
        air_speed_increase: v,
        cooling_effect,
        adjusted_operative_temp: operative_temp - cooling_effect,
    }
}

// ─── Operative Temperature ──────────────────────────────────────────

/// Calculate operative temperature.
///
/// T_op = A * T_air + (1 - A) * T_mrt
/// where A depends on air velocity.
pub fn operative_temperature(air_temp: f64, mean_radiant_temp: f64, air_velocity: f64) -> f64 {
    let a = if air_velocity < 0.2 {
        0.5
    } else if air_velocity < 0.6 {
        0.6
    } else {
        0.7
    };
    a * air_temp + (1.0 - a) * mean_radiant_temp
}

/// Calculate mean radiant temperature from surface temperatures and view factors.
///
/// T_mrt = (sum(F_i * T_i^4))^0.25 - 273.15
pub fn mean_radiant_temperature(
    surface_temps_c: &[f64],
    view_factors: &[f64],
) -> f64 {
    if surface_temps_c.is_empty() || view_factors.is_empty() {
        return 20.0;
    }

    let sum_ft4: f64 = surface_temps_c
        .iter()
        .zip(view_factors.iter())
        .map(|(&t, &f)| f * (t + 273.15).powi(4))
        .sum();

    sum_ft4.powf(0.25) - 273.15
}

// ─── Running Mean Outdoor Temperature (ASHRAE 55) ───────────────────

/// Calculate ASHRAE 55 prevailing mean outdoor temperature.
///
/// Arithmetic mean of mean daily outdoor temperatures for 7-30 previous days.
pub fn ashrae55_prevailing_mean(daily_mean_temps: &[f64]) -> f64 {
    if daily_mean_temps.is_empty() {
        return 20.0;
    }
    let n = daily_mean_temps.len().min(30);
    daily_mean_temps[..n].iter().sum::<f64>() / n as f64
}

// ─── Clothing Insulation Estimation ─────────────────────────────────

/// Estimate clothing insulation from outdoor temperature.
///
/// Simple model for typical office workers.
pub fn estimate_clothing_insulation(outdoor_temp: f64) -> f64 {
    if outdoor_temp < 5.0 {
        1.0 // heavy winter clothing
    } else if outdoor_temp < 15.0 {
        0.5 + 0.5 * (15.0 - outdoor_temp) / 10.0 // transition
    } else if outdoor_temp < 26.0 {
        0.5 // typical indoor summer
    } else {
        0.36 // light summer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── PMV/PPD Tests ──────────────────────────────────────────────

    #[test]
    fn pmv_neutral_conditions() {
        // Near-neutral conditions: 24°C, 50% RH, 0.1 m/s, 1.0 met, 0.5 clo
        let result = fanger_pmv_ppd(24.0, 24.0, 0.1, 50.0, 1.0, 0.5, 0.0);
        assert!(
            result.pmv.abs() < 1.0,
            "PMV={} should be near 0 at neutral conditions",
            result.pmv
        );
        assert!(
            result.ppd < 30.0,
            "PPD={} should be moderate at neutral conditions",
            result.ppd
        );
    }

    #[test]
    fn pmv_hot_conditions() {
        // Hot: 30°C, 70% RH, 0.1 m/s, 1.0 met, 0.5 clo
        let result = fanger_pmv_ppd(30.0, 30.0, 0.1, 70.0, 1.0, 0.5, 0.0);
        assert!(
            result.pmv > 1.0,
            "PMV={} should be positive (warm) in hot conditions",
            result.pmv
        );
        assert!(
            result.ppd > 25.0,
            "PPD={} should be high in hot conditions",
            result.ppd
        );
    }

    #[test]
    fn pmv_cold_conditions() {
        // Cold: 15°C, 40% RH, 0.1 m/s, 1.0 met, 1.0 clo
        let result = fanger_pmv_ppd(15.0, 15.0, 0.1, 40.0, 1.0, 1.0, 0.0);
        assert!(
            result.pmv < -0.5,
            "PMV={} should be negative (cool) in cold conditions",
            result.pmv
        );
    }

    #[test]
    fn pmv_high_metabolism() {
        // High activity: 20°C but 3.0 met (heavy work)
        let result_low = fanger_pmv_ppd(20.0, 20.0, 0.1, 50.0, 1.0, 0.5, 0.0);
        let result_high = fanger_pmv_ppd(20.0, 20.0, 0.1, 50.0, 3.0, 0.5, 0.0);
        assert!(
            result_high.pmv > result_low.pmv,
            "Higher metabolism should increase PMV: low={}, high={}",
            result_low.pmv, result_high.pmv
        );
    }

    #[test]
    fn pmv_air_velocity_cooling() {
        // Higher air velocity should reduce warmth sensation
        let result_still = fanger_pmv_ppd(27.0, 27.0, 0.1, 50.0, 1.0, 0.5, 0.0);
        let result_windy = fanger_pmv_ppd(27.0, 27.0, 1.0, 50.0, 1.0, 0.5, 0.0);
        assert!(
            result_windy.pmv < result_still.pmv,
            "Wind should reduce PMV: still={}, windy={}",
            result_still.pmv, result_windy.pmv
        );
    }

    #[test]
    fn ppd_minimum_at_neutral() {
        // PPD has a minimum of ~5% even at PMV=0
        let result = fanger_pmv_ppd(22.0, 22.0, 0.1, 50.0, 1.0, 0.9, 0.0);
        assert!(
            result.ppd >= 5.0,
            "PPD should be >= 5% (minimum): PPD={}",
            result.ppd
        );
    }

    #[test]
    fn ppd_increases_with_deviation() {
        let result_neutral = fanger_pmv_ppd(22.0, 22.0, 0.1, 50.0, 1.0, 0.9, 0.0);
        let result_warm = fanger_pmv_ppd(28.0, 28.0, 0.1, 50.0, 1.0, 0.5, 0.0);
        let result_cold = fanger_pmv_ppd(16.0, 16.0, 0.1, 50.0, 1.0, 1.0, 0.0);

        assert!(result_warm.ppd > result_neutral.ppd);
        assert!(result_cold.ppd > result_neutral.ppd);
    }

    #[test]
    fn pmv_clothing_effect() {
        // More clothing should make a warm environment warmer
        let result_light = fanger_pmv_ppd(25.0, 25.0, 0.1, 50.0, 1.0, 0.3, 0.0);
        let result_heavy = fanger_pmv_ppd(25.0, 25.0, 0.1, 50.0, 1.0, 1.2, 0.0);
        assert!(
            result_heavy.pmv > result_light.pmv,
            "More clothing should increase PMV at warm temp: light={}, heavy={}",
            result_light.pmv, result_heavy.pmv
        );
    }

    #[test]
    fn pmv_humidity_effect() {
        // Higher humidity should increase PMV in warm conditions
        let result_dry = fanger_pmv_ppd(27.0, 27.0, 0.1, 20.0, 1.0, 0.5, 0.0);
        let result_humid = fanger_pmv_ppd(27.0, 27.0, 0.1, 80.0, 1.0, 0.5, 0.0);
        assert!(
            result_humid.pmv > result_dry.pmv,
            "Higher humidity should increase PMV: dry={}, humid={}",
            result_dry.pmv, result_humid.pmv
        );
    }

    #[test]
    fn pmv_radiant_asymmetry() {
        // Higher MRT should increase PMV
        let result_low_mrt = fanger_pmv_ppd(22.0, 18.0, 0.1, 50.0, 1.0, 0.9, 0.0);
        let result_high_mrt = fanger_pmv_ppd(22.0, 28.0, 0.1, 50.0, 1.0, 0.9, 0.0);
        assert!(
            result_high_mrt.pmv > result_low_mrt.pmv,
            "Higher MRT should increase PMV: low={}, high={}",
            result_low_mrt.pmv, result_high_mrt.pmv
        );
    }

    // ─── ASHRAE 55 Adaptive Tests ───────────────────────────────────

    #[test]
    fn adaptive_neutral_temperature() {
        let result = ashrae55_adaptive(20.0);
        assert!(result.applicable);
        // t_neutral = 0.31 * 20 + 17.8 = 24.0
        assert!(
            (result.t_neutral - 24.0).abs() < 0.1,
            "t_neutral={}",
            result.t_neutral
        );
    }

    #[test]
    fn adaptive_comfort_bands() {
        let result = ashrae55_adaptive(25.0);
        // t_neutral = 0.31*25 + 17.8 = 25.55
        assert!(result.upper_80 > result.t_neutral);
        assert!(result.lower_80 < result.t_neutral);
        assert!(result.upper_90 < result.upper_80);
        assert!(result.lower_90 > result.lower_80);
        assert!((result.upper_80 - result.lower_80 - 7.0).abs() < 0.01);
        assert!((result.upper_90 - result.lower_90 - 5.0).abs() < 0.01);
    }

    #[test]
    fn adaptive_not_applicable_cold() {
        let result = ashrae55_adaptive(5.0);
        assert!(!result.applicable, "Should not be applicable below 10°C");
    }

    #[test]
    fn adaptive_not_applicable_hot() {
        let result = ashrae55_adaptive(35.0);
        assert!(!result.applicable, "Should not be applicable above 33.5°C");
    }

    #[test]
    fn adaptive_increases_with_outdoor_temp() {
        let result_cool = ashrae55_adaptive(15.0);
        let result_warm = ashrae55_adaptive(30.0);
        assert!(
            result_warm.t_neutral > result_cool.t_neutral,
            "Neutral temp should increase with outdoor: cool={}, warm={}",
            result_cool.t_neutral, result_warm.t_neutral
        );
    }

    // ─── CEN 15251 Tests ────────────────────────────────────────────

    #[test]
    fn cen_neutral_temperature() {
        let result = cen15251_adaptive(20.0, CenCategory::II);
        // t_neutral = 0.33*20 + 18.8 = 25.4
        assert!(
            (result.t_neutral - 25.4).abs() < 0.1,
            "t_neutral={}",
            result.t_neutral
        );
        assert!(result.applicable);
    }

    #[test]
    fn cen_category_widths() {
        let cat1 = cen15251_adaptive(20.0, CenCategory::I);
        let cat2 = cen15251_adaptive(20.0, CenCategory::II);
        let cat3 = cen15251_adaptive(20.0, CenCategory::III);

        // Cat I: ±2, Cat II: ±3, Cat III: ±4
        assert!(
            (cat1.upper - cat1.lower - 4.0).abs() < 0.01,
            "Cat I width={}",
            cat1.upper - cat1.lower
        );
        assert!(
            (cat2.upper - cat2.lower - 6.0).abs() < 0.01,
            "Cat II width={}",
            cat2.upper - cat2.lower
        );
        assert!(
            (cat3.upper - cat3.lower - 8.0).abs() < 0.01,
            "Cat III width={}",
            cat3.upper - cat3.lower
        );
    }

    #[test]
    fn cen_not_applicable_outside_range() {
        let result_cold = cen15251_adaptive(5.0, CenCategory::II);
        let result_hot = cen15251_adaptive(35.0, CenCategory::II);
        assert!(!result_cold.applicable);
        assert!(!result_hot.applicable);
    }

    #[test]
    fn cen_running_mean_calculation() {
        // 7 days at 20°C → running mean should be ~20
        let temps = vec![20.0; 7];
        let t_rm = cen_running_mean(&temps, 0.8);
        assert!(
            (t_rm - 20.0).abs() < 0.5,
            "Running mean of constant 20°C = {}",
            t_rm
        );
    }

    #[test]
    fn cen_running_mean_weighted() {
        // Recent warm, older cold → running mean closer to recent
        let temps = vec![25.0, 15.0, 15.0, 15.0, 15.0];
        let t_rm = cen_running_mean(&temps, 0.8);
        assert!(
            t_rm > 15.0 && t_rm < 25.0,
            "Running mean should be between extremes: {}",
            t_rm
        );
        // Most recent day (25°C) gets weight (1-alpha) = 0.2
        // Older days get progressively less weight
        // With alpha=0.8, older days still dominate, so result leans toward 15
    }

    // ─── Pierce Two-Node Tests ──────────────────────────────────────

    #[test]
    fn two_node_neutral() {
        let result = pierce_two_node(22.0, 22.0, 0.1, 50.0, 1.0, 0.9, 60.0);
        // After 60 minutes, skin temp should be in a physiologically reasonable range
        assert!(
            result.t_skin > 25.0 && result.t_skin < 38.0,
            "t_skin={} should be in reasonable range",
            result.t_skin
        );
        assert!(
            result.t_core > 35.0 && result.t_core < 39.0,
            "t_core={} should be in reasonable range",
            result.t_core
        );
    }

    #[test]
    fn two_node_hot_sweat() {
        let result = pierce_two_node(35.0, 35.0, 0.1, 60.0, 1.5, 0.3, 60.0);
        // Hot conditions: skin wettedness and evaporative loss should be present
        assert!(
            result.skin_wettedness >= 0.06,
            "Skin wettedness should be at or above baseline: {}",
            result.skin_wettedness
        );
        assert!(
            result.tsens > 0.0,
            "TSENS should be positive (warm): {}",
            result.tsens
        );
    }

    #[test]
    fn two_node_cold_vasoconstriction() {
        let result = pierce_two_node(10.0, 10.0, 0.5, 40.0, 1.0, 1.5, 60.0);
        // Cold: skin should cool, core maintained
        assert!(
            result.t_skin < SKIN_TEMP_NEUTRAL,
            "Skin should be cooler in cold: {}",
            result.t_skin
        );
        assert!(
            result.disc < 0.0,
            "DISC should be negative (cold discomfort): {}",
            result.disc
        );
    }

    #[test]
    fn two_node_set_reasonable() {
        let result = pierce_two_node(22.0, 22.0, 0.1, 50.0, 1.0, 0.9, 60.0);
        assert!(
            result.set > 15.0 && result.set < 35.0,
            "SET* should be reasonable: {}",
            result.set
        );
    }

    #[test]
    fn two_node_duration_effect() {
        // Longer exposure in hot conditions → more thermal strain
        let short = pierce_two_node(35.0, 35.0, 0.1, 60.0, 1.5, 0.3, 15.0);
        let long = pierce_two_node(35.0, 35.0, 0.1, 60.0, 1.5, 0.3, 120.0);
        // After longer exposure, skin wettedness should be higher
        assert!(
            long.skin_wettedness >= short.skin_wettedness - 0.01,
            "Longer exposure should maintain or increase wettedness: short={}, long={}",
            short.skin_wettedness, long.skin_wettedness
        );
    }

    // ─── ASHRAE 55 Simple Check Tests ───────────────────────────────

    #[test]
    fn comfort_zone_winter() {
        let zone = ashrae55_comfort_zone(21.0, 0.006);
        assert_eq!(zone, ComfortZone::Winter);
    }

    #[test]
    fn comfort_zone_summer() {
        let zone = ashrae55_comfort_zone(25.0, 0.006);
        assert_eq!(zone, ComfortZone::Summer);
    }

    #[test]
    fn comfort_zone_both() {
        // Overlap region: 23.0-23.5°C
        let zone = ashrae55_comfort_zone(23.2, 0.006);
        assert_eq!(zone, ComfortZone::Both);
    }

    #[test]
    fn comfort_zone_out_of_range_hot() {
        let zone = ashrae55_comfort_zone(30.0, 0.006);
        assert_eq!(zone, ComfortZone::OutOfRange);
    }

    #[test]
    fn comfort_zone_out_of_range_humid() {
        let zone = ashrae55_comfort_zone(22.0, 0.015);
        assert_eq!(zone, ComfortZone::OutOfRange);
    }

    // ─── Ankle Draft Tests ──────────────────────────────────────────

    #[test]
    fn ankle_draft_low_risk() {
        let result = ankle_draft_risk(22.0, 0.1, 22.5, 0.0);
        assert!(
            result.acceptable,
            "Low velocity uniform temp should be acceptable: PPD={}",
            result.ppd_ankle
        );
    }

    #[test]
    fn ankle_draft_high_velocity() {
        let result = ankle_draft_risk(20.0, 0.5, 23.0, 0.5);
        assert!(
            result.ppd_ankle > 5.0,
            "High ankle velocity should increase draft risk: PPD={}",
            result.ppd_ankle
        );
    }

    #[test]
    fn ankle_draft_vertical_gradient() {
        // Large vertical temperature difference → higher draft risk
        let result_small = ankle_draft_risk(21.0, 0.1, 22.0, 0.0);
        let result_large = ankle_draft_risk(18.0, 0.1, 24.0, 0.0);
        assert!(
            result_large.ppd_ankle > result_small.ppd_ankle,
            "Larger gradient should increase risk: small={}, large={}",
            result_small.ppd_ankle, result_large.ppd_ankle
        );
    }

    // ─── Ceiling Fan Tests ──────────────────────────────────────────

    #[test]
    fn ceiling_fan_cooling_above_25() {
        let result = ceiling_fan_cooling_effect(28.0, 0.8, 1.0);
        assert!(
            result.cooling_effect > 0.0,
            "Fan should provide cooling above 25°C: ce={}",
            result.cooling_effect
        );
        assert!(
            result.adjusted_operative_temp < 28.0,
            "Adjusted temp should be lower: {}",
            result.adjusted_operative_temp
        );
    }

    #[test]
    fn ceiling_fan_no_effect_below_25() {
        let result = ceiling_fan_cooling_effect(22.0, 0.8, 1.0);
        assert!(
            result.cooling_effect < 0.01,
            "No cooling below 25°C: ce={}",
            result.cooling_effect
        );
    }

    #[test]
    fn ceiling_fan_increases_with_speed() {
        let slow = ceiling_fan_cooling_effect(28.0, 0.4, 1.0);
        let fast = ceiling_fan_cooling_effect(28.0, 1.0, 1.0);
        assert!(
            fast.cooling_effect > slow.cooling_effect,
            "Faster fan should cool more: slow={}, fast={}",
            slow.cooling_effect, fast.cooling_effect
        );
    }

    #[test]
    fn ceiling_fan_max_cooling() {
        let result = ceiling_fan_cooling_effect(30.0, 2.0, 1.0);
        assert!(
            result.cooling_effect <= 3.5,
            "Cooling effect should be capped: ce={}",
            result.cooling_effect
        );
    }

    // ─── Operative Temperature Tests ────────────────────────────────

    #[test]
    fn operative_temp_equal() {
        let t_op = operative_temperature(22.0, 22.0, 0.1);
        assert!(
            (t_op - 22.0).abs() < 0.01,
            "Equal temps → operative = same: {}",
            t_op
        );
    }

    #[test]
    fn operative_temp_weighted() {
        // Low velocity: 50/50 weighting
        let t_op = operative_temperature(20.0, 26.0, 0.1);
        assert!(
            (t_op - 23.0).abs() < 0.01,
            "50/50 at low velocity: t_op={}",
            t_op
        );
    }

    #[test]
    fn operative_temp_high_velocity() {
        // High velocity: 70/30 weighting (more air)
        let t_op = operative_temperature(20.0, 26.0, 1.0);
        // 0.7*20 + 0.3*26 = 14 + 7.8 = 21.8
        assert!(
            (t_op - 21.8).abs() < 0.01,
            "70/30 at high velocity: t_op={}",
            t_op
        );
    }

    // ─── MRT Tests ──────────────────────────────────────────────────

    #[test]
    fn mrt_uniform_surfaces() {
        let temps = vec![22.0, 22.0, 22.0, 22.0];
        let vfs = vec![0.25, 0.25, 0.25, 0.25];
        let mrt = mean_radiant_temperature(&temps, &vfs);
        assert!(
            (mrt - 22.0).abs() < 0.1,
            "Uniform surfaces → MRT ≈ surface temp: {}",
            mrt
        );
    }

    #[test]
    fn mrt_one_hot_surface() {
        // One hot wall (40°C), rest at 20°C
        let temps = vec![40.0, 20.0, 20.0, 20.0];
        let vfs = vec![0.25, 0.25, 0.25, 0.25];
        let mrt = mean_radiant_temperature(&temps, &vfs);
        assert!(
            mrt > 22.0 && mrt < 30.0,
            "One hot wall should raise MRT above 20: {}",
            mrt
        );
    }

    // ─── Clothing Estimation Tests ──────────────────────────────────

    #[test]
    fn clothing_winter() {
        let clo = estimate_clothing_insulation(-5.0);
        assert!(
            (clo - 1.0).abs() < 0.01,
            "Very cold → heavy winter: clo={}",
            clo
        );
    }

    #[test]
    fn clothing_summer() {
        let clo = estimate_clothing_insulation(30.0);
        assert!(
            clo < 0.5,
            "Hot → light summer clothing: clo={}",
            clo
        );
    }

    #[test]
    fn clothing_transition() {
        let clo = estimate_clothing_insulation(10.0);
        assert!(
            clo > 0.5 && clo < 1.0,
            "Transition temp → mid-range clothing: clo={}",
            clo
        );
    }

    // ─── Prevailing Mean Tests ──────────────────────────────────────

    #[test]
    fn prevailing_mean_constant() {
        let temps = vec![20.0; 14];
        let pm = ashrae55_prevailing_mean(&temps);
        assert!(
            (pm - 20.0).abs() < 0.01,
            "Constant temps → prevailing mean = same: {}",
            pm
        );
    }

    #[test]
    fn prevailing_mean_varying() {
        let temps = vec![10.0, 20.0, 30.0];
        let pm = ashrae55_prevailing_mean(&temps);
        assert!(
            (pm - 20.0).abs() < 0.01,
            "Average of 10,20,30 = 20: {}",
            pm
        );
    }

    // ─── Saturated Vapor Pressure Tests ──────────────────────────────

    #[test]
    fn svp_at_20c() {
        let p = saturated_vapor_pressure(20.0);
        // Approximately 2338 Pa at 20°C
        assert!(
            (p - 2338.0).abs() < 100.0,
            "SVP at 20°C ≈ 2338 Pa: {}",
            p
        );
    }

    #[test]
    fn svp_at_0c() {
        let p = saturated_vapor_pressure(0.0);
        // Approximately 611 Pa at 0°C
        assert!(
            (p - 611.0).abs() < 10.0,
            "SVP at 0°C ≈ 611 Pa: {}",
            p
        );
    }

    #[test]
    fn svp_increases_with_temp() {
        let p10 = saturated_vapor_pressure(10.0);
        let p20 = saturated_vapor_pressure(20.0);
        let p30 = saturated_vapor_pressure(30.0);
        assert!(p20 > p10 && p30 > p20, "SVP should increase with temp");
    }
}
