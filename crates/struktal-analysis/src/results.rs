//! Result contracts. All values are SI (N, m, rad) in global axes unless the
//! field name says `local`.

use serde::{Deserialize, Serialize};

use crate::status::CaseStatus;

pub const RESULTS_SCHEMA_VERSION: u32 = 1;
pub const COMPONENTS: [&str; 6] = ["UX", "UY", "UZ", "RX", "RY", "RZ"];
pub const REACTION_COMPONENTS: [&str; 6] = ["FX", "FY", "FZ", "MX", "MY", "MZ"];
pub const STATION_COMPONENTS: [&str; 6] = ["P", "V2", "V3", "T", "M2", "M3"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub manifest: Manifest,
    pub cases: Vec<CaseResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub results_schema_version: u32,
    pub engine: String,
    pub engine_version: String,
    pub git_commit: String,
    pub model_schema_version: u32,
    pub model_sha256: String,
    pub input_units: String,
    pub result_units: String,
    pub precision: String,
    pub threads: usize,
    pub backend: String,
    pub equations: usize,
    pub solver: Option<serde_json::Value>,
    /// Wall-clock seconds per stage (not part of reproducible results).
    #[serde(default)]
    pub timings: Timings,
    /// Present when the stiffness is singular: null-space analysis.
    #[serde(default)]
    pub stability: Option<StabilityReport>,
    /// Frames that the analysis subdivided (decision 0015).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub analysis_mesh: Vec<MeshedFrame>,
    pub messages: Vec<String>,
}

/// Provenance of a subdivided frame: its pieces, from end I to end J.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshedFrame {
    pub frame: String,
    pub pieces: Vec<String>,
    /// Joints between the pieces, generated or existing.
    pub joints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityReport {
    pub method: String,
    pub backend: String,
    pub pivot_eps: f64,
    pub residual_tol: f64,
    pub gap_factor: f64,
    pub spectral_gap_ratio: Option<f64>,
    /// First 64 flagged DOFs (ordering-dependent, indicative only).
    pub flagged_dofs: Vec<String>,
    pub flagged_count: usize,
    pub truncated: bool,
    pub min_unflagged_pivot: f64,
    /// Null vectors found by inverse iteration on K with flagged DOFs fixed.
    pub found_by_iteration: usize,
    pub iterations: usize,
    pub iteration_converged: bool,
    /// Smallest eigenvalue estimate of the fixed stiffness (scaled); the upper
    /// bound requires `spectral_gap_ratio >= gap_factor`.
    pub fixed_min_eigenvalue: Option<f64>,
    pub solve_backward_error: f64,
    pub solves: usize,
    /// Number of verified, independent null vectors.
    pub nullity_lower_bound: usize,
    pub nullity_upper_bound: Option<usize>,
    /// Set only when both bounds agree.
    pub nullity: Option<usize>,
    pub max_residual: f64,
    pub modes: Vec<MechanismMode>,
}

/// A verified null vector. Values are displacements normalized so that the
/// largest translation is 1 (largest rotation if there is no translation).
/// Any basis of the null space is valid: modes are not unique.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MechanismMode {
    pub seed_dof: String,
    pub residual: f64,
    /// Norm of the projection on the six global rigid-body motions about the
    /// centroid (scaled coordinates). 1 means the whole structure moves as a
    /// rigid body (missing supports); intermediate values are not a measure
    /// of locality, see `moving_joints`.
    pub rigid_body_share: f64,
    /// Joints with a non-negligible component (> 1e-8 of the unit scaled
    /// vector). Fewer than `total_joints` means a local mechanism.
    pub moving_joints: usize,
    pub total_joints: usize,
    pub dominant: Vec<ModeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeEntry {
    pub joint: String,
    pub component: String,
    pub value: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timings {
    pub assembly_s: f64,
    pub factorization_s: f64,
    pub solve_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    pub name: String,
    pub kind: String,
    pub status: CaseStatus,
    pub messages: Vec<String>,
    pub joints: Vec<JointResult>,
    pub frames: Vec<FrameResult>,
    pub checks: Option<Checks>,
    /// Present in envelopes, which have no `joints` or `frames` of their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<Envelope>,
}

/// Maximum and minimum per component, with the term that governs each one.
/// In a tie the first term in the combination governs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounds {
    pub max: [f64; 6],
    pub max_case: [String; 6],
    pub min: [f64; 6],
    pub min_case: [String; 6],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub joints: Vec<JointBounds>,
    pub frames: Vec<FrameBounds>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointBounds {
    pub joint: String,
    pub displacement: Bounds,
    pub reaction: Option<Bounds>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub generated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameBounds {
    pub frame: String,
    pub stations: Vec<StationBounds>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationBounds {
    pub x: f64,
    pub rel: f64,
    pub actions: Bounds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointResult {
    pub joint: String,
    pub displacement: [f64; 6],
    /// Present only for joints with at least one restrained component.
    pub reaction: Option<[f64; 6]>,
    /// Created by the analysis mesh, not in the model.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub generated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameResult {
    pub frame: String,
    pub length: f64,
    /// Forces on the element at ends I and J in local axes.
    pub end_forces_local: [f64; 12],
    pub stations: Vec<Station>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub x: f64,
    pub rel: f64,
    /// [P, V2, V3, T, M2, M3], convention in decision 0003.
    pub actions: [f64; 6],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checks {
    /// ‖K u − f‖ / max(‖f‖, ‖K u‖) on the assembled operator.
    pub residual_relative: f64,
    /// Applied load resultant about the global origin [FX..MZ], computed from
    /// load definitions, not from equivalent nodal loads.
    pub applied_resultant: [f64; 6],
    pub reaction_resultant: [f64; 6],
    pub equilibrium_force_error: f64,
    pub equilibrium_moment_error: f64,
    /// ½ uᵀ f with consistent nodal loads.
    pub external_work: f64,
}
