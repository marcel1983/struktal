//! Equation numbering with kinematic constraints eliminated exactly
//! (decision 0011): u = C x, with x the independent DOFs. The reduced system
//! is Cᵀ K C x = Cᵀ f, which stays symmetric positive semidefinite, so the
//! SPD backends and the mechanism diagnosis apply with no penalty stiffness.
//!
//! A tied DOF of joint s with reference joint m and r = x_s − x_m follows
//! u_s = u_m + θ_m × r for the rotations that act through a lever arm
//! (the normal one in a diaphragm, all in a body, none in an equal), and
//! θ_s = θ_m.

use struktal_core::Snapshot;

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

        let mut eq = vec![usize::MAX; ndof];
        let mut count = 0usize;
        for (j, joint) in m.joints.iter().enumerate() {
            for d in 0..6 {
                if !joint.restraint[d] && tied_rows[6 * j + d].is_none() {
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
    pub fn reduced_element(&self, dofs: &[usize; 12], kg: &[[f64; 12]; 12]) -> ReducedElement {
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

    pub fn has_tied_dof(&self, dofs: &[usize; 12]) -> bool {
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
}
