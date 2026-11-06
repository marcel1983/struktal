//! Lowest modes of K φ = λ M φ by subspace iteration (Bathe), with K factored
//! and M positive semidefinite. Decision 0020.
//!
//! Subspace iteration can miss a mode in principle. There is no Sturm count here,
//! because the backends only factor positive definite matrices, so the result
//! carries a separate check: a fresh vector, cleared of the modes found, is
//! iterated, and its Rayleigh quotient must not fall below the highest mode.

#![allow(clippy::needless_range_loop)] // small dense matrices: index form mirrors the formulas

use crate::factor::Factor;
use crate::sparse::Csc;

#[derive(Debug, Clone)]
pub struct Modes {
    /// λ = ω², increasing.
    pub values: Vec<f64>,
    /// M-orthonormal vectors.
    pub vectors: Vec<Vec<f64>>,
    /// ‖K φ − λ M φ‖ / ‖K φ‖ per mode.
    pub residuals: Vec<f64>,
    pub iterations: usize,
    pub converged: bool,
    /// Rayleigh quotient of the vector orthogonal to the modes found.
    pub missed_mode_probe: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EigenError {
    /// M has no mass on any free DOF.
    NoMass,
    /// M-orthonormalization broke down: too few DOF with mass for the subspace.
    RankDeficient,
}

impl std::fmt::Display for EigenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EigenError::NoMass => write!(f, "no mass on any free degree of freedom"),
            EigenError::RankDeficient => write!(f, "too few degrees of freedom with mass for this subspace"),
        }
    }
}

impl std::error::Error for EigenError {}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Cyclic Jacobi on a dense symmetric matrix: eigenvalues and vectors by column.
fn jacobi(mut a: Vec<Vec<f64>>) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut v = vec![vec![0.0; n]; n];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for _ in 0..100 {
        let off: f64 = (0..n).flat_map(|i| (0..i).map(move |j| (i, j))).map(|(i, j)| a[i][j] * a[i][j]).sum();
        let diag: f64 = (0..n).map(|i| a[i][i] * a[i][i]).sum();
        if off <= 1e-30 * diag {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for r in 0..n {
                    let (arp, arq) = (a[r][p], a[r][q]);
                    a[r][p] = c * arp - s * arq;
                    a[r][q] = s * arp + c * arq;
                }
                for r in 0..n {
                    let (apr, aqr) = (a[p][r], a[q][r]);
                    a[p][r] = c * apr - s * aqr;
                    a[q][r] = s * apr + c * aqr;
                }
                for row in v.iter_mut() {
                    let (rp, rq) = (row[p], row[q]);
                    row[p] = c * rp - s * rq;
                    row[q] = s * rp + c * rq;
                }
            }
        }
    }
    ((0..n).map(|i| a[i][i]).collect(), v)
}

/// M-orthonormalizes in place by modified Gram–Schmidt, twice.
fn orthonormalize(x: &mut [Vec<f64>], m: &Csc) -> Result<(), EigenError> {
    for i in 0..x.len() {
        for _ in 0..2 {
            for j in 0..i {
                let mj = m.mul_vec(&x[j]);
                let d = dot(&x[i], &mj);
                let xj = x[j].clone();
                x[i].iter_mut().zip(&xj).for_each(|(a, b)| *a -= d * b);
            }
        }
        let norm2 = dot(&x[i], &m.mul_vec(&x[i]));
        if norm2 <= 0.0 || !norm2.is_finite() {
            return Err(EigenError::RankDeficient);
        }
        let norm = norm2.sqrt();
        x[i].iter_mut().for_each(|a| *a /= norm);
    }
    Ok(())
}

/// A deterministic start: results must not change between runs.
fn start_vectors(n: usize, q: usize, md: &[f64], kd: &[f64]) -> Vec<Vec<f64>> {
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64 - 0.5
    };
    // Bathe: the mass diagonal, then unit vectors where mass over stiffness peaks.
    let mut order: Vec<usize> = (0..n).filter(|&i| md[i] > 0.0).collect();
    order.sort_by(|&a, &b| {
        (md[b] / kd[b].max(f64::MIN_POSITIVE)).total_cmp(&(md[a] / kd[a].max(f64::MIN_POSITIVE))).then(a.cmp(&b))
    });
    let mut out = Vec::with_capacity(q);
    out.push(md.to_vec());
    for k in 1..q {
        let mut v: Vec<f64> = (0..n).map(|i| if md[i] > 0.0 { 0.1 * next() } else { 0.0 }).collect();
        if let Some(&i) = order.get(k - 1) {
            v[i] += 1.0;
        }
        out.push(v);
    }
    out
}

/// `modes` lowest modes, or fewer if M has fewer DOF with mass.
pub fn subspace_iteration(
    factor: &Factor,
    k: &Csc,
    m: &Csc,
    modes: usize,
    tol: f64,
    max_iter: usize,
) -> Result<Modes, EigenError> {
    let n = k.diagonal().len();
    let md = m.diagonal();
    let kd = k.diagonal();
    let with_mass = md.iter().filter(|v| **v > 0.0).count();
    if with_mass == 0 {
        return Err(EigenError::NoMass);
    }
    let p = modes.min(with_mass).min(n);
    let q = (2 * p).min(p + 8).min(with_mass).min(n);
    let mut x = start_vectors(n, q, &md, &kd);
    let mut values = vec![f64::INFINITY; q];
    let mut iterations = 0;
    let mut converged = false;
    for it in 1..=max_iter {
        iterations = it;
        let mut y: Vec<Vec<f64>> = x.iter().map(|v| factor.solve(&m.mul_vec(v))).collect();
        orthonormalize(&mut y, m)?;
        // Rayleigh-Ritz: with M-orthonormal y, a standard problem on yᵀ K y.
        let ky: Vec<Vec<f64>> = y.iter().map(|v| k.mul_vec(v)).collect();
        let mut kr = vec![vec![0.0; q]; q];
        for i in 0..q {
            for j in 0..=i {
                let v = dot(&y[i], &ky[j]);
                kr[i][j] = v;
                kr[j][i] = v;
            }
        }
        let (lambda, q_mat) = jacobi(kr);
        let mut order: Vec<usize> = (0..q).collect();
        order.sort_by(|&a, &b| lambda[a].total_cmp(&lambda[b]));
        let new_x: Vec<Vec<f64>> = order
            .iter()
            .map(|&c| {
                let mut v = vec![0.0; n];
                for (i, yi) in y.iter().enumerate() {
                    let f = q_mat[i][c];
                    v.iter_mut().zip(yi).for_each(|(a, b)| *a += f * b);
                }
                v
            })
            .collect();
        let new_values: Vec<f64> = order.iter().map(|&c| lambda[c]).collect();
        let change = (0..p)
            .map(|i| (new_values[i] - values[i]).abs() / new_values[i].abs().max(f64::MIN_POSITIVE))
            .fold(0.0, f64::max);
        x = new_x;
        values = new_values;
        if change <= tol {
            converged = true;
            break;
        }
    }
    let residuals: Vec<f64> = (0..p)
        .map(|i| {
            let (kx, mx) = (k.mul_vec(&x[i]), m.mul_vec(&x[i]));
            let r: f64 = kx.iter().zip(&mx).map(|(a, b)| (a - values[i] * b).powi(2)).sum::<f64>().sqrt();
            r / dot(&kx, &kx).sqrt().max(f64::MIN_POSITIVE)
        })
        .collect();

    // Is there a mode below the ones found? Iterate a fresh vector, kept clear of them.
    let mut missed_mode_probe = None;
    if q < with_mass {
        let mut probe: Vec<f64> = start_vectors(n, q + 1, &md, &kd).pop().unwrap_or_default();
        let clear = |v: &mut Vec<f64>| {
            for xi in x.iter().take(p) {
                let d = dot(v, &m.mul_vec(xi));
                v.iter_mut().zip(xi).for_each(|(a, b)| *a -= d * b);
            }
        };
        for _ in 0..8 {
            clear(&mut probe);
            let next = factor.solve(&m.mul_vec(&probe));
            let norm = dot(&next, &m.mul_vec(&next)).sqrt();
            if norm <= 0.0 || !norm.is_finite() {
                break;
            }
            probe = next.iter().map(|v| v / norm).collect();
        }
        clear(&mut probe);
        let mp = dot(&probe, &m.mul_vec(&probe));
        if mp > 0.0 {
            missed_mode_probe = Some(dot(&probe, &k.mul_vec(&probe)) / mp);
        }
    }
    x.truncate(p);
    values.truncate(p);
    Ok(Modes { values, vectors: x, residuals, iterations, converged, missed_mode_probe })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factor::{Backend, factorize};
    use crate::skyline::MatrixClass;
    use crate::sparse::TripletBuilder;

    /// Chain of n masses m and springs s, fixed at one end. Closed form:
    /// λ_j = (4 s / m) sin²((2j − 1) π / (2 (2n + 1))).
    fn chain(n: usize, s: f64, mass: f64) -> (Csc, Csc) {
        let mut k = TripletBuilder::new(n).expect("n");
        let mut m = TripletBuilder::new(n).expect("n");
        for i in 0..n {
            k.add(i, i, if i + 1 == n { s } else { 2.0 * s }).expect("add");
            if i + 1 < n {
                k.add(i, i + 1, -s).expect("add");
                k.add(i + 1, i, -s).expect("add");
            }
            m.add(i, i, mass).expect("add");
        }
        (k.to_csc(), m.to_csc())
    }

    #[test]
    fn chain_of_springs_matches_the_closed_form() {
        let (n, s, mass) = (60, 1.0e6, 2.0);
        let (k, m) = chain(n, s, mass);
        for backend in Backend::all() {
            let f = factorize(backend, &k, MatrixClass::Spd, 1 << 30).expect("factor");
            let r = subspace_iteration(&f, &k, &m, 6, 1e-12, 500).expect("modes");
            assert!(r.converged, "{} iterations", r.iterations);
            for (j, v) in r.values.iter().enumerate() {
                let angle = (2 * j + 1) as f64 * std::f64::consts::PI / (2.0 * (2 * n + 1) as f64);
                let exact = 4.0 * s / mass * angle.sin().powi(2);
                assert!((v - exact).abs() <= 1e-9 * exact, "mode {j}: {v} vs {exact}");
                assert!(r.residuals[j] < 1e-6, "mode {j}: residual {}", r.residuals[j]);
            }
            for a in 0..6 {
                for b in 0..6 {
                    let d = dot(&r.vectors[a], &m.mul_vec(&r.vectors[b]));
                    assert!((d - if a == b { 1.0 } else { 0.0 }).abs() < 1e-9, "{a},{b}: {d}");
                }
            }
            assert!(r.missed_mode_probe.expect("probe") >= r.values[5] * (1.0 - 1e-9));
        }
    }

    #[test]
    fn massless_dofs_and_too_few_masses() {
        // Three DOF, mass on one: there is one mode only, whatever is asked for.
        let (k, _) = chain(3, 100.0, 1.0);
        let mut mt = TripletBuilder::new(3).expect("n");
        mt.add(2, 2, 4.0).expect("add");
        mt.add(0, 0, 0.0).expect("add");
        let m = mt.to_csc();
        let f = factorize(Backend::Skyline, &k, MatrixClass::Spd, 1 << 30).expect("factor");
        let r = subspace_iteration(&f, &k, &m, 5, 1e-12, 100).expect("modes");
        assert_eq!(r.values.len(), 1);
        // Three springs in series, then the mass: λ = (s/3)/m.
        assert!((r.values[0] - 100.0 / 3.0 / 4.0).abs() < 1e-9, "{}", r.values[0]);

        let mut none = TripletBuilder::new(3).expect("n");
        none.add(0, 0, 0.0).expect("add");
        assert_eq!(subspace_iteration(&f, &k, &none.to_csc(), 1, 1e-12, 10).err(), Some(EigenError::NoMass));
    }

    #[test]
    fn jacobi_on_a_known_matrix() {
        let (values, vectors) = jacobi(vec![vec![2.0, 1.0], vec![1.0, 2.0]]);
        let mut sorted = values.clone();
        sorted.sort_by(f64::total_cmp);
        assert!((sorted[0] - 1.0).abs() < 1e-14 && (sorted[1] - 3.0).abs() < 1e-14);
        // A v = λ v by column.
        for c in 0..2 {
            let (x, y) = (vectors[0][c], vectors[1][c]);
            assert!((2.0 * x + y - values[c] * x).abs() < 1e-13);
        }
    }
}
