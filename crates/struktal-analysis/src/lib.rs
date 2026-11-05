//! Analysis cases, results and run manifest.

pub mod linear_static;
pub mod results;
pub mod status;

pub use linear_static::{Options, run};
pub use results::*;
pub use status::{CaseStatus, CompatibilityStatus, Completion, PhysicalStatus};
