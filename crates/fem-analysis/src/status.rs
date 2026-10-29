//! Per-case status triple (guide §14). Construction is validated so that
//! contradictory combinations cannot be represented in results.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Completion {
    Complete,
    Partial,
    Cancelled,
    Unsupported,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhysicalStatus {
    ResponseComputed,
    Mechanism,
    PhysicalLimit,
    NumericalNonConvergence,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityStatus {
    Verified,
    NotVerified,
    Approximate,
    OutOfProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawStatus")]
pub struct CaseStatus {
    completion: Completion,
    physical_status: PhysicalStatus,
    compatibility_status: CompatibilityStatus,
}

#[derive(Deserialize)]
struct RawStatus {
    completion: Completion,
    physical_status: PhysicalStatus,
    compatibility_status: CompatibilityStatus,
}

impl TryFrom<RawStatus> for CaseStatus {
    type Error = String;
    fn try_from(r: RawStatus) -> Result<Self, String> {
        CaseStatus::new(r.completion, r.physical_status, r.compatibility_status)
    }
}

impl CaseStatus {
    pub fn new(c: Completion, p: PhysicalStatus, k: CompatibilityStatus) -> Result<Self, String> {
        use Completion as C;
        use PhysicalStatus as P;
        let ok = match c {
            C::Complete => p == P::ResponseComputed,
            C::Partial => matches!(p, P::ResponseComputed | P::PhysicalLimit | P::NumericalNonConvergence),
            C::Cancelled | C::Unsupported => p == P::Indeterminate,
            C::Error => matches!(p, P::Mechanism | P::NumericalNonConvergence | P::Indeterminate),
        };
        if !ok {
            return Err(format!("contradictory status: completion {c:?} with physical status {p:?}"));
        }
        if k == CompatibilityStatus::Verified && c != C::Complete {
            return Err(format!("compatibility 'verified' requires a complete case, got {c:?}"));
        }
        Ok(Self { completion: c, physical_status: p, compatibility_status: k })
    }

    pub fn completion(&self) -> Completion {
        self.completion
    }
    pub fn physical_status(&self) -> PhysicalStatus {
        self.physical_status
    }
    pub fn compatibility_status(&self) -> CompatibilityStatus {
        self.compatibility_status
    }

    /// The engine alone never claims verified compatibility.
    /// Callers pass only valid pairs; checked in debug builds and by tests.
    pub(crate) fn engine(c: Completion, p: PhysicalStatus) -> Self {
        debug_assert!(Self::new(c, p, CompatibilityStatus::NotVerified).is_ok());
        Self { completion: c, physical_status: p, compatibility_status: CompatibilityStatus::NotVerified }
    }
}

impl Serialize for CaseStatus {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("CaseStatus", 3)?;
        st.serialize_field("completion", &self.completion)?;
        st.serialize_field("physical_status", &self.physical_status)?;
        st.serialize_field("compatibility_status", &self.compatibility_status)?;
        st.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contradictions_rejected() {
        use CompatibilityStatus as K;
        assert!(CaseStatus::new(Completion::Complete, PhysicalStatus::Mechanism, K::NotVerified).is_err());
        assert!(CaseStatus::new(Completion::Error, PhysicalStatus::ResponseComputed, K::NotVerified).is_err());
        assert!(CaseStatus::new(Completion::Partial, PhysicalStatus::ResponseComputed, K::Verified).is_err());
        assert!(CaseStatus::new(Completion::Unsupported, PhysicalStatus::Mechanism, K::OutOfProfile).is_err());
        assert!(CaseStatus::new(Completion::Error, PhysicalStatus::Mechanism, K::NotVerified).is_ok());
    }

    #[test]
    fn deserialization_validates() {
        let bad = r#"{"completion":"complete","physical_status":"mechanism","compatibility_status":"not_verified"}"#;
        assert!(serde_json::from_str::<CaseStatus>(bad).is_err());
        let good =
            r#"{"completion":"complete","physical_status":"response_computed","compatibility_status":"not_verified"}"#;
        assert!(serde_json::from_str::<CaseStatus>(good).is_ok());
    }
}
