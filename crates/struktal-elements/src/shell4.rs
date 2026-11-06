//! Flat four-node shell (decision 0019): 24 DOF, [u1 u2 u3 r1 r2 r3] per node in
//! the element's local axes, axis 3 normal.
//!
//! - Membrane: bilinear with Wilson–Taylor incompatible modes in the QM6 form
//!   (Taylor, Beresford and Wilson, 1976), condensed: it passes the patch test
//!   and bends in plane without parasitic shear.
//! - Plate: Reissner–Mindlin with the MITC4 assumed shear (Dvorkin and Bathe,
//!   1984): no shear locking in the thin limit.
//! - Drilling rotation: the Hughes–Brezzi term, which ties r3 to the rotation of
//!   the membrane field, ω = ½(∂v/∂x − ∂u/∂y). It is not a stiffness added to r3
//!   alone: rigid-body motions cost no energy, so mechanisms stay visible. With
//!   a bilinear field the term stiffens in-plane bending, so its factor is
//!   `DRILLING_FACTOR` × G, not G (see decision 0019 for what that costs).

#![allow(clippy::needless_range_loop)] // small dense matrices: index form mirrors the formulas

pub type Mat24 = [[f64; 24]; 24];

/// Isotropic shell properties. `membrane_t` and `bending_t` are 0 to leave that
/// behavior out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShellProps {
    pub e: f64,
    pub nu: f64,
    pub g: f64,
    pub membrane_t: f64,
    pub bending_t: f64,
    /// Shear correction times bending thickness; 5/6 t.
    pub shear_t: f64,
}

/// γ / G in the Hughes–Brezzi term. γ = G stiffens a wall in pure bending by 8 %
/// (V-SHELL-002); the error falls in proportion to this factor.
pub const DRILLING_FACTOR: f64 = 1e-3;

const GAUSS: [f64; 2] = [-0.577_350_269_189_625_8, 0.577_350_269_189_625_8];
/// Natural coordinates of the nodes, counterclockwise.
const XI: [f64; 4] = [-1.0, 1.0, 1.0, -1.0];
const ETA: [f64; 4] = [-1.0, -1.0, 1.0, 1.0];

/// Shape functions and their derivatives with respect to x and y, with det J.
struct Point {
    n: [f64; 4],
    dx: [f64; 4],
    dy: [f64; 4],
    det: f64,
}

fn jacobian(xy: &[[f64; 2]; 4], r: f64, s: f64) -> ([[f64; 2]; 2], f64) {
    let (mut j00, mut j01, mut j10, mut j11) = (0.0, 0.0, 0.0, 0.0);
    for i in 0..4 {
        let (dr, ds) = (0.25 * XI[i] * (1.0 + s * ETA[i]), 0.25 * ETA[i] * (1.0 + r * XI[i]));
        j00 += dr * xy[i][0];
        j01 += dr * xy[i][1];
        j10 += ds * xy[i][0];
        j11 += ds * xy[i][1];
    }
    ([[j00, j01], [j10, j11]], j00 * j11 - j01 * j10)
}

fn point(xy: &[[f64; 2]; 4], r: f64, s: f64) -> Point {
    let (j, det) = jacobian(xy, r, s);
    let mut p = Point { n: [0.0; 4], dx: [0.0; 4], dy: [0.0; 4], det };
    for i in 0..4 {
        p.n[i] = 0.25 * (1.0 + r * XI[i]) * (1.0 + s * ETA[i]);
        let (dr, ds) = (0.25 * XI[i] * (1.0 + s * ETA[i]), 0.25 * ETA[i] * (1.0 + r * XI[i]));
        p.dx[i] = (j[1][1] * dr - j[0][1] * ds) / det;
        p.dy[i] = (-j[1][0] * dr + j[0][0] * ds) / det;
    }
    p
}

/// Plane-stress or bending constitutive matrix per unit thickness.
fn plane_c(e: f64, nu: f64) -> [[f64; 3]; 3] {
    let f = e / (1.0 - nu * nu);
    [[f, f * nu, 0.0], [f * nu, f, 0.0], [0.0, 0.0, f * (1.0 - nu) / 2.0]]
}

/// Strain rows for the incompatible modes of the QM6 form: derivatives of
/// (1 − r²) and (1 − s²) mapped with the Jacobian at the center and scaled by
/// det J₀ / det J, which makes them vanish in the mean. That is the patch test.
fn incompatible(xy: &[[f64; 2]; 4], r: f64, s: f64, det: f64) -> [[f64; 2]; 2] {
    let (j, det0) = jacobian(xy, 0.0, 0.0);
    let factor = det0 / det;
    let (dr1, ds1) = (-2.0 * r, 0.0);
    let (dr2, ds2) = (0.0, -2.0 * s);
    let map = |dr: f64, ds: f64| {
        [(j[1][1] * dr - j[0][1] * ds) / det0 * factor, (-j[1][0] * dr + j[0][0] * ds) / det0 * factor]
    };
    let a = map(dr1, ds1);
    let b = map(dr2, ds2);
    [[a[0], b[0]], [a[1], b[1]]]
}

/// Membrane strains [εx, εy, γxy] at a point from the 12 DOF [u v ω] per node,
/// and the 4 internal modes, as a 3 × 16 matrix.
fn membrane_b(p: &Point, modes: &[[f64; 2]; 2]) -> [[f64; 16]; 3] {
    let mut b = [[0.0; 16]; 3];
    for i in 0..4 {
        b[0][3 * i] = p.dx[i];
        b[1][3 * i + 1] = p.dy[i];
        b[2][3 * i] = p.dy[i];
        b[2][3 * i + 1] = p.dx[i];
    }
    for m in 0..2 {
        let (dx, dy) = (modes[0][m], modes[1][m]);
        b[0][12 + 2 * m] = dx;
        b[1][12 + 2 * m + 1] = dy;
        b[2][12 + 2 * m] = dy;
        b[2][12 + 2 * m + 1] = dx;
    }
    b
}

/// Row of the Hughes–Brezzi term: ω − ½(∂v/∂x − ∂u/∂y), for [u v ω] per node.
fn drilling_row(p: &Point) -> [f64; 12] {
    let mut row = [0.0; 12];
    for i in 0..4 {
        row[3 * i] = 0.5 * p.dy[i];
        row[3 * i + 1] = -0.5 * p.dx[i];
        row[3 * i + 2] = p.n[i];
    }
    row
}

/// Solves the symmetric positive definite 4 × 4 internal block for several
/// right-hand sides, by Gauss elimination without pivoting.
fn solve4(a: &[[f64; 4]; 4], rhs: &mut [[f64; 4]]) {
    let mut m = *a;
    for i in 0..4 {
        let d = m[i][i];
        for r in i + 1..4 {
            let f = m[r][i] / d;
            for c in i..4 {
                m[r][c] -= f * m[i][c];
            }
            for b in rhs.iter_mut() {
                b[r] -= f * b[i];
            }
        }
    }
    for b in rhs.iter_mut() {
        for i in (0..4).rev() {
            let mut v = b[i];
            for c in i + 1..4 {
                v -= m[i][c] * b[c];
            }
            b[i] = v / m[i][i];
        }
    }
}

/// Condensed membrane stiffness, 12 DOF [u v ω] per node, and the operator that
/// recovers the internal modes from them: q = R d.
fn membrane(xy: &[[f64; 2]; 4], p: &ShellProps) -> ([[f64; 12]; 12], [[f64; 12]; 4]) {
    let c = plane_c(p.e, p.nu);
    let t = p.membrane_t;
    let mut k = [[0.0; 16]; 16];
    let mut drill = [[0.0; 12]; 12];
    for &r in &GAUSS {
        for &s in &GAUSS {
            let pt = point(xy, r, s);
            let modes = incompatible(xy, r, s, pt.det);
            let b = membrane_b(&pt, &modes);
            let w = pt.det * t;
            for i in 0..16 {
                let mut cb = [0.0; 3];
                for q in 0..3 {
                    cb[q] = (0..3).map(|n| c[q][n] * b[n][i]).sum();
                }
                for j in 0..16 {
                    k[i][j] += w * (0..3).map(|q| b[q][j] * cb[q]).sum::<f64>();
                }
            }
            let row = drilling_row(&pt);
            for i in 0..12 {
                for j in 0..12 {
                    drill[i][j] += w * DRILLING_FACTOR * p.g * row[i] * row[j];
                }
            }
        }
    }
    let mut kii = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            kii[i][j] = k[12 + i][12 + j];
        }
    }
    // Columns of K_ie: solve K_ii X = K_ie.
    let mut x = [[0.0; 4]; 12];
    for c_ in 0..12 {
        for i in 0..4 {
            x[c_][i] = k[12 + i][c_];
        }
    }
    solve4(&kii, &mut x);
    let mut out = [[0.0; 12]; 12];
    let mut recover = [[0.0; 12]; 4];
    for i in 0..12 {
        for j in 0..12 {
            let v: f64 = (0..4).map(|q| k[i][12 + q] * x[j][q]).sum();
            out[i][j] = k[i][j] - v + drill[i][j];
        }
    }
    for q in 0..4 {
        for j in 0..12 {
            recover[q][j] = -x[j][q];
        }
    }
    (out, recover)
}

/// Covariant-free MITC4 shear: strains [γxz, γyz] at (r, s) from the DOF
/// [w, θx, θy] per node, where θx and θy are the section rotations that give
/// γxz = ∂w/∂x + θx and γyz = ∂w/∂y + θy. Direct strains are sampled at the
/// midsides A (r = 0, s = −1), C (0, 1), D (−1, 0), B (1, 0).
fn mitc_shear(xy: &[[f64; 2]; 4], r: f64, s: f64) -> [[f64; 12]; 2] {
    // Covariant strain at a midside, from the two nodes on that side.
    let along = |a: usize, b: usize| -> ([f64; 12], f64) {
        let dx = 0.5 * (xy[b][0] - xy[a][0]);
        let dy = 0.5 * (xy[b][1] - xy[a][1]);
        let mut row = [0.0; 12];
        row[3 * a] = -0.5;
        row[3 * b] = 0.5;
        // Mean section rotation of the side, projected on the side direction.
        row[3 * a + 1] = 0.5 * dx;
        row[3 * b + 1] = 0.5 * dx;
        row[3 * a + 2] = 0.5 * dy;
        row[3 * b + 2] = 0.5 * dy;
        (row, 0.0)
    };
    let (e_a, _) = along(0, 1); // r direction, s = −1
    let (e_c, _) = along(3, 2); // r direction, s = +1
    let (e_d, _) = along(0, 3); // s direction, r = −1
    let (e_b, _) = along(1, 2); // s direction, r = +1
    let mut grs = [[0.0; 12]; 2];
    for i in 0..12 {
        grs[0][i] = 0.5 * (1.0 - s) * e_a[i] + 0.5 * (1.0 + s) * e_c[i];
        grs[1][i] = 0.5 * (1.0 - r) * e_d[i] + 0.5 * (1.0 + r) * e_b[i];
    }
    // From covariant (r, s) to Cartesian (x, y): γ_cart = J⁻¹ γ_cov.
    let (j, det) = jacobian(xy, r, s);
    let inv = [[j[1][1] / det, -j[0][1] / det], [-j[1][0] / det, j[0][0] / det]];
    let mut out = [[0.0; 12]; 2];
    for i in 0..12 {
        out[0][i] = inv[0][0] * grs[0][i] + inv[0][1] * grs[1][i];
        out[1][i] = inv[1][0] * grs[0][i] + inv[1][1] * grs[1][i];
    }
    out
}

/// Bending curvatures [κx, κy, κxy] from [w, θx, θy] per node, with the sign of
/// the section rotations above: κx = ∂θx/∂x and so on.
fn bending_b(p: &Point) -> [[f64; 12]; 3] {
    let mut b = [[0.0; 12]; 3];
    for i in 0..4 {
        b[0][3 * i + 1] = p.dx[i];
        b[1][3 * i + 2] = p.dy[i];
        b[2][3 * i + 1] = p.dy[i];
        b[2][3 * i + 2] = p.dx[i];
    }
    b
}

/// Plate stiffness on [w, θx, θy] per node.
fn plate(xy: &[[f64; 2]; 4], p: &ShellProps) -> [[f64; 12]; 12] {
    let c = plane_c(p.e, p.nu);
    let tb = p.bending_t.powi(3) / 12.0;
    let mut k = [[0.0; 12]; 12];
    for &r in &GAUSS {
        for &s in &GAUSS {
            let pt = point(xy, r, s);
            let b = bending_b(&pt);
            let gamma = mitc_shear(xy, r, s);
            for i in 0..12 {
                let mut cb = [0.0; 3];
                for q in 0..3 {
                    cb[q] = (0..3).map(|n| c[q][n] * b[n][i]).sum();
                }
                for j in 0..12 {
                    let bend: f64 = (0..3).map(|q| b[q][j] * cb[q]).sum();
                    let shear = gamma[0][i] * gamma[0][j] + gamma[1][i] * gamma[1][j];
                    k[i][j] += pt.det * (tb * bend + p.g * p.shear_t * shear);
                }
            }
        }
    }
    k
}

/// Global DOF of the membrane block: u, v, r3 of each node.
fn membrane_dof(i: usize) -> usize {
    let (node, comp) = (i / 3, i % 3);
    6 * node + [0, 1, 5][comp]
}

/// Plate block: w = u3, and the section rotations from the nodal rotations, with
/// the displacements u = z θx and v = z θy: θx = r2 and θy = −r1.
fn plate_dof(i: usize) -> (usize, f64) {
    let (node, comp) = (i / 3, i % 3);
    match comp {
        0 => (6 * node + 2, 1.0),
        1 => (6 * node + 4, 1.0),
        _ => (6 * node + 3, -1.0),
    }
}

/// Stiffness in local axes. `xy` are the node coordinates in the element plane.
pub fn local_stiffness(xy: &[[f64; 2]; 4], p: &ShellProps) -> Mat24 {
    let mut out = [[0.0; 24]; 24];
    if p.membrane_t > 0.0 {
        let (km, _) = membrane(xy, p);
        for i in 0..12 {
            for j in 0..12 {
                out[membrane_dof(i)][membrane_dof(j)] += km[i][j];
            }
        }
    }
    if p.bending_t > 0.0 {
        let kp = plate(xy, p);
        for i in 0..12 {
            let (a, sa) = plate_dof(i);
            for j in 0..12 {
                let (b, sb) = plate_dof(j);
                out[a][b] += sa * sb * kp[i][j];
            }
        }
    }
    out
}

/// Area of each node's tributary in ∫ Nᵢ dA, the consistent share of a uniform
/// pressure. They add up to the area of the element.
pub fn nodal_areas(xy: &[[f64; 2]; 4]) -> [f64; 4] {
    let mut a = [0.0; 4];
    for &r in &GAUSS {
        for &s in &GAUSS {
            let pt = point(xy, r, s);
            for i in 0..4 {
                a[i] += pt.n[i] * pt.det;
            }
        }
    }
    a
}

/// Section forces per unit length at (r, s): [F11, F22, F12, M11, M22, M12, V13,
/// V23], from the local nodal displacements. M11 is the moment from stresses σ11:
/// positive with tension on the +3 face.
pub fn resultants(xy: &[[f64; 2]; 4], p: &ShellProps, u: &[f64; 24], r: f64, s: f64) -> [f64; 8] {
    let c = plane_c(p.e, p.nu);
    let pt = point(xy, r, s);
    let mut out = [0.0; 8];
    if p.membrane_t > 0.0 {
        let (_, recover) = membrane(xy, p);
        let mut d = [0.0; 16];
        for i in 0..12 {
            d[i] = u[membrane_dof(i)];
        }
        for q in 0..4 {
            d[12 + q] = (0..12).map(|j| recover[q][j] * d[j]).sum();
        }
        let modes = incompatible(xy, r, s, pt.det);
        let b = membrane_b(&pt, &modes);
        let strain: [f64; 3] = std::array::from_fn(|q| (0..16).map(|i| b[q][i] * d[i]).sum());
        for q in 0..3 {
            out[q] = p.membrane_t * (0..3).map(|n| c[q][n] * strain[n]).sum::<f64>();
        }
    }
    if p.bending_t > 0.0 {
        let mut d = [0.0; 12];
        for i in 0..12 {
            let (a, sa) = plate_dof(i);
            d[i] = sa * u[a];
        }
        let b = bending_b(&pt);
        let kappa: [f64; 3] = std::array::from_fn(|q| (0..12).map(|i| b[q][i] * d[i]).sum());
        let tb = p.bending_t.powi(3) / 12.0;
        for q in 0..3 {
            // A positive θx gain along x stretches the +3 face: tension there.
            out[3 + q] = tb * (0..3).map(|n| c[q][n] * kappa[n]).sum::<f64>();
        }
        let gamma = mitc_shear(xy, r, s);
        for q in 0..2 {
            out[6 + q] = p.g * p.shear_t * (0..12).map(|i| gamma[q][i] * d[i]).sum::<f64>();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROPS: ShellProps =
        ShellProps { e: 2.0e11, nu: 0.3, g: 2.0e11 / 2.6, membrane_t: 0.2, bending_t: 0.2, shear_t: 0.2 * 5.0 / 6.0 };

    /// Irregular quad, to catch formulas that only work on rectangles.
    const QUAD: [[f64; 2]; 4] = [[0.1, -0.2], [2.3, 0.1], [2.8, 1.9], [-0.3, 1.4]];

    fn mul(k: &Mat24, u: &[f64; 24]) -> [f64; 24] {
        std::array::from_fn(|i| (0..24).map(|j| k[i][j] * u[j]).sum())
    }

    #[test]
    fn stiffness_is_symmetric_with_six_force_free_rigid_modes() {
        let k = local_stiffness(&QUAD, &PROPS);
        let scale = k.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
        for i in 0..24 {
            for j in 0..24 {
                assert!((k[i][j] - k[j][i]).abs() <= 1e-10 * scale, "{i},{j}");
            }
        }
        for mode in 0..6 {
            let mut w = [0.0; 3];
            let mut t = [0.0; 3];
            if mode < 3 {
                t[mode] = 1.0;
            } else {
                w[mode - 3] = 1.0;
            }
            let mut u = [0.0; 24];
            for n in 0..4 {
                let r = [QUAD[n][0], QUAD[n][1], 0.0];
                let c = [w[1] * r[2] - w[2] * r[1], w[2] * r[0] - w[0] * r[2], w[0] * r[1] - w[1] * r[0]];
                for k_ in 0..3 {
                    u[6 * n + k_] = t[k_] + c[k_];
                    u[6 * n + 3 + k_] = w[k_];
                }
            }
            for v in mul(&k, &u) {
                assert!(v.abs() <= 1e-7 * scale, "mode {mode}: {v} (scale {scale})");
            }
        }
    }

    /// Rank by Gaussian elimination with full pivoting.
    fn rank(k: &Mat24) -> usize {
        let mut m = *k;
        let scale = m.iter().flatten().fold(0.0_f64, |a, v| a.max(v.abs()));
        let mut rank = 0;
        for c in 0..24 {
            let (mut pr, mut pc, mut best) = (c, c, 0.0);
            for r in c..24 {
                for q in c..24 {
                    if m[r][q].abs() > best {
                        (pr, pc, best) = (r, q, m[r][q].abs());
                    }
                }
            }
            if best <= 1e-9 * scale {
                break;
            }
            m.swap(c, pr);
            for row in m.iter_mut() {
                row.swap(c, pc);
            }
            rank += 1;
            for r in c + 1..24 {
                let f = m[r][c] / m[c][c];
                for q in c..24 {
                    m[r][q] -= f * m[c][q];
                }
            }
        }
        rank
    }

    #[test]
    fn only_the_rigid_modes_are_free() {
        // No spurious zero-energy mode, drilling included: 24 − 6.
        assert_eq!(rank(&local_stiffness(&QUAD, &PROPS)), 18);
    }

    #[test]
    fn membrane_patch_test_constant_strain_is_exact_on_a_distorted_quad() {
        // Linear field with no rotation: u = a x + b y, v = b x + c y, r3 = 0.
        let (a, b, c) = (1e-3, 4e-4, -7e-4);
        let props = ShellProps { bending_t: 0.0, shear_t: 0.0, ..PROPS };
        let mut u = [0.0; 24];
        for n in 0..4 {
            u[6 * n] = a * QUAD[n][0] + b * QUAD[n][1];
            u[6 * n + 1] = b * QUAD[n][0] + c * QUAD[n][1];
        }
        let cm = plane_c(props.e, props.nu);
        let strain = [a, c, 2.0 * b];
        let expected: [f64; 3] =
            std::array::from_fn(|q| props.membrane_t * (0..3).map(|n| cm[q][n] * strain[n]).sum::<f64>());
        for (r, s) in [(0.0, 0.0), (0.5, -0.3), (-1.0, 1.0)] {
            let f = resultants(&QUAD, &props, &u, r, s);
            for q in 0..3 {
                assert!(
                    (f[q] - expected[q]).abs() <= 1e-6 * expected[q].abs().max(1.0),
                    "{r},{s} {q}: {} vs {}",
                    f[q],
                    expected[q]
                );
            }
        }
    }

    #[test]
    fn plate_patch_test_constant_curvature_has_no_shear() {
        // w = −(a x² + 2 c x y + b y²)/2 with matching rotations: θx = −∂w/∂x.
        let (a, b, c) = (1e-3, -6e-4, 3e-4);
        let props = ShellProps { membrane_t: 0.0, ..PROPS };
        let mut u = [0.0; 24];
        for n in 0..4 {
            let (x, y) = (QUAD[n][0], QUAD[n][1]);
            let (wx, wy) = (-(a * x + c * y), -(b * y + c * x));
            u[6 * n + 2] = -(a * x * x + 2.0 * c * x * y + b * y * y) / 2.0;
            // θx = r2 and θy = −r1, with θ = −∇w.
            u[6 * n + 4] = -wx;
            u[6 * n + 3] = wy;
        }
        let cm = plane_c(props.e, props.nu);
        let kappa = [a, b, 2.0 * c];
        let tb = props.bending_t.powi(3) / 12.0;
        let expected: [f64; 3] = std::array::from_fn(|q| tb * (0..3).map(|n| cm[q][n] * kappa[n]).sum::<f64>());
        let f = resultants(&QUAD, &props, &u, 0.2, -0.4);
        for q in 0..3 {
            assert!(
                (f[3 + q] - expected[q]).abs() <= 1e-6 * expected[q].abs().max(1.0),
                "{q}: {} vs {}",
                f[3 + q],
                expected[q]
            );
        }
    }

    #[test]
    fn nodal_areas_add_up_to_the_area() {
        let a = nodal_areas(&QUAD);
        // Shoelace formula.
        let mut area = 0.0;
        for n in 0..4 {
            let m = (n + 1) % 4;
            area += QUAD[n][0] * QUAD[m][1] - QUAD[m][0] * QUAD[n][1];
        }
        assert!((a.iter().sum::<f64>() - area / 2.0).abs() < 1e-12);
    }
}
