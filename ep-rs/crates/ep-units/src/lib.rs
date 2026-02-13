//! Physical units type system for EnergyPlus-rs.
//!
//! Provides zero-cost newtype wrappers around `f64` for common physical quantities.
//! This prevents mixing incompatible units at compile time while generating
//! identical machine code to raw `f64` operations.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Macro to define a physical quantity newtype wrapping f64.
///
/// Each quantity supports:
/// - Addition/subtraction with same type
/// - Multiplication/division by dimensionless f64
/// - Division of same type yielding dimensionless f64
/// - Display with unit string
macro_rules! quantity {
    ($name:ident, $unit_str:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
        #[repr(transparent)]
        pub struct $name(pub f64);

        impl $name {
            pub const ZERO: Self = Self(0.0);

            #[inline(always)]
            pub const fn new(val: f64) -> Self {
                Self(val)
            }

            #[inline(always)]
            pub const fn value(self) -> f64 {
                self.0
            }

            #[inline(always)]
            pub fn abs(self) -> Self {
                Self(self.0.abs())
            }

            #[inline(always)]
            pub fn is_finite(self) -> bool {
                self.0.is_finite()
            }

            #[inline(always)]
            pub fn is_nan(self) -> bool {
                self.0.is_nan()
            }

            #[inline(always)]
            pub fn max(self, other: Self) -> Self {
                Self(self.0.max(other.0))
            }

            #[inline(always)]
            pub fn min(self, other: Self) -> Self {
                Self(self.0.min(other.0))
            }

            #[inline(always)]
            pub fn clamp(self, min: Self, max: Self) -> Self {
                Self(self.0.clamp(min.0, max.0))
            }
        }

        impl Add for $name {
            type Output = Self;
            #[inline(always)]
            fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }
        }

        impl AddAssign for $name {
            #[inline(always)]
            fn add_assign(&mut self, rhs: Self) {
                self.0 += rhs.0;
            }
        }

        impl Sub for $name {
            type Output = Self;
            #[inline(always)]
            fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }
        }

        impl SubAssign for $name {
            #[inline(always)]
            fn sub_assign(&mut self, rhs: Self) {
                self.0 -= rhs.0;
            }
        }

        impl Neg for $name {
            type Output = Self;
            #[inline(always)]
            fn neg(self) -> Self {
                Self(-self.0)
            }
        }

        impl Mul<f64> for $name {
            type Output = Self;
            #[inline(always)]
            fn mul(self, rhs: f64) -> Self {
                Self(self.0 * rhs)
            }
        }

        impl Mul<$name> for f64 {
            type Output = $name;
            #[inline(always)]
            fn mul(self, rhs: $name) -> $name {
                $name(self * rhs.0)
            }
        }

        impl Div<f64> for $name {
            type Output = Self;
            #[inline(always)]
            fn div(self, rhs: f64) -> Self {
                Self(self.0 / rhs)
            }
        }

        // Dividing same-type quantities yields a dimensionless ratio.
        impl Div<$name> for $name {
            type Output = f64;
            #[inline(always)]
            fn div(self, rhs: $name) -> f64 {
                self.0 / rhs.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                if $unit_str.is_empty() {
                    write!(f, "{}", self.0)
                } else {
                    write!(f, "{} {}", self.0, $unit_str)
                }
            }
        }

        impl std::iter::Sum for $name {
            fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
                Self(iter.map(|q| q.0).sum())
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Thermodynamic quantities
// ---------------------------------------------------------------------------

quantity!(Temperature, "K");
quantity!(TempDelta, "deltaK");
quantity!(Power, "W");
quantity!(Energy, "J");
quantity!(HeatFlux, "W/m2");
quantity!(ThermalConductivity, "W/(m*K)");
quantity!(ThermalResistance, "m2*K/W");
quantity!(SpecificHeat, "J/(kg*K)");
quantity!(Enthalpy, "J/kg");
quantity!(HeatCapacity, "J/K");

// ---------------------------------------------------------------------------
// Flow quantities
// ---------------------------------------------------------------------------

quantity!(MassFlowRate, "kg/s");
quantity!(VolumeFlowRate, "m3/s");
quantity!(Pressure, "Pa");
quantity!(Velocity, "m/s");

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

quantity!(Area, "m2");
quantity!(Length, "m");
quantity!(Volume, "m3");
quantity!(Angle, "rad");

// ---------------------------------------------------------------------------
// Solar / optical (dimensionless fractions)
// ---------------------------------------------------------------------------

quantity!(Irradiance, "W/m2");
quantity!(Illuminance, "lux");
quantity!(Transmittance, "");
quantity!(Absorptance, "");
quantity!(Emissivity, "");
quantity!(Reflectance, "");

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

quantity!(Duration, "s");

// ---------------------------------------------------------------------------
// Humidity and fluid properties
// ---------------------------------------------------------------------------

quantity!(HumidityRatio, "kg/kg");
quantity!(RelativeHumidity, "%");
quantity!(Density, "kg/m3");
quantity!(DynamicViscosity, "Pa*s");
quantity!(KinematicViscosity, "m2/s");

// ---------------------------------------------------------------------------
// Dimensionless
// ---------------------------------------------------------------------------

quantity!(Dimensionless, "");

// ===========================================================================
// Temperature convenience methods
// ===========================================================================

impl Temperature {
    /// Convert from Celsius to internal Kelvin representation.
    #[inline(always)]
    pub fn from_celsius(c: f64) -> Self {
        Self(c + 273.15)
    }

    /// Convert from Fahrenheit to internal Kelvin representation.
    #[inline(always)]
    pub fn from_fahrenheit(f: f64) -> Self {
        Self((f - 32.0) * 5.0 / 9.0 + 273.15)
    }

    /// Get value in Celsius for display/output.
    #[inline(always)]
    pub fn to_celsius(self) -> f64 {
        self.0 - 273.15
    }

    /// Get value in Fahrenheit for display/output.
    #[inline(always)]
    pub fn to_fahrenheit(self) -> f64 {
        (self.0 - 273.15) * 9.0 / 5.0 + 32.0
    }

    /// Difference between two temperatures yields a TempDelta.
    #[inline(always)]
    pub fn delta(self, other: Temperature) -> TempDelta {
        TempDelta(self.0 - other.0)
    }
}

impl TempDelta {
    /// Create from Celsius delta (same magnitude as Kelvin delta).
    #[inline(always)]
    pub fn from_celsius(c: f64) -> Self {
        Self(c)
    }
}

// ===========================================================================
// Angle convenience methods
// ===========================================================================

impl Angle {
    /// Convert from degrees to radians.
    #[inline(always)]
    pub fn from_degrees(deg: f64) -> Self {
        Self(deg * std::f64::consts::PI / 180.0)
    }

    /// Get value in degrees.
    #[inline(always)]
    pub fn to_degrees(self) -> f64 {
        self.0 * 180.0 / std::f64::consts::PI
    }

    #[inline(always)]
    pub fn sin(self) -> f64 {
        self.0.sin()
    }

    #[inline(always)]
    pub fn cos(self) -> f64 {
        self.0.cos()
    }

    #[inline(always)]
    pub fn tan(self) -> f64 {
        self.0.tan()
    }

    #[inline(always)]
    pub fn asin(val: f64) -> Self {
        Self(val.asin())
    }

    #[inline(always)]
    pub fn acos(val: f64) -> Self {
        Self(val.acos())
    }

    #[inline(always)]
    pub fn atan2(y: f64, x: f64) -> Self {
        Self(y.atan2(x))
    }
}

// ===========================================================================
// Duration convenience methods
// ===========================================================================

impl Duration {
    #[inline(always)]
    pub fn from_hours(h: f64) -> Self {
        Self(h * 3600.0)
    }

    #[inline(always)]
    pub fn from_minutes(m: f64) -> Self {
        Self(m * 60.0)
    }

    #[inline(always)]
    pub fn to_hours(self) -> f64 {
        self.0 / 3600.0
    }

    #[inline(always)]
    pub fn to_minutes(self) -> f64 {
        self.0 / 60.0
    }
}

// ===========================================================================
// Pressure convenience methods
// ===========================================================================

impl Pressure {
    /// Standard atmospheric pressure (101325 Pa).
    pub const STANDARD_ATMOSPHERE: Self = Self(101325.0);
}

// ===========================================================================
// Cross-quantity operations
// ===========================================================================

// HeatFlux * Area = Power
impl Mul<Area> for HeatFlux {
    type Output = Power;
    #[inline(always)]
    fn mul(self, rhs: Area) -> Power {
        Power(self.0 * rhs.0)
    }
}

// Power / Area = HeatFlux
impl Div<Area> for Power {
    type Output = HeatFlux;
    #[inline(always)]
    fn div(self, rhs: Area) -> HeatFlux {
        HeatFlux(self.0 / rhs.0)
    }
}

// Power * Duration = Energy
impl Mul<Duration> for Power {
    type Output = Energy;
    #[inline(always)]
    fn mul(self, rhs: Duration) -> Energy {
        Energy(self.0 * rhs.0)
    }
}

// Energy / Duration = Power
impl Div<Duration> for Energy {
    type Output = Power;
    #[inline(always)]
    fn div(self, rhs: Duration) -> Power {
        Power(self.0 / rhs.0)
    }
}

// Enthalpy * MassFlowRate = Power
impl Mul<MassFlowRate> for Enthalpy {
    type Output = Power;
    #[inline(always)]
    fn mul(self, rhs: MassFlowRate) -> Power {
        Power(self.0 * rhs.0)
    }
}

// MassFlowRate * Enthalpy = Power
impl Mul<Enthalpy> for MassFlowRate {
    type Output = Power;
    #[inline(always)]
    fn mul(self, rhs: Enthalpy) -> Power {
        Power(self.0 * rhs.0)
    }
}

// MassFlowRate * SpecificHeat = HeatCapacity/s (Power/K)
// This is used in Q = m_dot * Cp * DeltaT calculations.
// We represent the intermediate as f64 via manual calculation.

// SpecificHeat * TempDelta = Enthalpy
impl Mul<TempDelta> for SpecificHeat {
    type Output = Enthalpy;
    #[inline(always)]
    fn mul(self, rhs: TempDelta) -> Enthalpy {
        Enthalpy(self.0 * rhs.0)
    }
}

// ThermalConductivity * Area / Length gives Power/K — used in conduction.
// Handled via raw f64 in practice for complex expressions.

// Density * VolumeFlowRate = MassFlowRate
impl Mul<VolumeFlowRate> for Density {
    type Output = MassFlowRate;
    #[inline(always)]
    fn mul(self, rhs: VolumeFlowRate) -> MassFlowRate {
        MassFlowRate(self.0 * rhs.0)
    }
}

// MassFlowRate / Density = VolumeFlowRate
impl Div<Density> for MassFlowRate {
    type Output = VolumeFlowRate;
    #[inline(always)]
    fn div(self, rhs: Density) -> VolumeFlowRate {
        VolumeFlowRate(self.0 / rhs.0)
    }
}

// Length * Length = Area
impl Mul<Length> for Length {
    type Output = Area;
    #[inline(always)]
    fn mul(self, rhs: Length) -> Area {
        Area(self.0 * rhs.0)
    }
}

// Area * Length = Volume
impl Mul<Length> for Area {
    type Output = Volume;
    #[inline(always)]
    fn mul(self, rhs: Length) -> Volume {
        Volume(self.0 * rhs.0)
    }
}

// Irradiance * Area = Power (same dimension as HeatFlux * Area)
impl Mul<Area> for Irradiance {
    type Output = Power;
    #[inline(always)]
    fn mul(self, rhs: Area) -> Power {
        Power(self.0 * rhs.0)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature_conversions() {
        let t = Temperature::from_celsius(100.0);
        assert!((t.value() - 373.15).abs() < 1e-10);
        assert!((t.to_celsius() - 100.0).abs() < 1e-10);

        let t2 = Temperature::from_fahrenheit(212.0);
        assert!((t2.to_celsius() - 100.0).abs() < 1e-10);

        let boiling = Temperature::from_celsius(100.0);
        let freezing = Temperature::from_celsius(0.0);
        let delta = boiling.delta(freezing);
        assert!((delta.value() - 100.0).abs() < 1e-10);
    }

    #[test]
    fn angle_conversions() {
        let a = Angle::from_degrees(180.0);
        assert!((a.value() - std::f64::consts::PI).abs() < 1e-10);
        assert!((a.to_degrees() - 180.0).abs() < 1e-10);
        assert!(a.sin().abs() < 1e-10);
        assert!((a.cos() + 1.0).abs() < 1e-10);
    }

    #[test]
    fn duration_conversions() {
        let d = Duration::from_hours(1.0);
        assert!((d.value() - 3600.0).abs() < 1e-10);
        assert!((d.to_minutes() - 60.0).abs() < 1e-10);
    }

    #[test]
    fn arithmetic_same_type() {
        let a = Power::new(100.0);
        let b = Power::new(50.0);
        assert_eq!((a + b).value(), 150.0);
        assert_eq!((a - b).value(), 50.0);
        assert_eq!((-a).value(), -100.0);
        assert_eq!((a * 2.0).value(), 200.0);
        assert_eq!((2.0 * a).value(), 200.0);
        assert_eq!((a / 2.0).value(), 50.0);
        // Same-type division yields dimensionless ratio
        assert_eq!(a / b, 2.0);
    }

    #[test]
    fn add_assign_sub_assign() {
        let mut p = Power::new(100.0);
        p += Power::new(50.0);
        assert_eq!(p.value(), 150.0);
        p -= Power::new(25.0);
        assert_eq!(p.value(), 125.0);
    }

    #[test]
    fn cross_quantity_heat_flux_area() {
        let flux = HeatFlux::new(500.0);
        let area = Area::new(10.0);
        let power: Power = flux * area;
        assert_eq!(power.value(), 5000.0);

        let flux_back: HeatFlux = power / area;
        assert_eq!(flux_back.value(), 500.0);
    }

    #[test]
    fn cross_quantity_power_duration() {
        let p = Power::new(1000.0);
        let d = Duration::from_hours(1.0);
        let e: Energy = p * d;
        assert_eq!(e.value(), 3_600_000.0);

        let p_back: Power = e / d;
        assert!((p_back.value() - 1000.0).abs() < 1e-10);
    }

    #[test]
    fn cross_quantity_enthalpy_mass_flow() {
        let h = Enthalpy::new(50_000.0);
        let mdot = MassFlowRate::new(2.0);
        let p: Power = h * mdot;
        assert_eq!(p.value(), 100_000.0);
        let p2: Power = mdot * h;
        assert_eq!(p2.value(), 100_000.0);
    }

    #[test]
    fn cross_quantity_density_volume_flow() {
        let rho = Density::new(1000.0);
        let vdot = VolumeFlowRate::new(0.01);
        let mdot: MassFlowRate = rho * vdot;
        assert_eq!(mdot.value(), 10.0);

        let vdot_back: VolumeFlowRate = mdot / rho;
        assert!((vdot_back.value() - 0.01).abs() < 1e-12);
    }

    #[test]
    fn cross_quantity_length_area_volume() {
        let l = Length::new(3.0);
        let w = Length::new(4.0);
        let a: Area = l * w;
        assert_eq!(a.value(), 12.0);

        let h = Length::new(2.5);
        let v: Volume = a * h;
        assert_eq!(v.value(), 30.0);
    }

    #[test]
    fn cross_quantity_specific_heat_temp_delta() {
        let cp = SpecificHeat::new(4186.0);
        let dt = TempDelta::new(10.0);
        let h: Enthalpy = cp * dt;
        assert_eq!(h.value(), 41860.0);
    }

    #[test]
    fn display_formatting() {
        assert_eq!(format!("{}", Temperature::from_celsius(20.0)), "293.15 K");
        assert_eq!(format!("{}", Power::new(1000.0)), "1000 W");
        assert_eq!(format!("{}", Transmittance::new(0.8)), "0.8");
    }

    #[test]
    fn sum_iterator() {
        let powers = vec![Power::new(100.0), Power::new(200.0), Power::new(300.0)];
        let total: Power = powers.into_iter().sum();
        assert_eq!(total.value(), 600.0);
    }

    #[test]
    fn min_max_clamp() {
        let a = Temperature::from_celsius(10.0);
        let b = Temperature::from_celsius(30.0);
        assert_eq!(a.max(b), b);
        assert_eq!(a.min(b), a);

        let c = Temperature::from_celsius(50.0);
        assert_eq!(c.clamp(a, b), b);
    }

    #[test]
    fn standard_atmosphere() {
        assert_eq!(Pressure::STANDARD_ATMOSPHERE.value(), 101325.0);
    }

    #[test]
    fn zero_default() {
        assert_eq!(Power::ZERO.value(), 0.0);
        assert_eq!(Power::default().value(), 0.0);
    }
}
