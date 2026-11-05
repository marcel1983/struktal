//! Pure-Rust skyline (profile) LDLᵀ backend for symmetric positive definite
//! matrices, with symmetric diagonal scaling and RCM ordering.
//!
//! Factorization proceeds without pivoting. It is used only for the `Spd`
//! matrix class. Zero and negative pivots are not hidden: they are collected
//! and returned as `Singular` / `NotPositiveDefinite` diagnostics. Inertia
//! counts are reliable only when the factorization completes without zero
//! pivots; otherwise they are reported as estimates.

use serde::Serialize;

use crate::ordering::reverse_cuthill_mckee;
use crate::sparse::Csc;

/// Pivot of the scaled matrix (unit diagonal) at or below this value is zero.
pub const PIVOT_TOL: f64 = 1e-10;

/// Replacement for flagged pivots in PSD-regularized factorizations.
pub const SENTINEL: f64 = 1e30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatrixClass {
    Spd,
    SymmetricIndefinite,
    GeneralUnsymmetric,
    ComplexSymmetric,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendCapabilities {
    pub name: &'static str,
    pub version: &'static str,
    pub pure_rust: bool,
    pub matrix_classes: Vec<MatrixClass>,
    pub pivoting: &'static str,
    pub singularity_detection: &'static str,
    pub inertia: &'static str,
    pub multiple_rhs: bool,
    pub index_type: &'static str,
    pub memory_estimate_before_factorization: bool,
    pub ordering: &'static str,
    pub threads: usize,
}

pub fn capabilities() -> BackendCapabilities {
    BackendCapabilities {
        name: "skyline-ldlt",
        version: "0.1.0",
        pure_rust: true,
        matrix_classes: vec![MatrixClass::Spd],
        pivoting: "none",
        singularity_detection: "zero pivots of the diagonally scaled matrix (|d| <= 1e-10); count is a numerical estimate of nullity",
        inertia: "counts of positive/negative/zero pivots; exact by Sylvester's law only when no zero pivot occurs",
        multiple_rhs: true,
        index_type: "u32 in CSC, usize in profile",
        memory_estimate_before_factorization: true,
        ordering: "reverse Cuthill-McKee",
        threads: 1,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SolverError {
    UnsupportedBackend {
        class: MatrixClass,
        backend: &'static str,
    },
    NotSymmetric {
        relative_error: f64,
    },
    NonFinite,
    ResourceExceeded {
        required_bytes: usize,
        budget_bytes: usize,
    },
    /// Zero pivots (indices in the caller's numbering).
    Singular {
        zero_pivots: Vec<usize>,
        inertia: Inertia,
    },
    NotPositiveDefinite {
        negative_pivots: Vec<usize>,
        zero_pivots: Vec<usize>,
        inertia: Inertia,
    },
}

impl std::fmt::Display for SolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolverError::UnsupportedBackend { class, backend } => {
                write!(f, "unsupported_backend: {backend} cannot factorize {class:?}")
            }
            SolverError::NotSymmetric { relative_error } => {
                write!(f, "matrix not symmetric (relative error {relative_error:e})")
            }
            SolverError::NonFinite => write!(f, "matrix contains non-finite values"),
            SolverError::ResourceExceeded { required_bytes, budget_bytes } => {
                write!(f, "factor needs {required_bytes} bytes, budget is {budget_bytes}")
            }
            SolverError::Singular { zero_pivots, .. } => {
                write!(f, "singular matrix: {} zero pivot(s)", zero_pivots.len())
            }
            SolverError::NotPositiveDefinite { negative_pivots, zero_pivots, .. } => write!(
                f,
                "matrix not positive definite: {} negative and {} zero pivot(s)",
                negative_pivots.len(),
                zero_pivots.len()
            ),
        }
    }
}

impl std::error::Error for SolverError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Inertia {
    pub positive: usize,
    pub negative: usize,
    pub zero: usize,
    /// False when zero pivots were bypassed; counts are then estimates.
    pub exact: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FactorDiagnostics {
    pub n: usize,
    pub nnz_input: usize,
    pub profile_entries: usize,
    pub factor_bytes: usize,
    pub min_scaled_pivot: f64,
    pub max_scaled_pivot: f64,
    pub inertia: Inertia,
}

#[derive(Debug, Clone)]
pub struct SkylineFactor {
    n: usize,
    /// perm[new] = old
    perm: Vec<usize>,
    /// Symmetric scaling factors in original numbering.
    scale: Vec<f64>,
    /// First row of each column in the permuted matrix.
    first: Vec<usize>,
    /// Start of each column in `data`; column j holds rows first[j]..=j.
    start: Vec<usize>,
    /// Off-diagonal entries hold Lᵀ, diagonal entries hold D.
    data: Vec<f64>,
    pub diagnostics: FactorDiagnostics,
}

pub fn factorize(a: &Csc, class: MatrixClass, budget_bytes: usize) -> Result<SkylineFactor, SolverError> {
    factorize_impl(a, class, budget_bytes, None).map(|(f, _)| f)
}

/// Factorization of a matrix known to be positive semidefinite, for null-space
/// analysis. Every pivot `d <= eps` of the scaled matrix (zero or negative by
/// roundoff) is replaced by [`SENTINEL`]; this is exactly the factorization of
/// K + Σ_j c_j e_j e_jᵀ (scaled coordinates) with c_j ≈ SENTINEL at the flagged
/// positions. Returns the factor and the flagged DOFs (original numbering).
/// Not for matrices that may be genuinely indefinite.
pub fn factorize_psd_regularized(
    a: &Csc,
    budget_bytes: usize,
    eps: f64,
) -> Result<(SkylineFactor, Vec<usize>), SolverError> {
    factorize_impl(a, MatrixClass::Spd, budget_bytes, Some(eps))
}

fn factorize_impl(
    a: &Csc,
    class: MatrixClass,
    budget_bytes: usize,
    psd_eps: Option<f64>,
) -> Result<(SkylineFactor, Vec<usize>), SolverError> {
    if class != MatrixClass::Spd {
        return Err(SolverError::UnsupportedBackend { class, backend: "skyline-ldlt" });
    }
    if !a.all_finite() {
        return Err(SolverError::NonFinite);
    }
    let sym = a.symmetry_error();
    if sym > 1e-12 {
        return Err(SolverError::NotSymmetric { relative_error: sym });
    }
    let n = a.n;
    let perm = reverse_cuthill_mckee(a);
    let mut inv = vec![0usize; n];
    for (new, &old) in perm.iter().enumerate() {
        inv[old] = new;
    }

    // Scaling s_i = 1/sqrt(a_ii) for positive diagonals; non-positive
    // diagonals are left unscaled and will surface as zero/negative pivots.
    let diag = a.diagonal();
    let scale: Vec<f64> = diag.iter().map(|&d| if d > 0.0 { 1.0 / d.sqrt() } else { 1.0 }).collect();

    // Profile: first[j] = min row index (permuted) in column j (upper part).
    let mut first: Vec<usize> = (0..n).collect();
    for c_old in 0..n {
        let c = inv[c_old];
        for (r_old, _) in a.column(c_old) {
            let r = inv[r_old];
            if r < c && r < first[c] {
                first[c] = r;
            }
        }
    }
    let mut start = vec![0usize; n + 1];
    for j in 0..n {
        start[j + 1] = start[j] + (j - first[j] + 1);
    }
    let profile = start[n];
    let factor_bytes = profile * std::mem::size_of::<f64>() + 4 * n * std::mem::size_of::<usize>();
    if factor_bytes > budget_bytes {
        return Err(SolverError::ResourceExceeded { required_bytes: factor_bytes, budget_bytes });
    }

    let mut data = vec![0.0f64; profile];
    let idx = |start: &[usize], first: &[usize], i: usize, j: usize| start[j] + (i - first[j]);
    for c_old in 0..n {
        let c = inv[c_old];
        for (r_old, v) in a.column(c_old) {
            let r = inv[r_old];
            if r <= c {
                data[idx(&start, &first, r, c)] = v * scale[r_old] * scale[c_old];
            }
        }
    }

    let mut zero_new = Vec::new();
    let mut neg_new = Vec::new();
    let mut flagged_new = Vec::new();
    let mut is_zero = vec![false; n];
    let (mut dmin, mut dmax) = (f64::INFINITY, 0.0_f64);
    for j in 0..n {
        let fj = first[j];
        // g_i = a_ij - Σ_k l_ki g_k  (stored in place in column j)
        for i in fj..j {
            let fi = first[i];
            let k0 = fi.max(fj);
            let mut s = data[idx(&start, &first, i, j)];
            for k in k0..i {
                s -= data[idx(&start, &first, k, i)] * data[idx(&start, &first, k, j)];
            }
            data[idx(&start, &first, i, j)] = s;
        }
        let mut d = data[idx(&start, &first, j, j)];
        for i in fj..j {
            let g = data[idx(&start, &first, i, j)];
            let l = if is_zero[i] { 0.0 } else { g / data[idx(&start, &first, i, i)] };
            d -= l * g;
            data[idx(&start, &first, i, j)] = l;
        }
        if let Some(eps) = psd_eps {
            if d <= eps {
                flagged_new.push(j);
                d = SENTINEL;
            } else {
                dmin = dmin.min(d);
                dmax = dmax.max(d);
            }
        } else {
            dmin = dmin.min(d.abs());
            dmax = dmax.max(d.abs());
            if d.abs() <= PIVOT_TOL {
                is_zero[j] = true;
                zero_new.push(j);
            } else if d < 0.0 {
                neg_new.push(j);
            }
        }
        data[idx(&start, &first, j, j)] = d;
    }

    let inertia = Inertia {
        positive: n - zero_new.len() - neg_new.len(),
        negative: neg_new.len(),
        zero: zero_new.len(),
        exact: zero_new.is_empty(),
    };
    let to_old = |v: &[usize]| -> Vec<usize> {
        let mut o: Vec<usize> = v.iter().map(|&k| perm[k]).collect();
        o.sort_unstable();
        o
    };
    let flagged_old = to_old(&flagged_new);
    if !neg_new.is_empty() {
        return Err(SolverError::NotPositiveDefinite {
            negative_pivots: to_old(&neg_new),
            zero_pivots: to_old(&zero_new),
            inertia,
        });
    }
    if !zero_new.is_empty() {
        return Err(SolverError::Singular { zero_pivots: to_old(&zero_new), inertia });
    }

    Ok((
        SkylineFactor {
            n,
            perm,
            scale,
            first,
            start,
            data,
            diagnostics: FactorDiagnostics {
                n,
                nnz_input: a.nnz(),
                profile_entries: profile,
                factor_bytes,
                min_scaled_pivot: if n == 0 { 0.0 } else { dmin },
                max_scaled_pivot: dmax,
                inertia,
            },
        },
        flagged_old,
    ))
}

impl SkylineFactor {
    pub fn dim(&self) -> usize {
        self.n
    }

    /// Solves A x = b for one right-hand side (original numbering).
    pub fn solve(&self, b: &[f64]) -> Vec<f64> {
        let n = self.n;
        // Off-diagonal part of column j: rows first[j]..j, i.e. l_kj = L[j][k].
        let col = |j: usize| &self.data[self.start[j]..self.start[j + 1] - 1];
        let diag = |j: usize| self.data[self.start[j + 1] - 1];
        // y = S P b
        let mut y: Vec<f64> = (0..n).map(|k| b[self.perm[k]] * self.scale[self.perm[k]]).collect();
        // L z = y
        for j in 0..n {
            let s: f64 = col(j).iter().zip(&y[self.first[j]..j]).map(|(l, yk)| l * yk).sum();
            y[j] -= s;
        }
        for (j, yj) in y.iter_mut().enumerate() {
            *yj /= diag(j);
        }
        // Lᵀ x = z
        for j in (0..n).rev() {
            let xj = y[j];
            for (yk, l) in y[self.first[j]..j].iter_mut().zip(col(j)) {
                *yk -= l * xj;
            }
        }
        let mut x = vec![0.0; n];
        for (k, &old) in self.perm.iter().enumerate() {
            x[old] = y[k] * self.scale[old];
        }
        x
    }

    pub fn solve_many(&self, rhs: &[Vec<f64>]) -> Vec<Vec<f64>> {
        rhs.iter().map(|b| self.solve(b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sparse::{TripletBuilder, relative_residual};

    fn dense(rows: &[&[f64]]) -> Csc {
        let n = rows.len();
        let mut t = TripletBuilder::new(n).expect("n");
        for (i, r) in rows.iter().enumerate() {
            for (j, &v) in r.iter().enumerate() {
                if v != 0.0 {
                    t.add(i, j, v).expect("ok");
                }
            }
        }
        t.to_csc()
    }

    #[test]
    fn spd_known_solution() {
        // A x = b with x = [1, 2, 3] computed by hand.
        let a = dense(&[&[4.0, 1.0, 0.0], &[1.0, 3.0, 1.0], &[0.0, 1.0, 2.0]]);
        let b = [6.0, 10.0, 8.0];
        let f = factorize(&a, MatrixClass::Spd, 1 << 20).expect("spd");
        let x = f.solve(&b);
        for (xi, ei) in x.iter().zip([1.0, 2.0, 3.0]) {
            assert!((xi - ei).abs() < 1e-14);
        }
        assert!(relative_residual(&a, &x, &b) < 1e-15);
        assert_eq!(f.diagnostics.inertia, Inertia { positive: 3, negative: 0, zero: 0, exact: true });
    }

    #[test]
    fn badly_scaled_spd() {
        // Mixed units: 1e12 and 1e-2 diagonal blocks.
        let a = dense(&[&[1e12, 1e5], &[1e5, 1e-2 + 1e-2]]);
        let b = [1e12 + 1e5, 1e5 + 2e-2];
        let f = factorize(&a, MatrixClass::Spd, 1 << 20).expect("spd");
        let x = f.solve(&b);
        assert!(relative_residual(&a, &x, &b) < 1e-12);
    }

    #[test]
    fn singular_detected_with_index() {
        // Rank 1 matrix; nullity 1.
        let a = dense(&[&[1.0, 1.0], &[1.0, 1.0]]);
        match factorize(&a, MatrixClass::Spd, 1 << 20) {
            Err(SolverError::Singular { zero_pivots, inertia }) => {
                assert_eq!(zero_pivots.len(), 1);
                assert_eq!(inertia.zero, 1);
                assert!(!inertia.exact);
            }
            other => panic!("expected singular, got {other:?}"),
        }
    }

    #[test]
    fn free_free_spring_chain_nullity_one() {
        // Three masses, two springs, no supports: one rigid-body mode.
        let a = dense(&[&[1.0, -1.0, 0.0], &[-1.0, 2.0, -1.0], &[0.0, -1.0, 1.0]]);
        match factorize(&a, MatrixClass::Spd, 1 << 20) {
            Err(SolverError::Singular { zero_pivots, .. }) => assert_eq!(zero_pivots.len(), 1),
            other => panic!("expected singular, got {other:?}"),
        }
    }

    #[test]
    fn indefinite_rejected_with_inertia() {
        let a = dense(&[&[2.0, 0.0], &[0.0, -3.0]]);
        match factorize(&a, MatrixClass::Spd, 1 << 20) {
            Err(SolverError::NotPositiveDefinite { negative_pivots, inertia, .. }) => {
                assert_eq!(negative_pivots, vec![1]);
                assert_eq!(inertia.negative, 1);
                assert_eq!(inertia.positive, 1);
            }
            other => panic!("expected indefinite, got {other:?}"),
        }
    }

    #[test]
    fn other_classes_unsupported() {
        let a = dense(&[&[1.0]]);
        for c in [MatrixClass::SymmetricIndefinite, MatrixClass::GeneralUnsymmetric, MatrixClass::ComplexSymmetric] {
            assert!(matches!(factorize(&a, c, 1 << 20), Err(SolverError::UnsupportedBackend { .. })));
        }
    }

    #[test]
    fn unsymmetric_rejected() {
        let a = dense(&[&[2.0, 1.0], &[0.0, 2.0]]);
        assert!(matches!(factorize(&a, MatrixClass::Spd, 1 << 20), Err(SolverError::NotSymmetric { .. })));
    }

    #[test]
    fn budget_enforced_before_allocation() {
        let a = dense(&[&[2.0, 1.0], &[1.0, 2.0]]);
        assert!(matches!(factorize(&a, MatrixClass::Spd, 8), Err(SolverError::ResourceExceeded { .. })));
    }

    #[test]
    fn laplacian_1d_large_residual_and_profile() {
        // Tridiagonal SPD, n = 2000: profile must stay O(n), not O(n²).
        let n = 2000;
        let mut t = TripletBuilder::new(n).expect("n");
        for i in 0..n {
            t.add(i, i, 2.0).expect("ok");
            if i + 1 < n {
                t.add(i, i + 1, -1.0).expect("ok");
                t.add(i + 1, i, -1.0).expect("ok");
            }
        }
        let a = t.to_csc();
        let f = factorize(&a, MatrixClass::Spd, 1 << 24).expect("spd");
        assert!(f.diagnostics.profile_entries <= 2 * n);
        let b = vec![1.0; n];
        let x = f.solve(&b);
        assert!(relative_residual(&a, &x, &b) < 1e-10);
    }
}
