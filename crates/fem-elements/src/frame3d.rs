//! Two-node 3D Euler–Bernoulli frame element (no shear deformation, no
//! offsets, no releases, no geometric stiffness). See
//! docs/decisions/0003-frame-euler-bernoulli.md.
//!
//! Local DOF order per end: [u1, u2, u3, r1, r2, r3], end I then end J.
//! Rotations are right-handed about local axes, so r3 = dv/dx (v = u2) and
//! r2 = −dw/dx (w = u3).

#![allow(clippy::needless_range_loop)] // small dense matrices: index form mirrors the formulas

pub type Vec3 = [f64; 3];
pub type Mat12 = [[f64; 12]; 12];

/// sin of the angle between the member and the global Z axis below which the
/// member is treated as vertical for the default local-axis rule.
/// SAP2000's exact threshold is pending confirmation (oracle_pending).
pub const VERTICAL_SIN_TOL: f64 = 1e-3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionProps {
    pub e: f64,
    pub g: f64,
    pub area: f64,
    pub i33: f64,
    pub i22: f64,
    pub j: f64,
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn scale(a: Vec3, s: f64) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn norm(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}

pub fn length(xi: Vec3, xj: Vec3) -> f64 {
    norm(sub(xj, xi))
}

/// Rows are local axes 1, 2, 3 expressed in global coordinates.
///
/// Default rule (SAP2000 documentation, to be confirmed against the build):
/// axis 1 runs from I to J; for non-vertical members the 1-2 plane contains
/// global Z and axis 2 points upward; for vertical members axis 2 is global +X.
/// Axis 3 = 1 × 2. `angle_deg` then rotates axes 2 and 3 about axis 1.
pub fn local_axes(xi: Vec3, xj: Vec3, angle_deg: f64) -> [Vec3; 3] {
    let d = sub(xj, xi);
    let l = norm(d);
    let e1 = scale(d, 1.0 / l);
    let horiz = (e1[0] * e1[0] + e1[1] * e1[1]).sqrt();
    let e2 = if horiz < VERTICAL_SIN_TOL {
        [1.0, 0.0, 0.0]
    } else {
        let z = [0.0, 0.0, 1.0];
        let v = sub(z, scale(e1, dot(z, e1)));
        scale(v, 1.0 / norm(v))
    };
    // Re-orthogonalize in the vertical case (nearly vertical members).
    let e2 = {
        let v = sub(e2, scale(e1, dot(e2, e1)));
        scale(v, 1.0 / norm(v))
    };
    let e3 = cross(e1, e2);
    let (s, c) = angle_deg.to_radians().sin_cos();
    let r2 = [c * e2[0] + s * e3[0], c * e2[1] + s * e3[1], c * e2[2] + s * e3[2]];
    let r3 = [-s * e2[0] + c * e3[0], -s * e2[1] + c * e3[1], -s * e2[2] + c * e3[2]];
    [e1, r2, r3]
}

pub fn local_stiffness(l: f64, p: &SectionProps) -> Mat12 {
    let mut k = [[0.0; 12]; 12];
    let ea = p.e * p.area / l;
    let gj = p.g * p.j / l;
    k[0][0] = ea;
    k[6][6] = ea;
    k[0][6] = -ea;
    k[6][0] = -ea;
    k[3][3] = gj;
    k[9][9] = gj;
    k[3][9] = -gj;
    k[9][3] = -gj;
    let place = |k: &mut Mat12, idx: [usize; 4], ei: f64, sgn: f64| {
        let (l2, l3) = (l * l, l * l * l);
        let a = 12.0 * ei / l3;
        let b = sgn * 6.0 * ei / l2;
        let c4 = 4.0 * ei / l;
        let c2 = 2.0 * ei / l;
        let m = [[a, b, -a, b], [b, c4, -b, c2], [-a, -b, a, -b], [b, c2, -b, c4]];
        for r in 0..4 {
            for c in 0..4 {
                k[idx[r]][idx[c]] = m[r][c];
            }
        }
    };
    // Bending in 1-2 plane about axis 3: (u2, r3), r3 = +dv/dx.
    place(&mut k, [1, 5, 7, 11], p.e * p.i33, 1.0);
    // Bending in 1-3 plane about axis 2: (u3, r2), r2 = −dw/dx.
    place(&mut k, [2, 4, 8, 10], p.e * p.i22, -1.0);
    k
}

/// 12×12 block-diagonal transformation T with u_local = T u_global.
pub fn transformation(axes: &[Vec3; 3]) -> Mat12 {
    let mut t = [[0.0; 12]; 12];
    for b in 0..4 {
        for r in 0..3 {
            for c in 0..3 {
                t[3 * b + r][3 * b + c] = axes[r][c];
            }
        }
    }
    t
}

/// Tᵀ K T
pub fn to_global(k_local: &Mat12, t: &Mat12) -> Mat12 {
    let mut kt = [[0.0; 12]; 12];
    for i in 0..12 {
        for j in 0..12 {
            let mut s = 0.0;
            for m in 0..12 {
                s += k_local[i][m] * t[m][j];
            }
            kt[i][j] = s;
        }
    }
    let mut out = [[0.0; 12]; 12];
    for i in 0..12 {
        for j in 0..12 {
            let mut s = 0.0;
            for m in 0..12 {
                s += t[m][i] * kt[m][j];
            }
            out[i][j] = s;
        }
    }
    out
}

pub fn mat_vec(m: &Mat12, v: &[f64; 12]) -> [f64; 12] {
    let mut out = [0.0; 12];
    for i in 0..12 {
        out[i] = (0..12).map(|j| m[i][j] * v[j]).sum();
    }
    out
}

pub fn mat_t_vec(m: &Mat12, v: &[f64; 12]) -> [f64; 12] {
    let mut out = [0.0; 12];
    for i in 0..12 {
        out[i] = (0..12).map(|j| m[j][i] * v[j]).sum();
    }
    out
}

/// Span loads in local components: uniform force per length over the full
/// member and concentrated forces at absolute distance `a` from end I.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalSpanLoads {
    pub uniform: Vec3,
    pub points: Vec<(f64, Vec3)>,
}

/// Consistent equivalent nodal loads f = ∫ Nᵀ q dx in local coordinates.
pub fn equivalent_nodal_loads(l: f64, loads: &LocalSpanLoads) -> [f64; 12] {
    let mut f = [0.0; 12];
    let [q1, q2, q3] = loads.uniform;
    let l2 = l * l;
    f[0] += q1 * l / 2.0;
    f[6] += q1 * l / 2.0;
    f[1] += q2 * l / 2.0;
    f[5] += q2 * l2 / 12.0;
    f[7] += q2 * l / 2.0;
    f[11] -= q2 * l2 / 12.0;
    f[2] += q3 * l / 2.0;
    f[4] -= q3 * l2 / 12.0;
    f[8] += q3 * l / 2.0;
    f[10] += q3 * l2 / 12.0;
    for &(a, [p1, p2, p3]) in &loads.points {
        let b = l - a;
        let l3 = l2 * l;
        f[0] += p1 * b / l;
        f[6] += p1 * a / l;
        f[1] += p2 * b * b * (3.0 * a + b) / l3;
        f[5] += p2 * a * b * b / l2;
        f[7] += p2 * a * a * (a + 3.0 * b) / l3;
        f[11] -= p2 * a * a * b / l2;
        f[2] += p3 * b * b * (3.0 * a + b) / l3;
        f[4] -= p3 * a * b * b / l2;
        f[8] += p3 * a * a * (a + 3.0 * b) / l3;
        f[10] += p3 * a * a * b / l2;
    }
    f
}

/// Element end forces in local axes, acting on the element:
/// F = K_local T u_global − f_eq.
pub fn end_forces_local(k_local: &Mat12, t: &Mat12, u_global: &[f64; 12], f_eq: &[f64; 12]) -> [f64; 12] {
    let ul = mat_vec(t, u_global);
    let ku = mat_vec(k_local, &ul);
    let mut f = [0.0; 12];
    for i in 0..12 {
        f[i] = ku[i] - f_eq[i];
    }
    f
}

/// Section actions at distance x from end I: [P, V2, V3, T, M2, M3].
///
/// rust-fem convention (decision 0003): components of the force and moment
/// vectors exerted on the positive cut face (outward normal +1), i.e. by the
/// part beyond x on the part [0, x]. P > 0 is tension. Point loads located
/// exactly at x are excluded (value just before the load). The mapping to
/// SAP2000 sign conventions for V2, V3, M2, M3 is pending (oracle_pending).
pub fn station_actions(end_i: &[f64; 12], loads: &LocalSpanLoads, x: f64) -> [f64; 6] {
    let [q1, q2, q3] = loads.uniform;
    let mut f = [end_i[0] + q1 * x, end_i[1] + q2 * x, end_i[2] + q3 * x];
    let mut m2 = end_i[4] + x * end_i[2] + x * x * q3 / 2.0;
    let mut m3 = end_i[5] - x * end_i[1] - x * x * q2 / 2.0;
    for &(a, [p1, p2, p3]) in &loads.points {
        if a < x {
            f[0] += p1;
            f[1] += p2;
            f[2] += p3;
            m2 += (x - a) * p3;
            m3 -= (x - a) * p2;
        }
    }
    [-f[0], -f[1], -f[2], -end_i[3], -m2, -m3]
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: SectionProps = SectionProps { e: 200e9, g: 76.923e9, area: 0.01, i33: 1e-4, i22: 2e-4, j: 3e-4 };

    fn assert_close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    #[test]
    fn axes_horizontal_x_member() {
        let ax = local_axes([0.0; 3], [3.0, 0.0, 0.0], 0.0);
        assert_eq!(ax[0], [1.0, 0.0, 0.0]);
        assert_eq!(ax[1], [0.0, 0.0, 1.0]);
        assert_eq!(ax[2], [0.0, -1.0, 0.0]);
    }

    #[test]
    fn axes_vertical_member_and_angle() {
        let ax = local_axes([0.0; 3], [0.0, 0.0, 3.0], 0.0);
        assert_eq!(ax[1], [1.0, 0.0, 0.0]);
        assert_eq!(ax[2], [0.0, 1.0, 0.0]);
        let ax = local_axes([0.0; 3], [0.0, 0.0, 3.0], 90.0);
        assert_close(ax[1][1], 1.0, 1e-15);
        assert_close(ax[2][0], -1.0, 1e-15);
    }

    #[test]
    fn axes_orthonormal_right_handed_oblique() {
        let ax = local_axes([1.0, -2.0, 0.5], [4.0, 2.0, 3.0], 37.0);
        for i in 0..3 {
            for j in 0..3 {
                let expect = if i == j { 1.0 } else { 0.0 };
                assert_close(dot(ax[i], ax[j]), expect, 1e-14);
            }
        }
        let c = cross(ax[0], ax[1]);
        for k in 0..3 {
            assert_close(c[k], ax[2][k], 1e-14);
        }
        // Default axis 2 has upward component.
        let ax0 = local_axes([1.0, -2.0, 0.5], [4.0, 2.0, 3.0], 0.0);
        assert!(ax0[1][2] > 0.0);
    }

    #[test]
    fn stiffness_symmetric_and_rigid_body_modes_force_free() {
        let (xi, xj) = ([1.0, -2.0, 0.5], [4.0, 2.0, 3.0]);
        let ax = local_axes(xi, xj, 25.0);
        let kl = local_stiffness(length(xi, xj), &P);
        let kg = to_global(&kl, &transformation(&ax));
        let scale = kg.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
        for i in 0..12 {
            for j in 0..12 {
                assert_close(kg[i][j], kg[j][i], 1e-12 * scale);
            }
        }
        // Six rigid-body motions built from geometry, independent of K.
        let mut modes: Vec<[f64; 12]> = Vec::new();
        for d in 0..3 {
            let mut u = [0.0; 12];
            u[d] = 1.0;
            u[6 + d] = 1.0;
            modes.push(u);
        }
        for d in 0..3 {
            let mut w = [0.0; 3];
            w[d] = 1.0;
            let ui = cross(w, xi);
            let uj = cross(w, xj);
            let mut u = [0.0; 12];
            u[..3].copy_from_slice(&ui);
            u[3..6].copy_from_slice(&w);
            u[6..9].copy_from_slice(&uj);
            u[9..12].copy_from_slice(&w);
            modes.push(u);
        }
        for u in modes {
            let f = mat_vec(&kg, &u);
            for v in f {
                assert!(v.abs() <= 1e-6 * scale, "rigid body force {v}");
            }
        }
    }

    #[test]
    fn equivalent_loads_preserve_resultant_and_moment() {
        let l = 4.0;
        let loads = LocalSpanLoads { uniform: [1.0, -2.0, 3.0], points: vec![(1.0, [5.0, 7.0, -11.0])] };
        let f = equivalent_nodal_loads(l, &loads);
        // Force resultants.
        assert_close(f[0] + f[6], 1.0 * l + 5.0, 1e-12);
        assert_close(f[1] + f[7], -2.0 * l + 7.0, 1e-12);
        assert_close(f[2] + f[8], 3.0 * l - 11.0, 1e-12);
        // Moment about end I, axis 3: Σ x·F2 + M3 terms.
        let m3_exact = -2.0 * l * l / 2.0 + 7.0 * 1.0;
        assert_close(f[5] + f[11] + l * f[7], m3_exact, 1e-12);
        // Axis 2: moment of F3 at x is −x·F3.
        let m2_exact = -(3.0 * l * l / 2.0) - (-11.0 * 1.0);
        assert_close(f[4] + f[10] - l * f[8], m2_exact, 1e-12);
    }

    #[test]
    fn station_actions_match_end_j_and_beam_theory() {
        // Simply supported span L with uniform q2 (no K needed): end forces
        // from statics. F_I2 = −q L/2 (support pushes against load).
        let (l, q) = (6.0, -5000.0);
        let loads = LocalSpanLoads { uniform: [0.0, q, 0.0], points: vec![] };
        let mut end_i = [0.0; 12];
        end_i[1] = -q * l / 2.0;
        let s = station_actions(&end_i, &loads, l / 2.0);
        assert_close(s[5], -q * l * l / 8.0, 1e-9);
        assert_close(s[1], 0.0, 1e-9);
        let s_end = station_actions(&end_i, &loads, l);
        assert_close(s_end[5], 0.0, 1e-9);
    }
}
