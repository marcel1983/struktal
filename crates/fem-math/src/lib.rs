//! Sparse assembly, ordering and the linear-solver contract.
//!
//! Conceptual operations (guide §6) map as follows in v0.1:
//! `analyze_pattern` → RCM + profile computation inside [`skyline::factorize`],
//! `factorize` → [`skyline::factorize`], `solve` → [`skyline::SkylineFactor::solve`],
//! `apply_operator` → [`sparse::Csc::mul_vec`], error estimation →
//! [`sparse::relative_residual`], diagnostics → [`skyline::FactorDiagnostics`].
//! Separate symbolic reuse across factorizations is pending.

pub mod ordering;
pub mod skyline;
pub mod sparse;

pub use skyline::{BackendCapabilities, FactorDiagnostics, Inertia, MatrixClass, SkylineFactor, SolverError};
pub use sparse::{Csc, SparseError, TripletBuilder, relative_residual};
