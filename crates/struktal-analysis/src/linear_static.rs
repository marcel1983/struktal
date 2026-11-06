//! Linear static analysis of 3D frames (P02/P04 scope).

use std::collections::HashMap;

use std::time::Instant;
use struktal_core::{Combination, FrameLoadKind, LoadCase, LoadDirection, Snapshot};
use struktal_elements::frame3d::{self, LocalSpanLoads, Mat12, SectionProps, Vec3};
use struktal_elements::release::Condensation;

use struktal_math::nullspace::{self, NullSpaceOptions, NullVector};
use struktal_math::{Backend, MatrixClass, SolverError, TripletBuilder, relative_residual};

use crate::constraints::DofMap;
use crate::results::*;
use crate::status::{CaseStatus, Completion, PhysicalStatus};

#[derive(Debug, Clone)]
pub struct Options {
    /// Equally spaced output stations per frame (>= 2, includes both ends).
    pub stations: usize,
    /// Memory budget for the factor in bytes.
    pub memory_budget: usize,
    pub backend: Backend,
}

impl Default for Options {
    fn default() -> Self {
        Self { stations: 3, memory_budget: 1 << 30, backend: Backend::default() }
    }
}

struct FrameData {
    length: f64,
    axes: [Vec3; 3],
    k_local: Mat12,
    /// End-release condensation; `None` for a fully connected member.
    cond: Option<Condensation>,
    /// Stiffness acting on joint DOFs (local axes): condensed or `k_local`.
    k_node: Mat12,
    /// From global joint DOFs to local element-end DOFs, offsets included.
    t: Mat12,
    dofs: [usize; 12],
    /// Position of element end I (joint plus offset).
    xi: Vec3,
    props: SectionProps,
}

pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn git_commit() -> String {
    option_env!("STRUKTAL_GIT_SHA").unwrap_or("unknown").to_string()
}

fn global_dir(dir: LoadDirection, axes: &[Vec3; 3]) -> Vec3 {
    match dir {
        LoadDirection::Local1 => axes[0],
        LoadDirection::Local2 => axes[1],
        LoadDirection::Local3 => axes[2],
        LoadDirection::GlobalX => [1.0, 0.0, 0.0],
        LoadDirection::GlobalY => [0.0, 1.0, 0.0],
        LoadDirection::GlobalZ => [0.0, 0.0, 1.0],
        LoadDirection::Gravity => [0.0, 0.0, -1.0],
    }
}

fn to_local(v: Vec3, axes: &[Vec3; 3]) -> Vec3 {
    [0, 1, 2].map(|k| axes[k][0] * v[0] + axes[k][1] * v[1] + axes[k][2] * v[2])
}

pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dof_name(s: &Snapshot, global_dof: usize) -> String {
    format!("joint {} {}", s.model.joints[global_dof / 6].name, COMPONENTS[global_dof % 6])
}

/// Per-case loading: global nodal vector (joint loads + consistent element
/// loads), span loads per frame in local axes, and the applied resultant.
struct CaseLoads {
    f_full: Vec<f64>,
    joint_loads: Vec<f64>,
    span: Vec<LocalSpanLoads>,
    resultant: [f64; 6],
    /// Imposed displacements by global DOF (decision 0012).
    imposed: Vec<f64>,
}

/// Load vector of the free system: applied loads, minus the elastic forces of
/// the support movements (−K u_p), plus the forces of the grounded spring ends
/// that move (k u_g).
fn effective_loads(s: &Snapshot, frames: &[FrameData], cl: &CaseLoads) -> Vec<f64> {
    let mut f = cl.f_full.clone();
    if cl.imposed.iter().all(|&v| v == 0.0) {
        return f;
    }
    let held = |g: usize| s.model.joints[g / 6].restraint[g % 6];
    for fd in frames {
        let up = fd.dofs.map(|d| if held(d) { cl.imposed[d] } else { 0.0 });
        if up.iter().all(|&v| v == 0.0) {
            continue;
        }
        let ku = frame3d::mat_vec(&fd.k_node, &frame3d::mat_vec(&fd.t, &up));
        let fg = frame3d::mat_t_vec(&fd.t, &ku);
        for (k, &d) in fd.dofs.iter().enumerate() {
            f[d] -= fg[k];
        }
    }
    for (g, v) in f.iter_mut().enumerate() {
        if !held(g) {
            *v += s.model.joints[g / 6].springs[g % 6] * cl.imposed[g];
        }
    }
    f
}

fn add_resultant(res: &mut [f64; 6], at: Vec3, force: Vec3) {
    let m = cross(at, force);
    for k in 0..3 {
        res[k] += force[k];
        res[3 + k] += m[k];
    }
}

fn build_case_loads(s: &Snapshot, frames: &[FrameData], loads: &[(usize, f64)]) -> CaseLoads {
    let m = &s.model;
    let ndof = 6 * m.joints.len();
    let mut joint_loads = vec![0.0; ndof];
    let mut span = vec![LocalSpanLoads::default(); frames.len()];
    let mut resultant = [0.0; 6];
    let scale_of: HashMap<usize, f64> = {
        let mut h = HashMap::new();
        for &(p, sc) in loads {
            *h.entry(p).or_insert(0.0) += sc;
        }
        h
    };
    let pscale = |name: &str| s.pattern_index.get(name).and_then(|p| scale_of.get(p)).copied().unwrap_or(0.0);

    for jl in &m.joint_loads {
        let sc = pscale(&jl.pattern);
        if sc == 0.0 {
            continue;
        }
        let j = s.joint_index[&jl.joint];
        for d in 0..6 {
            joint_loads[6 * j + d] += sc * jl.values[d];
        }
        let at = s.joint_xyz(j);
        add_resultant(&mut resultant, at, [0, 1, 2].map(|k| sc * jl.values[k]));
        for k in 0..3 {
            resultant[3 + k] += sc * jl.values[3 + k];
        }
    }
    let mut imposed = vec![0.0; ndof];
    for jd in &m.joint_displacements {
        let sc = pscale(&jd.pattern);
        let j = s.joint_index[&jd.joint];
        for d in 0..6 {
            imposed[6 * j + d] += sc * jd.values[d];
        }
    }
    let add_uniform = |span: &mut [LocalSpanLoads], resultant: &mut [f64; 6], f: usize, w_global: Vec3| {
        let fd = &frames[f];
        let wl = to_local(w_global, &fd.axes);
        for (q, w) in span[f].uniform.iter_mut().zip(wl) {
            *q += w;
        }
        let mid = [0, 1, 2].map(|k| fd.xi[k] + 0.5 * fd.length * fd.axes[0][k]);
        add_resultant(resultant, mid, w_global.map(|c| c * fd.length));
    };
    for p in &m.load_patterns {
        let sc = pscale(&p.name) * p.self_weight_multiplier;
        if sc == 0.0 {
            continue;
        }
        for (f, &(_, _, sec, mat)) in s.frame_refs.iter().enumerate() {
            let w = m.materials[mat].unit_weight * m.sections[sec].area * sc;
            add_uniform(&mut span, &mut resultant, f, [0.0, 0.0, -w]);
        }
    }
    for fl in &m.frame_loads {
        let sc = pscale(&fl.pattern);
        if sc == 0.0 {
            continue;
        }
        let f = s.frame_index[&fl.frame];
        match fl.load {
            FrameLoadKind::Uniform { direction, value } => {
                let g = global_dir(direction, &frames[f].axes).map(|c| c * value * sc);
                add_uniform(&mut span, &mut resultant, f, g);
            }
            FrameLoadKind::Point { direction, rel, value } => {
                let fd = &frames[f];
                let g = global_dir(direction, &fd.axes).map(|c| c * value * sc);
                let a = rel * fd.length;
                span[f].points.push((a, to_local(g, &fd.axes)));
                let at = [0, 1, 2].map(|k| fd.xi[k] + a * fd.axes[0][k]);
                add_resultant(&mut resultant, at, g);
            }
        }
    }
    let mut f_full = joint_loads.clone();
    for (f, fd) in frames.iter().enumerate() {
        let feq = frame3d::equivalent_nodal_loads(fd.length, &fd.props, &span[f]);
        let fnode = fd.cond.as_ref().map_or(feq, |c| c.condense_loads(&feq));
        let fg = frame3d::mat_t_vec(&fd.t, &fnode);
        for (k, &d) in fd.dofs.iter().enumerate() {
            f_full[d] += fg[k];
        }
    }
    CaseLoads { f_full, joint_loads, span, resultant, imposed }
}

/// Null-space analysis after a failed SPD factorization (decision 0006).
fn mechanism_report(
    s: &Snapshot,
    k: &struktal_math::Csc,
    free_to_global: &[usize],
    opts: &Options,
) -> (String, Option<StabilityReport>) {
    let backend = opts.backend.capabilities().name;
    let ns_opts = NullSpaceOptions { budget_bytes: opts.memory_budget, ..Default::default() };
    let r = match nullspace::analyze(opts.backend, k, &ns_opts) {
        Ok(r) => r,
        Err(e) => {
            return (
                format!(
                    "mechanism: stiffness is singular or numerically singular (positive semidefinite by construction, \
                     non-positive pivots in {backend}); null-space analysis unavailable: {e}; nullity >= 1"
                ),
                None,
            );
        }
    };
    let count = match (r.exact(), r.nullity_upper_bound) {
        (Some(n), _) => format!("nullity = {n} (exact)"),
        (None, Some(u)) => format!("nullity >= {} and <= {u}", r.nullity_lower_bound.max(1)),
        (None, None) => format!(
            "nullity >= {} (upper bound not certified: spectral gap ratio {})",
            r.nullity_lower_bound.max(1),
            r.spectral_gap_ratio.map(|g| format!("{g:.1}")).unwrap_or_else(|| "n/a".into())
        ),
    };
    let rigid = rigid_body_basis(s, free_to_global, k);
    let modes: Vec<MechanismMode> =
        r.vectors.iter().take(MAX_REPORTED_MODES).map(|v| describe_mode(s, free_to_global, v, &rigid)).collect();
    let msg = format!(
        "mechanism: {count}; {} verified null vector(s), max scaled residual {:.1e} (tol {:.0e}); {} flagged pivot(s) in {backend}{}. \
         Modes (dominant DOFs): {}",
        r.nullity_lower_bound,
        r.max_residual,
        r.options.residual_tol,
        r.flagged.len(),
        if r.truncated { ", search truncated" } else { "" },
        modes
            .iter()
            .take(6)
            .map(|m| format!(
                "[{} | moves {}/{} joints]",
                m.dominant
                    .iter()
                    .take(3)
                    .map(|d| format!("{} {}", d.joint, d.component))
                    .collect::<Vec<_>>()
                    .join(", "),
                m.moving_joints,
                m.total_joints
            ))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let report = StabilityReport {
        method: "PSD-regularized LDLT + verified null vectors (decision 0006)".into(),
        backend: backend.into(),
        pivot_eps: r.options.pivot_eps,
        residual_tol: r.options.residual_tol,
        gap_factor: r.options.gap_factor,
        spectral_gap_ratio: r.spectral_gap_ratio,
        flagged_dofs: r.flagged.iter().take(64).map(|&e| dof_name(s, free_to_global[e])).collect(),
        flagged_count: r.flagged.len(),
        truncated: r.truncated,
        min_unflagged_pivot: r.min_unflagged_pivot,
        found_by_iteration: r.found_by_iteration,
        iterations: r.iterations,
        iteration_converged: r.iteration_converged,
        fixed_min_eigenvalue: r.fixed_min_eigenvalue,
        solve_backward_error: r.solve_backward_error,
        solves: r.solves,
        nullity_lower_bound: r.nullity_lower_bound,
        nullity_upper_bound: r.nullity_upper_bound,
        nullity: r.exact(),
        max_residual: r.max_residual,
        modes,
    };
    (msg, Some(report))
}

const MAX_REPORTED_MODES: usize = 24;

/// Orthonormal basis (scaled coordinates, free DOFs) of the six global
/// rigid-body motions about the centroid, used only to describe modes.
fn rigid_body_basis(s: &Snapshot, free_to_global: &[usize], k: &struktal_math::Csc) -> Vec<Vec<f64>> {
    let n = free_to_global.len();
    let sc: Vec<f64> = k.diagonal().iter().map(|&d| if d > 0.0 { 1.0 / d.sqrt() } else { 1.0 }).collect();
    let nj = s.model.joints.len().max(1) as f64;
    let mut c = [0.0; 3];
    for j in 0..s.model.joints.len() {
        let p = s.joint_xyz(j);
        for d in 0..3 {
            c[d] += p[d] / nj;
        }
    }
    let mut basis: Vec<Vec<f64>> = Vec::new();
    for mode in 0..6 {
        let mut y = vec![0.0; n];
        for (e, &g) in free_to_global.iter().enumerate() {
            let (j, comp) = (g / 6, g % 6);
            let p = s.joint_xyz(j);
            let r = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
            let val = if mode < 3 {
                if comp == mode { 1.0 } else { 0.0 }
            } else {
                let mut w = [0.0; 3];
                w[mode - 3] = 1.0;
                if comp < 3 { cross(w, r)[comp] } else { w[comp - 3] }
            };
            y[e] = val / sc[e];
        }
        for _ in 0..2 {
            for b in &basis {
                let d: f64 = y.iter().zip(b).map(|(p, q)| p * q).sum();
                y.iter_mut().zip(b).for_each(|(p, q)| *p -= d * q);
            }
        }
        let nr = y.iter().map(|v| v * v).sum::<f64>().sqrt();
        if nr > 1e-9 {
            y.iter_mut().for_each(|v| *v /= nr);
            basis.push(y);
        }
    }
    basis
}

fn describe_mode(s: &Snapshot, free_to_global: &[usize], v: &NullVector, rigid: &[Vec<f64>]) -> MechanismMode {
    let share = rigid
        .iter()
        .map(|b| b.iter().zip(&v.scaled).map(|(p, q)| p * q).sum::<f64>().powi(2))
        .sum::<f64>()
        .sqrt()
        .min(1.0);
    // Display normalization: largest translation = 1 (rotations if none).
    let max_t =
        free_to_global.iter().zip(&v.original).filter(|(g, _)| *g % 6 < 3).fold(0.0_f64, |m, (_, x)| m.max(x.abs()));
    let max_r =
        free_to_global.iter().zip(&v.original).filter(|(g, _)| *g % 6 >= 3).fold(0.0_f64, |m, (_, x)| m.max(x.abs()));
    let norm = if max_t > 1e-12 * max_r.max(1e-300) { max_t } else { max_r.max(f64::MIN_POSITIVE) };
    let mut idx: Vec<usize> = (0..v.scaled.len()).collect();
    idx.sort_by(|&a, &b| v.scaled[b].abs().total_cmp(&v.scaled[a].abs()).then(a.cmp(&b)));
    // Entries below 1e-8 of a unit scaled vector are numerical zeros.
    const NEGLIGIBLE: f64 = 1e-8;
    let mut moving = std::collections::BTreeSet::new();
    for (e, &g) in free_to_global.iter().enumerate() {
        if v.scaled[e].abs() > NEGLIGIBLE {
            moving.insert(g / 6);
        }
    }
    let dominant = idx
        .into_iter()
        .filter(|&e| v.scaled[e].abs() > NEGLIGIBLE)
        .take(8)
        .map(|e| {
            let g = free_to_global[e];
            ModeEntry {
                joint: s.model.joints[g / 6].name.clone(),
                component: COMPONENTS[g % 6].to_string(),
                value: v.original[e] / norm,
            }
        })
        .collect();
    MechanismMode {
        seed_dof: dof_name(s, free_to_global[v.seed]),
        residual: v.residual,
        rigid_body_share: share,
        moving_joints: moving.len(),
        total_joints: s.model.joints.len(),
        dominant,
    }
}

fn failed_case(name: &str, kind: &str, c: Completion, p: PhysicalStatus, messages: Vec<String>) -> CaseResult {
    CaseResult {
        name: name.to_string(),
        kind: kind.to_string(),
        status: CaseStatus::engine(c, p),
        messages,
        joints: vec![],
        frames: vec![],
        checks: None,
    }
}

pub fn run(s: &Snapshot, opts: &Options) -> RunResult {
    let m = &s.model;
    let nj = m.joints.len();
    let mut manifest = Manifest {
        results_schema_version: RESULTS_SCHEMA_VERSION,
        engine: "struktal".into(),
        engine_version: engine_version(),
        git_commit: git_commit(),
        model_schema_version: m.schema_version,
        model_sha256: s.hash.clone(),
        input_units: format!("{:?}, {:?}", s.original_units.force, s.original_units.length),
        result_units: "N, m, rad".into(),
        precision: "f64".into(),
        threads: 1,
        backend: format!("{} {}", opts.backend.capabilities().name, opts.backend.capabilities().version),
        equations: 0,
        solver: None,
        timings: Timings::default(),
        stability: None,
        messages: vec![],
    };

    // Equation numbering: independent DOFs in joint order.
    let map = DofMap::build(s);
    let n = map.n;
    let eq = &map.eq;
    manifest.equations = n;

    let frames: Result<Vec<FrameData>, String> = s
        .frame_refs
        .iter()
        .zip(&m.frames)
        .enumerate()
        .map(|(f, (&(i, j, sec, mat), fr))| {
            let (xi, xj) = s.frame_ends(f);
            let length = frame3d::length(xi, xj);
            let axes = frame3d::local_axes(xi, xj, fr.angle_deg);
            let mt = &m.materials[mat];
            let sc = &m.sections[sec];
            let props = SectionProps {
                e: mt.e,
                g: mt.g.unwrap_or(mt.e / (2.0 * (1.0 + mt.nu))),
                area: sc.area,
                i33: sc.i33,
                i22: sc.i22,
                j: sc.j,
                as2: sc.as2,
                as3: sc.as3,
            };
            let k_local = frame3d::local_stiffness(length, &props);
            let cond = match &fr.releases {
                Some(r) if r.i.iter().chain(&r.j).any(|&x| x) => {
                    let mut released = [false; 12];
                    let mut springs = [0.0; 12];
                    released[..6].copy_from_slice(&r.i);
                    released[6..].copy_from_slice(&r.j);
                    springs[..6].copy_from_slice(&r.i_springs);
                    springs[6..].copy_from_slice(&r.j_springs);
                    Some(
                        Condensation::new(&k_local, released, springs)
                            .map_err(|e| format!("frame {}: {e}", fr.name))?,
                    )
                }
                _ => None,
            };
            let k_node = cond.as_ref().map_or(k_local, |c| c.k_c);
            let t = frame3d::transformation(&axes);
            let t = fr.offsets.map_or(t, |o| frame3d::offset_transformation(&t, o.i, o.j));
            let mut dofs = [0usize; 12];
            for d in 0..6 {
                dofs[d] = 6 * i + d;
                dofs[6 + d] = 6 * j + d;
            }
            Ok(FrameData { length, axes, k_local, cond, k_node, t, dofs, xi, props })
        })
        .collect::<Result<_, String>>();
    let frames: Vec<FrameData> = match frames {
        Ok(f) => f,
        Err(msg) => {
            manifest.messages.push(msg.clone());
            let cases = m
                .load_cases
                .iter()
                .map(|c| c.name())
                .chain(m.combinations.iter().map(|c| c.name()))
                .map(|name| {
                    failed_case(
                        name,
                        "linear_static",
                        Completion::Error,
                        PhysicalStatus::Indeterminate,
                        vec![msg.clone()],
                    )
                })
                .collect();
            return RunResult { manifest, cases };
        }
    };

    let t_asm = Instant::now();
    let mut tb = match TripletBuilder::new(n) {
        Ok(t) => t,
        Err(e) => {
            manifest.messages.push(e.to_string());
            return RunResult { manifest, cases: vec![] };
        }
    };
    for fd in &frames {
        let kg = frame3d::to_global(&fd.k_node, &fd.t);
        if map.has_tied_dof(&fd.dofs) {
            let r = map.reduced_element(&fd.dofs, &kg);
            let size = r.eqs.len();
            for (a, &ea) in r.eqs.iter().enumerate() {
                for (b, &eb) in r.eqs.iter().enumerate() {
                    if let Err(e) = tb.add(ea, eb, r.k[a * size + b]) {
                        manifest.messages.push(e.to_string());
                        return RunResult { manifest, cases: vec![] };
                    }
                }
            }
            continue;
        }
        for (row, &da) in kg.iter().zip(&fd.dofs) {
            for &(ea, ca) in map.terms(da) {
                for (&kab, &db) in row.iter().zip(&fd.dofs) {
                    for &(eb, cb) in map.terms(db) {
                        if let Err(e) = tb.add(ea, eb, ca * cb * kab) {
                            manifest.messages.push(e.to_string());
                            return RunResult { manifest, cases: vec![] };
                        }
                    }
                }
            }
        }
    }
    for (j, joint) in m.joints.iter().enumerate() {
        for (d, &kd) in joint.springs.iter().enumerate() {
            if kd == 0.0 {
                continue;
            }
            for &(ea, ca) in map.terms(6 * j + d) {
                for &(eb, cb) in map.terms(6 * j + d) {
                    if let Err(e) = tb.add(ea, eb, ca * cb * kd) {
                        manifest.messages.push(e.to_string());
                        return RunResult { manifest, cases: vec![] };
                    }
                }
            }
        }
    }
    let k = tb.to_csc();
    manifest.timings.assembly_s = t_asm.elapsed().as_secs_f64();

    let factor = if n == 0 {
        None
    } else {
        let t_fact = Instant::now();
        let fres = struktal_math::factorize(opts.backend, &k, MatrixClass::Spd, opts.memory_budget);
        manifest.timings.factorization_s = t_fact.elapsed().as_secs_f64();
        match fres {
            Ok(f) => {
                manifest.solver = serde_json::to_value(f.diagnostics()).ok();
                Some(f)
            }
            Err(err) => {
                manifest.solver = serde_json::to_value(&err).ok();

                let free_to_global: Vec<usize> = {
                    let mut v = vec![0; n];
                    for (g, &e) in eq.iter().enumerate() {
                        if e != usize::MAX {
                            v[e] = g;
                        }
                    }
                    v
                };
                let (phys, msgs) = match &err {
                    // The assembled K is positive semidefinite by construction
                    // (sum of frame matrices with validated E, G, A, I, J > 0),
                    // so zero and negative pivots both mean (numerical)
                    // singularity, never negative stiffness. Pivot counts
                    // without a rank-revealing method are not a nullity.
                    SolverError::Singular { .. } | SolverError::NotPositiveDefinite { .. } => {
                        let (msg, report) = mechanism_report(s, &k, &free_to_global, opts);
                        manifest.stability = report;
                        (PhysicalStatus::Mechanism, vec![msg])
                    }
                    other => (PhysicalStatus::Indeterminate, vec![other.to_string()]),
                };
                let cases = m
                    .load_cases
                    .iter()
                    .map(|c| failed_case(c.name(), "linear_static", Completion::Error, phys, msgs.clone()))
                    .chain(m.combinations.iter().map(|c| {
                        failed_case(
                            c.name(),
                            "combination_linear_add",
                            Completion::Error,
                            PhysicalStatus::Indeterminate,
                            vec!["depends on a case that did not complete".into()],
                        )
                    }))
                    .collect();
                return RunResult { manifest, cases };
            }
        }
    };

    // Joints with a reaction: a restraint or a grounded spring.
    let restrained_joint: Vec<bool> =
        m.joints.iter().map(|j| j.restraint.iter().any(|&r| r) || j.springs.iter().any(|&k| k > 0.0)).collect();
    let mut by_name: HashMap<String, CaseResult> = HashMap::new();
    let mut out = Vec::new();

    for case in &m.load_cases {
        let LoadCase::LinearStatic { name, loads } = case;
        let pats: Vec<(usize, f64)> = loads.iter().map(|sp| (s.pattern_index[&sp.pattern], sp.scale)).collect();
        let cl = build_case_loads(s, &frames, &pats);
        let rhs = map.reduce(&effective_loads(s, &frames, &cl));
        let t_solve = Instant::now();
        let x = factor.as_ref().map(|f| f.solve(&rhs)).unwrap_or_default();
        manifest.timings.solve_s += t_solve.elapsed().as_secs_f64();
        let residual = if n == 0 { 0.0 } else { relative_residual(&k, &x, &rhs) };
        let mut u = map.expand(&x);
        for (j, joint) in m.joints.iter().enumerate() {
            for d in (0..6).filter(|&d| joint.restraint[d]) {
                u[6 * j + d] = cl.imposed[6 * j + d];
            }
        }

        // Internal nodal forces from element end forces.
        let mut internal = vec![0.0; 6 * nj];
        let mut frame_results = Vec::with_capacity(frames.len());
        for (f, fd) in frames.iter().enumerate() {
            let ue: [f64; 12] = fd.dofs.map(|d| u[d]);
            let feq = frame3d::equivalent_nodal_loads(fd.length, &fd.props, &cl.span[f]);
            let ul = frame3d::mat_vec(&fd.t, &ue);
            // Forces on joints: condensed stiffness and loads.
            let fnode = fd.cond.as_ref().map_or(feq, |c| c.condense_loads(&feq));
            let ku = frame3d::mat_vec(&fd.k_node, &ul);
            let nodal: [f64; 12] = std::array::from_fn(|k| ku[k] - fnode[k]);
            let fg = frame3d::mat_t_vec(&fd.t, &nodal);
            // Forces on the element ends, from the element-end displacements.
            let fl = match &fd.cond {
                None => nodal,
                Some(c) => {
                    let w = c.end_displacements(&ul, &feq);
                    let kw = frame3d::mat_vec(&fd.k_local, &w);
                    std::array::from_fn(|k| kw[k] - feq[k])
                }
            };
            for (kk, &d) in fd.dofs.iter().enumerate() {
                internal[d] += fg[kk];
            }
            let ns = opts.stations.max(2);
            let stations = (0..ns)
                .map(|i| {
                    let rel = i as f64 / (ns - 1) as f64;
                    let x = rel * fd.length;
                    Station { x, rel, actions: frame3d::station_actions(&fl, &cl.span[f], x) }
                })
                .collect();
            frame_results.push(FrameResult {
                frame: m.frames[f].name.clone(),
                length: fd.length,
                end_forces_local: fl,
                stations,
            });
        }

        let mut reaction_res = [0.0; 6];
        // Work of the reactions on the imposed displacements.
        let mut reaction_work = 0.0;
        // Largest single reaction: the scale of the equilibrium check when the
        // reactions balance each other, as under support movements alone.
        let mut gross = [0.0_f64; 2];
        let joints: Vec<JointResult> = (0..nj)
            .map(|j| {
                let reaction = restrained_joint[j].then(|| {
                    let mut r = [0.0; 6];
                    for (d, rd) in r.iter_mut().enumerate() {
                        let g = 6 * j + d;
                        *rd = if m.joints[j].restraint[d] {
                            internal[g] - cl.joint_loads[g]
                        } else {
                            m.joints[j].springs[d] * (cl.imposed[g] - u[g])
                        };
                        reaction_work += *rd * cl.imposed[g];
                    }
                    add_resultant(&mut reaction_res, s.joint_xyz(j), [r[0], r[1], r[2]]);
                    let arm = cross(s.joint_xyz(j), [r[0], r[1], r[2]]);
                    for k in 0..3 {
                        reaction_res[3 + k] += r[3 + k];
                        gross[0] = gross[0].max(r[k].abs());
                        gross[1] = gross[1].max(r[3 + k].abs()).max(arm[k].abs());
                    }
                    r
                });
                let mut disp = [0.0; 6];
                disp.copy_from_slice(&u[6 * j..6 * j + 6]);
                JointResult { joint: m.joints[j].name.clone(), displacement: disp, reaction }
            })
            .collect();

        let fmag = cl.resultant[..3].iter().chain(&reaction_res[..3]).fold(gross[0], |a, v| a.max(v.abs()));
        let mmag = cl.resultant[3..].iter().chain(&reaction_res[3..]).fold(gross[1], |a, v| a.max(v.abs()));
        let ferr = (0..3).map(|k| (cl.resultant[k] + reaction_res[k]).abs()).fold(0.0, f64::max);
        let merr = (3..6).map(|k| (cl.resultant[k] + reaction_res[k]).abs()).fold(0.0, f64::max);
        let checks = Checks {
            residual_relative: residual,
            applied_resultant: cl.resultant,
            reaction_resultant: reaction_res,
            equilibrium_force_error: if fmag > 0.0 { ferr / fmag } else { ferr },
            equilibrium_moment_error: if mmag > 0.0 { merr / mmag } else { merr },
            external_work: 0.5 * (u.iter().zip(&cl.f_full).map(|(a, b)| a * b).sum::<f64>() + reaction_work),
        };
        let mut messages = Vec::new();
        let unbalanced = !restrained_joint.iter().any(|&r| r) && n > 0;
        if unbalanced {
            messages.push("no restrained joints".into());
        }
        let cr = CaseResult {
            name: name.clone(),
            kind: "linear_static".into(),
            status: CaseStatus::engine(Completion::Complete, PhysicalStatus::ResponseComputed),
            messages,
            joints,
            frames: frame_results,
            checks: Some(checks),
        };
        by_name.insert(name.clone(), cr.clone());
        out.push(cr);
    }

    // Combinations in dependency order (validated acyclic).
    let mut pending: Vec<&Combination> = m.combinations.iter().collect();
    while !pending.is_empty() {
        let before = pending.len();
        pending.retain(|c| {
            let Combination::LinearAdd { name, cases } = c;
            if !cases.iter().all(|t| by_name.contains_key(&t.case)) {
                return true;
            }
            let cr = combine(name, cases.iter().map(|t| (&by_name[&t.case], t.scale)).collect());
            by_name.insert(name.clone(), cr.clone());
            out.push(cr);
            false
        });
        if pending.len() == before {
            for c in pending.drain(..) {
                out.push(failed_case(
                    c.name(),
                    "combination_linear_add",
                    Completion::Error,
                    PhysicalStatus::Indeterminate,
                    vec!["unresolved dependency".into()],
                ));
            }
        }
    }
    RunResult { manifest, cases: out }
}

fn combine(name: &str, terms: Vec<(&CaseResult, f64)>) -> CaseResult {
    if terms.iter().any(|(c, _)| c.status.completion() != Completion::Complete) {
        return failed_case(
            name,
            "combination_linear_add",
            Completion::Error,
            PhysicalStatus::Indeterminate,
            vec!["a referenced case is not complete".into()],
        );
    }
    let Some((first, _)) = terms.first() else {
        return failed_case(
            name,
            "combination_linear_add",
            Completion::Error,
            PhysicalStatus::Indeterminate,
            vec!["empty combination".into()],
        );
    };
    let mut joints = first.joints.clone();
    let mut frames = first.frames.clone();
    for j in &mut joints {
        j.displacement = [0.0; 6];
        j.reaction = j.reaction.map(|_| [0.0; 6]);
    }
    for f in &mut frames {
        f.end_forces_local = [0.0; 12];
        for st in &mut f.stations {
            st.actions = [0.0; 6];
        }
    }
    for (c, sc) in &terms {
        for (a, b) in joints.iter_mut().zip(&c.joints) {
            for d in 0..6 {
                a.displacement[d] += sc * b.displacement[d];
            }
            if let (Some(ra), Some(rb)) = (a.reaction.as_mut(), b.reaction) {
                for d in 0..6 {
                    ra[d] += sc * rb[d];
                }
            }
        }
        for (a, b) in frames.iter_mut().zip(&c.frames) {
            for d in 0..12 {
                a.end_forces_local[d] += sc * b.end_forces_local[d];
            }
            for (sa, sb) in a.stations.iter_mut().zip(&b.stations) {
                for d in 0..6 {
                    sa.actions[d] += sc * sb.actions[d];
                }
            }
        }
    }
    CaseResult {
        name: name.to_string(),
        kind: "combination_linear_add".into(),
        status: CaseStatus::engine(Completion::Complete, PhysicalStatus::ResponseComputed),
        messages: vec![format!(
            "terms: {}",
            terms.iter().map(|(c, s)| format!("{s} x {}", c.name)).collect::<Vec<_>>().join(" + ")
        )],
        joints,
        frames,
        checks: None,
    }
}
