//! Verified null-space vectors of a positive semidefinite matrix (F08-007).
//!
//! Method (decision 0006):
//! 1. Factorize K with every scaled pivot `d <= pivot_eps` replaced by a huge
//!    value. This is exactly K_reg = K + Σ_j c_j e_j e_jᵀ (scaled coordinates)
//!    for the flagged set Z.
//! 2. For each flagged j, solve K_reg z = e_j. Because c_j dominates, z has
//!    z_j ≈ 1, z_k ≈ 0 for other k in Z, and equilibrium K z = 0 on the other
//!    rows: a candidate null vector (Farhat & Géradin, 1998).
//! 3. Verify each candidate against the original K in scaled coordinates
//!    (unit diagonal, so translations and rotations are comparable):
//!    ρ = ‖S K S y‖ / ‖y‖ <= residual_tol.
//! 4. Orthonormalize accepted vectors (classical Gram–Schmidt, twice) and keep
//!    only independent ones. Their count is a verified lower bound.
//! 5. Upper bound. Let K_fix be K with the flagged DOFs Z fixed. A null
//!    vector of K that vanishes on Z is a null vector of K_fix, so
//!    nullity(K) <= |Z| + nullity(K_fix). The smallest eigenvalues of K_fix
//!    are estimated by block inverse iteration with the regularized factor
//!    (which acts as K_fix⁻¹ on the complement of Z) and Rayleigh–Ritz with
//!    the true K. Null vectors found there are verified and added to the
//!    lower bound (this recovers null directions whose pivots escaped the
//!    flagging through roundoff). The upper bound |Z| + found is claimed only
//!    if the iteration converged and there is a spectral gap: the smallest
//!    remaining Ritz value λ_fix >= gap_factor · max(largest residual of the
//!    accepted null vectors, 100 · η · ‖K_s‖), η being the measured backward
//!    error of the regularized solves. A pivot-size criterion is NOT used: it
//!    gave a wrong "exact" nullity with skyline on a 53 754-equation frame.
//!
//! Candidates are refined twice (y ← y − K_fix⁻¹ (K y)_free) before testing.
//! Measured floors on free 3D frames: null residuals ~1e-13 at 900
//! equations, ~1e-11 at 108 000, while λ_fix falls to ~5e-11; above ~1e5
//! equations the gap may be too small to certify the count (decision 0006).
//!
//! The bounds are numerical: "null" means ρ <= residual_tol, so a stable but
//! extremely flexible direction can be reported as a mechanism.

use serde::Serialize;

use crate::factor::{Backend, factorize_psd_regularized};
use crate::skyline::SolverError;
use crate::sparse::{Csc, norm2};

#[derive(Debug, Clone, Serialize)]
pub struct NullSpaceOptions {
    /// Scaled pivots at or below this value are flagged.
    pub pivot_eps: f64,
    /// Maximum ‖S K S y‖ for a unit scaled vector y to count as null.
    pub residual_tol: f64,
    /// Required ratio between λ_fix and the null-vector residual level.
    pub gap_factor: f64,
    /// Resource bound on flagged candidates.
    pub max_vectors: usize,
    /// Block size and iteration limit of the inverse iteration on K_fix.
    pub block_size: usize,
    pub max_iterations: usize,
    pub budget_bytes: usize,
}

impl Default for NullSpaceOptions {
    fn default() -> Self {
        Self {
            pivot_eps: 1e-8,
            residual_tol: 1e-10,
            gap_factor: 10.0,
            max_vectors: 64,
            block_size: 8,
            max_iterations: 40,
            budget_bytes: 1 << 30,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NullVector {
    /// DOF (caller numbering) whose flagged pivot generated the candidate.
    pub seed: usize,
    /// Unit vector in scaled coordinates (y = S⁻¹ x).
    pub scaled: Vec<f64>,
    /// Same direction in original coordinates, x = S y.
    pub original: Vec<f64>,
    /// ‖S K S y‖ for the stored (orthonormalized) y.
    pub residual: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct NullSpaceReport {
    pub backend: &'static str,
    pub options: NullSpaceOptions,
    pub flagged: Vec<usize>,
    pub candidates_tested: usize,
    pub rejected_residual: usize,
    pub rejected_dependent: usize,
    /// True if `max_vectors` stopped the search before all flagged DOFs.
    pub truncated: bool,
    pub min_unflagged_pivot: f64,
    /// Null vectors found by the inverse iteration on K_fix (not flagged).
    pub found_by_iteration: usize,
    pub iterations: usize,
    pub iteration_converged: bool,
    /// Smallest Ritz value of K_fix (scaled) after deflating null vectors.
    pub fixed_min_eigenvalue: Option<f64>,
    /// Max normwise backward error of the regularized solves on free rows.
    pub solve_backward_error: f64,
    pub solves: usize,
    pub nullity_lower_bound: usize,
    pub nullity_upper_bound: Option<usize>,
    pub max_residual: f64,
    /// λ_fix / max(null residual level, solve error level); >= gap_factor
    /// is required for the upper bound.
    pub spectral_gap_ratio: Option<f64>,
    #[serde(skip)]
    pub vectors: Vec<NullVector>,
}

impl NullSpaceReport {
    pub fn exact(&self) -> Option<usize> {
        (self.nullity_upper_bound == Some(self.nullity_lower_bound)).then_some(self.nullity_lower_bound)
    }
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// ‖S K S y‖ with S = diag(1/sqrt(K_ii)) (1 where K_ii <= 0).
fn scaled_residual(a: &Csc, s: &[f64], y: &[f64]) -> f64 {
    let x: Vec<f64> = y.iter().zip(s).map(|(yi, si)| yi * si).collect();
    let kx = a.mul_vec(&x);
    let r: Vec<f64> = kx.iter().zip(s).map(|(v, si)| v * si).collect();
    norm2(&r) / norm2(y).max(f64::MIN_POSITIVE)
}

/// Adds `y` (scaled, any norm) to the orthonormal `basis` if it is
/// independent; returns the orthonormalized vector.
fn orthonormalize_against(y: &mut [f64], basis: &[&[f64]]) -> Option<()> {
    let n0 = norm2(y);
    if !(n0.is_finite() && n0 > 0.0) {
        return None;
    }
    y.iter_mut().for_each(|v| *v /= n0);
    for _ in 0..2 {
        for b in basis {
            let c = dot(y, b);
            y.iter_mut().zip(b.iter()).for_each(|(v, w)| *v -= c * w);
        }
    }
    let nr = norm2(y);
    if nr < 1e-6 {
        return None;
    }
    y.iter_mut().for_each(|v| *v /= nr);
    Some(())
}

/// Eigen-decomposition of a small dense symmetric matrix (cyclic Jacobi).
/// Returns eigenvalues ascending and eigenvectors as columns `q[row][col]`.
fn jacobi_eigen(mut h: Vec<Vec<f64>>) -> (Vec<f64>, Vec<Vec<f64>>) {
    let p = h.len();
    let mut q: Vec<Vec<f64>> = (0..p).map(|i| (0..p).map(|j| if i == j { 1.0 } else { 0.0 }).collect()).collect();
    for _sweep in 0..100 {
        let off: f64 = (0..p)
            .flat_map(|i| (0..p).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| h[i][j] * h[i][j])
            .sum();
        let diag: f64 = (0..p).map(|i| h[i][i] * h[i][i]).sum();
        if off <= 1e-30 * diag.max(f64::MIN_POSITIVE) {
            break;
        }
        for i in 0..p {
            for j in i + 1..p {
                if h[i][j] == 0.0 {
                    continue;
                }
                let theta = (h[j][j] - h[i][i]) / (2.0 * h[i][j]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let sn = t * c;
                for row in h.iter_mut() {
                    let (hki, hkj) = (row[i], row[j]);
                    row[i] = c * hki - sn * hkj;
                    row[j] = sn * hki + c * hkj;
                }
                let (lo, hi) = h.split_at_mut(j);
                for (hik, hjk) in lo[i].iter_mut().zip(hi[0].iter_mut()) {
                    let (a, b) = (*hik, *hjk);
                    *hik = c * a - sn * b;
                    *hjk = sn * a + c * b;
                }
                for row in q.iter_mut() {
                    let (qi, qj) = (row[i], row[j]);
                    row[i] = c * qi - sn * qj;
                    row[j] = sn * qi + c * qj;
                }
            }
        }
    }
    let mut order: Vec<usize> = (0..p).collect();
    order.sort_by(|&a, &b| h[a][a].total_cmp(&h[b][b]));
    let vals = order.iter().map(|&k| h[k][k]).collect();
    let vecs = (0..p).map(|r| order.iter().map(|&k| q[r][k]).collect()).collect();
    (vals, vecs)
}

/// Deterministic pseudo-random start vectors (splitmix64).
fn start_vector(seed: &mut u64, n: usize, fixed: &[bool]) -> Vec<f64> {
    (0..n)
        .map(|i| {
            *seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = *seed;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            if fixed[i] { 0.0 } else { (z >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0 }
        })
        .collect()
}

pub fn analyze(backend: Backend, a: &Csc, opts: &NullSpaceOptions) -> Result<NullSpaceReport, SolverError> {
    let n = a.n;
    let s: Vec<f64> = a.diagonal().iter().map(|&d| if d > 0.0 { 1.0 / d.sqrt() } else { 1.0 }).collect();
    let (factor, flagged) = factorize_psd_regularized(backend, a, opts.budget_bytes, opts.pivot_eps)?;
    let min_unflagged_pivot = factor.diagnostics().min_scaled_pivot;
    let mut fixed = vec![false; n];
    for &j in &flagged {
        fixed[j] = true;
    }
    // K_s y = S K S y
    let ks = |y: &[f64]| -> Vec<f64> {
        let x: Vec<f64> = y.iter().zip(&s).map(|(yi, si)| yi * si).collect();
        a.mul_vec(&x).iter().zip(&s).map(|(v, si)| v * si).collect()
    };
    // ‖K_s‖ upper bound: max absolute row sum (symmetric).
    let norm_ks = (0..n)
        .map(|c| a.column(c).map(|(r, v)| (v * s[r] * s[c]).abs()).sum::<f64>())
        .fold(0.0_f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let mut solves = 0usize;
    let mut eta = 0.0_f64;
    let op = |y: &[f64], eta: &mut f64| -> Vec<f64> {
        let b: Vec<f64> = y.iter().zip(&s).map(|(yi, si)| yi / si).collect();
        let x = factor.solve(&b);
        let mut z: Vec<f64> = x.iter().zip(&s).map(|(xi, si)| xi / si).collect();
        for (zi, &f) in z.iter_mut().zip(&fixed) {
            if f {
                *zi = 0.0;
            }
        }
        let r = ks(&z);
        let rn = r
            .iter()
            .zip(y)
            .zip(&fixed)
            .filter(|(_, f)| !**f)
            .map(|((ri, yi), _)| (ri - yi).powi(2))
            .sum::<f64>()
            .sqrt();
        let denom = norm_ks * norm2(&z) + norm2(y);
        if denom > 0.0 {
            *eta = eta.max(rn / denom);
        }
        z
    };
    // Iterative refinement of a candidate null vector: keep its components on
    // the fixed DOFs, re-solve the free rows so that (K y)_free -> 0.
    let refine = |y: &mut Vec<f64>, eta: &mut f64, solves: &mut usize| {
        for _ in 0..2 {
            let mut r = ks(y);
            for (ri, &f) in r.iter_mut().zip(&fixed) {
                if f {
                    *ri = 0.0;
                }
            }
            let c = op(&r, eta);
            *solves += 1;
            y.iter_mut().zip(&c).for_each(|(yi, ci)| *yi -= ci);
        }
    };

    // Phase 1: candidates from flagged pivots.
    let mut basis: Vec<NullVector> = Vec::new();
    let (mut tested, mut rej_res, mut rej_dep) = (0, 0, 0);
    let mut e = vec![0.0; n];
    for &j in flagged.iter().take(opts.max_vectors) {
        tested += 1;
        e[j] = 1.0;
        let x = factor.solve(&e);
        solves += 1;
        e[j] = 0.0;
        let mut y: Vec<f64> = x.iter().zip(&s).map(|(xi, si)| xi / si).collect();
        let ny = norm2(&y);
        if ny.is_finite() && ny > 0.0 {
            y.iter_mut().for_each(|v| *v /= ny);
            refine(&mut y, &mut eta, &mut solves);
        }
        let ny = norm2(&y);
        if !(ny.is_finite() && ny > 0.0) || scaled_residual(a, &s, &y) > opts.residual_tol {
            rej_res += 1;
            continue;
        }
        let refs: Vec<&[f64]> = basis.iter().map(|b| b.scaled.as_slice()).collect();
        if orthonormalize_against(&mut y, &refs).is_none() {
            rej_dep += 1;
            continue;
        }
        let residual = scaled_residual(a, &s, &y);
        if residual > opts.residual_tol {
            rej_res += 1;
            continue;
        }
        let original = y.iter().zip(&s).map(|(yi, si)| yi * si).collect();
        basis.push(NullVector { seed: j, scaled: y, original, residual });
    }
    let truncated = flagged.len() > opts.max_vectors;

    // Phase 2: block inverse iteration on K_fix.
    let free = n - flagged.len();
    let p = opts.block_size.max(1).min(free);
    let mut found: Vec<Vec<f64>> = Vec::new();
    let mut iterations = 0;
    let mut converged = free == 0;
    let mut theta_min = None;
    let mut seed = 0x5EED_u64;
    let mut block: Vec<Vec<f64>> = (0..p).map(|_| start_vector(&mut seed, n, &fixed)).collect();
    while p > 0 && iterations < opts.max_iterations {
        iterations += 1;
        let mut w: Vec<Vec<f64>> = Vec::with_capacity(p);
        for y in &block {
            let mut z = op(y, &mut eta);
            solves += 1;
            let refs: Vec<&[f64]> = found.iter().map(Vec::as_slice).chain(w.iter().map(Vec::as_slice)).collect();
            if orthonormalize_against(&mut z, &refs).is_none() {
                let mut r = start_vector(&mut seed, n, &fixed);
                let refs: Vec<&[f64]> = found.iter().map(Vec::as_slice).chain(w.iter().map(Vec::as_slice)).collect();
                if orthonormalize_against(&mut r, &refs).is_none() {
                    continue;
                }
                z = r;
            }
            w.push(z);
        }
        if w.is_empty() {
            converged = true;
            break;
        }
        let kw: Vec<Vec<f64>> = w.iter().map(|v| ks(v)).collect();
        let h: Vec<Vec<f64>> = (0..w.len())
            .map(|i| (0..w.len()).map(|j| 0.5 * (dot(&w[i], &kw[j]) + dot(&w[j], &kw[i]))).collect())
            .collect();
        let (theta, q) = jacobi_eigen(h);
        let combine = |src: &[Vec<f64>], col: usize| -> Vec<f64> {
            let mut out = vec![0.0; n];
            for (k, v) in src.iter().enumerate() {
                let c = q[k][col];
                out.iter_mut().zip(v).for_each(|(o, x)| *o += c * x);
            }
            out
        };
        let ritz: Vec<Vec<f64>> = (0..w.len()).map(|c| combine(&w, c)).collect();
        let kritz: Vec<Vec<f64>> = (0..w.len()).map(|c| combine(&kw, c)).collect();
        let mut new_null = false;
        for (u, ku) in ritz.iter().zip(&kritz) {
            // Cheap pre-filter, then refinement and the real test.
            if norm2(ku) <= 1e3 * opts.residual_tol {
                let mut y = u.clone();
                refine(&mut y, &mut eta, &mut solves);
                let refs: Vec<&[f64]> = basis.iter().map(|b| b.scaled.as_slice()).collect();
                if orthonormalize_against(&mut y, &refs).is_some() {
                    let residual = scaled_residual(a, &s, &y);
                    if residual <= opts.residual_tol {
                        let original = y.iter().zip(&s).map(|(yi, si)| yi * si).collect();
                        let seed_dof = y
                            .iter()
                            .enumerate()
                            .max_by(|x, z| x.1.abs().total_cmp(&z.1.abs()))
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        basis.push(NullVector { seed: seed_dof, scaled: y, original, residual });
                        found.push(u.clone());
                        new_null = true;
                    }
                }
            }
        }
        if new_null {
            block = (0..p).map(|_| start_vector(&mut seed, n, &fixed)).collect();
            continue;
        }
        // Eigen-residual of K_fix: rows of fixed DOFs do not belong to it.
        let r0: Vec<f64> = kritz[0]
            .iter()
            .zip(&ritz[0])
            .zip(&fixed)
            .map(|((k, u), &f)| if f { 0.0 } else { k - theta[0] * u })
            .collect();
        theta_min = Some(theta[0]);
        if norm2(&r0) <= 1e-2 * theta[0].abs() {
            converged = true;
            break;
        }
        block = ritz;
    }

    let max_residual = basis.iter().map(|v| v.residual).fold(0.0, f64::max);
    let noise = max_residual.max(100.0 * eta * norm_ks);
    let gap_ratio = theta_min.map(|t| t / noise.max(f64::MIN_POSITIVE));
    let certified = free == 0 || (converged && gap_ratio.is_some_and(|g| g >= opts.gap_factor));
    let upper = (!truncated && certified).then_some(flagged.len() + found.len());
    Ok(NullSpaceReport {
        backend: backend.capabilities().name,
        options: opts.clone(),
        candidates_tested: tested,
        rejected_residual: rej_res,
        rejected_dependent: rej_dep,
        truncated,
        min_unflagged_pivot,
        found_by_iteration: found.len(),
        iterations,
        iteration_converged: converged,
        fixed_min_eigenvalue: theta_min,
        solve_backward_error: eta,
        solves,
        nullity_lower_bound: basis.len(),
        nullity_upper_bound: upper,
        max_residual,
        spectral_gap_ratio: if free == 0 { None } else { gap_ratio },
        flagged,
        vectors: basis,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sparse::TripletBuilder;

    /// Block-diagonal union of `parts` path-graph Laplacians of size m; the
    /// nullity is exactly `parts` (one constant vector per component).
    fn laplacian_components(parts: usize, m: usize, stiff: f64) -> Csc {
        let n = parts * m;
        let mut t = TripletBuilder::new(n).expect("n");
        for p in 0..parts {
            let k = stiff * (p + 1) as f64 * 1e3_f64.powi(p as i32); // mixed magnitudes
            for i in 0..m - 1 {
                let (a, b) = (p * m + i, p * m + i + 1);
                t.add(a, a, k).expect("ok");
                t.add(b, b, k).expect("ok");
                t.add(a, b, -k).expect("ok");
                t.add(b, a, -k).expect("ok");
            }
        }
        t.to_csc()
    }

    fn grid_laplacian(m: usize) -> Csc {
        let n = m * m * m;
        let id = |i: usize, j: usize, k: usize| (k * m + j) * m + i;
        let mut t = TripletBuilder::new(n).expect("n");
        for k in 0..m {
            for j in 0..m {
                for i in 0..m {
                    for (a, b, c) in [(i + 1, j, k), (i, j + 1, k), (i, j, k + 1)] {
                        if a < m && b < m && c < m {
                            let (p, q) = (id(i, j, k), id(a, b, c));
                            t.add(p, p, 1.0).expect("ok");
                            t.add(q, q, 1.0).expect("ok");
                            t.add(p, q, -1.0).expect("ok");
                            t.add(q, p, -1.0).expect("ok");
                        }
                    }
                }
            }
        }
        t.to_csc()
    }

    fn backends() -> Vec<Backend> {
        Backend::all()
    }

    #[test]
    fn exact_nullity_of_disconnected_components() {
        let a = laplacian_components(4, 50, 2.0);
        for b in backends() {
            let r = analyze(b, &a, &NullSpaceOptions::default()).expect("analyze");
            assert_eq!(r.exact(), Some(4), "{b:?}: {r:?}");
            assert!(r.max_residual < 1e-12, "{b:?}");
            // Orthonormal basis.
            for (i, u) in r.vectors.iter().enumerate() {
                for (j, v) in r.vectors.iter().enumerate() {
                    let expect = if i == j { 1.0 } else { 0.0 };
                    assert!((dot(&u.scaled, &v.scaled) - expect).abs() < 1e-12);
                }
            }
        }
    }

    #[test]
    fn exact_nullity_one_on_3d_grid() {
        let a = grid_laplacian(12);
        for b in backends() {
            let r = analyze(b, &a, &NullSpaceOptions::default()).expect("analyze");
            assert_eq!(r.exact(), Some(1), "{b:?}: {r:?}");
            // The null vector of a connected graph Laplacian is constant.
            let v = &r.vectors[0].original;
            let (lo, hi) = v.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &x| (l.min(x), h.max(x)));
            assert!((hi - lo).abs() < 1e-10 * hi.abs().max(lo.abs()), "{b:?}");
        }
    }

    #[test]
    fn null_directions_missed_by_flagging_are_recovered() {
        // pivot_eps so small that roundoff pivots escape flagging: the inverse
        // iteration on K_fix must still find every null vector.
        let a = laplacian_components(4, 50, 2.0);
        let opts = NullSpaceOptions { pivot_eps: 1e-300, ..Default::default() };
        for b in backends() {
            let r = analyze(b, &a, &opts).expect("analyze");
            assert_eq!(r.exact(), Some(4), "{b:?}: {r:?}");
            assert!(r.found_by_iteration >= 1, "{b:?}: iteration not exercised");
        }
    }

    #[test]
    fn spd_matrix_has_trivial_null_space() {
        let mut a = grid_laplacian(6);
        for k in 0..a.n {
            // shift the diagonal: SPD
            let (s0, s1) = (a.col_ptr[k], a.col_ptr[k + 1]);
            for p in s0..s1 {
                if a.row_idx[p] as usize == k {
                    a.values[p] += 0.5;
                }
            }
        }
        for b in backends() {
            let r = analyze(b, &a, &NullSpaceOptions::default()).expect("analyze");
            assert_eq!(r.exact(), Some(0), "{b:?}");
        }
    }

    #[test]
    fn resource_bound_gives_lower_bound_only() {
        let a = laplacian_components(5, 10, 1.0);
        let opts = NullSpaceOptions { max_vectors: 2, ..Default::default() };
        for b in backends() {
            let r = analyze(b, &a, &opts).expect("analyze");
            assert!(r.truncated);
            assert_eq!(r.nullity_lower_bound, 2);
            assert_eq!(r.nullity_upper_bound, None);
            assert_eq!(r.exact(), None);
        }
    }

    #[test]
    fn near_mechanism_rejected_by_residual_when_tolerance_is_strict() {
        // Two blocks joined by a spring 1e-6 times softer: not singular.
        let mut t = TripletBuilder::new(4).expect("n");
        let add = |t: &mut TripletBuilder, a: usize, b: usize, k: f64| {
            t.add(a, a, k).expect("ok");
            t.add(b, b, k).expect("ok");
            t.add(a, b, -k).expect("ok");
            t.add(b, a, -k).expect("ok");
        };
        add(&mut t, 0, 1, 1.0);
        add(&mut t, 1, 2, 1e-6);
        add(&mut t, 2, 3, 1.0);
        t.add(0, 0, 1.0).expect("ground");
        let a = t.to_csc();
        for b in backends() {
            let r = analyze(b, &a, &NullSpaceOptions::default()).expect("analyze");
            assert_eq!(r.nullity_lower_bound, 0, "{b:?}: {r:?}");
        }
    }
}
