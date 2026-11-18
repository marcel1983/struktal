//! Axisymmetric element: the meridional section revolved about the global z
//! axis, four nodes with two degrees of freedom each (`u` radial, `w` axial).
//!
//! The strains are `εr = ∂u/∂r`, `εz = ∂w/∂z`, `γrz = ∂u/∂z + ∂w/∂r` and the
//! hoop strain `εθ = u/r`, which is the one that makes it axisymmetric. Every
//! integral carries the weight `2π r` of the revolution, so the element has no
//! thickness: the section is the whole revolution.
//!
//! The nodes are `(r, z)` with `r >= 0`, counterclockwise seen from +z, as in
//! the areas. The Gauss points are interior, so `r > 0` there and `εθ = u/r` is
//! finite; a section that reaches the axis has its first column of nodes at
//! `r = 0`, which is fine.

#![allow(clippy::needless_range_loop)] // small dense matrices: index form mirrors the formulas

use std::f64::consts::PI;

use crate::plane::{Mat8, quad_point};

/// Isotropic properties of an axisymmetric element (no thickness: the
/// revolution supplies the `2π r` of every integral).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisymProps {
    pub e: f64,
    pub nu: f64,
}

const GAUSS: f64 = 0.577_350_269_189_625_8;

/// Axisymmetric constitutive matrix for `[σr, σz, τrz, σθ]` from
/// `[εr, εz, γrz, εθ]`.
pub fn axisym_d(e: f64, nu: f64) -> [[f64; 4]; 4] {
    let c = e / ((1.0 + nu) * (1.0 - 2.0 * nu));
    let s = c * (1.0 - nu);
    let t = c * nu;
    let g = c * (1.0 - 2.0 * nu) / 2.0;
    [[s, t, 0.0, t], [t, s, 0.0, t], [0.0, 0.0, g, 0.0], [t, t, 0.0, s]]
}

/// Strain rows `[εr, εz, γrz, εθ]` of the quadrilateral at `(r, s)`, with the
/// hoop row carrying `N / ρ` for the radius `ρ` of the point.
fn quad_b(xy: &[[f64; 2]; 4], r: f64, s: f64) -> ([[f64; 8]; 4], f64) {
    let pt = quad_point(xy, r, s);
    let radius: f64 = (0..4).map(|i| pt.n[i] * xy[i][0]).sum();
    let mut b = [[0.0; 8]; 4];
    for i in 0..4 {
        b[0][2 * i] = pt.dx[i];
        b[1][2 * i + 1] = pt.dy[i];
        b[2][2 * i] = pt.dy[i];
        b[2][2 * i + 1] = pt.dx[i];
        b[3][2 * i] = pt.n[i] / radius;
    }
    (b, radius * pt.det)
}

/// K += w Bᵀ D B, for the four strain rows.
fn add_stiffness(b: &[[f64; 8]; 4], d: &[[f64; 4]; 4], w: f64, k: &mut Mat8) {
    // D B, kept whole so the index form below is the formula itself.
    let db: [[f64; 8]; 4] = std::array::from_fn(|q| std::array::from_fn(|c| (0..4).map(|n| d[q][n] * b[n][c]).sum()));
    for a in 0..8 {
        for c in 0..8 {
            k[a][c] += w * (0..4).map(|q| b[q][a] * db[q][c]).sum::<f64>();
        }
    }
}

/// Stiffness of the four-node axisymmetric element, 8 DOF, with the `2π r`
/// weight of the revolution.
pub fn quad_stiffness(xy: &[[f64; 2]; 4], p: &AxisymProps) -> Mat8 {
    let d = axisym_d(p.e, p.nu);
    let mut k = [[0.0; 8]; 8];
    for &r in &[-GAUSS, GAUSS] {
        for &s in &[-GAUSS, GAUSS] {
            let (b, det) = quad_b(xy, r, s);
            add_stiffness(&b, &d, 2.0 * PI * det, &mut k);
        }
    }
    k
}

/// Stresses `[σr, σz, τrz, σθ]` at `(r, s)` of the quadrilateral, from its
/// 8 DOF.
pub fn quad_stress(xy: &[[f64; 2]; 4], p: &AxisymProps, u: &[f64; 8], r: f64, s: f64) -> [f64; 4] {
    let (b, _) = quad_b(xy, r, s);
    let strain: [f64; 4] = std::array::from_fn(|q| (0..8).map(|i| b[q][i] * u[i]).sum());
    let d = axisym_d(p.e, p.nu);
    std::array::from_fn(|q| (0..4).map(|n| d[q][n] * strain[n]).sum())
}

/// ∫ Nᵢ 2π r dA per node: the share of the volume of revolution of each node,
/// for consistent loads.
pub fn quad_nodal_volumes(xy: &[[f64; 2]; 4]) -> [f64; 4] {
    let mut v = [0.0; 4];
    for &r in &[-GAUSS, GAUSS] {
        for &s in &[-GAUSS, GAUSS] {
            let pt = quad_point(xy, r, s);
            let radius: f64 = (0..4).map(|i| pt.n[i] * xy[i][0]).sum();
            for i in 0..4 {
                v[i] += 2.0 * PI * pt.n[i] * radius * pt.det;
            }
        }
    }
    v
}

/// ∫ Nᵢ 2π r² dA per node: the radius-weighted share of each node for the
/// centrifugal body force `ρ ω² r`, which grows with the radius and so is not
/// a uniform load.
pub fn quad_nodal_centrifugal(xy: &[[f64; 2]; 4]) -> [f64; 4] {
    let mut v = [0.0; 4];
    for &r in &[-GAUSS, GAUSS] {
        for &s in &[-GAUSS, GAUSS] {
            let pt = quad_point(xy, r, s);
            let radius: f64 = (0..4).map(|i| pt.n[i] * xy[i][0]).sum();
            for i in 0..4 {
                v[i] += 2.0 * PI * pt.n[i] * radius * radius * pt.det;
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROPS: AxisymProps = AxisymProps { e: 2.0e11, nu: 0.3 };
    /// A quadrilateral that is not a rectangle, away from the axis.
    const QUAD: [[f64; 2]; 4] = [[1.1, -0.2], [2.3, 0.1], [2.8, 1.9], [1.6, 1.4]];

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
    }

    /// The stresses are of order `E · strain`, so a component that is zero is
    /// compared against that scale: round-off in the derivatives is not zero.
    fn close_to_scale(got: f64, want: f64, state: [f64; 4]) -> bool {
        let scale = state.iter().fold(0.0_f64, |m, v| m.max(v.abs())).max(1.0);
        (got - want).abs() <= 1e-9 * scale
    }

    fn forces(k: &Mat8, u: &[f64; 8]) -> [f64; 8] {
        std::array::from_fn(|i| (0..8).map(|j| k[i][j] * u[j]).sum())
    }

    fn energy(k: &Mat8, u: &[f64; 8]) -> f64 {
        (0..8).map(|i| u[i] * forces(k, u)[i]).sum()
    }

    /// The displacement field `u = c r`, `w = 0`: a constant hoop strain `c`,
    /// with `εr = c` and the other two zero, at every point of the element.
    fn radial(c: f64) -> [f64; 8] {
        let mut u = [0.0; 8];
        for (i, p) in QUAD.iter().enumerate() {
            u[2 * i] = c * p[0];
        }
        u
    }

    #[test]
    fn a_radial_extension_is_a_patch_test() {
        let c = 1e-3;
        let u = radial(c);
        let want: [f64; 4] = {
            let d = axisym_d(PROPS.e, PROPS.nu);
            let strain = [c, 0.0, 0.0, c];
            std::array::from_fn(|q| (0..4).map(|n| d[q][n] * strain[n]).sum())
        };
        for (r, s) in [(0.0, 0.0), (0.4, -0.3), (-0.8, 0.6), (0.9, 0.9)] {
            let got = quad_stress(&QUAD, &PROPS, &u, r, s);
            for q in 0..4 {
                assert!(close_to_scale(got[q], want[q], want), "{r},{s} {q}: {} vs {}", got[q], want[q]);
            }
        }
    }

    #[test]
    fn an_axial_extension_strains_only_z() {
        // w = a z: εz = a, and the hoop strain stays zero because u = 0.
        let a = 2e-3;
        let mut u = [0.0; 8];
        for (i, p) in QUAD.iter().enumerate() {
            u[2 * i + 1] = a * p[1];
        }
        let want: [f64; 4] = {
            let d = axisym_d(PROPS.e, PROPS.nu);
            let strain = [0.0, a, 0.0, 0.0];
            std::array::from_fn(|q| (0..4).map(|n| d[q][n] * strain[n]).sum())
        };
        for (r, s) in [(0.0, 0.0), (-0.5, 0.7)] {
            let got = quad_stress(&QUAD, &PROPS, &u, r, s);
            for q in 0..4 {
                assert!(close_to_scale(got[q], want[q], want), "{r},{s} {q}: {} vs {}", got[q], want[q]);
            }
        }
    }

    #[test]
    fn the_stiffness_is_symmetric_and_positive_on_a_strain() {
        let k = quad_stiffness(&QUAD, &PROPS);
        for i in 0..8 {
            for j in 0..8 {
                assert!(close(k[i][j], k[j][i]), "{i},{j}");
            }
        }
        // A radial extension stores energy: εθ = c is not zero.
        let e = energy(&k, &radial(1e-3));
        assert!(e > 0.0, "{e}");
        // An axial extension stores energy too.
        let mut w = [0.0; 8];
        for (i, p) in QUAD.iter().enumerate() {
            w[2 * i + 1] = 1e-3 * p[1];
        }
        assert!(energy(&k, &w) > 0.0);
    }

    #[test]
    fn a_translation_along_the_axis_is_the_only_rigid_mode() {
        let k = quad_stiffness(&QUAD, &PROPS);
        let scale = k.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
        // Axial translation: no strain at all.
        let mut u = [0.0; 8];
        for i in 0..4 {
            u[2 * i + 1] = 1.0;
        }
        for f in forces(&k, &u) {
            assert!(f.abs() <= 1e-9 * scale, "axial: {f} (scale {scale})");
        }
        // A radial translation is not rigid: it stretches the hoop, so it must
        // produce a force.
        let mut v = [0.0; 8];
        for i in 0..4 {
            v[2 * i] = 1.0;
        }
        assert!(forces(&k, &v).iter().any(|&f| f.abs() > 1e-6 * scale), "radial is not rigid");
    }

    #[test]
    fn nodal_volumes_add_up_to_the_volume_of_revolution() {
        // r from 1 to 2, z from 0 to 3: π (2² - 1²) · 3.
        let rect = [[1.0, 0.0], [2.0, 0.0], [2.0, 3.0], [1.0, 3.0]];
        let total: f64 = quad_nodal_volumes(&rect).iter().sum();
        assert!(close(total, 9.0 * PI), "{total} vs {}", 9.0 * PI);
        // Every node carries a positive share, and the distorted element too.
        assert!(quad_nodal_volumes(&rect).iter().all(|&v| v > 0.0));
        let distorted: f64 = quad_nodal_volumes(&QUAD).iter().sum();
        assert!(distorted > 0.0, "{distorted}");
    }

    #[test]
    fn centrifugal_shares_add_up_to_the_radius_weighted_volume() {
        // ∫ 2π r² dA over r in [1, 2] and z in [0, 3]: 2π · (2³ - 1³) / 3 · 3.
        let rect = [[1.0, 0.0], [2.0, 0.0], [2.0, 3.0], [1.0, 3.0]];
        let want = 14.0 * PI;
        let total: f64 = quad_nodal_centrifugal(&rect).iter().sum();
        assert!(close(total, want), "{total} vs {want}");
        assert!(quad_nodal_centrifugal(&rect).iter().all(|&v| v > 0.0));
        // The same integral on a distorted element stays finite and positive.
        let distorted: f64 = quad_nodal_centrifugal(&QUAD).iter().sum();
        assert!(distorted > 0.0 && distorted.is_finite(), "{distorted}");
    }

    #[test]
    fn a_section_that_reaches_the_axis_has_finite_stiffness() {
        // First column of nodes on r = 0: the Gauss points are interior, so
        // the hoop strain stays finite there.
        let on_axis = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let k = quad_stiffness(&on_axis, &PROPS);
        assert!(k.iter().flatten().all(|v| v.is_finite()));
        let u = radial(1e-3);
        let s = quad_stress(&on_axis, &PROPS, &u, 0.0, 0.0);
        assert!(s.iter().all(|v| v.is_finite()), "{s:?}");
    }
}
