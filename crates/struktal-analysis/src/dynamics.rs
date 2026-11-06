//! Lumped masses, natural modes and response spectra (decision 0020).

use struktal_core::{ModalCombination, Snapshot, SpectrumFunction};
use struktal_elements::shell4;
use struktal_math::Csc;
use struktal_math::eigen;
use struktal_math::factor::Factor;
use struktal_math::sparse::TripletBuilder;

use crate::areas::AreaData;
use crate::constraints::DofMap;
use crate::linear_static::{FrameData, Response};
use crate::results::*;
use crate::status::{CaseStatus, Completion, PhysicalStatus};

/// Reduced mass, with what the modal cases need.
pub(crate) struct MassMatrix {
    pub m: Csc,
    /// Free translation of the structure along each axis, in reduced DOFs.
    pub rigid: [Vec<f64>; 3],
    pub total: [f64; 3],
}

/// Half of each frame's mass at each end, and each area's by the share of its
/// corners, on translations; added masses as given. Then Cᵀ M C, so a tied joint's
/// mass acts on its reference through the lever arm.
pub(crate) fn assemble_mass(s: &Snapshot, map: &DofMap, frames: &[FrameData], areas: &[AreaData]) -> MassMatrix {
    let m = &s.model;
    let mut lumped = vec![0.0; 6 * m.joints.len()];
    for (j, joint) in m.joints.iter().enumerate() {
        for d in 0..6 {
            lumped[6 * j + d] += joint.added_mass[d];
        }
    }
    for (fd, &(_, _, sec, mat)) in frames.iter().zip(&s.frame_refs) {
        let half = 0.5 * m.materials[mat].mass_density * m.sections[sec].area * fd.length;
        for d in 0..3 {
            lumped[fd.dofs[d]] += half;
            lumped[fd.dofs[6 + d]] += half;
        }
    }
    for (a, def) in areas.iter().zip(&m.areas) {
        let sec = m.area_sections.iter().find(|x| x.name == def.section).expect("validated section");
        let density = m.materials.iter().find(|x| x.name == sec.material).map_or(0.0, |x| x.mass_density);
        for (n, share) in shell4::nodal_areas(&a.xy).iter().enumerate() {
            for d in 0..3 {
                lumped[a.dofs[6 * n + d]] += density * sec.thickness * share;
            }
        }
    }
    let mut tb = TripletBuilder::new(map.n).expect("size checked with the stiffness");
    for (g, &v) in lumped.iter().enumerate() {
        if v == 0.0 {
            continue;
        }
        for &(ea, ca) in map.terms(g) {
            for &(eb, cb) in map.terms(g) {
                tb.add(ea, eb, ca * cb * v).expect("indices from the map");
            }
        }
    }
    let matrix = tb.to_csc();
    let rigid: [Vec<f64>; 3] = [0, 1, 2].map(|axis| {
        let mut r = vec![0.0; map.n];
        for (g, &e) in map.eq.iter().enumerate() {
            if e != usize::MAX && g % 6 == axis {
                r[e] = 1.0;
            }
        }
        r
    });
    let total = [0, 1, 2].map(|axis| {
        let mr = matrix.mul_vec(&rigid[axis]);
        rigid[axis].iter().zip(&mr).map(|(a, b)| a * b).sum()
    });
    MassMatrix { m: matrix, rigid, total }
}

/// A solved modal case, kept for the spectra that use it.
#[derive(Clone)]
pub(crate) struct Solved {
    pub omega: Vec<f64>,
    pub participation: Vec<[f64; 3]>,
    pub mass_ratio: Vec<[f64; 3]>,
    pub vectors: Vec<Vec<f64>>,
}

const EIGEN_TOL: f64 = 1e-11;
const EIGEN_MAX_ITER: usize = 400;
/// Largest ‖K φ − λ M φ‖ / ‖K φ‖ accepted in a mode.
const EIGEN_RESIDUAL_MAX: f64 = 1e-6;

fn failed(name: &str, kind: &str, why: String) -> CaseResult {
    CaseResult {
        name: name.to_string(),
        kind: kind.to_string(),
        status: CaseStatus::engine(Completion::Error, PhysicalStatus::Indeterminate),
        messages: vec![why],
        joints: vec![],
        frames: vec![],
        checks: None,
        areas: vec![],
        modal: None,
        envelope: None,
    }
}

pub(crate) fn modal_case(
    name: &str,
    modes: usize,
    s: &Snapshot,
    map: &DofMap,
    k: &Csc,
    factor: Option<&Factor>,
    mass: Option<&MassMatrix>,
) -> (CaseResult, Result<Solved, ()>) {
    const KIND: &str = "modal";
    let (Some(factor), Some(mass)) = (factor, mass) else {
        return (failed(name, KIND, "no free degrees of freedom".into()), Err(()));
    };
    let r = match eigen::subspace_iteration(factor, k, &mass.m, modes, EIGEN_TOL, EIGEN_MAX_ITER) {
        Ok(r) => r,
        Err(e) => return (failed(name, KIND, format!("modes: {e}")), Err(())),
    };
    let mut problems = Vec::new();
    if !r.converged {
        problems.push(format!("no convergence in {} iterations", r.iterations));
    }
    if let Some(worst) = r.residuals.iter().cloned().reduce(f64::max)
        && worst > EIGEN_RESIDUAL_MAX
    {
        problems.push(format!("residual {worst:e} above {EIGEN_RESIDUAL_MAX:e}"));
    }
    if let (Some(probe), Some(&top)) = (r.missed_mode_probe, r.values.last())
        && probe < top * (1.0 - 1e-6)
    {
        problems.push(format!("a mode was missed: found λ = {probe:e} below the highest, {top:e}"));
    }
    if !problems.is_empty() {
        return (failed(name, KIND, problems.join("; ")), Err(()));
    }

    let mut cumulative = [0.0; 3];
    let mut out = Vec::with_capacity(r.values.len());
    let mut solved =
        Solved { omega: Vec::new(), participation: Vec::new(), mass_ratio: Vec::new(), vectors: r.vectors.clone() };
    for (i, &lambda) in r.values.iter().enumerate() {
        let omega = lambda.max(0.0).sqrt();
        let mphi = mass.m.mul_vec(&r.vectors[i]);
        let gamma: [f64; 3] = [0, 1, 2].map(|a| mass.rigid[a].iter().zip(&mphi).map(|(p, q)| p * q).sum());
        let ratio: [f64; 3] =
            [0, 1, 2].map(|a| if mass.total[a] > 0.0 { gamma[a] * gamma[a] / mass.total[a] } else { 0.0 });
        for a in 0..3 {
            cumulative[a] += ratio[a];
        }
        let full = map.expand(&r.vectors[i]);
        let shape = full.chunks(6).map(|c| std::array::from_fn(|d| c[d])).collect();
        out.push(ModeResult {
            period: 2.0 * std::f64::consts::PI / omega,
            frequency: omega / (2.0 * std::f64::consts::PI),
            omega,
            participation: gamma,
            mass_ratio: ratio,
            cumulative_mass_ratio: cumulative,
            residual: r.residuals[i],
            shape,
        });
        solved.omega.push(omega);
        solved.participation.push(gamma);
        solved.mass_ratio.push(ratio);
    }
    let mut messages = Vec::new();
    if r.values.len() < modes {
        messages.push(format!("{} of {modes} modes: that many degrees of freedom have mass", r.values.len()));
    }
    let names: Vec<String> = s.model.joints.iter().map(|j| j.name.clone()).collect();
    messages.push(format!("mode shapes per joint, in this order: {}", names.len()));
    let case = CaseResult {
        name: name.to_string(),
        kind: KIND.into(),
        status: CaseStatus::engine(Completion::Complete, PhysicalStatus::ResponseComputed),
        messages,
        joints: vec![],
        frames: vec![],
        checks: None,
        areas: vec![],
        modal: Some(ModalResult {
            total_mass: mass.total,
            modes: out,
            iterations: r.iterations,
            missed_mode_probe: r.missed_mode_probe,
        }),
        envelope: None,
    };
    (case, Ok(solved))
}

/// A spectrum case, resolved against the model.
pub(crate) struct SpectrumSet<'a> {
    pub f: &'a SpectrumFunction,
    pub axis: usize,
    pub scale: f64,
    pub combination: ModalCombination,
    pub damping: f64,
}

/// Linear in the period; flat beyond the ends of the function, and it says so.
fn acceleration(f: &SpectrumFunction, period: f64) -> (f64, bool) {
    let p = &f.points;
    if period <= p[0][0] {
        return (p[0][1], period < p[0][0]);
    }
    if period >= p[p.len() - 1][0] {
        return (p[p.len() - 1][1], period > p[p.len() - 1][0]);
    }
    let k = p.partition_point(|q| q[0] <= period);
    let (a, b) = (p[k - 1], p[k]);
    (a[1] + (b[1] - a[1]) * (period - a[0]) / (b[0] - a[0]), false)
}

/// Der Kiureghian's coefficient for equal damping.
pub fn cqc_rho(wi: f64, wj: f64, z: f64) -> f64 {
    let r = wi / wj;
    let d = (1.0 - r * r).powi(2) + 4.0 * z * z * r * (1.0 + r).powi(2);
    if d == 0.0 { 1.0 } else { 8.0 * z * z * (1.0 + r) * r.powf(1.5) / d }
}

/// The response of each mode to the spectrum.
pub(crate) fn modal_responses<F>(
    solved: &Solved,
    set: &SpectrumSet,
    map: &DofMap,
    respond: F,
) -> (Vec<Response>, Vec<f64>, (usize, f64))
where
    F: Fn(&[f64]) -> Response,
{
    let mut out = Vec::with_capacity(solved.omega.len());
    let mut outside = 0;
    let mut captured = 0.0;
    for i in 0..solved.omega.len() {
        let w = solved.omega[i];
        let (sa, off) = acceleration(set.f, 2.0 * std::f64::consts::PI / w);
        outside += usize::from(off);
        captured += solved.mass_ratio[i][set.axis];
        let q = solved.participation[i][set.axis] * sa * set.scale / (w * w);
        let x: Vec<f64> = solved.vectors[i].iter().map(|v| v * q).collect();
        out.push(respond(&map.expand(&x)));
    }
    (out, solved.omega.clone(), (outside, captured))
}

fn combine(values: &[f64], rho: &[Vec<f64>]) -> f64 {
    let mut sum = 0.0;
    for (i, a) in values.iter().enumerate() {
        for (j, b) in values.iter().enumerate() {
            sum += rho[i][j] * a * b;
        }
    }
    sum.max(0.0).sqrt()
}

/// Every result combined component by component: magnitudes, with no sign, and
/// no longer in equilibrium among them.
pub(crate) fn combine_response(
    name: &str,
    per_mode: &[Response],
    omega: &[f64],
    cover: (usize, f64),
    set: &SpectrumSet,
) -> CaseResult {
    const KIND: &str = "response_spectrum";
    let Some(first) = per_mode.first() else {
        return failed(name, KIND, "the modal case has no modes".into());
    };
    let n = omega.len();
    let rho: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| match set.combination {
                    ModalCombination::Srss => f64::from(i == j),
                    ModalCombination::Cqc => cqc_rho(omega[i], omega[j], set.damping),
                })
                .collect()
        })
        .collect();
    let joints = first
        .joints
        .iter()
        .enumerate()
        .map(|(j, joint)| {
            let at = |get: &dyn Fn(&JointResult) -> f64| {
                let v: Vec<f64> = per_mode.iter().map(|r| get(&r.joints[j])).collect();
                combine(&v, &rho)
            };
            JointResult {
                joint: joint.joint.clone(),
                displacement: std::array::from_fn(|d| at(&|r| r.displacement[d])),
                reaction: joint.reaction.map(|_| std::array::from_fn(|d| at(&|r| r.reaction.map_or(0.0, |x| x[d])))),
                generated: joint.generated,
            }
        })
        .collect();
    let frames = first
        .frames
        .iter()
        .enumerate()
        .map(|(f, frame)| FrameResult {
            frame: frame.frame.clone(),
            length: frame.length,
            end_forces_local: std::array::from_fn(|d| {
                combine(&per_mode.iter().map(|r| r.frames[f].end_forces_local[d]).collect::<Vec<_>>(), &rho)
            }),
            stations: frame
                .stations
                .iter()
                .enumerate()
                .map(|(k, st)| Station {
                    x: st.x,
                    rel: st.rel,
                    actions: std::array::from_fn(|d| {
                        combine(&per_mode.iter().map(|r| r.frames[f].stations[k].actions[d]).collect::<Vec<_>>(), &rho)
                    }),
                })
                .collect(),
        })
        .collect();
    let areas = first
        .areas
        .iter()
        .enumerate()
        .map(|(a, area)| AreaResult {
            area: area.area.clone(),
            center: std::array::from_fn(|d| {
                combine(&per_mode.iter().map(|r| r.areas[a].center[d]).collect::<Vec<_>>(), &rho)
            }),
            corners: std::array::from_fn(|c| {
                std::array::from_fn(|d| {
                    combine(&per_mode.iter().map(|r| r.areas[a].corners[c][d]).collect::<Vec<_>>(), &rho)
                })
            }),
        })
        .collect();
    let mut messages = vec![format!(
        "{} modes, {:.1} % of the mass along {}; no missing-mass correction",
        n,
        cover.1 * 100.0,
        ["X", "Y", "Z"][set.axis]
    )];
    if cover.0 > 0 {
        messages.push(format!("{} mode(s) outside the periods of the function: its end value was used", cover.0));
    }
    CaseResult {
        name: name.to_string(),
        kind: KIND.into(),
        status: CaseStatus::engine(Completion::Complete, PhysicalStatus::ResponseComputed),
        messages,
        joints,
        frames,
        checks: None,
        areas,
        modal: None,
        envelope: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rho_is_one_on_the_diagonal_and_falls_with_separation() {
        assert!((cqc_rho(10.0, 10.0, 0.05) - 1.0).abs() < 1e-12);
        let close = cqc_rho(10.0, 10.5, 0.05);
        let far = cqc_rho(10.0, 40.0, 0.05);
        assert!(close > 0.5 && far < 0.01, "{close} {far}");
        assert!((cqc_rho(10.0, 30.0, 0.05) - cqc_rho(30.0, 10.0, 0.05)).abs() < 1e-12);
    }

    #[test]
    fn spectrum_is_linear_and_flat_at_the_ends() {
        let f = SpectrumFunction { name: "F".into(), points: vec![[0.5, 2.0], [1.0, 4.0], [2.0, 1.0]] };
        assert_eq!(acceleration(&f, 0.75), (3.0, false));
        assert_eq!(acceleration(&f, 1.5), (2.5, false));
        assert_eq!(acceleration(&f, 1.0), (4.0, false));
        assert_eq!(acceleration(&f, 0.1), (2.0, true));
        assert_eq!(acceleration(&f, 5.0), (1.0, true));
        assert_eq!(acceleration(&f, 2.0), (1.0, false));
    }

    #[test]
    fn combination_of_opposite_modes_keeps_the_correlation() {
        let identity = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        assert!((combine(&[3.0, 4.0], &identity) - 5.0).abs() < 1e-14);
        // Fully correlated modes add algebraically.
        let full = vec![vec![1.0, 1.0], vec![1.0, 1.0]];
        assert!((combine(&[3.0, -1.0], &full) - 2.0).abs() < 1e-14);
    }
}
