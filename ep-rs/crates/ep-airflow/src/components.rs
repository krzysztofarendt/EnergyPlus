//! Airflow network component models.
//!
//! Each component relates pressure drop to airflow rate.
//! Components implement the `AirflowComponent` trait which returns
//! flow and its derivative dF/dP for the Newton-Raphson solver.

/// Air state for density and viscosity calculations.
#[derive(Debug, Clone, Copy)]
pub struct AirState {
    pub temperature: f64,    // C
    pub humidity_ratio: f64, // kg/kg
    pub density: f64,        // kg/m3
    pub viscosity: f64,      // Pa*s
}

impl AirState {
    pub fn new(temperature: f64, humidity_ratio: f64, pressure: f64) -> Self {
        let density = ep_psychrometrics::rho_air(pressure, temperature, humidity_ratio);
        let viscosity = 1.71432e-5 + 4.828e-8 * temperature;
        Self { temperature, humidity_ratio, density, viscosity }
    }

    pub fn standard() -> Self {
        Self::new(20.0, 0.008, 101325.0)
    }
}

impl Default for AirState {
    fn default() -> Self {
        Self::standard()
    }
}

/// Result of a component flow calculation.
#[derive(Debug, Clone, Copy, Default)]
pub struct FlowResult {
    /// Forward flow (kg/s). Positive = node_1 to node_2.
    pub flow: f64,
    /// Reverse flow (kg/s). For bidirectional components (large openings).
    pub flow_reverse: f64,
    /// Derivative of forward flow w.r.t. pressure drop (kg/(s*Pa)).
    pub df_dp: f64,
    /// Derivative of reverse flow w.r.t. pressure drop.
    pub df_dp_reverse: f64,
}

/// Trait for all airflow network components.
pub trait AirflowComponent {
    /// Calculate flow given pressure drop between upstream and downstream nodes.
    ///
    /// # Arguments
    /// * `dp` - Pressure drop from node_1 to node_2 (Pa). Positive = node_1 has higher pressure.
    /// * `state_1` - Air state at node_1
    /// * `state_2` - Air state at node_2
    /// * `laminar` - If true, use linearized (laminar) flow for initial guess
    fn calculate(&self, dp: f64, state_1: &AirState, state_2: &AirState, laminar: bool) -> FlowResult;
}

/// Surface crack / power-law leakage component.
///
/// F = C * (dP)^n where:
/// - C = flow coefficient (kg/s at 1 Pa)
/// - n = flow exponent (0.5 for turbulent, 1.0 for laminar, typically 0.65)
#[derive(Debug, Clone)]
pub struct SurfaceCrack {
    pub name: String,
    /// Flow coefficient at reference conditions (kg/s at 1 Pa).
    pub flow_coefficient: f64,
    /// Flow exponent (typically 0.65).
    pub flow_exponent: f64,
}

impl SurfaceCrack {
    pub fn new(name: impl Into<String>, flow_coefficient: f64, flow_exponent: f64) -> Self {
        Self {
            name: name.into(),
            flow_coefficient,
            flow_exponent: flow_exponent.clamp(0.5, 1.0),
        }
    }
}

impl AirflowComponent for SurfaceCrack {
    fn calculate(&self, dp: f64, state_1: &AirState, _state_2: &AirState, laminar: bool) -> FlowResult {
        let c = self.flow_coefficient;
        let n = self.flow_exponent;

        if laminar {
            // Linear approximation for initialization
            let df = c * n; // slope at dp=1
            return FlowResult {
                flow: df * dp,
                df_dp: df,
                ..Default::default()
            };
        }

        let rho_ratio = state_1.density / 1.2; // Density correction
        let abs_dp = dp.abs();

        if abs_dp < 1e-10 {
            // Near zero: return linear approximation
            let df = c * n * rho_ratio.sqrt();
            return FlowResult {
                flow: 0.0,
                df_dp: df,
                ..Default::default()
            };
        }

        let sign = dp.signum();
        let flow = sign * c * rho_ratio.sqrt() * abs_dp.powf(n);
        let df = c * n * rho_ratio.sqrt() * abs_dp.powf(n - 1.0);

        FlowResult {
            flow,
            df_dp: df,
            ..Default::default()
        }
    }
}

/// Effective leakage area component (Sherman-Grimsrud model).
///
/// Converts leakage area to equivalent power-law coefficients.
#[derive(Debug, Clone)]
pub struct EffectiveLeakageArea {
    pub name: String,
    /// Effective leakage area (m2).
    pub leakage_area: f64,
    /// Discharge coefficient (typically 1.0).
    pub discharge_coefficient: f64,
    /// Reference pressure difference (Pa, typically 4.0).
    pub reference_pressure: f64,
}

impl EffectiveLeakageArea {
    pub fn new(name: impl Into<String>, leakage_area: f64) -> Self {
        Self {
            name: name.into(),
            leakage_area,
            discharge_coefficient: 1.0,
            reference_pressure: 4.0,
        }
    }

    /// Convert to equivalent flow coefficient.
    fn flow_coefficient(&self, rho: f64) -> f64 {
        self.discharge_coefficient * self.leakage_area * (2.0 * rho).sqrt()
            / self.reference_pressure.powf(0.5)
    }
}

impl AirflowComponent for EffectiveLeakageArea {
    fn calculate(&self, dp: f64, state_1: &AirState, _state_2: &AirState, laminar: bool) -> FlowResult {
        let c = self.flow_coefficient(state_1.density);
        let n = 0.65; // Standard exponent

        if laminar {
            let df = c * n;
            return FlowResult { flow: df * dp, df_dp: df, ..Default::default() };
        }

        let abs_dp = dp.abs();
        if abs_dp < 1e-10 {
            return FlowResult { flow: 0.0, df_dp: c * n, ..Default::default() };
        }

        let sign = dp.signum();
        let flow = sign * c * abs_dp.powf(n);
        let df = c * n * abs_dp.powf(n - 1.0);

        FlowResult { flow, df_dp: df, ..Default::default() }
    }
}

/// Simple opening (large opening like a door or window).
///
/// Uses orifice equation with discharge coefficient.
/// For large openings, bidirectional flow is possible when there's
/// a temperature-driven density difference (buoyancy).
#[derive(Debug, Clone)]
pub struct SimpleOpening {
    pub name: String,
    /// Opening area (m2).
    pub area: f64,
    /// Discharge coefficient (typically 0.65).
    pub discharge_coefficient: f64,
    /// Opening factor (0-1, from schedule or venting control).
    pub opening_factor: f64,
    /// Minimum density difference for two-way flow (kg/m3).
    pub min_density_diff: f64,
}

impl SimpleOpening {
    pub fn new(name: impl Into<String>, area: f64) -> Self {
        Self {
            name: name.into(),
            area,
            discharge_coefficient: 0.65,
            opening_factor: 1.0,
            min_density_diff: 0.0001,
        }
    }
}

impl AirflowComponent for SimpleOpening {
    fn calculate(&self, dp: f64, state_1: &AirState, state_2: &AirState, laminar: bool) -> FlowResult {
        let a = self.area * self.opening_factor;
        let cd = self.discharge_coefficient;

        if a <= 1e-10 {
            return FlowResult::default();
        }

        if laminar {
            // Linear approximation
            let rho_avg = (state_1.density + state_2.density) / 2.0;
            let df = cd * a * (2.0 * rho_avg).sqrt() * 0.5;
            return FlowResult { flow: df * dp, df_dp: df, ..Default::default() };
        }

        let abs_dp = dp.abs();
        let rho_avg = (state_1.density + state_2.density) / 2.0;

        if abs_dp < 1e-10 {
            let df = cd * a * (2.0 * rho_avg).sqrt() * 0.5;
            return FlowResult { flow: 0.0, df_dp: df, ..Default::default() };
        }

        let sign = dp.signum();
        // Orifice equation: F = Cd * A * sqrt(2 * rho * |dP|)
        let flow = sign * cd * a * (2.0 * rho_avg * abs_dp).sqrt();
        let df = cd * a * (rho_avg / (2.0 * abs_dp)).sqrt();

        FlowResult {
            flow,
            df_dp: df,
            ..Default::default()
        }
    }
}

/// Constant pressure drop element.
#[derive(Debug, Clone)]
pub struct ConstantPressureDrop {
    pub name: String,
    /// Fixed pressure drop (Pa).
    pub pressure_drop: f64,
}

impl AirflowComponent for ConstantPressureDrop {
    fn calculate(&self, dp: f64, _state_1: &AirState, _state_2: &AirState, _laminar: bool) -> FlowResult {
        // This component doesn't directly calculate flow;
        // it modifies the pressure drop for the link
        let effective_dp = dp - self.pressure_drop;
        FlowResult {
            flow: effective_dp * 0.01, // Small linear flow
            df_dp: 0.01,
            ..Default::default()
        }
    }
}

/// Duct component with hydraulic resistance.
///
/// Uses Darcy-Weisbach equation with Colebrook friction factor.
#[derive(Debug, Clone)]
pub struct Duct {
    pub name: String,
    /// Hydraulic diameter (m).
    pub hydraulic_diameter: f64,
    /// Cross-section area (m2).
    pub cross_section_area: f64,
    /// Length (m).
    pub length: f64,
    /// Surface roughness (m).
    pub roughness: f64,
    /// Minor loss coefficient (dimensionless).
    pub minor_loss_coef: f64,
}

impl Duct {
    pub fn new(name: impl Into<String>, diameter: f64, length: f64) -> Self {
        let area = std::f64::consts::PI * diameter * diameter / 4.0;
        Self {
            name: name.into(),
            hydraulic_diameter: diameter,
            cross_section_area: area,
            length,
            roughness: 0.0009, // typical for galvanized steel
            minor_loss_coef: 0.0,
        }
    }

    /// Calculate friction factor using simplified Colebrook-White.
    fn friction_factor(&self, reynolds: f64) -> f64 {
        if reynolds < 2300.0 {
            // Laminar
            if reynolds > 0.0 { 64.0 / reynolds } else { 0.04 }
        } else {
            // Turbulent: Swamee-Jain approximation
            let e_d = self.roughness / self.hydraulic_diameter;
            let term = e_d / 3.7 + 5.74 / reynolds.powf(0.9);
            if term > 0.0 {
                0.25 / (term.log10() * term.log10())
            } else {
                0.02
            }
        }
    }
}

impl AirflowComponent for Duct {
    fn calculate(&self, dp: f64, state_1: &AirState, _state_2: &AirState, laminar: bool) -> FlowResult {
        let rho = state_1.density;
        let mu = state_1.viscosity;
        let a = self.cross_section_area;
        let d = self.hydraulic_diameter;

        if laminar || dp.abs() < 1e-10 {
            // Laminar: dP = 128*mu*L*Q / (pi*D^4) for round duct
            // Q = dP * pi * D^4 / (128 * mu * L)
            let df = if self.length > 0.0 && mu > 0.0 {
                rho * std::f64::consts::PI * d.powi(4) / (128.0 * mu * self.length)
            } else {
                0.01
            };
            return FlowResult { flow: df * dp, df_dp: df, ..Default::default() };
        }

        let abs_dp = dp.abs();
        let sign = dp.signum();

        // Estimate velocity from Bernoulli: v ≈ sqrt(2*|dP|/rho)
        let v_est = (2.0 * abs_dp / rho).sqrt();
        let re = rho * v_est * d / mu;
        let f = self.friction_factor(re);

        // Total resistance: dP = (f*L/D + K_minor) * rho*v^2/2
        let resistance = f * self.length / d + self.minor_loss_coef;
        if resistance <= 0.0 {
            return FlowResult { flow: sign * rho * a * v_est, df_dp: 0.01, ..Default::default() };
        }

        // Solve for velocity: v = sqrt(2*|dP| / (rho * resistance))
        let v = (2.0 * abs_dp / (rho * resistance)).sqrt();
        let flow = sign * rho * a * v;
        let df = rho * a / (2.0 * (2.0 * rho * resistance * abs_dp).sqrt().max(1e-10));

        FlowResult { flow, df_dp: df, ..Default::default() }
    }
}

/// Enum wrapping all component types for storage in the network.
#[derive(Debug, Clone)]
pub enum Component {
    Crack(SurfaceCrack),
    LeakageArea(EffectiveLeakageArea),
    Opening(SimpleOpening),
    Duct(Duct),
    ConstantDrop(ConstantPressureDrop),
}

impl AirflowComponent for Component {
    fn calculate(&self, dp: f64, s1: &AirState, s2: &AirState, laminar: bool) -> FlowResult {
        match self {
            Component::Crack(c) => c.calculate(dp, s1, s2, laminar),
            Component::LeakageArea(c) => c.calculate(dp, s1, s2, laminar),
            Component::Opening(c) => c.calculate(dp, s1, s2, laminar),
            Component::Duct(c) => c.calculate(dp, s1, s2, laminar),
            Component::ConstantDrop(c) => c.calculate(dp, s1, s2, laminar),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crack_power_law() {
        let crack = SurfaceCrack::new("Crack1", 0.001, 0.65);
        let state = AirState::standard();
        let result = crack.calculate(10.0, &state, &state, false);

        // F = 0.001 * 10^0.65 ≈ 0.001 * 4.467 ≈ 0.00447
        assert!(result.flow > 0.003 && result.flow < 0.006, "flow={}", result.flow);
        assert!(result.df_dp > 0.0);
    }

    #[test]
    fn crack_negative_dp() {
        let crack = SurfaceCrack::new("Crack1", 0.001, 0.65);
        let state = AirState::standard();
        let result = crack.calculate(-10.0, &state, &state, false);
        assert!(result.flow < 0.0, "Should be negative flow");
    }

    #[test]
    fn crack_laminar_init() {
        let crack = SurfaceCrack::new("Crack1", 0.001, 0.65);
        let state = AirState::standard();
        let result = crack.calculate(10.0, &state, &state, true);
        // Laminar: linear approximation
        assert!(result.flow > 0.0);
    }

    #[test]
    fn opening_orifice() {
        let opening = SimpleOpening::new("Door", 2.0);
        let state = AirState::standard();
        let result = opening.calculate(5.0, &state, &state, false);

        // F = 0.65 * 2.0 * sqrt(2 * 1.2 * 5) = 1.3 * sqrt(12) ≈ 4.5 kg/s
        assert!(result.flow > 3.0 && result.flow < 6.0, "flow={}", result.flow);
    }

    #[test]
    fn opening_closed() {
        let mut opening = SimpleOpening::new("Window", 1.0);
        opening.opening_factor = 0.0;
        let state = AirState::standard();
        let result = opening.calculate(10.0, &state, &state, false);
        assert!((result.flow).abs() < 1e-10);
    }

    #[test]
    fn duct_laminar() {
        let duct = Duct::new("Supply", 0.3, 10.0);
        let state = AirState::standard();
        let result = duct.calculate(50.0, &state, &state, false);
        assert!(result.flow > 0.0);
    }

    #[test]
    fn duct_friction_factor() {
        let duct = Duct::new("Duct", 0.3, 10.0);
        // Laminar
        let f_lam = duct.friction_factor(1000.0);
        assert!((f_lam - 0.064).abs() < 0.001);
        // Turbulent
        let f_turb = duct.friction_factor(100000.0);
        assert!(f_turb > 0.01 && f_turb < 0.05, "f={f_turb}");
    }

    #[test]
    fn leakage_area_conversion() {
        let ela = EffectiveLeakageArea::new("ELA", 0.01);
        let state = AirState::standard();
        let result = ela.calculate(4.0, &state, &state, false);
        // At reference pressure (4 Pa), flow should be based on leakage area
        assert!(result.flow > 0.0);
    }

    #[test]
    fn component_enum_dispatch() {
        let comp = Component::Crack(SurfaceCrack::new("C", 0.001, 0.65));
        let state = AirState::standard();
        let result = comp.calculate(10.0, &state, &state, false);
        assert!(result.flow > 0.0);
    }
}
