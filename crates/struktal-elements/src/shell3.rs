//! Flat three-node shell (decision 0021), on the same 24-DOF frame as `shell4`,
//! with the fourth node's rows and columns zero, so the assembly treats both alike.
//!
//! - Membrane: constant strain, with the same Hughes–Brezzi drilling term.
//! - Plate: Reissner–Mindlin with the MITC3 assumed shear (Lee and Bathe, 2004).
//!
//! A constant-strain membrane is stiff in in-plane bending: it needs a finer mesh
//! than the four-node element for walls.

#![allow(clippy::needless_range_loop)] // small dense matrices: index form mirrors the formulas

use crate::shell4::{DRILLING_FACTOR, Mat24, ShellProps};

/// Midside integration: exact for the quadratic energies below.
const POINTS: [(f64, f64); 3] = [(0.5, 0.0), (0.5, 0.5), (0.0, 0.5)];
const WEIGHT: f64 = 1.0 / 3.0;

struct Geometry {
    /// Twice the area.
    det: f64,
    dx: [f64; 3],
    dy: [f64; 3],
    /// Tangents ∂x/∂r and ∂x/∂s.
    gr: [f64; 2],
    gs: [f64; 2],
}

fn geometry(xy: &[[f64; 2]; 3]) -> Geometry {
    let gr = [xy[1][0] - xy[0][0], xy[1][1] - xy[0][1]];
    let gs = [xy[2][0] - xy[0][0], xy[2][1] - xy[0][1]];
    let det = gr[0] * gs[1] - gr[1] * gs[0];
    let dx = [(xy[1][1] - xy[2][1]) / det, (xy[2][1] - xy[0][1]) / det, (xy[0][1] - xy[1][1]) / det];
    let dy = [(xy[2][0] - xy[1][0]) / det, (xy[0][0] - xy[2][0]) / det, (xy[1][0] - xy[0][0]) / det];
    Geometry { det, dx, dy, gr, gs }
}

fn shape(r: f64, s: f64) -> [f64; 3] {
    [1.0 - r - s, r, s]
}

fn plane_c(e: f64, nu: f64) -> [[f64; 3]; 3] {
    let f = e / (1.0 - nu * nu);
    [[f, f * nu, 0.0], [f * nu, f, 0.0], [0.0, 0.0, f * (1.0 - nu) / 2.0]]
}

fn membrane_dof(i: usize) -> usize {
    6 * (i / 3) + [0, 1, 5][i % 3]
}

/// w, θx = r2, θy = −r1, as in `shell4`.
fn plate_dof(i: usize) -> (usize, f64) {
    let node = i / 3;
    match i % 3 {
        0 => (6 * node + 2, 1.0),
        1 => (6 * node + 4, 1.0),
        _ => (6 * node + 3, -1.0),
    }
}

fn membrane_b(g: &Geometry) -> [[f64; 9]; 3] {
    let mut b = [[0.0; 9]; 3];
    for i in 0..3 {
        b[0][3 * i] = g.dx[i];
        b[1][3 * i + 1] = g.dy[i];
        b[2][3 * i] = g.dy[i];
        b[2][3 * i + 1] = g.dx[i];
    }
    b
}

fn bending_b(g: &Geometry) -> [[f64; 9]; 3] {
    let mut b = [[0.0; 9]; 3];
    for i in 0..3 {
        b[0][3 * i + 1] = g.dx[i];
        b[1][3 * i + 2] = g.dy[i];
        b[2][3 * i + 1] = g.dy[i];
        b[2][3 * i + 2] = g.dx[i];
    }
    b
}

/// Covariant shear strains at (r, s) on [w, θx, θy] per node, directly.
fn covariant(g: &Geometry, r: f64, s: f64) -> ([f64; 9], [f64; 9]) {
    let n = shape(r, s);
    let (mut er, mut es) = ([0.0; 9], [0.0; 9]);
    let (dnr, dns) = ([-1.0, 1.0, 0.0], [-1.0, 0.0, 1.0]);
    for i in 0..3 {
        er[3 * i] = dnr[i];
        er[3 * i + 1] = n[i] * g.gr[0];
        er[3 * i + 2] = n[i] * g.gr[1];
        es[3 * i] = dns[i];
        es[3 * i + 1] = n[i] * g.gs[0];
        es[3 * i + 2] = n[i] * g.gs[1];
    }
    (er, es)
}

/// MITC3: covariant strains tied at the midsides. Cartesian [γxz, γyz] at (r, s).
fn mitc_shear(g: &Geometry, r: f64, s: f64) -> [[f64; 9]; 2] {
    let (e1r, _) = covariant(g, 0.5, 0.0);
    let (_, e2s) = covariant(g, 0.0, 0.5);
    let (e3r, e3s) = covariant(g, 0.5, 0.5);
    let mut er = [0.0; 9];
    let mut es = [0.0; 9];
    for i in 0..9 {
        let c = (e3r[i] - e1r[i]) - (e3s[i] - e2s[i]);
        er[i] = e1r[i] + c * s;
        es[i] = e2s[i] - c * r;
    }
    // γ_cart = J⁻¹ γ_cov, with J = [gr; gs].
    let inv = [[g.gs[1] / g.det, -g.gr[1] / g.det], [-g.gs[0] / g.det, g.gr[0] / g.det]];
    let mut out = [[0.0; 9]; 2];
    for i in 0..9 {
        out[0][i] = inv[0][0] * er[i] + inv[0][1] * es[i];
        out[1][i] = inv[1][0] * er[i] + inv[1][1] * es[i];
    }
    out
}

pub fn local_stiffness(xy: &[[f64; 2]; 3], p: &ShellProps) -> Mat24 {
    let g = geometry(xy);
    let area = 0.5 * g.det;
    let c = plane_c(p.e, p.nu);
    let mut out = [[0.0; 24]; 24];
    if p.membrane_t > 0.0 {
        let b = membrane_b(&g);
        for i in 0..9 {
            for j in 0..9 {
                let v: f64 = (0..3).map(|q| (0..3).map(|n| b[q][i] * c[q][n] * b[n][j]).sum::<f64>()).sum();
                out[membrane_dof(i)][membrane_dof(j)] += area * p.membrane_t * v;
            }
        }
        for &(r, s) in &POINTS {
            let n = shape(r, s);
            let mut row = [0.0; 9];
            for i in 0..3 {
                row[3 * i] = 0.5 * g.dy[i];
                row[3 * i + 1] = -0.5 * g.dx[i];
                row[3 * i + 2] = n[i];
            }
            for i in 0..9 {
                for j in 0..9 {
                    out[membrane_dof(i)][membrane_dof(j)] +=
                        WEIGHT * area * p.membrane_t * DRILLING_FACTOR * p.g * row[i] * row[j];
                }
            }
        }
    }
    if p.bending_t > 0.0 {
        let tb = p.bending_t.powi(3) / 12.0;
        let b = bending_b(&g);
        let mut k = [[0.0; 9]; 9];
        for i in 0..9 {
            for j in 0..9 {
                let v: f64 = (0..3).map(|q| (0..3).map(|n| b[q][i] * c[q][n] * b[n][j]).sum::<f64>()).sum();
                k[i][j] += area * tb * v;
            }
        }
        for &(r, s) in &POINTS {
            let gamma = mitc_shear(&g, r, s);
            for i in 0..9 {
                for j in 0..9 {
                    k[i][j] +=
                        WEIGHT * area * p.g * p.shear_t * (gamma[0][i] * gamma[0][j] + gamma[1][i] * gamma[1][j]);
                }
            }
        }
        for i in 0..9 {
            let (a, sa) = plate_dof(i);
            for j in 0..9 {
                let (b_, sb) = plate_dof(j);
                out[a][b_] += sa * sb * k[i][j];
            }
        }
    }
    out
}

/// ∫ Nᵢ dA: a third of the area each.
pub fn nodal_areas(xy: &[[f64; 2]; 3]) -> [f64; 4] {
    let a = 0.5 * geometry(xy).det / 3.0;
    [a, a, a, 0.0]
}

/// Section forces at (r, s), as in `shell4::resultants`.
pub fn resultants(xy: &[[f64; 2]; 3], p: &ShellProps, u: &[f64; 24], r: f64, s: f64) -> [f64; 8] {
    let g = geometry(xy);
    let c = plane_c(p.e, p.nu);
    let mut out = [0.0; 8];
    if p.membrane_t > 0.0 {
        let b = membrane_b(&g);
        let strain: [f64; 3] = std::array::from_fn(|q| (0..9).map(|i| b[q][i] * u[membrane_dof(i)]).sum());
        for q in 0..3 {
            out[q] = p.membrane_t * (0..3).map(|n| c[q][n] * strain[n]).sum::<f64>();
        }
    }
    if p.bending_t > 0.0 {
        let d: [f64; 9] = std::array::from_fn(|i| {
            let (a, sa) = plate_dof(i);
            sa * u[a]
        });
        let b = bending_b(&g);
        let kappa: [f64; 3] = std::array::from_fn(|q| (0..9).map(|i| b[q][i] * d[i]).sum());
        let tb = p.bending_t.powi(3) / 12.0;
        for q in 0..3 {
            out[3 + q] = tb * (0..3).map(|n| c[q][n] * kappa[n]).sum::<f64>();
        }
        let gamma = mitc_shear(&g, r, s);
        for q in 0..2 {
            out[6 + q] = p.g * p.shear_t * (0..9).map(|i| gamma[q][i] * d[i]).sum::<f64>();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROPS: ShellProps =
        ShellProps { e: 2.0e11, nu: 0.3, g: 2.0e11 / 2.6, membrane_t: 0.2, bending_t: 0.2, shear_t: 0.2 * 5.0 / 6.0 };
    const TRI: [[f64; 2]; 3] = [[0.1, -0.2], [2.3, 0.1], [0.7, 1.9]];

    fn mul(k: &Mat24, u: &[f64; 24]) -> [f64; 24] {
        std::array::from_fn(|i| (0..24).map(|j| k[i][j] * u[j]).sum())
    }

    #[test]
    fn symmetric_with_six_force_free_rigid_modes_and_a_zero_fourth_node() {
        let k = local_stiffness(&TRI, &PROPS);
        let scale = k.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
        for i in 0..24 {
            for j in 0..24 {
                assert!((k[i][j] - k[j][i]).abs() <= 1e-10 * scale, "{i},{j}");
                if i >= 18 || j >= 18 {
                    assert_eq!(k[i][j], 0.0);
                }
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
            for n in 0..3 {
                let r = [TRI[n][0], TRI[n][1], 0.0];
                let c = [w[1] * r[2] - w[2] * r[1], w[2] * r[0] - w[0] * r[2], w[0] * r[1] - w[1] * r[0]];
                for q in 0..3 {
                    u[6 * n + q] = t[q] + c[q];
                    u[6 * n + 3 + q] = w[q];
                }
            }
            for v in mul(&k, &u) {
                assert!(v.abs() <= 1e-7 * scale, "mode {mode}: {v}");
            }
        }
    }

    #[test]
    fn only_the_rigid_modes_are_free() {
        let k = local_stiffness(&TRI, &PROPS);
        let mut m = k;
        let scale = m.iter().flatten().fold(0.0_f64, |a, v| a.max(v.abs()));
        let mut rank = 0;
        for c in 0..18 {
            let (mut pr, mut pc, mut best) = (c, c, 0.0);
            for r in c..18 {
                for q in c..18 {
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
            for r in c + 1..18 {
                let f = m[r][c] / m[c][c];
                for q in c..18 {
                    m[r][q] -= f * m[c][q];
                }
            }
        }
        assert_eq!(rank, 12, "18 DOF minus 6 rigid modes");
    }

    #[test]
    fn constant_strain_and_constant_curvature_are_exact() {
        let cm = plane_c(PROPS.e, PROPS.nu);
        let (a, b, c) = (1e-3, 4e-4, -7e-4);
        let mut u = [0.0; 24];
        for n in 0..3 {
            let (x, y) = (TRI[n][0], TRI[n][1]);
            u[6 * n] = a * x + b * y;
            u[6 * n + 1] = b * x + c * y;
            // Curvature: w = −(a x² + 2 b x y + c y²)/2, θ = −∇w.
            u[6 * n + 2] = -(a * x * x + 2.0 * b * x * y + c * y * y) / 2.0;
            u[6 * n + 4] = a * x + b * y;
            u[6 * n + 3] = -(c * y + b * x);
        }
        let strain = [a, c, 2.0 * b];
        let f = resultants(&TRI, &PROPS, &u, 0.3, 0.3);
        for q in 0..3 {
            let m: f64 = (0..3).map(|n| cm[q][n] * strain[n]).sum();
            assert!((f[q] - PROPS.membrane_t * m).abs() <= 1e-7 * m.abs().max(1.0), "F{q}");
            let tb = PROPS.bending_t.powi(3) / 12.0;
            assert!((f[3 + q] - tb * m).abs() <= 1e-7 * (tb * m).abs().max(1.0), "M{q}");
        }
    }

    #[test]
    fn nodal_areas_add_up_to_the_area() {
        let s: f64 = nodal_areas(&TRI).iter().sum();
        let exact = 0.5
            * ((TRI[1][0] - TRI[0][0]) * (TRI[2][1] - TRI[0][1]) - (TRI[1][1] - TRI[0][1]) * (TRI[2][0] - TRI[0][0]));
        assert!((s - exact).abs() < 1e-14);
    }
}
