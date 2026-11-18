//! Equation numbering with kinematic constraints eliminated exactly
//! (decision 0011): u = C x, with x the independent DOFs. The reduced system
//! is Cᵀ K C x = Cᵀ f, which stays symmetric positive semidefinite, so the
//! SPD backends and the mechanism diagnosis apply with no penalty stiffness.
//!
//! A tied DOF of joint s with reference joint m and r = x_s − x_m follows
//! u_s = u_m + θ_m × r for the rotations that act through a lever arm
//! (the normal one in a diaphragm, all in a body, none in an equal), and
//! θ_s = θ_m.

use std::collections::HashSet;

use struktal_core::{AreaBehavior, Snapshot, plane_axis};

/// Restraints the analysis uses: the model's own, plus the four out-of-plane
/// DOFs that a plane or axisymmetric area leaves without stiffness (decisions
/// 0033 and 0034).
///
/// Both are two-dimensional idealisations: their element has two degrees of
/// freedom per node, in the plane of the section, and nothing else. Holding the
/// other four is not a modelling choice the user has to make, and it is not
/// written back to the model: it belongs to the analysis of that area. The four
/// are the translation normal to the plane and the three rotations; validation
/// has already refused an area that is not parallel to a global axis plane, so
/// the normal is one component.
pub fn held_joints(s: &Snapshot) -> Vec<[bool; 6]> {
    let m = &s.model;
    let mut held: Vec<[bool; 6]> = m.joints.iter().map(|j| j.restraint).collect();
    let flat: HashSet<&str> = m
        .area_sections
        .iter()
        .filter(|sec| matches!(sec.behavior, AreaBehavior::Plane | AreaBehavior::Axisymmetric))
        .map(|sec| sec.name.as_str())
        .collect();
    for a in &m.areas {
        if !flat.contains(a.section.as_str()) {
            continue;
        }
        let pts: Vec<[f64; 3]> =
            a.joints.iter().filter_map(|n| s.joint_index.get(n)).map(|&k| s.joint_xyz(k)).collect();
        let Some(axis) = plane_axis(&pts) else { continue };
        for name in &a.joints {
            if let Some(&k) = s.joint_index.get(name) {
                held[k][axis] = true;
                held[k][3..6].fill(true);
            }
        }
    }
    held
}

use crate::linear_static::cross;

/// A DOF depends on at most one translation and two rotations of the reference.
#[derive(Debug, Clone, Copy)]
struct Terms {
    len: u8,
    t: [(usize, f64); 3],
}

impl Terms {
    const NONE: Terms = Terms { len: 0, t: [(0, 0.0); 3] };

    fn push(&mut self, dof: usize, c: f64) {
        self.t[self.len as usize] = (dof, c);
        self.len += 1;
    }

    fn as_slice(&self) -> &[(usize, f64)] {
        &self.t[..self.len as usize]
    }
}

/// Relative rounding level of an element stiffness matrix after rotation.
const ROUNDING_LEVEL: f64 = 1e-13;

/// Square matrix over `eqs`, row-major. Rigid-body entries are exactly 0.
#[derive(Debug, Clone)]
pub struct ReducedElement {
    pub eqs: Vec<usize>,
    pub k: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct DofMap {
    /// Independent DOFs.
    pub n: usize,
    /// Per global DOF: its equation if independent, `usize::MAX` otherwise.
    pub eq: Vec<usize>,
    /// Per global DOF: row of C.
    terms: Vec<Terms>,
}

impl DofMap {
    pub fn build(s: &Snapshot) -> Self {
        let m = &s.model;
        let ndof = 6 * m.joints.len();
        // Tied DOF -> its row of C, still in global reference DOFs.
        let mut tied_rows: Vec<Option<Terms>> = vec![None; ndof];
        for c in &m.constraints {
            let Some((first, rest)) = c.joints().split_first() else { continue };
            let master = s.joint_index[first];
            let tied = c.tied_components();
            let levers = c.lever_rotations();
            let xm = s.joint_xyz(master);
            for name in rest {
                let slave = s.joint_index[name];
                let xs = s.joint_xyz(slave);
                let r = [0, 1, 2].map(|k| xs[k] - xm[k]);
                for d in (0..6).filter(|&d| tied[d]) {
                    let mut row = Terms::NONE;
                    row.push(6 * master + d, 1.0);
                    if d < 3 {
                        for w in (0..3).filter(|&w| levers[w]) {
                            let mut axis = [0.0; 3];
                            axis[w] = 1.0;
                            let arm = cross(axis, r)[d];
                            if arm != 0.0 {
                                row.push(6 * master + 3 + w, arm);
                            }
                        }
                    }
                    tied_rows[6 * slave + d] = Some(row);
                }
            }
        }

        let held = held_joints(s);
        let mut eq = vec![usize::MAX; ndof];
        let mut count = 0usize;
        for j in 0..m.joints.len() {
            for d in 0..6 {
                if !held[j][d] && tied_rows[6 * j + d].is_none() {
                    eq[6 * j + d] = count;
                    count += 1;
                }
            }
        }

        let mut terms = vec![Terms::NONE; ndof];
        for g in 0..ndof {
            let t = &mut terms[g];
            match &tied_rows[g] {
                None => {
                    if eq[g] != usize::MAX {
                        t.push(eq[g], 1.0);
                    }
                }
                // Validation rejects restraints on tied components, so the
                // reference DOFs always have an equation.
                Some(row) => {
                    for &(reference, c) in row.as_slice() {
                        t.push(eq[reference], c);
                    }
                }
            }
        }
        DofMap { n: count, eq, terms }
    }

    /// Row of C for a global DOF: (equation, coefficient). Empty if restrained.
    pub fn terms(&self, g: usize) -> &[(usize, f64)] {
        self.terms[g].as_slice()
    }

    /// Reduced element matrix Cᵀ K C over the equations of its DOFs.
    ///
    /// A bar with its ends tied by a rigid constraint has rigid-body rows in
    /// C, so these entries are exactly zero in theory and rounding noise in
    /// floating point. Noise would then look like stiffness: after diagonal
    /// scaling it can pass as a pivot and hide a mechanism. Entries below the
    /// rounding level of the element matrix are therefore set to zero. This
    /// removes noise, it adds nothing, and elements with no tied DOF never
    /// come here.
    pub fn reduced_element<const N: usize>(&self, dofs: &[usize; N], kg: &[[f64; N]; N]) -> ReducedElement {
        let mut eqs: Vec<usize> = dofs.iter().flat_map(|&g| self.terms(g).iter().map(|&(e, _)| e)).collect();
        eqs.sort_unstable();
        eqs.dedup();
        let pos = |e: usize| eqs.binary_search(&e).unwrap_or(0);
        let n = eqs.len();
        let mut k = vec![0.0; n * n];
        let mut lever = vec![0.0; n * n];
        for (row, &da) in kg.iter().zip(dofs) {
            for &(ea, ca) in self.terms(da) {
                let a = pos(ea);
                for (&kab, &db) in row.iter().zip(dofs) {
                    for &(eb, cb) in self.terms(db) {
                        let b = pos(eb);
                        k[a * n + b] += ca * cb * kab;
                        lever[a * n + b] += (ca * cb).abs();
                    }
                }
            }
        }
        let kmax = kg.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
        for (v, l) in k.iter_mut().zip(&lever) {
            if v.abs() <= ROUNDING_LEVEL * l * kmax {
                *v = 0.0;
            }
        }
        ReducedElement { eqs, k }
    }

    pub fn has_tied_dof(&self, dofs: &[usize]) -> bool {
        dofs.iter().any(|&g| self.eq[g] == usize::MAX && !self.terms(g).is_empty())
    }

    /// Cᵀ f
    pub fn reduce(&self, full: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; self.n];
        for (g, v) in full.iter().enumerate() {
            for &(e, c) in self.terms(g) {
                out[e] += c * v;
            }
        }
        out
    }

    /// C x
    pub fn expand(&self, x: &[f64]) -> Vec<f64> {
        (0..self.terms.len()).map(|g| self.terms(g).iter().map(|&(e, c)| c * x[e]).sum()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(constraint: serde_json::Value) -> Snapshot {
        let text = serde_json::json!({
            "schema_version": 3,
            "materials": [{"name": "S", "e": 2e11, "nu": 0.3}],
            "sections": [{"name": "B", "material": "S", "area": 0.01, "i33": 1e-4, "i22": 1e-4, "j": 2e-4}],
            "joints": [
                {"name": "A", "x": 1.0, "y": 2.0, "z": 3.0},
                {"name": "B", "x": 4.0, "y": 6.0, "z": 8.0},
                {"name": "C", "x": 0.0, "y": 0.0, "z": 0.0, "restraint": [true, true, true, true, true, true]},
            ],
            "frames": [{"name": "F", "i": "C", "j": "B", "section": "B"}],
            "constraints": [constraint],
        })
        .to_string();
        struktal_core::validate(&struktal_core::model_from_json(&text).expect("parse")).expect("valid")
    }

    fn diaphragm(axis: &str) -> serde_json::Value {
        serde_json::json!({"kind": "Diaphragm", "name": "D", "axis": axis, "joints": ["A", "B"]})
    }

    #[test]
    fn diaphragm_rows_are_the_rigid_kinematics_for_every_axis() {
        // r = B - A = (3, 4, 5). With the reference moving by x, B must move
        // by u + θ n × r in the plane and keep its normal translation free.
        for (axis, n) in [("X", 0), ("Y", 1), ("Z", 2)] {
            let s = snapshot(diaphragm(axis));
            let map = DofMap::build(&s);
            // A: 6 independent; B: 3 independent; C: none.
            assert_eq!(map.n, 9, "axis {axis}");
            let x: Vec<f64> = (1..=9).map(|v| v as f64 * 0.1).collect();
            let u = map.expand(&x);
            let (ua, tha) = (&u[0..3], &u[3..6]);
            let mut w = [0.0; 3];
            w[n] = tha[n];
            let arm = cross(w, [3.0, 4.0, 5.0]);
            for d in 0..3 {
                if d != n {
                    assert!((u[6 + d] - ua[d] - arm[d]).abs() < 1e-14, "axis {axis}, translation {d}");
                }
            }
            assert_eq!(u[9 + n], tha[n], "axis {axis}");
            assert!(u[12..].iter().all(|&v| v == 0.0));
        }
    }

    #[test]
    fn body_rows_are_the_full_rigid_motion() {
        let s = snapshot(serde_json::json!({"kind": "Body", "name": "D", "joints": ["A", "B"]}));
        let map = DofMap::build(&s);
        assert_eq!(map.n, 6);
        let x = [0.1, -0.2, 0.3, 0.04, -0.05, 0.06];
        let u = map.expand(&x);
        let arm = cross([x[3], x[4], x[5]], [3.0, 4.0, 5.0]);
        for d in 0..3 {
            assert!((u[6 + d] - x[d] - arm[d]).abs() < 1e-15, "translation {d}");
            assert_eq!(u[9 + d], x[3 + d], "rotation {d}");
        }
    }

    #[test]
    fn equal_rows_have_no_lever_arm_and_tie_only_what_is_selected() {
        let s = snapshot(serde_json::json!({
            "kind": "Equal", "name": "D", "components": [true, false, false, false, false, true], "joints": ["A", "B"]
        }));
        let map = DofMap::build(&s);
        assert_eq!(map.n, 10);
        let x: Vec<f64> = (1..=10).map(|v| v as f64).collect();
        let u = map.expand(&x);
        assert_eq!(u[6], u[0]);
        assert_eq!(u[11], u[5]);
        assert_eq!(&u[7..11], &[7.0, 8.0, 9.0, 10.0]);
    }

    #[test]
    fn bar_inside_a_body_reduces_to_exact_zeros() {
        // Oblique bar and irregular lengths, so the rounding noise is not 0.
        let (xi, xj) = ([1.0, 2.0, 3.0], [4.0, 6.0, 8.0]);
        let p = struktal_elements::frame3d::SectionProps {
            e: 2e11,
            g: 7.7e10,
            area: 0.01,
            i33: 1e-4,
            i22: 2e-4,
            j: 3e-4,
            as2: 0.0,
            as3: 0.0,
        };
        use struktal_elements::frame3d as f;
        let t = f::transformation(&f::local_axes(xi, xj, 17.0));
        let kg = f::to_global(&f::local_stiffness(f::length(xi, xj), &p), &t);
        let s = snapshot(serde_json::json!({"kind": "Body", "name": "D", "joints": ["A", "B"]}));
        let map = DofMap::build(&s);
        let dofs: [usize; 12] = std::array::from_fn(|k| k);
        assert!(map.has_tied_dof(&dofs));
        let r = map.reduced_element(&dofs, &kg);
        assert_eq!(r.eqs, vec![0, 1, 2, 3, 4, 5]);
        assert!(r.k.iter().all(|&v| v == 0.0), "{:?}", r.k);

        // A diaphragm keeps what the plane does not tie: real stiffness stays.
        let s = snapshot(diaphragm("Z"));
        let r = DofMap::build(&s).reduced_element(&dofs, &kg);
        assert!(r.k.iter().any(|&v| v != 0.0));
    }

    #[test]
    fn reduce_is_the_transpose_of_expand() {
        for c in [diaphragm("Z"), serde_json::json!({"kind": "Body", "name": "D", "joints": ["A", "B"]})] {
            let map = DofMap::build(&snapshot(c));
            let f: Vec<f64> = (0..18).map(|v| (v as f64 * 0.7).sin()).collect();
            let x: Vec<f64> = (0..map.n).map(|v| (v as f64 * 1.3).cos()).collect();
            let lhs: f64 = map.expand(&x).iter().zip(&f).map(|(a, b)| a * b).sum();
            let rhs: f64 = map.reduce(&f).iter().zip(&x).map(|(a, b)| a * b).sum();
            assert!((lhs - rhs).abs() < 1e-13);
        }
    }
    /// A model with one plane area, in the plane the corners give.
    fn plane(joints: serde_json::Value, corners: serde_json::Value) -> Snapshot {
        let text = serde_json::json!({
            "schema_version": 11,
            "materials": [{"name": "S", "e": 2e11, "nu": 0.3}],
            "sections": [{"name": "B", "material": "S", "area": 0.01, "i33": 1e-4, "i22": 1e-4, "j": 2e-4}],
            "area_sections": [{"name": "P", "material": "S", "thickness": 0.1, "behavior": "Plane"}],
            "joints": joints,
            "frames": [],
            "areas": [{"name": "Q", "section": "P", "joints": corners}],
        })
        .to_string();
        struktal_core::validate(&struktal_core::model_from_json(&text).expect("parse")).expect("valid")
    }

    #[test]
    fn held_joints_hold_the_normal_and_the_rotations_of_a_plane_area() {
        // A horizontal area: z and the three rotations, plus what the model
        // restrains itself.
        let s = plane(
            serde_json::json!([
                {"name": "A", "x": 0.0, "y": 0.0, "z": 0.0, "restraint": [true, false, false, false, false, false]},
                {"name": "B", "x": 1.0, "y": 0.0, "z": 0.0},
                {"name": "C", "x": 1.0, "y": 1.0, "z": 0.0},
                {"name": "D", "x": 0.0, "y": 1.0, "z": 0.0},
            ]),
            serde_json::json!(["A", "B", "C", "D"]),
        );
        let held = held_joints(&s);
        assert_eq!(held[0], [true, false, true, true, true, true], "the model's UX, then z and the rotations");
        for (k, h) in held.iter().enumerate().skip(1) {
            assert_eq!(*h, [false, false, true, true, true, true], "joint {k}");
        }
        // Four joints x two in-plane DOF, minus the one the model restrains.
        assert_eq!(DofMap::build(&s).n, 7);
    }

    #[test]
    fn held_joints_hold_the_normal_of_a_wall() {
        // The same area standing up in the plane y = 5: y is the normal.
        let s = plane(
            serde_json::json!([
                {"name": "A", "x": 0.0, "y": 5.0, "z": 0.0},
                {"name": "B", "x": 2.0, "y": 5.0, "z": 0.0},
                {"name": "C", "x": 2.0, "y": 5.0, "z": 3.0},
                {"name": "D", "x": 0.0, "y": 5.0, "z": 3.0},
            ]),
            serde_json::json!(["A", "B", "C", "D"]),
        );
        let held = held_joints(&s);
        for (k, h) in held.iter().enumerate() {
            assert_eq!(*h, [false, true, false, true, true, true], "joint {k}");
        }
        assert_eq!(DofMap::build(&s).n, 8, "four joints x two in-plane DOF, none restrained by the model");
    }

    #[test]
    fn held_joints_hold_the_tangential_dof_of_an_axisymmetric_area() {
        // The area stands in the plane x = 0, between r = 1 m and r = 2 m: x
        // is the tangential direction of the revolution, and the element has
        // radial and axial degrees of freedom only (decision 0034).
        let text = serde_json::json!({
            "schema_version": 12,
            "materials": [{"name": "S", "e": 2e11, "nu": 0.3}],
            "sections": [{"name": "B", "material": "S", "area": 0.01, "i33": 1e-4, "i22": 1e-4, "j": 2e-4}],
            "area_sections": [{"name": "R", "material": "S", "thickness": 0.1, "behavior": "Axisymmetric"}],
            "joints": [
                {"name": "A", "x": 0.0, "y": 1.0, "z": 0.0},
                {"name": "B", "x": 0.0, "y": 2.0, "z": 0.0},
                {"name": "C", "x": 0.0, "y": 2.0, "z": 1.0},
                {"name": "D", "x": 0.0, "y": 1.0, "z": 1.0}
            ],
            "frames": [],
            "areas": [{"name": "Q", "section": "R", "joints": ["A", "B", "C", "D"]}],
        })
        .to_string();
        let s = struktal_core::validate(&struktal_core::model_from_json(&text).expect("parse")).expect("valid");

        let held = held_joints(&s);
        for (k, h) in held.iter().enumerate() {
            assert_eq!(*h, [true, false, false, true, true, true], "joint {k}: x and the rotations");
        }
        // Four joints x two degrees of freedom, radial and axial, none held by
        // the model: 8 equations.
        assert_eq!(DofMap::build(&s).n, 8);
    }
}
