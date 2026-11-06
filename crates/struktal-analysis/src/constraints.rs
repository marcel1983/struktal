//! Equation numbering with kinematic constraints eliminated exactly
//! (decision 0011): u = C x, with x the independent DOFs. The reduced system
//! is Cᵀ K C x = Cᵀ f, which stays symmetric positive semidefinite, so the
//! SPD backends and the mechanism diagnosis apply with no penalty stiffness.
//!
//! A diaphragm slave s with reference m, normal n and r = x_s − x_m has
//! u_s = u_m + θ_m n × r in the plane and θ_s = θ_m about n.

use struktal_core::{Constraint, Snapshot};

use crate::linear_static::cross;

/// Reference DOF, plus the reference rotation DOF and its lever arm if any.
type Dependence = (usize, Option<(usize, f64)>);

/// A DOF depends on at most one translation and one rotation of the reference.
#[derive(Debug, Clone, Copy)]
struct Terms {
    len: u8,
    t: [(usize, f64); 2],
}

impl Terms {
    const NONE: Terms = Terms { len: 0, t: [(0, 0.0); 2] };

    fn push(&mut self, eq: usize, c: f64) {
        self.t[self.len as usize] = (eq, c);
        self.len += 1;
    }
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
        let mut dep: Vec<Option<Dependence>> = vec![None; ndof];
        for c in &m.constraints {
            let Constraint::Diaphragm { axis, joints, .. } = c;
            let Some((first, rest)) = joints.split_first() else { continue };
            let master = s.joint_index[first];
            let n = axis.index();
            let tied = c.tied_components();
            let mut w = [0.0; 3];
            w[n] = 1.0;
            let xm = s.joint_xyz(master);
            for name in rest {
                let slave = s.joint_index[name];
                let xs = s.joint_xyz(slave);
                let arm = cross(w, [0, 1, 2].map(|k| xs[k] - xm[k]));
                for d in 0..3 {
                    if tied[d] {
                        dep[6 * slave + d] = Some((6 * master + d, Some((6 * master + 3 + n, arm[d]))));
                    }
                }
                dep[6 * slave + 3 + n] = Some((6 * master + 3 + n, None));
            }
        }

        let mut eq = vec![usize::MAX; ndof];
        let mut count = 0usize;
        for (j, joint) in m.joints.iter().enumerate() {
            for d in 0..6 {
                if !joint.restraint[d] && dep[6 * j + d].is_none() {
                    eq[6 * j + d] = count;
                    count += 1;
                }
            }
        }

        let mut terms = vec![Terms::NONE; ndof];
        for g in 0..ndof {
            let t = &mut terms[g];
            match dep[g] {
                None => {
                    if eq[g] != usize::MAX {
                        t.push(eq[g], 1.0);
                    }
                }
                // Validation rejects restraints on tied components, so the
                // reference DOFs always have an equation.
                Some((reference, rotation)) => {
                    t.push(eq[reference], 1.0);
                    if let Some((r, c)) = rotation
                        && c != 0.0
                    {
                        t.push(eq[r], c);
                    }
                }
            }
        }
        DofMap { n: count, eq, terms }
    }

    /// Row of C for a global DOF: (equation, coefficient). Empty if restrained.
    pub fn terms(&self, g: usize) -> &[(usize, f64)] {
        let t = &self.terms[g];
        &t.t[..t.len as usize]
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

    fn snapshot(axis: &str) -> Snapshot {
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
            "constraints": [{"kind": "Diaphragm", "name": "D", "axis": axis, "joints": ["A", "B"]}],
        })
        .to_string();
        struktal_core::validate(&struktal_core::model_from_json(&text).expect("parse")).expect("valid")
    }

    #[test]
    fn diaphragm_rows_are_the_rigid_kinematics_for_every_axis() {
        // r = B - A = (3, 4, 5). With the reference moving by x, B must move
        // by u + θ n × r in the plane and keep its normal translation free.
        for (axis, n) in [("X", 0), ("Y", 1), ("Z", 2)] {
            let s = snapshot(axis);
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
    fn reduce_is_the_transpose_of_expand() {
        let s = snapshot("Z");
        let map = DofMap::build(&s);
        let f: Vec<f64> = (0..18).map(|v| (v as f64 * 0.7).sin()).collect();
        let x: Vec<f64> = (0..map.n).map(|v| (v as f64 * 1.3).cos()).collect();
        let lhs: f64 = map.expand(&x).iter().zip(&f).map(|(a, b)| a * b).sum();
        let rhs: f64 = map.reduce(&f).iter().zip(&x).map(|(a, b)| a * b).sum();
        assert!((lhs - rhs).abs() < 1e-13);
    }
}
