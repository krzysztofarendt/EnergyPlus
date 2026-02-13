//! Performance curve evaluation for EnergyPlus-rs.
//!
//! Supports all EnergyPlus curve types: polynomial, exponential, sigmoid, etc.
//! Ported from EnergyPlus CurveManager.cc.

/// Input variable limits (min/max clamping).
#[derive(Debug, Clone, Copy, Default)]
pub struct Limits {
    pub min: f64,
    pub max: f64,
    pub min_present: bool,
    pub max_present: bool,
}

impl Limits {
    pub fn new(min: f64, max: f64) -> Self {
        Self {
            min,
            max,
            min_present: true,
            max_present: true,
        }
    }

    pub fn clamp(&self, val: f64) -> f64 {
        let mut v = val;
        if self.min_present {
            v = v.max(self.min);
        }
        if self.max_present {
            v = v.min(self.max);
        }
        v
    }
}

/// Curve type enumeration matching EnergyPlus CurveType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveType {
    Linear,
    Quadratic,
    Cubic,
    Quartic,
    BiQuadratic,
    QuadraticLinear,
    CubicLinear,
    BiCubic,
    TriQuadratic,
    Exponent,
    FanPressureRise,
    ExponentialSkewNormal,
    Sigmoid,
    RectangularHyperbola1,
    RectangularHyperbola2,
    ExponentialDecay,
    DoubleExponentialDecay,
    QuadLinear,
    QuintLinear,
    ChillerPartLoadWithLift,
    TableLookup,
}

/// Performance curve definition with coefficients, limits, and type.
#[derive(Debug, Clone)]
pub struct Curve {
    pub name: String,
    pub curve_type: CurveType,
    pub num_dims: u8,
    pub coefficients: Vec<f64>,
    pub input_limits: Vec<Limits>,
    pub output_limits: Limits,
}

impl Curve {
    /// Create a new curve with given type and coefficients.
    pub fn new(name: impl Into<String>, curve_type: CurveType, coefficients: Vec<f64>) -> Self {
        let num_dims = curve_type.num_dims();
        let input_limits = vec![Limits::default(); num_dims as usize];
        Self {
            name: name.into(),
            curve_type,
            num_dims,
            coefficients,
            input_limits,
            output_limits: Limits::default(),
        }
    }

    /// Create a linear curve: y = c0 + c1*x.
    pub fn linear(c0: f64, c1: f64) -> Self {
        Self::new("", CurveType::Linear, vec![c0, c1])
    }

    /// Create a quadratic curve: y = c0 + c1*x + c2*x^2.
    pub fn quadratic(c0: f64, c1: f64, c2: f64) -> Self {
        Self::new("", CurveType::Quadratic, vec![c0, c1, c2])
    }

    /// Create a biquadratic curve: y = c0 + c1*x + c2*x^2 + c3*y + c4*y^2 + c5*x*y.
    pub fn biquadratic(c0: f64, c1: f64, c2: f64, c3: f64, c4: f64, c5: f64) -> Self {
        Self::new("", CurveType::BiQuadratic, vec![c0, c1, c2, c3, c4, c5])
    }

    /// Create a cubic curve: y = c0 + c1*x + c2*x^2 + c3*x^3.
    pub fn cubic(c0: f64, c1: f64, c2: f64, c3: f64) -> Self {
        Self::new("", CurveType::Cubic, vec![c0, c1, c2, c3])
    }

    /// Set input limits for a specific dimension.
    pub fn set_input_limits(&mut self, dim: usize, min: f64, max: f64) {
        if dim < self.input_limits.len() {
            self.input_limits[dim] = Limits::new(min, max);
        }
    }

    /// Set output limits.
    pub fn set_output_limits(&mut self, min: Option<f64>, max: Option<f64>) {
        if let Some(min) = min {
            self.output_limits.min = min;
            self.output_limits.min_present = true;
        }
        if let Some(max) = max {
            self.output_limits.max = max;
            self.output_limits.max_present = true;
        }
    }

    /// Evaluate the curve with 1 independent variable.
    pub fn evaluate1(&self, v1: f64) -> f64 {
        let v1 = self.clamp_input(0, v1);
        let val = self.eval_raw(&[v1]);
        self.clamp_output(val)
    }

    /// Evaluate the curve with 2 independent variables.
    pub fn evaluate2(&self, v1: f64, v2: f64) -> f64 {
        let v1 = self.clamp_input(0, v1);
        let v2 = self.clamp_input(1, v2);
        let val = self.eval_raw(&[v1, v2]);
        self.clamp_output(val)
    }

    /// Evaluate the curve with 3 independent variables.
    pub fn evaluate3(&self, v1: f64, v2: f64, v3: f64) -> f64 {
        let v1 = self.clamp_input(0, v1);
        let v2 = self.clamp_input(1, v2);
        let v3 = self.clamp_input(2, v3);
        let val = self.eval_raw(&[v1, v2, v3]);
        self.clamp_output(val)
    }

    /// Evaluate the curve with 4 independent variables.
    pub fn evaluate4(&self, v1: f64, v2: f64, v3: f64, v4: f64) -> f64 {
        let v1 = self.clamp_input(0, v1);
        let v2 = self.clamp_input(1, v2);
        let v3 = self.clamp_input(2, v3);
        let v4 = self.clamp_input(3, v4);
        let val = self.eval_raw(&[v1, v2, v3, v4]);
        self.clamp_output(val)
    }

    /// Evaluate the curve with 5 independent variables.
    pub fn evaluate5(&self, v1: f64, v2: f64, v3: f64, v4: f64, v5: f64) -> f64 {
        let v1 = self.clamp_input(0, v1);
        let v2 = self.clamp_input(1, v2);
        let v3 = self.clamp_input(2, v3);
        let v4 = self.clamp_input(3, v4);
        let v5 = self.clamp_input(4, v5);
        let val = self.eval_raw(&[v1, v2, v3, v4, v5]);
        self.clamp_output(val)
    }

    /// Generic evaluate with a slice of values.
    pub fn evaluate(&self, vars: &[f64]) -> f64 {
        let clamped: Vec<f64> = vars
            .iter()
            .enumerate()
            .map(|(i, &v)| self.clamp_input(i, v))
            .collect();
        let val = self.eval_raw(&clamped);
        self.clamp_output(val)
    }

    fn clamp_input(&self, dim: usize, val: f64) -> f64 {
        if dim < self.input_limits.len() {
            self.input_limits[dim].clamp(val)
        } else {
            val
        }
    }

    fn clamp_output(&self, val: f64) -> f64 {
        self.output_limits.clamp(val)
    }

    /// Raw evaluation without clamping. Uses Horner's method for polynomials.
    fn eval_raw(&self, v: &[f64]) -> f64 {
        let c = &self.coefficients;
        match self.curve_type {
            CurveType::Linear => {
                // c0 + c1*V1
                c[0] + v[0] * c[1]
            }
            CurveType::Quadratic => {
                // c0 + V1*(c1 + V1*c2)  -- Horner's method
                c[0] + v[0] * (c[1] + v[0] * c[2])
            }
            CurveType::Cubic => {
                // c0 + V1*(c1 + V1*(c2 + V1*c3))
                c[0] + v[0] * (c[1] + v[0] * (c[2] + v[0] * c[3]))
            }
            CurveType::Quartic => {
                // c0 + V1*(c1 + V1*(c2 + V1*(c3 + V1*c4)))
                c[0] + v[0] * (c[1] + v[0] * (c[2] + v[0] * (c[3] + v[0] * c[4])))
            }
            CurveType::BiQuadratic => {
                // c0 + V1*(c1 + V1*c2) + V2*(c3 + V2*c4) + V1*V2*c5
                c[0] + v[0] * (c[1] + v[0] * c[2]) + v[1] * (c[3] + v[1] * c[4]) + v[0] * v[1] * c[5]
            }
            CurveType::QuadraticLinear => {
                // (c0 + V1*(c1 + V1*c2)) + (c3 + V1*(c4 + V1*c5))*V2
                (c[0] + v[0] * (c[1] + v[0] * c[2])) + (c[3] + v[0] * (c[4] + v[0] * c[5])) * v[1]
            }
            CurveType::CubicLinear => {
                // (c0 + V1*(c1 + V1*(c2 + V1*c3))) + (c4 + V1*c5)*V2
                (c[0] + v[0] * (c[1] + v[0] * (c[2] + v[0] * c[3]))) + (c[4] + v[0] * c[5]) * v[1]
            }
            CurveType::BiCubic => {
                // 10 coefficients
                c[0] + v[0] * c[1]
                    + v[0] * v[0] * c[2]
                    + v[1] * c[3]
                    + v[1] * v[1] * c[4]
                    + v[0] * v[1] * c[5]
                    + v[0] * v[0] * v[0] * c[6]
                    + v[1] * v[1] * v[1] * c[7]
                    + v[0] * v[0] * v[1] * c[8]
                    + v[0] * v[1] * v[1] * c[9]
            }
            CurveType::TriQuadratic => {
                // 27 coefficients
                let (v1, v2, v3) = (v[0], v[1], v[2]);
                let v1s = v1 * v1;
                let v2s = v2 * v2;
                let v3s = v3 * v3;
                c[0]
                    + c[1] * v1s
                    + c[2] * v1
                    + c[3] * v2s
                    + c[4] * v2
                    + c[5] * v3s
                    + c[6] * v3
                    + c[7] * v1s * v2s
                    + c[8] * v1 * v2
                    + c[9] * v1 * v2s
                    + c[10] * v1s * v2
                    + c[11] * v1s * v3s
                    + c[12] * v1 * v3
                    + c[13] * v1 * v3s
                    + c[14] * v1s * v3
                    + c[15] * v2s * v3s
                    + c[16] * v2 * v3
                    + c[17] * v2 * v3s
                    + c[18] * v2s * v3
                    + c[19] * v1s * v2s * v3s
                    + c[20] * v1s * v2s * v3
                    + c[21] * v1s * v2 * v3s
                    + c[22] * v1 * v2s * v3s
                    + c[23] * v1s * v2 * v3
                    + c[24] * v1 * v2s * v3
                    + c[25] * v1 * v2 * v3s
                    + c[26] * v1 * v2 * v3
            }
            CurveType::Exponent => {
                // c0 + c1 * V1^c2
                c[0] + c[1] * v[0].powf(c[2])
            }
            CurveType::FanPressureRise => {
                // V1*(c0*V1 + c1 + c2*sqrt(V2)) + c3*V2
                v[0] * (c[0] * v[0] + c[1] + c[2] * v[1].sqrt()) + c[3] * v[1]
            }
            CurveType::ExponentialSkewNormal => {
                let z1 = (v[0] - c[0]) / c[1];
                let z2 = (c[3] * v[0] * (c[2] * v[0]).exp() - c[0]) / c[1];
                let z3 = -c[0] / c[1];
                let sqrt_2_inv = 1.0 / 2.0_f64.sqrt();
                let numer = (-0.5 * z1 * z1).exp() * (1.0 + z2.abs().mul_add(sqrt_2_inv, 0.0).erf_sign(z2));
                let denom = (-0.5 * z3 * z3).exp() * (1.0 + z3.abs().mul_add(sqrt_2_inv, 0.0).erf_sign(z3));
                if denom.abs() < 1.0e-30 {
                    0.0
                } else {
                    numer / denom
                }
            }
            CurveType::Sigmoid => {
                // c0 + c1 / (1 + exp((c2 - V1)/c3))^c4
                let exp_val = ((c[2] - v[0]) / c[3]).exp();
                c[0] + c[1] / (1.0 + exp_val).powf(c[4])
            }
            CurveType::RectangularHyperbola1 => {
                // (c0*V1)/(c1+V1) + c2
                (c[0] * v[0]) / (c[1] + v[0]) + c[2]
            }
            CurveType::RectangularHyperbola2 => {
                // (c0*V1)/(c1+V1) + c2*V1
                (c[0] * v[0]) / (c[1] + v[0]) + c[2] * v[0]
            }
            CurveType::ExponentialDecay => {
                // c0 + c1*exp(c2*V1)
                c[0] + c[1] * (c[2] * v[0]).exp()
            }
            CurveType::DoubleExponentialDecay => {
                // c0 + c1*exp(c2*V1) + c3*exp(c4*V1)
                c[0] + c[1] * (c[2] * v[0]).exp() + c[3] * (c[4] * v[0]).exp()
            }
            CurveType::QuadLinear => {
                // c0 + c1*V1 + c2*V2 + c3*V3 + c4*V4
                c[0] + c[1] * v[0] + c[2] * v[1] + c[3] * v[2] + c[4] * v[3]
            }
            CurveType::QuintLinear => {
                // c0 + c1*V1 + c2*V2 + c3*V3 + c4*V4 + c5*V5
                c[0] + c[1] * v[0] + c[2] * v[1] + c[3] * v[2] + c[4] * v[3] + c[5] * v[4]
            }
            CurveType::ChillerPartLoadWithLift => {
                // 12 coefficients
                let (v1, v2, v3) = (v[0], v[1], v[2]);
                c[0]
                    + c[1] * v1
                    + c[2] * v1 * v1
                    + c[3] * v2
                    + c[4] * v2 * v2
                    + c[5] * v1 * v2
                    + c[6] * v1 * v1 * v1
                    + c[7] * v2 * v2 * v2
                    + c[8] * v1 * v1 * v2
                    + c[9] * v1 * v2 * v2
                    + c[10] * v1 * v1 * v2 * v2
                    + c[11] * v3 * v2 * v2 * v2
            }
            CurveType::TableLookup => {
                // Table lookup requires external interpolation data
                // For now, return 0.0 (table data must be loaded separately)
                0.0
            }
        }
    }
}

impl CurveType {
    /// Number of independent variables for this curve type.
    pub fn num_dims(&self) -> u8 {
        match self {
            Self::Linear
            | Self::Quadratic
            | Self::Cubic
            | Self::Quartic
            | Self::Exponent
            | Self::ExponentialSkewNormal
            | Self::Sigmoid
            | Self::RectangularHyperbola1
            | Self::RectangularHyperbola2
            | Self::ExponentialDecay
            | Self::DoubleExponentialDecay => 1,

            Self::BiQuadratic
            | Self::QuadraticLinear
            | Self::CubicLinear
            | Self::BiCubic
            | Self::FanPressureRise => 2,

            Self::TriQuadratic | Self::ChillerPartLoadWithLift => 3,

            Self::QuadLinear => 4,
            Self::QuintLinear => 5,
            Self::TableLookup => 6, // Up to 6D
        }
    }

    /// Number of coefficients for this curve type.
    pub fn num_coefficients(&self) -> usize {
        match self {
            Self::Linear => 2,
            Self::Quadratic => 3,
            Self::Cubic => 4,
            Self::Quartic | Self::QuadLinear => 5,
            Self::BiQuadratic | Self::QuadraticLinear | Self::CubicLinear | Self::QuintLinear => 6,
            Self::BiCubic => 10,
            Self::TriQuadratic => 27,
            Self::Exponent | Self::RectangularHyperbola1 | Self::RectangularHyperbola2 | Self::ExponentialDecay => 3,
            Self::FanPressureRise => 4,
            Self::ExponentialSkewNormal | Self::Sigmoid => 5,
            Self::DoubleExponentialDecay => 5,
            Self::ChillerPartLoadWithLift => 12,
            Self::TableLookup => 0,
        }
    }
}

/// Helper trait for ExponentialSkewNormal erf computation.
trait ErfHelper {
    fn erf_sign(self, sign_val: f64) -> f64;
}

impl ErfHelper for f64 {
    fn erf_sign(self, sign_val: f64) -> f64 {
        // Approximate erf using the standard library
        let abs_val = self;
        let erf_approx = erf_approx(abs_val);
        if sign_val >= 0.0 {
            erf_approx
        } else {
            -erf_approx
        }
    }
}

/// Approximate error function using Abramowitz and Stegun formula 7.1.26.
fn erf_approx(x: f64) -> f64 {
    let a1 = 0.254_829_592;
    let a2 = -0.284_496_736;
    let a3 = 1.421_413_741;
    let a4 = -1.453_152_027;
    let a5 = 1.061_405_429;
    let p = 0.327_591_1;

    let t = 1.0 / (1.0 + p * x.abs());
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();

    if x < 0.0 {
        -y
    } else {
        y
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_curve() {
        let curve = Curve::new("test_linear", CurveType::Linear, vec![1.0, 2.0]);
        assert_eq!(curve.evaluate1(0.0), 1.0);
        assert_eq!(curve.evaluate1(1.0), 3.0);
        assert_eq!(curve.evaluate1(5.0), 11.0);
    }

    #[test]
    fn quadratic_curve() {
        // y = 1 + 2x + 3x²
        let curve = Curve::new("test_quad", CurveType::Quadratic, vec![1.0, 2.0, 3.0]);
        assert_eq!(curve.evaluate1(0.0), 1.0);
        assert_eq!(curve.evaluate1(1.0), 6.0); // 1 + 2 + 3
        assert_eq!(curve.evaluate1(2.0), 17.0); // 1 + 4 + 12
    }

    #[test]
    fn biquadratic_curve() {
        // y = 1 + 2*V1 + 3*V1² + 4*V2 + 5*V2² + 6*V1*V2
        let curve = Curve::new("test_biquad", CurveType::BiQuadratic, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(curve.evaluate2(0.0, 0.0), 1.0);
        // V1=1, V2=0: 1 + 2 + 3 = 6
        assert_eq!(curve.evaluate2(1.0, 0.0), 6.0);
        // V1=0, V2=1: 1 + 4 + 5 = 10
        assert_eq!(curve.evaluate2(0.0, 1.0), 10.0);
        // V1=1, V2=1: 1 + 2 + 3 + 4 + 5 + 6 = 21
        assert_eq!(curve.evaluate2(1.0, 1.0), 21.0);
    }

    #[test]
    fn input_clamping() {
        let mut curve = Curve::new("test_clamp", CurveType::Linear, vec![0.0, 1.0]);
        curve.set_input_limits(0, 0.0, 10.0);
        assert_eq!(curve.evaluate1(-5.0), 0.0); // Clamped to min=0
        assert_eq!(curve.evaluate1(15.0), 10.0); // Clamped to max=10
        assert_eq!(curve.evaluate1(5.0), 5.0); // Within range
    }

    #[test]
    fn output_clamping() {
        let mut curve = Curve::new("test_out_clamp", CurveType::Linear, vec![0.0, 1.0]);
        curve.set_output_limits(Some(0.0), Some(5.0));
        assert_eq!(curve.evaluate1(-3.0), 0.0); // Output clamped to min
        assert_eq!(curve.evaluate1(8.0), 5.0); // Output clamped to max
    }

    #[test]
    fn exponent_curve() {
        // y = 1 + 2 * V1^3
        let curve = Curve::new("test_exp", CurveType::Exponent, vec![1.0, 2.0, 3.0]);
        assert_eq!(curve.evaluate1(0.0), 1.0);
        assert_eq!(curve.evaluate1(1.0), 3.0); // 1 + 2*1^3
        assert_eq!(curve.evaluate1(2.0), 17.0); // 1 + 2*8
    }

    #[test]
    fn sigmoid_curve() {
        // c0=0, c1=1, c2=0, c3=1, c4=1: y = 1/(1+exp(-V1))
        let curve = Curve::new("test_sigmoid", CurveType::Sigmoid, vec![0.0, 1.0, 0.0, 1.0, 1.0]);
        let y = curve.evaluate1(0.0);
        assert!((y - 0.5).abs() < 1e-10, "y={y}");

        let y_pos = curve.evaluate1(10.0);
        assert!(y_pos > 0.99, "y_pos={y_pos}");
    }

    #[test]
    fn exponential_decay_curve() {
        // c0=1, c1=2, c2=-1: y = 1 + 2*exp(-V1)
        let curve = Curve::new("test_expdecay", CurveType::ExponentialDecay, vec![1.0, 2.0, -1.0]);
        assert_eq!(curve.evaluate1(0.0), 3.0); // 1 + 2*1
        let y10 = curve.evaluate1(10.0);
        assert!((y10 - 1.0).abs() < 0.001, "y10={y10}"); // Decayed to ~1
    }

    #[test]
    fn rectangular_hyperbola1() {
        // y = (c0*V1)/(c1+V1) + c2 = (10*V1)/(5+V1) + 2
        let curve = Curve::new("test_rh1", CurveType::RectangularHyperbola1, vec![10.0, 5.0, 2.0]);
        assert_eq!(curve.evaluate1(0.0), 2.0); // 0 + 2
        assert_eq!(curve.evaluate1(5.0), 7.0); // 50/10 + 2
    }

    #[test]
    fn quad_linear() {
        // y = 1 + 2*V1 + 3*V2 + 4*V3 + 5*V4
        let curve = Curve::new("test_ql", CurveType::QuadLinear, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(curve.evaluate4(1.0, 1.0, 1.0, 1.0), 15.0);
    }

    #[test]
    fn num_dims_and_coefficients() {
        assert_eq!(CurveType::Linear.num_dims(), 1);
        assert_eq!(CurveType::Linear.num_coefficients(), 2);
        assert_eq!(CurveType::BiQuadratic.num_dims(), 2);
        assert_eq!(CurveType::BiQuadratic.num_coefficients(), 6);
        assert_eq!(CurveType::TriQuadratic.num_dims(), 3);
        assert_eq!(CurveType::TriQuadratic.num_coefficients(), 27);
    }

    #[test]
    fn generic_evaluate() {
        let curve = Curve::new("test_gen", CurveType::Linear, vec![1.0, 2.0]);
        assert_eq!(curve.evaluate(&[3.0]), 7.0);
    }
}
