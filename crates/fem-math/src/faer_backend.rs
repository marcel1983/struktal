//! Sparse Cholesky (LLᵀ) backend on top of `faer` 0.24.4, behind the same
//! contract as [`crate::skyline`]. Feasibility status: decision 0005.
//!
//! What faer provides here (checked in its source, not inferred from names):
//! AMD ordering, simplicial or supernodal symbolic analysis with the factor
//! size known before numeric allocation, LLᵀ that stops at the first
//! non-positive pivot without classifying it, and LDLᵀ without pivoting with
//! optional dynamic regularization: with `signs = None`, a pivot with
//! |d| <= epsilon is replaced by ±delta and the factorization continues. The
//! sparse LBLᵀ pivots only inside supernodes and is not used.
//!
//! This wrapper uses LDLᵀ on the symmetrically scaled matrix (unit diagonal,
//! same as skyline) with epsilon = [`PIVOT_TOL`] and delta = [`SENTINEL`].
//! Replacing a zero pivot by a huge value decouples that DOF, which is what
//! skyline's zero-pivot bypass does; reading D afterwards gives the zero
//! pivots (|d| == SENTINEL), the negative pivots and hence the inertia. Such
//! a factor is never used to solve: it is reported as `Singular` or
//! `NotPositiveDefinite`.

use faer::dyn_stack::{MemBuffer, MemStack};
use faer::linalg::cholesky::ldlt::factor::LdltRegularization;
pub use faer::sparse::linalg::SupernodalThreshold;
use faer::sparse::linalg::cholesky::{
    CholeskySymbolicParams, LdltRef, SymbolicCholesky, SymbolicCholeskyRaw, SymmetricOrdering,
    factorize_symbolic_cholesky,
};
use faer::sparse::{SparseColMatRef, SymbolicSparseColMatRef};
use faer::{Conj, MatMut, Par, Side};

use crate::skyline::{BackendCapabilities, FactorDiagnostics, Inertia, MatrixClass, PIVOT_TOL, SolverError};
use crate::sparse::Csc;

pub const NAME: &str = "faer-sparse-ldlt";

/// Replacement value for zero pivots of the scaled matrix (unit diagonal).
pub const SENTINEL: f64 = 1e30;

pub fn capabilities() -> BackendCapabilities {
    BackendCapabilities {
        name: NAME,
        version: "faer 0.24.4",
        pure_rust: true,
        matrix_classes: vec![MatrixClass::Spd],
        pivoting: "none (LDLT)",
        singularity_detection: "zero pivots of the diagonally scaled matrix (|d| <= 1e-10) replaced by a sentinel through \
                                faer dynamic regularization and counted; count is a numerical estimate of nullity",
        inertia: "signs of D; exact by Sylvester's law only when no zero pivot occurs",
        multiple_rhs: true,
        index_type: "u32",
        memory_estimate_before_factorization: true,
        ordering: "AMD (faer)",
        threads: 1,
    }
}

/// Factor in faer's numbering; `perm[new] = old` is applied by faer itself.
pub struct FaerFactor {
    symbolic: SymbolicCholesky<u32>,
    values: Vec<f64>,
    scale: Vec<f64>,
    solve_bytes: usize,
    pub diagnostics: FactorDiagnostics,
}

fn to_u32(v: usize) -> Result<u32, SolverError> {
    u32::try_from(v).map_err(|_| SolverError::ResourceExceeded { required_bytes: usize::MAX, budget_bytes: 0 })
}

/// Index of the original DOF for each position of the factor.
fn forward_perm(sym: &SymbolicCholesky<u32>) -> Vec<usize> {
    match sym.perm() {
        Some(p) => p.arrays().0.iter().map(|&i| i as usize).collect(),
        None => (0..sym.nrows()).collect(),
    }
}

/// Diagonal entries of the factor (D for LDLᵀ) in factor ordering.
fn l_diagonal(sym: &SymbolicCholesky<u32>, values: &[f64]) -> Vec<f64> {
    let n = sym.nrows();
    let mut d = vec![0.0; n];
    match sym.raw() {
        SymbolicCholeskyRaw::Simplicial(s) => {
            let (cp, ri) = (s.col_ptr(), s.row_idx());
            for (j, dj) in d.iter_mut().enumerate() {
                let (a, b) = (cp[j] as usize, cp[j + 1] as usize);
                if let Some(k) = ri[a..b].iter().position(|&r| r as usize == j) {
                    *dj = values[a + k];
                }
            }
        }
        SymbolicCholeskyRaw::Supernodal(s) => {
            let (begin, end) = (s.supernode_begin(), s.supernode_end());
            let cpv = s.col_ptr_for_val();
            for sn in 0..s.n_supernodes() {
                let (c0, c1) = (begin[sn] as usize, end[sn] as usize);
                let ncols = c1 - c0;
                let block = &values[cpv[sn] as usize..cpv[sn + 1] as usize];
                let nrows = block.len() / ncols.max(1);
                for k in 0..ncols {
                    d[c0 + k] = block[k * nrows + k];
                }
            }
        }
    }
    d
}

pub fn factorize(a: &Csc, class: MatrixClass, budget_bytes: usize) -> Result<FaerFactor, SolverError> {
    factorize_with(a, class, budget_bytes, SupernodalThreshold::AUTO)
}

/// `threshold` selects simplicial/supernodal; exposed for tests of both paths.
pub fn factorize_with(
    a: &Csc,
    class: MatrixClass,
    budget_bytes: usize,
    threshold: SupernodalThreshold,
) -> Result<FaerFactor, SolverError> {
    if class != MatrixClass::Spd {
        return Err(SolverError::UnsupportedBackend { class, backend: NAME });
    }
    if !a.all_finite() {
        return Err(SolverError::NonFinite);
    }
    let sym_err = a.symmetry_error();
    if sym_err > 1e-12 {
        return Err(SolverError::NotSymmetric { relative_error: sym_err });
    }
    let n = a.n;
    let diag = a.diagonal();
    let scale: Vec<f64> = diag.iter().map(|&d| if d > 0.0 { 1.0 / d.sqrt() } else { 1.0 }).collect();

    // Upper triangle of S A S with u32 indices.
    let mut col_ptr = Vec::with_capacity(n + 1);
    let mut row_idx = Vec::new();
    let mut vals = Vec::new();
    col_ptr.push(0u32);
    for c in 0..n {
        for (r, v) in a.column(c) {
            if r <= c {
                row_idx.push(to_u32(r)?);
                vals.push(v * scale[r] * scale[c]);
            }
        }
        col_ptr.push(to_u32(row_idx.len())?);
    }
    let n32 = to_u32(n)? as usize;
    let symbolic_pattern = SymbolicSparseColMatRef::new_checked(n32, n32, &col_ptr, None, &row_idx);
    let mat = SparseColMatRef::new(symbolic_pattern, &vals);

    let symbolic = factorize_symbolic_cholesky(
        symbolic_pattern,
        Side::Upper,
        SymmetricOrdering::Amd,
        CholeskySymbolicParams { supernodal_flop_ratio_threshold: threshold, ..Default::default() },
    )
    .map_err(|_| SolverError::ResourceExceeded { required_bytes: usize::MAX, budget_bytes })?;

    let len_val = symbolic.len_val();
    let numeric_scratch = symbolic.factorize_numeric_ldlt_scratch::<f64>(Par::Seq, Default::default());
    let solve_bytes = symbolic.solve_in_place_scratch::<f64>(1, Par::Seq).unaligned_bytes_required();
    let factor_bytes = len_val * std::mem::size_of::<f64>();
    let required = factor_bytes + numeric_scratch.unaligned_bytes_required().max(solve_bytes);
    if required > budget_bytes {
        return Err(SolverError::ResourceExceeded { required_bytes: required, budget_bytes });
    }

    let mut values = vec![0.0f64; len_val];
    let mut mem = MemBuffer::try_new(numeric_scratch)
        .map_err(|_| SolverError::ResourceExceeded { required_bytes: required, budget_bytes })?;
    let fwd = forward_perm(&symbolic);
    let reg = LdltRegularization {
        dynamic_regularization_signs: None,
        dynamic_regularization_delta: SENTINEL,
        dynamic_regularization_epsilon: PIVOT_TOL,
    };
    let result = symbolic.factorize_numeric_ldlt::<f64>(
        &mut values,
        mat,
        Side::Upper,
        reg,
        Par::Seq,
        MemStack::new(&mut mem),
        Default::default(),
    );
    drop(mem);
    if result.is_err() {
        // With regularization enabled faer reports no zero pivots; keep a
        // defined outcome anyway.
        return Err(SolverError::NonFinite);
    }

    let d = l_diagonal(&symbolic, &values);
    let mut zero = Vec::new();
    let mut neg = Vec::new();
    let (mut dmin, mut dmax) = (f64::INFINITY, 0.0_f64);
    for (k, &dk) in d.iter().enumerate() {
        if dk.abs() >= 0.5 * SENTINEL {
            zero.push(fwd[k]);
            continue;
        }
        if !dk.is_finite() {
            return Err(SolverError::NonFinite);
        }
        dmin = dmin.min(dk.abs());
        dmax = dmax.max(dk.abs());
        if dk < 0.0 {
            neg.push(fwd[k]);
        }
    }
    zero.sort_unstable();
    neg.sort_unstable();
    let inertia =
        Inertia { positive: n - zero.len() - neg.len(), negative: neg.len(), zero: zero.len(), exact: zero.is_empty() };
    if !neg.is_empty() {
        return Err(SolverError::NotPositiveDefinite { negative_pivots: neg, zero_pivots: zero, inertia });
    }
    if !zero.is_empty() {
        return Err(SolverError::Singular { zero_pivots: zero, inertia });
    }

    Ok(FaerFactor {
        diagnostics: FactorDiagnostics {
            n,
            nnz_input: a.nnz(),
            profile_entries: len_val,
            factor_bytes,
            min_scaled_pivot: if n == 0 { 0.0 } else { dmin },
            max_scaled_pivot: dmax,
            inertia,
        },
        symbolic,
        values,
        scale,
        solve_bytes,
    })
}

impl FaerFactor {
    pub fn dim(&self) -> usize {
        self.scale.len()
    }

    pub fn solve(&self, b: &[f64]) -> Vec<f64> {
        let n = self.dim();
        let mut x: Vec<f64> = (0..n).map(|i| b[i] * self.scale[i]).collect();
        let ldlt = LdltRef::new(&self.symbolic, &self.values);
        let mut mem = MemBuffer::new(self.symbolic.solve_in_place_scratch::<f64>(1, Par::Seq));
        let _ = self.solve_bytes;
        ldlt.solve_in_place_with_conj(
            Conj::No,
            MatMut::from_column_major_slice_mut(&mut x, n, 1),
            Par::Seq,
            MemStack::new(&mut mem),
        );
        for (xi, s) in x.iter_mut().zip(&self.scale) {
            *xi *= s;
        }
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sparse::{TripletBuilder, relative_residual};

    fn dense(rows: &[&[f64]]) -> Csc {
        let mut t = TripletBuilder::new(rows.len()).expect("n");
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
        let a = dense(&[&[4.0, 1.0, 0.0], &[1.0, 3.0, 1.0], &[0.0, 1.0, 2.0]]);
        let f = factorize(&a, MatrixClass::Spd, 1 << 20).expect("spd");
        let x = f.solve(&[6.0, 10.0, 8.0]);
        for (xi, ei) in x.iter().zip([1.0, 2.0, 3.0]) {
            assert!((xi - ei).abs() < 1e-14, "{xi}");
        }
    }

    #[test]
    fn badly_scaled_spd() {
        let a = dense(&[&[1e12, 1e5], &[1e5, 2e-2]]);
        let b = [1e12 + 1e5, 1e5 + 2e-2];
        let f = factorize(&a, MatrixClass::Spd, 1 << 20).expect("spd");
        assert!(relative_residual(&a, &f.solve(&b), &b) < 1e-12);
    }

    #[test]
    fn exactly_singular_is_not_solved() {
        let a = dense(&[&[1.0, 1.0], &[1.0, 1.0]]);
        let r = factorize(&a, MatrixClass::Spd, 1 << 20);
        assert!(
            matches!(r, Err(SolverError::Singular { .. }) | Err(SolverError::NotPositiveDefinite { .. })),
            "{:?}",
            r.err()
        );
    }

    #[test]
    fn free_free_chain_is_not_solved() {
        let a = dense(&[&[1.0, -1.0, 0.0], &[-1.0, 2.0, -1.0], &[0.0, -1.0, 1.0]]);
        match factorize(&a, MatrixClass::Spd, 1 << 20) {
            Err(SolverError::Singular { zero_pivots, .. }) => assert_eq!(zero_pivots.len(), 1),
            other => panic!("expected singular, got {:?}", other.err()),
        }
    }

    #[test]
    fn indefinite_rejected() {
        let a = dense(&[&[2.0, 0.0], &[0.0, -3.0]]);
        match factorize(&a, MatrixClass::Spd, 1 << 20) {
            Err(SolverError::NotPositiveDefinite { negative_pivots, inertia, .. }) => {
                assert_eq!(negative_pivots, vec![1]);
                assert_eq!((inertia.positive, inertia.negative, inertia.zero), (1, 1, 0));
                assert!(inertia.exact);
            }
            other => panic!("expected indefinite, got {:?}", other.err()),
        }
    }

    #[test]
    fn other_classes_unsupported_and_budget_enforced() {
        let a = dense(&[&[2.0, 1.0], &[1.0, 2.0]]);
        assert!(matches!(
            factorize(&a, MatrixClass::SymmetricIndefinite, 1 << 20),
            Err(SolverError::UnsupportedBackend { .. })
        ));
        assert!(matches!(factorize(&a, MatrixClass::Spd, 8), Err(SolverError::ResourceExceeded { .. })));
    }

    /// Graph Laplacian of an m×m×m grid shifted by −s·I. Eigenvalues are
    /// Σ_i (2 − 2 cos(k_i π / m)), k_i = 0..m−1 (Neumann), so the number of
    /// negative and zero eigenvalues is known analytically.
    fn grid_laplacian(m: usize, shift: f64) -> Csc {
        let n = m * m * m;
        let id = |i: usize, j: usize, k: usize| (k * m + j) * m + i;
        let mut t = TripletBuilder::new(n).expect("n");
        for k in 0..m {
            for j in 0..m {
                for i in 0..m {
                    let p = id(i, j, k);
                    let mut deg = 0.0;
                    for (di, dj, dk) in [(1, 0, 0), (0, 1, 0), (0, 0, 1)] {
                        let (a, b, c) = (i + di, j + dj, k + dk);
                        if a < m && b < m && c < m {
                            let q = id(a, b, c);
                            t.add(p, q, -1.0).expect("ok");
                            t.add(q, p, -1.0).expect("ok");
                        }
                    }
                    for (a, b, c) in [
                        (i.wrapping_sub(1), j, k),
                        (i + 1, j, k),
                        (i, j.wrapping_sub(1), k),
                        (i, j + 1, k),
                        (i, j, k.wrapping_sub(1)),
                        (i, j, k + 1),
                    ] {
                        if a < m && b < m && c < m {
                            deg += 1.0;
                        }
                    }
                    t.add(p, p, deg - shift).expect("ok");
                }
            }
        }
        t.to_csc()
    }

    fn analytic_negative_count(m: usize, shift: f64) -> usize {
        let ev: Vec<f64> = (0..m).map(|k| 2.0 - 2.0 * (k as f64 * std::f64::consts::PI / m as f64).cos()).collect();
        let mut c = 0;
        for a in &ev {
            for b in &ev {
                for d in &ev {
                    if a + b + d < shift {
                        c += 1;
                    }
                }
            }
        }
        c
    }

    #[test]
    fn inertia_matches_analytic_on_both_paths() {
        let (m, shift) = (12, 0.937);
        let a = grid_laplacian(m, shift);
        let expected = analytic_negative_count(m, shift);
        assert!(expected > 1);
        for th in [SupernodalThreshold::FORCE_SIMPLICIAL, SupernodalThreshold::FORCE_SUPERNODAL] {
            match factorize_with(&a, MatrixClass::Spd, 1 << 28, th) {
                Err(SolverError::NotPositiveDefinite { negative_pivots, inertia, .. }) => {
                    assert_eq!(negative_pivots.len(), expected, "{th:?}");
                    assert!(inertia.exact, "{th:?}");
                }
                other => panic!("{th:?}: expected indefinite, got {:?}", other.err()),
            }
        }
        // Skyline agrees.
        match crate::skyline::factorize(&a, MatrixClass::Spd, 1 << 28) {
            Err(SolverError::NotPositiveDefinite { negative_pivots, .. }) => {
                assert_eq!(negative_pivots.len(), expected)
            }
            other => panic!("skyline: {:?}", other.err()),
        }
    }

    #[test]
    fn graph_laplacian_nullity_one_on_both_paths() {
        let a = grid_laplacian(10, 0.0);
        for th in [SupernodalThreshold::FORCE_SIMPLICIAL, SupernodalThreshold::FORCE_SUPERNODAL] {
            match factorize_with(&a, MatrixClass::Spd, 1 << 28, th) {
                Err(SolverError::Singular { zero_pivots, .. }) => assert_eq!(zero_pivots.len(), 1, "{th:?}"),
                other => panic!("{th:?}: expected singular, got {:?}", other.err()),
            }
        }
    }

    #[test]
    fn spd_supernodal_solution_matches_skyline() {
        let a = grid_laplacian(10, -0.5);
        let n = a.n;
        let b: Vec<f64> = (0..n).map(|i| ((i * 7) % 13) as f64 - 6.0).collect();
        let f = factorize_with(&a, MatrixClass::Spd, 1 << 28, SupernodalThreshold::FORCE_SUPERNODAL).expect("spd");
        let x1 = f.solve(&b);
        let x2 = crate::skyline::factorize(&a, MatrixClass::Spd, 1 << 28).expect("sky").solve(&b);
        assert!(relative_residual(&a, &x1, &b) < 1e-13);
        let diff = x1.iter().zip(&x2).map(|(p, q)| (p - q).abs()).fold(0.0, f64::max);
        assert!(diff < 1e-10, "{diff}");
    }

    #[test]
    fn laplacian_matches_skyline() {
        let n = 500;
        let mut t = TripletBuilder::new(n).expect("n");
        for i in 0..n {
            t.add(i, i, 2.0 + (i % 7) as f64).expect("ok");
            if i + 3 < n {
                t.add(i, i + 3, -1.0).expect("ok");
                t.add(i + 3, i, -1.0).expect("ok");
            }
        }
        let a = t.to_csc();
        let b: Vec<f64> = (0..n).map(|i| (i as f64).sin()).collect();
        let x1 = factorize(&a, MatrixClass::Spd, 1 << 24).expect("faer").solve(&b);
        let x2 = crate::skyline::factorize(&a, MatrixClass::Spd, 1 << 24).expect("sky").solve(&b);
        let diff = x1.iter().zip(&x2).map(|(p, q)| (p - q).abs()).fold(0.0, f64::max);
        assert!(diff < 1e-12, "{diff}");
    }
}
