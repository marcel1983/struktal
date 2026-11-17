//! Plane-stress elements: constant-strain triangle and bilinear quadrilateral,
//! two degrees of freedom per node (`u`, `v`), with the thickness as a factor.
//!
//! - Triangle: constant strain (CST), one point.
//! - Quadrilateral: bilinear, 2 × 2 Gauss. It has no drilling term, so the
//!   rigid-body rotation is free and the patch test is exact.
//!
//! The nodes are counterclockwise seen from +z, as in the areas.

#![allow(clippy::needless_range_loop)] // small dense matrices: index form mirrors the formulas

pub type Mat8 = [[f64; 8]; 8];

/// Isotropic plane-stress properties.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaneProps {
    pub e: f64,
    pub nu: f64,
    /// Thickness, as a factor of the stiffness and the loads.
    pub thickness: f64,
}

const GAUSS: f64 = 0.577_350_269_189_625_8;
const XI: [f64; 4] = [-1.0, 1.0, 1.0, -1.0];
const ETA: [f64; 4] = [-1.0, -1.0, 1.0, 1.0];

/// Plane-stress constitutive matrix for [σx, σy, τxy] from [εx, εy, γxy].
pub fn plane_d(e: f64, nu: f64) -> [[f64; 3]; 3] {
    let f = e / (1.0 - nu * nu);
    [[f, f * nu, 0.0], [f * nu, f, 0.0], [0.0, 0.0, f * (1.0 - nu) / 2.0]]
}

/// Strain rows of the triangle, and twice its area.
fn tri_b(xy: &[[f64; 2]; 3]) -> ([[f64; 6]; 3], f64) {
    let (x1, y1) = (xy[0][0], xy[0][1]);
    let (x2, y2) = (xy[1][0], xy[1][1]);
    let (x3, y3) = (xy[2][0], xy[2][1]);
    let det = (x2 - x1) * (y3 - y1) - (x3 - x1) * (y2 - y1);
    let (bx, by) = ([y2 - y3, y3 - y1, y1 - y2], [x3 - x2, x1 - x3, x2 - x1]);
    let mut b = [[0.0; 6]; 3];
    for i in 0..3 {
        b[0][2 * i] = bx[i] / det;
        b[1][2 * i + 1] = by[i] / det;
        b[2][2 * i] = by[i] / det;
        b[2][2 * i + 1] = bx[i] / det;
    }
    (b, det)
}

/// Shape functions and derivatives of the bilinear quadrilateral, shared with
/// the axisymmetric element (`axisym.rs`).
pub(crate) struct QuadPoint {
    pub(crate) n: [f64; 4],
    pub(crate) dx: [f64; 4],
    pub(crate) dy: [f64; 4],
    pub(crate) det: f64,
}

pub(crate) fn quad_point(xy: &[[f64; 2]; 4], r: f64, s: f64) -> QuadPoint {
    let (mut j00, mut j01, mut j10, mut j11) = (0.0, 0.0, 0.0, 0.0);
    for i in 0..4 {
        let (dr, ds) = (0.25 * XI[i] * (1.0 + s * ETA[i]), 0.25 * ETA[i] * (1.0 + r * XI[i]));
        j00 += dr * xy[i][0];
        j01 += dr * xy[i][1];
        j10 += ds * xy[i][0];
        j11 += ds * xy[i][1];
    }
    let det = j00 * j11 - j01 * j10;
    let mut p = QuadPoint { n: [0.0; 4], dx: [0.0; 4], dy: [0.0; 4], det };
    for i in 0..4 {
        p.n[i] = 0.25 * (1.0 + r * XI[i]) * (1.0 + s * ETA[i]);
        let (dr, ds) = (0.25 * XI[i] * (1.0 + s * ETA[i]), 0.25 * ETA[i] * (1.0 + r * XI[i]));
        p.dx[i] = (j11 * dr - j01 * ds) / det;
        p.dy[i] = (-j10 * dr + j00 * ds) / det;
    }
    p
}

fn quad_b(p: &QuadPoint) -> [[f64; 8]; 3] {
    let mut b = [[0.0; 8]; 3];
    for i in 0..4 {
        b[0][2 * i] = p.dx[i];
        b[1][2 * i + 1] = p.dy[i];
        b[2][2 * i] = p.dy[i];
        b[2][2 * i + 1] = p.dx[i];
    }
    b
}

/// K += w Bᵀ D B, for any number of nodes.
fn add_stiffness<const N: usize>(b: &[[f64; N]; 3], d: &[[f64; 3]; 3], w: f64, k: &mut [[f64; N]; N]) {
    for i in 0..N {
        let mut db = [0.0; 3];
        for q in 0..3 {
            db[q] = (0..3).map(|n| d[q][n] * b[n][i]).sum();
        }
        for j in 0..N {
            k[i][j] += w * (0..3).map(|q| b[q][j] * db[q]).sum::<f64>();
        }
    }
}

fn stress<const N: usize>(b: &[[f64; N]; 3], d: &[[f64; 3]; 3], u: &[f64; N]) -> [f64; 3] {
    let strain: [f64; 3] = std::array::from_fn(|q| (0..u.len()).map(|i| b[q][i] * u[i]).sum());
    std::array::from_fn(|q| (0..3).map(|n| d[q][n] * strain[n]).sum())
}

/// Stiffness of the constant-strain triangle, 6 DOF.
pub fn tri_stiffness(xy: &[[f64; 2]; 3], p: &PlaneProps) -> [[f64; 6]; 6] {
    let (b, det) = tri_b(xy);
    let mut k = [[0.0; 6]; 6];
    add_stiffness(&b, &plane_d(p.e, p.nu), p.thickness * 0.5 * det, &mut k);
    k
}

/// Stiffness of the bilinear quadrilateral, 8 DOF.
pub fn quad_stiffness(xy: &[[f64; 2]; 4], p: &PlaneProps) -> Mat8 {
    let mut k = [[0.0; 8]; 8];
    for &r in &[-GAUSS, GAUSS] {
        for &s in &[-GAUSS, GAUSS] {
            let pt = quad_point(xy, r, s);
            add_stiffness(&quad_b(&pt), &plane_d(p.e, p.nu), p.thickness * pt.det, &mut k);
        }
    }
    k
}

/// ∫ Nᵢ dA per node, for a consistent pressure.
pub fn tri_nodal_areas(xy: &[[f64; 2]; 3]) -> [f64; 4] {
    let a = 0.5 * tri_b(xy).1 / 3.0;
    [a, a, a, 0.0]
}

/// ∫ Nᵢ dA per node, for a consistent pressure.
pub fn quad_nodal_areas(xy: &[[f64; 2]; 4]) -> [f64; 4] {
    let mut a = [0.0; 4];
    for &r in &[-GAUSS, GAUSS] {
        for &s in &[-GAUSS, GAUSS] {
            let pt = quad_point(xy, r, s);
            for i in 0..4 {
                a[i] += pt.n[i] * pt.det;
            }
        }
    }
    a
}

/// Stress at the centroid of the triangle, from its 6 DOF.
pub fn tri_stress(xy: &[[f64; 2]; 3], p: &PlaneProps, u: &[f64; 6]) -> [f64; 3] {
    stress(&tri_b(xy).0, &plane_d(p.e, p.nu), u)
}

/// Stress at (r, s) of the quadrilateral, from its 8 DOF.
pub fn quad_stress(xy: &[[f64; 2]; 4], p: &PlaneProps, u: &[f64; 8], r: f64, s: f64) -> [f64; 3] {
    stress(&quad_b(&quad_point(xy, r, s)), &plane_d(p.e, p.nu), u)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROPS: PlaneProps = PlaneProps { e: 2.0e11, nu: 0.3, thickness: 0.1 };
    /// A triangle and a quadrilateral that are not rectangles.
    const TRI: [[f64; 2]; 3] = [[0.1, -0.2], [2.3, 0.1], [0.7, 1.9]];
    const QUAD: [[f64; 2]; 4] = [[0.1, -0.2], [2.3, 0.1], [2.8, 1.9], [-0.3, 1.4]];

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
    }

    /// Constant strain, so the stress is the same everywhere: [a, c, 2b].
    fn state(a: f64, b: f64, c: f64) -> ([f64; 6], [f64; 8]) {
        let (mut u3, mut u4) = ([0.0; 6], [0.0; 8]);
        for i in 0..3 {
            u3[2 * i] = a * TRI[i][0] + b * TRI[i][1];
            u3[2 * i + 1] = b * TRI[i][0] + c * TRI[i][1];
        }
        for i in 0..4 {
            u4[2 * i] = a * QUAD[i][0] + b * QUAD[i][1];
            u4[2 * i + 1] = b * QUAD[i][0] + c * QUAD[i][1];
        }
        (u3, u4)
    }

    #[test]
    fn triangle_patch_test_is_exact() {
        let (a, b, c) = (1e-3, 4e-4, -7e-4);
        let (u, _) = state(a, b, c);
        let d = plane_d(PROPS.e, PROPS.nu);
        let strain = [a, c, 2.0 * b];
        let want: [f64; 3] = std::array::from_fn(|q| (0..3).map(|n| d[q][n] * strain[n]).sum());
        let got = tri_stress(&TRI, &PROPS, &u);
        for q in 0..3 {
            assert!(close(got[q], want[q]), "{q}: {} vs {}", got[q], want[q]);
        }
    }

    #[test]
    fn quadrilateral_patch_test_is_exact_on_a_distorted_shape() {
        let (a, b, c) = (1e-3, 4e-4, -7e-4);
        let (_, u) = state(a, b, c);
        let d = plane_d(PROPS.e, PROPS.nu);
        let strain = [a, c, 2.0 * b];
        let want: [f64; 3] = std::array::from_fn(|q| (0..3).map(|n| d[q][n] * strain[n]).sum());
        for (r, s) in [(0.0, 0.0), (0.4, -0.3), (-0.8, 0.6)] {
            let got = quad_stress(&QUAD, &PROPS, &u, r, s);
            for q in 0..3 {
                assert!(close(got[q], want[q]), "{r},{s} {q}: {} vs {}", got[q], want[q]);
            }
        }
    }

    fn forces(k: &[[f64; 8]; 8], u: &[f64; 8]) -> [f64; 8] {
        std::array::from_fn(|i| (0..8).map(|j| k[i][j] * u[j]).sum())
    }

    fn rigid_modes(xy: &[[f64; 2]; 4]) -> [f64; 8] {
        // Rotation about the origin: u = -ω y, v = ω x.
        let mut r = [0.0; 8];
        for i in 0..4 {
            r[2 * i] = -xy[i][1];
            r[2 * i + 1] = xy[i][0];
        }
        r
    }

    #[test]
    fn quadrilateral_has_three_free_rigid_modes() {
        let k = quad_stiffness(&QUAD, &PROPS);
        let scale = k.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
        for i in 0..8 {
            for j in 0..8 {
                assert!(close(k[i][j], k[j][i]), "{i},{j}");
            }
        }
        for axis in 0..2 {
            let mut u = [0.0; 8];
            for i in 0..4 {
                u[2 * i + axis] = 1.0;
            }
            for f in forces(&k, &u) {
                assert!(f.abs() <= 1e-9 * scale, "{axis}: {f} (scale {scale})");
            }
        }
        let r = rigid_modes(&QUAD);
        for f in forces(&k, &r) {
            assert!(f.abs() <= 1e-9 * scale, "rotation: {f}");
        }
    }

    #[test]
    fn triangle_has_three_free_rigid_modes() {
        let k6 = tri_stiffness(&TRI, &PROPS);
        let mut k = [[0.0; 8]; 8];
        for i in 0..6 {
            for j in 0..6 {
                k[i][j] = k6[i][j];
            }
        }
        let scale = k.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
        for axis in 0..2 {
            let mut u = [0.0; 8];
            for i in 0..3 {
                u[2 * i + axis] = 1.0;
            }
            for f in forces(&k, &u) {
                assert!(f.abs() <= 1e-9 * scale, "{axis}: {f} (scale {scale})");
            }
        }
        let mut r = [0.0; 8];
        for i in 0..3 {
            r[2 * i] = -TRI[i][1];
            r[2 * i + 1] = TRI[i][0];
        }
        for f in forces(&k, &r) {
            assert!(f.abs() <= 1e-9 * scale, "rotation: {f}");
        }
    }

    #[test]
    fn nodal_areas_add_up_to_the_area() {
        let tri: f64 = tri_nodal_areas(&TRI).iter().sum();
        let det = tri_b(&TRI).1;
        assert!(close(tri, 0.5 * det), "{tri} vs {}", 0.5 * det);
        let quad: f64 = quad_nodal_areas(&QUAD).iter().sum();
        // Shoelace.
        let mut twice = 0.0;
        for i in 0..4 {
            let j = (i + 1) % 4;
            twice += QUAD[i][0] * QUAD[j][1] - QUAD[j][0] * QUAD[i][1];
        }
        assert!(close(quad, 0.5 * twice), "{quad} vs {}", 0.5 * twice);
    }

    #[test]
    fn a_pressure_on_a_triangle_matches_its_area() {
        let shares = tri_nodal_areas(&TRI);
        let total: f64 = shares.iter().sum();
        assert!(close(total * 1.0, 0.5 * tri_b(&TRI).1));
    }

    #[test]
    fn clockwise_nodes_have_negative_area() {
        // Clockwise nodes: twice the signed area is negative, so the element is
        // degenerate. The caller (validation) has to catch it; the kernel does
        // not hide it.
        let cw = [TRI[0], TRI[2], TRI[1]];
        assert!(tri_b(&cw).1 < 0.0);
    }
}
