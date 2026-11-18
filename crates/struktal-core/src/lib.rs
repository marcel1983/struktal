//! Canonical model, units and validation for struktal.

pub mod laminate;
pub mod model;
pub mod section;
pub mod units;
pub mod validate;

pub use laminate::{LaminateAbc, Layer, laminate_abc};
pub use model::*;
pub use units::{Dim, ForceUnit, LengthUnit, UnitSystem};
pub use validate::{Issue, LoadError, Snapshot, model_from_json, model_to_json, plane_axis, validate};
