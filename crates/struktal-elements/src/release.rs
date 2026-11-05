//! End releases and partial-fixity springs by static condensation at
//! element level (decision 0008).
//!
//! Each released local component p of an element end becomes an internal
//! DOF e_p (the displacement of the element end), joined to the joint DOF
//! u_p by a spring k_p (k_p = 0: full release). With w the element-end
//! displacements (w_p = u_p if not released, e_p if released):
//!
//!   Π = ½ wᵀ K w − wᵀ f + Σ ½ k_p (e_p − u_p)²
//!
//! Partitioning the augmented DOFs [u, e] and eliminating e gives
//!   K_c = K_uu − K_ue K_ee⁻¹ K_eu,   f_c = f_u − K_ue K_ee⁻¹ f_e,
//! and e = K_ee⁻¹ (f_e − K_eu u). Both stiffness and span loads are
//! condensed. K_ee must be positive definite: a singular block means the
//! release set leaves the element with a rigid motion, which is reported as
//! an error, never regularized.

#![allow(clippy::needless_range_loop)] // small dense matrices: index form mirrors the formulas

use crate::frame3d::Mat12;

#[derive(Debug, Clone, PartialEq)]
pub enum ReleaseError {
    /// The released block is not positive definite (relative pivot shown).
    UnstableReleases { pivot: f64 },
}

impl std::fmt::Display for ReleaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReleaseError::UnstableReleases { pivot } => {
                write!(f, "released block of the element is singular (relative pivot {pivot:e})")
            }
        }
    }
}

impl std::error::Error for ReleaseError {}

#[derive(Debug, Clone)]
pub struct Condensation {
    /// Released local DOFs (0..12).
    released: Vec<usize>,
    /// Map from the 12 element-end DOFs to the augmented index: 0..12 are
    /// joint DOFs u, 12.. are internal DOFs e.
    map: [usize; 12],
    /// K_ue (12 × m), row-major.
    k_ue: Vec<Vec<f64>>,
    /// Cholesky factor of K_ee (lower, m × m).
    chol: Vec<Vec<f64>>,
    /// Condensed stiffness on joint DOFs.
    pub k_c: Mat12,
}

fn cholesky(a: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, ReleaseError> {
    let m = a.len();
    let scale = (0..m).map(|i| a[i][i].abs()).fold(0.0_f64, f64::max).max(f64::MIN_POSITIVE);
    let mut l = vec![vec![0.0; m]; m];
    for j in 0..m {
        let mut d = a[j][j];
        for k in 0..j {
            d -= l[j][k] * l[j][k];
        }
        if d <= 1e-12 * scale {
            return Err(ReleaseError::UnstableReleases { pivot: d / scale });
        }
        let d = d.sqrt();
        l[j][j] = d;
        for i in j + 1..m {
            let mut s = a[i][j];
            for k in 0..j {
                s -= l[i][k] * l[j][k];
            }
            l[i][j] = s / d;
        }
    }
    Ok(l)
}

fn chol_solve(l: &[Vec<f64>], b: &[f64]) -> Vec<f64> {
    let m = l.len();
    let mut y = b.to_vec();
    for i in 0..m {
        for k in 0..i {
            y[i] -= l[i][k] * y[k];
        }
        y[i] /= l[i][i];
    }
    for i in (0..m).rev() {
        for k in i + 1..m {
            y[i] -= l[k][i] * y[k];
        }
        y[i] /= l[i][i];
    }
    y
}

impl Condensation {
    /// `released[p]` and `springs[p]` for the 12 local DOFs [I: u1..r3, J: u1..r3].
    pub fn new(k: &Mat12, released: [bool; 12], springs: [f64; 12]) -> Result<Self, ReleaseError> {
        let rel: Vec<usize> = (0..12).filter(|&p| released[p]).collect();
        let m = rel.len();
        let mut map = [0usize; 12];
        for p in 0..12 {
            map[p] = p;
        }
        for (r, &p) in rel.iter().enumerate() {
            map[p] = 12 + r;
        }
        let n = 12 + m;
        let mut aug = vec![vec![0.0; n]; n];
        for p in 0..12 {
            for q in 0..12 {
                aug[map[p]][map[q]] += k[p][q];
            }
        }
        for (r, &p) in rel.iter().enumerate() {
            let kp = springs[p];
            let e = 12 + r;
            aug[p][p] += kp;
            aug[e][e] += kp;
            aug[p][e] -= kp;
            aug[e][p] -= kp;
        }
        let k_ee: Vec<Vec<f64>> = (0..m).map(|i| (0..m).map(|j| aug[12 + i][12 + j]).collect()).collect();
        let chol = cholesky(&k_ee)?;
        let k_ue: Vec<Vec<f64>> = (0..12).map(|i| (0..m).map(|j| aug[i][12 + j]).collect()).collect();
        // X = K_ee⁻¹ K_eu (m × 12), column by column.
        let mut x = vec![vec![0.0; 12]; m];
        for c in 0..12 {
            let col: Vec<f64> = (0..m).map(|r| aug[12 + r][c]).collect();
            let sol = chol_solve(&chol, &col);
            for r in 0..m {
                x[r][c] = sol[r];
            }
        }
        let mut k_c = [[0.0; 12]; 12];
        for i in 0..12 {
            for j in 0..12 {
                let mut v = aug[i][j];
                for r in 0..m {
                    v -= k_ue[i][r] * x[r][j];
                }
                k_c[i][j] = v;
            }
        }
        // Remove roundoff asymmetry.
        for i in 0..12 {
            for j in i + 1..12 {
                let v = 0.5 * (k_c[i][j] + k_c[j][i]);
                k_c[i][j] = v;
                k_c[j][i] = v;
            }
        }
        Ok(Self { released: rel, map, k_ue, chol, k_c })
    }

    /// Span loads on element-end DOFs → condensed loads on joint DOFs.
    pub fn condense_loads(&self, f_eq: &[f64; 12]) -> [f64; 12] {
        let mut f_u = [0.0; 12];
        let mut f_e = vec![0.0; self.released.len()];
        for p in 0..12 {
            if self.map[p] < 12 {
                f_u[p] += f_eq[p];
            } else {
                f_e[self.map[p] - 12] += f_eq[p];
            }
        }
        let y = chol_solve(&self.chol, &f_e);
        for (i, fi) in f_u.iter_mut().enumerate() {
            *fi -= self.k_ue[i].iter().zip(&y).map(|(a, b)| a * b).sum::<f64>();
        }
        f_u
    }

    /// Element-end displacements w from joint displacements u (local).
    pub fn end_displacements(&self, u: &[f64; 12], f_eq: &[f64; 12]) -> [f64; 12] {
        let m = self.released.len();
        let mut rhs = vec![0.0; m];
        for p in 0..12 {
            if self.map[p] >= 12 {
                rhs[self.map[p] - 12] += f_eq[p];
            }
        }
        // rhs = f_e − K_eu u, with K_eu = K_ueᵀ.
        for (r, v) in rhs.iter_mut().enumerate() {
            *v -= (0..12).map(|i| self.k_ue[i][r] * u[i]).sum::<f64>();
        }
        let e = chol_solve(&self.chol, &rhs);
        let mut w = *u;
        for (r, &p) in self.released.iter().enumerate() {
            w[p] = e[r];
        }
        w
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame3d::{LocalSpanLoads, SectionProps, equivalent_nodal_loads, local_stiffness, mat_vec};

    const P: SectionProps =
        SectionProps { e: 200e9, g: 76.9e9, area: 0.01, i33: 1e-4, i22: 2e-4, j: 3e-4, as2: 0.0, as3: 0.0 };

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    #[test]
    fn pinned_fixed_beam_stiffness() {
        // M3 released at J: the classic 3EI/L³ propped stiffness for v_I with
        // θ_I fixed and v_J fixed: K_c[1][1] = 3EI/L³, K_c[5][5] = 3EI/L.
        let l = 4.0;
        let k = local_stiffness(l, &P);
        let mut rel = [false; 12];
        rel[11] = true;
        let c = Condensation::new(&k, rel, [0.0; 12]).expect("stable");
        let ei = P.e * P.i33;
        close(c.k_c[1][1], 3.0 * ei / l.powi(3), 1e-6 * ei);
        close(c.k_c[5][5], 3.0 * ei / l, 1e-6 * ei);
        for i in 0..12 {
            close(c.k_c[11][i], 0.0, 1e-9 * ei);
        }
    }

    #[test]
    fn released_moment_end_force_is_zero_and_loads_condensed() {
        // Uniform q on a fixed–pinned member (M3 released at J): fixed-end
        // forces are R_I = 5qL/8, M_I = qL²/8, R_J = 3qL/8 (on the member,
        // opposing the load), i.e. condensed f = (5qL/8, qL²/8, 3qL/8, 0)·sign.
        let (l, q) = (4.0, -1000.0);
        let k = local_stiffness(l, &P);
        let loads = LocalSpanLoads { uniform: [0.0, q, 0.0], points: vec![] };
        let f = equivalent_nodal_loads(l, &P, &loads);
        let mut rel = [false; 12];
        rel[11] = true;
        let c = Condensation::new(&k, rel, [0.0; 12]).expect("stable");
        let fc = c.condense_loads(&f);
        close(fc[1], 5.0 * q * l / 8.0, 1e-9);
        close(fc[5], q * l * l / 8.0, 1e-9);
        close(fc[7], 3.0 * q * l / 8.0, 1e-9);
        close(fc[11], 0.0, 1e-9);
        // With joints fixed (u = 0) the end moment at J vanishes.
        let w = c.end_displacements(&[0.0; 12], &f);
        let fw = mat_vec(&k, &w);
        close(fw[11] - f[11], 0.0, 1e-9);
    }

    #[test]
    fn stiff_spring_recovers_rigid_connection_and_zero_spring_full_release() {
        let l = 3.0;
        let k = local_stiffness(l, &P);
        let mut rel = [false; 12];
        rel[11] = true;
        rel[5] = true;
        let mut springs = [0.0; 12];
        springs[11] = 1e18;
        springs[5] = 1e18;
        let c = Condensation::new(&k, rel, springs).expect("stable");
        let scale = k.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
        for i in 0..12 {
            for j in 0..12 {
                close(c.k_c[i][j], k[i][j], 1e-5 * scale);
            }
        }
    }

    #[test]
    fn rotational_spring_matches_series_flexibility() {
        // Cantilever clamped at I, M3 spring k at I: tip flexibility for a
        // tip moment is L/EI + 1/k.
        let l = 2.0;
        let k = local_stiffness(l, &P);
        let ks = 3.0e6;
        let mut rel = [false; 12];
        rel[5] = true;
        let mut springs = [0.0; 12];
        springs[5] = ks;
        let c = Condensation::new(&k, rel, springs).expect("stable");
        // Clamp joint I: flexibility of (v_J, θ_J) block.
        let (a, b, d) = (c.k_c[7][7], c.k_c[7][11], c.k_c[11][11]);
        let det = a * d - b * b;
        let ei = P.e * P.i33;
        close(a / det, l / ei + 1.0 / ks, 1e-9 * (l / ei + 1.0 / ks));
    }

    #[test]
    fn unstable_release_block_is_an_error() {
        let k = local_stiffness(3.0, &P);
        let mut rel = [false; 12];
        rel[0] = true;
        rel[6] = true; // axial both ends
        assert!(matches!(Condensation::new(&k, rel, [0.0; 12]), Err(ReleaseError::UnstableReleases { .. })));
        let mut rel = [false; 12];
        rel[5] = true;
        rel[11] = true;
        rel[1] = true; // M3 both + V2 at I
        assert!(Condensation::new(&k, rel, [0.0; 12]).is_err());
    }

    #[test]
    fn condensed_matrix_keeps_rigid_body_modes_and_symmetry() {
        let l = 3.0;
        let k = local_stiffness(l, &P);
        let mut rel = [false; 12];
        for p in [4, 5, 10, 11, 3] {
            rel[p] = true; // truss-like: M2, M3 both ends, T at I
        }
        let c = Condensation::new(&k, rel, [0.0; 12]).expect("stable");
        let ea = P.e * P.area / l;
        // Only axial stiffness remains.
        close(c.k_c[0][0], ea, 1e-9 * ea);
        close(c.k_c[0][6], -ea, 1e-9 * ea);
        for i in 1..6 {
            for j in 0..12 {
                if i != 3 && j != 9 {
                    close(c.k_c[i][j], 0.0, 1e-6 * ea);
                }
            }
        }
    }
}
