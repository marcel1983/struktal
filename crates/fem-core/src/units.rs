//! Unit systems at the input boundary. Internally everything is SI: m, N, s, rad.
//!
//! Only force and length are supported in schema v1; mass, time and temperature
//! units are pending (see spec/units-and-conventions.md).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForceUnit {
    N,
    #[serde(rename = "kN")]
    KN,
    #[serde(rename = "MN")]
    MN,
    #[serde(rename = "kgf")]
    Kgf,
    #[serde(rename = "tonf")]
    Tonf,
    #[serde(rename = "lbf")]
    Lbf,
    #[serde(rename = "kip")]
    Kip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LengthUnit {
    #[serde(rename = "m")]
    M,
    #[serde(rename = "cm")]
    Cm,
    #[serde(rename = "mm")]
    Mm,
    #[serde(rename = "ft")]
    Ft,
    #[serde(rename = "in")]
    In,
}

impl ForceUnit {
    /// Newtons per unit. kgf/tonf use standard gravity 9.80665 m/s² (exact by definition).
    pub fn to_si(self) -> f64 {
        match self {
            ForceUnit::N => 1.0,
            ForceUnit::KN => 1.0e3,
            ForceUnit::MN => 1.0e6,
            ForceUnit::Kgf => 9.806_65,
            ForceUnit::Tonf => 9_806.65,
            ForceUnit::Lbf => 4.448_221_615_260_5,
            ForceUnit::Kip => 4_448.221_615_260_5,
        }
    }
}

impl LengthUnit {
    /// Metres per unit (international foot/inch, exact).
    pub fn to_si(self) -> f64 {
        match self {
            LengthUnit::M => 1.0,
            LengthUnit::Cm => 1.0e-2,
            LengthUnit::Mm => 1.0e-3,
            LengthUnit::Ft => 0.3048,
            LengthUnit::In => 0.0254,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitSystem {
    pub force: ForceUnit,
    pub length: LengthUnit,
}

impl Default for UnitSystem {
    fn default() -> Self {
        Self { force: ForceUnit::N, length: LengthUnit::M }
    }
}

/// Physical dimension as exponents of force and length. Used to convert any
/// quantity in one place instead of scattering factors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dim {
    pub force: i32,
    pub length: i32,
}

impl Dim {
    pub const LENGTH: Dim = Dim { force: 0, length: 1 };
    pub const AREA: Dim = Dim { force: 0, length: 2 };
    pub const INERTIA: Dim = Dim { force: 0, length: 4 };
    pub const FORCE: Dim = Dim { force: 1, length: 0 };
    pub const MOMENT: Dim = Dim { force: 1, length: 1 };
    pub const STRESS: Dim = Dim { force: 1, length: -2 };
    pub const FORCE_PER_LENGTH: Dim = Dim { force: 1, length: -1 };
    pub const FORCE_PER_VOLUME: Dim = Dim { force: 1, length: -3 };
    pub const DIMENSIONLESS: Dim = Dim { force: 0, length: 0 };
}

impl UnitSystem {
    /// Factor that converts a value with dimension `dim` in this system to SI.
    pub fn factor(&self, dim: Dim) -> f64 {
        self.force.to_si().powi(dim.force) * self.length.to_si().powi(dim.length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stress_kn_mm_to_pa() {
        let u = UnitSystem { force: ForceUnit::KN, length: LengthUnit::Mm };
        // 200 kN/mm² = 200e9 Pa
        let f = u.factor(Dim::STRESS);
        assert!((200.0 * f - 200.0e9).abs() <= 1e-3);
    }

    #[test]
    fn inertia_mm4_to_m4() {
        let u = UnitSystem { force: ForceUnit::N, length: LengthUnit::Mm };
        assert!((1.0e8 * u.factor(Dim::INERTIA) - 1.0e-4).abs() <= 1e-18);
    }

    #[test]
    fn si_is_identity() {
        let u = UnitSystem::default();
        for d in [Dim::LENGTH, Dim::STRESS, Dim::MOMENT, Dim::FORCE_PER_VOLUME] {
            assert_eq!(u.factor(d), 1.0);
        }
    }
}
