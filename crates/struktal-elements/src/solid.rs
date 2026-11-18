//! Solid element: eight-node hexahedron with three degrees of freedom per
//! node (`u`, `v`, `w`), an isotropic 3D constitutive matrix and 2 × 2 × 2
//! Gauss quadrature.
//!
//! The shape functions are trilinear, `N = (1 + ξ ξᵢ)(1 + η ηᵢ)(1 + τ τᵢ) / 8`,
//! and the eight Gauss points are interior, so the Jacobian of a valid element
//! is positive there and never lands on a node.
//!
//! The nodes are 0..3 on the face `τ = -1` counterclockwise seen from +z, and
//! 4..7 above them on `τ = +1` in the same order, which is the order that gives
//! a positive Jacobian. A mirrored ordering has a negative one; the kernel
//! reports it (as the areas do) and the validation is the one that has to
//! refuse it.
//!
//! The strains are `[εx, εy, εz, γxy, γyz, γzx]`, with the engineering shear
//! strains, and the stresses `[σx, σy, σz, τxy, τyz, τzx]`.

#![allow(clippy::needless_range_loop)] // small dense matrices: index form mirrors the formulas

/// Stiffness matrix of the hexahedron: 24 DOF, 3 per node.
pub type Mat24 = [[f64; 24]; 24];

/// Isotropic properties of a solid: no thickness, the element has a volume.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolidProps {
    pub e: f64,
    pub nu: f64,
}

const GAUSS: f64 = 0.577_350_269_189_625_8;
const XI: [f64; 8] = [-1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0];
const ETA: [f64; 8] = [-1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0];
const TAU: [f64; 8] = [-1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0];

/// Isotropic 3D constitutive matrix for `[σx, σy, σz, τxy, τyz, τzx]` from
/// `[εx, εy, εz, γxy, γyz, γzx]` (engineering shears).
pub fn solid_d(e: f64, nu: f64) -> [[f64; 6]; 6] {
    let c = e / ((1.0 + nu) * (1.0 - 2.0 * nu));
    let s = c * (1.0 - nu);
    let t = c * nu;
    let g = c * (1.0 - 2.0 * nu) / 2.0;
    [
        [s, t, t, 0.0, 0.0, 0.0],
        [t, s, t, 0.0, 0.0, 0.0],
        [t, t, s, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, g, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0, g, 0.0],
        [0.0, 0.0, 0.0, 0.0, 0.0, g],
    ]
}

/// Cofactor of the Jacobian at `(row, column)`.
fn cofactor(j: &[[f64; 3]; 3], row: usize, col: usize) -> f64 {
    let rows: [usize; 2] = match row {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    };
    let cols: [usize; 2] = match col {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    };
    let minor = j[rows[0]][cols[0]] * j[rows[1]][cols[1]] - j[rows[0]][cols[1]] * j[rows[1]][cols[0]];
    if (row + col).is_multiple_of(2) { minor } else { -minor }
}

/// Shape functions, their derivatives in the global axes and the Jacobian
/// determinant of the hexahedron at `(r, s, t)`.
pub(crate) struct HexPoint {
    pub(crate) n: [f64; 8],
    /// Derivatives `[∂N/∂x, ∂N/∂y, ∂N/∂z]` of each shape function.
    pub(crate) d: [[f64; 3]; 8],
    pub(crate) det: f64,
}

pub(crate) fn hex_point(xyz: &[[f64; 3]; 8], r: f64, s: f64, t: f64) -> HexPoint {
    // Jacobian J[a][b] = ∂x_b/∂ξ_a, with ξ = (r, s, t).
    let mut j = [[0.0; 3]; 3];
    let mut nat = [[0.0; 3]; 8];
    let mut n = [0.0; 8];
    for i in 0..8 {
        n[i] = 0.125 * (1.0 + r * XI[i]) * (1.0 + s * ETA[i]) * (1.0 + t * TAU[i]);
        nat[i] = [
            0.125 * XI[i] * (1.0 + s * ETA[i]) * (1.0 + t * TAU[i]),
            0.125 * ETA[i] * (1.0 + r * XI[i]) * (1.0 + t * TAU[i]),
            0.125 * TAU[i] * (1.0 + r * XI[i]) * (1.0 + s * ETA[i]),
        ];
        for a in 0..3 {
            for b in 0..3 {
                j[a][b] += nat[i][a] * xyz[i][b];
            }
        }
    }
    let det = (0..3).map(|b| j[0][b] * cofactor(&j, 0, b)).sum();
    // ∂N/∂x_c = Σ_a ∂N/∂ξ_a (J⁻¹)_{c a}, and (J⁻¹)_{c a} = cofactor(a, c) / det.
    let mut d = [[0.0; 3]; 8];
    for i in 0..8 {
        for c in 0..3 {
            d[i][c] = (0..3).map(|a| nat[i][a] * cofactor(&j, a, c)).sum::<f64>() / det;
        }
    }
    HexPoint { n, d, det }
}

/// Strain rows `[εx, εy, εz, γxy, γyz, γzx]` of the hexahedron at `(r, s, t)`.
fn hex_b(p: &HexPoint) -> [[f64; 24]; 6] {
    let mut b = [[0.0; 24]; 6];
    for i in 0..8 {
        let (dx, dy, dz) = (p.d[i][0], p.d[i][1], p.d[i][2]);
        b[0][3 * i] = dx;
        b[1][3 * i + 1] = dy;
        b[2][3 * i + 2] = dz;
        b[3][3 * i] = dy;
        b[3][3 * i + 1] = dx;
        b[4][3 * i + 1] = dz;
        b[4][3 * i + 2] = dy;
        b[5][3 * i] = dz;
        b[5][3 * i + 2] = dx;
    }
    b
}

/// K += w Bᵀ D B, for the six strain rows.
fn add_stiffness(b: &[[f64; 24]; 6], d: &[[f64; 6]; 6], w: f64, k: &mut Mat24) {
    // D B, kept whole so the index form below is the formula itself.
    let db: [[f64; 24]; 6] = std::array::from_fn(|q| std::array::from_fn(|c| (0..6).map(|n| d[q][n] * b[n][c]).sum()));
    for a in 0..24 {
        for c in 0..24 {
            k[a][c] += w * (0..6).map(|q| b[q][a] * db[q][c]).sum::<f64>();
        }
    }
}

/// Stiffness of the eight-node hexahedron, 24 DOF, with 2 × 2 × 2 Gauss.
pub fn hex_stiffness(xyz: &[[f64; 3]; 8], p: &SolidProps) -> Mat24 {
    let d = solid_d(p.e, p.nu);
    let mut k = [[0.0; 24]; 24];
    for &r in &[-GAUSS, GAUSS] {
        for &s in &[-GAUSS, GAUSS] {
            for &t in &[-GAUSS, GAUSS] {
                let pt = hex_point(xyz, r, s, t);
                add_stiffness(&hex_b(&pt), &d, pt.det, &mut k);
            }
        }
    }
    k
}

/// Stresses `[σx, σy, σz, τxy, τyz, τzx]` at `(r, s, t)` of the hexahedron,
/// from its 24 DOF.
pub fn hex_stress(xyz: &[[f64; 3]; 8], p: &SolidProps, u: &[f64; 24], r: f64, s: f64, t: f64) -> [f64; 6] {
    let pt = hex_point(xyz, r, s, t);
    let b = hex_b(&pt);
    let strain: [f64; 6] = std::array::from_fn(|q| (0..24).map(|i| b[q][i] * u[i]).sum());
    let d = solid_d(p.e, p.nu);
    std::array::from_fn(|q| (0..6).map(|n| d[q][n] * strain[n]).sum())
}

/// ∫ Nᵢ dV per node: the share of the volume of each node, for consistent
/// body forces (its own weight, for instance). The quadrature is exact: N is
/// trilinear and the Jacobian determinant has degree 2 in each variable.
pub fn hex_nodal_volumes(xyz: &[[f64; 3]; 8]) -> [f64; 8] {
    let mut v = [0.0; 8];
    for &r in &[-GAUSS, GAUSS] {
        for &s in &[-GAUSS, GAUSS] {
            for &t in &[-GAUSS, GAUSS] {
                let pt = hex_point(xyz, r, s, t);
                for i in 0..8 {
                    v[i] += pt.n[i] * pt.det;
                }
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROPS: SolidProps = SolidProps { e: 2.0e11, nu: 0.3 };
    /// A hexahedron that is not a box: every face is a trapezoid and the
    /// corners are out of alignment.
    const HEX: [[f64; 3]; 8] = [
        [0.0, 0.0, 0.0],
        [2.0, 0.1, -0.1],
        [2.2, 1.5, 0.2],
        [0.1, 1.7, 0.0],
        [-0.1, 0.1, 1.8],
        [2.1, -0.1, 2.0],
        [2.4, 1.6, 2.1],
        [0.2, 1.8, 1.9],
    ];

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
    }

    /// The strains are of order `E · strain`, so a component that is zero is
    /// compared against that scale: round-off in the derivatives is not zero.
    fn close_to_scale(got: f64, want: f64, state: [f64; 6]) -> bool {
        let scale = state.iter().fold(0.0_f64, |m, v| m.max(v.abs())).max(1.0);
        (got - want).abs() <= 1e-9 * scale
    }

    fn forces(k: &Mat24, u: &[f64; 24]) -> [f64; 24] {
        std::array::from_fn(|i| (0..24).map(|j| k[i][j] * u[j]).sum())
    }

    fn energy(k: &Mat24, u: &[f64; 24]) -> f64 {
        (0..24).map(|i| u[i] * forces(k, u)[i]).sum()
    }

    /// A linear displacement field in each direction: constant strain
    /// everywhere, which is what a patch test asks of the element.
    ///
    /// The nine coefficients are `[u; v; w] × [x, y, z]`.
    fn linear(xyz: &[[f64; 3]; 8], c: [f64; 9]) -> [f64; 24] {
        let mut u = [0.0; 24];
        for (i, p) in xyz.iter().enumerate() {
            for (dir, row) in [(0usize, 0usize), (1, 3), (2, 6)] {
                u[3 * i + dir] = c[row] * p[0] + c[row + 1] * p[1] + c[row + 2] * p[2];
            }
        }
        u
    }

    /// The strain `[εx, εy, εz, γxy, γyz, γzx]` of that same field.
    fn linear_strain(c: [f64; 9]) -> [f64; 6] {
        [c[0], c[4], c[8], c[1] + c[3], c[5] + c[7], c[2] + c[6]]
    }

    #[test]
    fn a_linear_field_is_a_patch_test() {
        // A distorted hexahedron and a full 3D state, shears included.
        let c = [1e-3, -4e-4, 7e-4, 2e-4, 5e-4, -3e-4, 6e-4, -8e-4, 1e-4];
        let u = linear(&HEX, c);
        let want: [f64; 6] = {
            let d = solid_d(PROPS.e, PROPS.nu);
            let strain = linear_strain(c);
            std::array::from_fn(|q| (0..6).map(|n| d[q][n] * strain[n]).sum())
        };
        for (r, s, t) in [(0.0, 0.0, 0.0), (0.4, -0.3, 0.5), (-0.8, 0.6, -0.7), (0.9, 0.9, 0.9)] {
            let got = hex_stress(&HEX, &PROPS, &u, r, s, t);
            for q in 0..6 {
                assert!(close_to_scale(got[q], want[q], want), "{r},{s},{t} {q}: {} vs {}", got[q], want[q]);
            }
        }
    }

    #[test]
    fn the_stiffness_is_symmetric_and_positive_on_a_strain() {
        let k = hex_stiffness(&HEX, &PROPS);
        for i in 0..24 {
            for j in 0..24 {
                assert!(close(k[i][j], k[j][i]), "{i},{j}");
            }
        }
        // An extension in x stores energy.
        let c = [1e-3, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let e = energy(&k, &linear(&HEX, c));
        assert!(e > 0.0, "{e}");
        // A pure shear stores energy too.
        let c = [0.0, 1e-3, 0.0, 1e-3, 0.0, 0.0, 0.0, 0.0, 0.0];
        assert!(energy(&k, &linear(&HEX, c)) > 0.0);
    }

    /// A rigid motion has no strain, so it produces no force: three
    /// translations and three rotations about the axes through the origin.
    fn rigid(xyz: &[[f64; 3]; 8], mode: usize) -> [f64; 24] {
        let mut u = [0.0; 24];
        for (i, p) in xyz.iter().enumerate() {
            let (px, py, pz) = (p[0], p[1], p[2]);
            let d = match mode {
                0 => [1.0, 0.0, 0.0],
                1 => [0.0, 1.0, 0.0],
                2 => [0.0, 0.0, 1.0],
                // ω × p for ω = (1, 0, 0), (0, 1, 0), (0, 0, 1).
                3 => [0.0, -pz, py],
                4 => [pz, 0.0, -px],
                _ => [-py, px, 0.0],
            };
            for axis in 0..3 {
                u[3 * i + axis] = d[axis];
            }
        }
        u
    }

    #[test]
    fn six_rigid_body_modes_produce_no_force() {
        let k = hex_stiffness(&HEX, &PROPS);
        let scale = k.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
        for mode in 0..6 {
            let u = rigid(&HEX, mode);
            for f in forces(&k, &u) {
                assert!(f.abs() <= 1e-9 * scale, "mode {mode}: {f} (scale {scale})");
            }
        }
    }

    #[test]
    fn nodal_volumes_add_up_to_the_volume() {
        // A box 2 x 3 x 4.
        let mut box_ = [[0.0; 3]; 8];
        for (i, p) in box_.iter_mut().enumerate() {
            *p = [
                if i % 4 == 1 || i % 4 == 2 { 2.0 } else { 0.0 },
                if i % 4 >= 2 { 3.0 } else { 0.0 },
                if i >= 4 { 4.0 } else { 0.0 },
            ];
        }
        let total: f64 = hex_nodal_volumes(&box_).iter().sum();
        assert!(close(total, 24.0), "{total} vs 24");
        assert!(hex_nodal_volumes(&box_).iter().all(|&v| v > 0.0));
        // The distorted element: positive shares and a finite volume.
        let shares = hex_nodal_volumes(&HEX);
        assert!(shares.iter().all(|v| v.is_finite() && *v > 0.0), "{shares:?}");
    }

    #[test]
    fn an_affine_element_has_the_volume_of_its_parallelepiped() {
        // Edges (2, 0, 0), (0, 3, 0) and (0, 0, 4) sheared: |det| of the edges.
        let mut p = [[0.0; 3]; 8];
        for (i, node) in p.iter_mut().enumerate() {
            let (r, s, t) = (
                if i % 4 == 1 || i % 4 == 2 { 1.0 } else { 0.0 },
                if i % 4 >= 2 { 1.0 } else { 0.0 },
                if i >= 4 { 1.0 } else { 0.0 },
            );
            *node = [2.0 * r + t, 3.0 * s + t, 4.0 * t];
        }
        let total: f64 = hex_nodal_volumes(&p).iter().sum();
        assert!(close(total, 24.0), "{total} vs 24");
    }

    #[test]
    fn nodes_in_the_wrong_order_have_a_negative_jacobian() {
        // The bottom face clockwise instead of counterclockwise: the element is
        // inside out. The kernel does not hide it; validation has to catch it.
        let mut flipped = HEX;
        flipped.swap(1, 3);
        flipped.swap(5, 7);
        let k = hex_stiffness(&flipped, &PROPS);
        assert!(k.iter().flatten().all(|v| v.is_finite()));
        let shares: f64 = hex_nodal_volumes(&flipped).iter().sum();
        assert!(shares < 0.0, "{shares}");
    }
}
