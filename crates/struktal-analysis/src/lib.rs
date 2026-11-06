//! Analysis cases, results and run manifest.

pub mod areas;
pub mod constraints;
mod dynamics;
pub mod linear_static;
pub mod mesh;
pub mod results;
pub mod status;

pub use linear_static::{Options, run};
pub use results::*;
pub use status::{CaseStatus, CompatibilityStatus, Completion, PhysicalStatus};
