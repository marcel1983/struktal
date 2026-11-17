//! Area elements in the assembly: local axes, stiffness in global axes, loads
//! and section forces (decision 0019).

use std::collections::HashMap;

use struktal_core::{AreaBehavior, AreaSection, Model, Name, Snapshot, laminate_abc};
use struktal_elements::frame3d::Vec3;
use struktal_elements::plane::{self, PlaneProps};
use struktal_elements::shell3;
use struktal_elements::shell4::{self, Mat24, ShellProps};

use crate::linear_static::cross;
use crate::results::AreaResult;

/// A triangle has `nodes` = 3 and repeats its third joint as a fourth: that slot
/// has zero stiffness, load and mass, so the 24-DOF frame serves both shapes.
pub struct AreaData {
    pub nodes: usize,
    pub dofs: [usize; 24],
    pub axes: [Vec3; 3],
    pub xy: [[f64; 2]; 4],
    pub corners: [Vec3; 4],
    pub props: ShellProps,
    /// Plane-stress properties, when the area is a plane one (decision 0033).
    pub plane: Option<PlaneProps>,
    pub k_global: Mat24,
    /// Weight per unit area.
    pub weight: f64,
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(a: Vec3) -> Vec3 {
    let n = dot(a, a).sqrt();
    a.map(|c| c / n)
}

/// Local axes: 3 is the normal by the right-hand rule on the joint order; 1 is
/// horizontal, Z × 3; 2 = 3 × 1 points upward. If the area is horizontal, 1 = +X.
/// With the third corner repeated, the diagonals give a triangle's normal too.
/// Agreement with SAP2000 is pending (decision 0019).
pub fn local_axes(corners: &[Vec3; 4]) -> [Vec3; 3] {
    let d = |a: Vec3, b: Vec3| [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e3 = unit(cross(d(corners[0], corners[2]), d(corners[1], corners[3])));
    let h = cross([0.0, 0.0, 1.0], e3);
    let e1 = if dot(h, h).sqrt() < 1e-3 { unit(cross([0.0, 1.0, 0.0], e3)) } else { unit(h) };
    [e1, cross(e3, e1), e3]
}

/// Properties of a laminated area section (decision 0032): `a` and `d` per unit
/// width from the layer stack, and the transverse shear as `k ∫ G dz` with
/// k = 5/6, carried as an equivalent `g · shear_t` so the element needs no new
/// input. Validation has already refused a stack that couples (`b ≠ 0`).
fn laminate_props(mut props: ShellProps, m: &Model, sec: &AreaSection) -> ShellProps {
    let stiffness: HashMap<Name, (f64, f64)> = m.materials.iter().map(|x| (x.name.clone(), (x.e, x.nu))).collect();
    let abc = laminate_abc(&sec.layers, &stiffness).expect("validated laminate");
    let total: f64 = sec.layers.iter().map(|l| l.thickness).sum();
    let g_integral: f64 = sec
        .layers
        .iter()
        .map(|l| {
            let (e, nu) = stiffness[&l.material];
            e / (2.0 * (1.0 + nu)) * l.thickness
        })
        .sum();
    props.laminate = Some(abc);
    props.membrane_t = if props.membrane_t > 0.0 { total } else { 0.0 };
    props.bending_t = if props.bending_t > 0.0 { total } else { 0.0 };
    props.shear_t = if props.bending_t > 0.0 { 5.0 / 6.0 * total } else { 0.0 };
    props.g = g_integral / total;
    props
}
/// Plane-stress stiffness in the 24-DOF frame: the 2 in-plane DOF of each node.
fn plane_local(xy: &[[f64; 2]; 4], nodes: usize, p: &PlaneProps) -> Mat24 {
    let mut k = [[0.0; 8]; 8];
    if nodes == 3 {
        let k6 = plane::tri_stiffness(&[xy[0], xy[1], xy[2]], p);
        for i in 0..6 {
            for j in 0..6 {
                k[i][j] = k6[i][j];
            }
        }
    } else {
        k = plane::quad_stiffness(xy, p);
    }
    let mut out = [[0.0; 24]; 24];
    for n in 0..nodes {
        for m in 0..nodes {
            for i in 0..2 {
                for j in 0..2 {
                    out[6 * n + i][6 * m + j] = k[2 * n + i][2 * m + j];
                }
            }
        }
    }
    out
}

/// Per area, in the order of the model. Sections and joints are validated.
pub fn build(s: &Snapshot) -> Vec<AreaData> {
    let m = &s.model;
    m.areas
        .iter()
        .map(|a| {
            let nodes = a.joints.len();
            let joints = [0, 1, 2, 3].map(|k: usize| s.joint_index[&a.joints[k.min(nodes - 1)]]);
            let corners = joints.map(|j| s.joint_xyz(j));
            let axes = local_axes(&corners);
            let center: Vec3 = [0, 1, 2].map(|k| corners[..nodes].iter().map(|c| c[k]).sum::<f64>() / nodes as f64);
            let xy = corners.map(|c| {
                let r = [c[0] - center[0], c[1] - center[1], c[2] - center[2]];
                [dot(r, axes[0]), dot(r, axes[1])]
            });
            let sec = m.area_sections.iter().find(|x| x.name == a.section).expect("validated section");
            let mat = m.materials.iter().find(|x| x.name == sec.material).expect("validated material");
            let g = mat.g.unwrap_or(mat.e / (2.0 * (1.0 + mat.nu)));
            let bending = sec.bending_thickness.unwrap_or(sec.thickness);
            // A laminate carries `a` and `d` per unit width, so the element takes
            // them instead of an equivalent E and t (decision 0032).
            let mut props = ShellProps {
                e: mat.e,
                nu: mat.nu,
                g,
                membrane_t: if sec.behavior == AreaBehavior::Plate { 0.0 } else { sec.thickness },
                bending_t: if sec.behavior == AreaBehavior::Membrane { 0.0 } else { bending },
                shear_t: if sec.behavior == AreaBehavior::Membrane { 0.0 } else { 5.0 / 6.0 * bending },
                laminate: None,
            };
            if !sec.layers.is_empty() {
                props = laminate_props(props, m, sec);
            }
            // A plane area is a 2D idealisation with two DOF per node
            // (decision 0033). Its two are the local axes 1 and 2 of the
            // area, so the element matrix goes straight into the 24-DOF frame
            // and `to_global` does the rotation.
            let mut plane_props = None;
            let local = if sec.behavior == AreaBehavior::Plane {
                let pp = PlaneProps { e: mat.e, nu: mat.nu, thickness: sec.thickness };
                plane_props = Some(pp);
                plane_local(&xy, nodes, &pp)
            } else if nodes == 3 {
                shell3::local_stiffness(&[xy[0], xy[1], xy[2]], &props)
            } else {
                shell4::local_stiffness(&xy, &props)
            };
            let mut dofs = [0usize; 24];
            for (n, &j) in joints.iter().enumerate() {
                for d in 0..6 {
                    dofs[6 * n + d] = 6 * j + d;
                }
            }
            AreaData {
                nodes,
                dofs,
                axes,
                xy,
                corners,
                props,
                plane: plane_props,
                k_global: to_global(&local, &axes),
                weight: mat.unit_weight * sec.thickness,
            }
        })
        .collect()
}

/// R ᵀ K R, by the 3 × 3 blocks of R, for each of the 8 translation and
/// rotation triplets of the element.
fn to_global(k: &Mat24, axes: &[Vec3; 3]) -> Mat24 {
    let block = |row: usize, col: usize| -> [[f64; 3]; 3] {
        let mut out = [[0.0; 3]; 3];
        for a in 0..3 {
            for b in 0..3 {
                out[a][b] =
                    (0..3).map(|p| (0..3).map(|q| axes[p][a] * k[row + p][col + q] * axes[q][b]).sum::<f64>()).sum();
            }
        }
        out
    };
    let mut out = [[0.0; 24]; 24];
    for bi in 0..8 {
        for bj in 0..8 {
            let b = block(3 * bi, 3 * bj);
            for a in 0..3 {
                for c in 0..3 {
                    out[3 * bi + a][3 * bj + c] = b[a][c];
                }
            }
        }
    }
    out
}

impl AreaData {
    /// Displacements at the nodes in local axes, from the global ones.
    pub fn to_local(&self, u: &[f64; 24]) -> [f64; 24] {
        let mut out = [0.0; 24];
        for b in 0..8 {
            for a in 0..3 {
                out[3 * b + a] = (0..3).map(|c| self.axes[a][c] * u[3 * b + c]).sum();
            }
        }
        out
    }

    /// ∫ Nᵢ dA per node.
    pub fn nodal_areas(&self) -> [f64; 4] {
        if self.plane.is_some() {
            return if self.nodes == 3 {
                plane::tri_nodal_areas(&[self.xy[0], self.xy[1], self.xy[2]])
            } else {
                plane::quad_nodal_areas(&self.xy)
            };
        }
        if self.nodes == 3 { shell3::nodal_areas(&self.triangle()) } else { shell4::nodal_areas(&self.xy) }
    }

    /// The in-plane displacements of the nodes, as the plane element wants them.
    fn plane_uv(&self, local: &[f64; 24]) -> [f64; 8] {
        let mut uv = [0.0; 8];
        for n in 0..self.nodes {
            uv[2 * n] = local[6 * n];
            uv[2 * n + 1] = local[6 * n + 1];
        }
        uv
    }

    /// Section forces [F11, F22, F12] per unit length in the area's local
    /// axes, and zeros in the six shell slots (decision 0033). A plane area
    /// has no plate result, and its stresses are reported as the forces per
    /// unit length the shell reports, so `F11` means the same in both.
    fn plane_stress(&self, pp: &PlaneProps, local: &[f64; 24], r: f64, s: f64) -> [f64; 8] {
        let uv = self.plane_uv(local);
        let sigma = if self.nodes == 3 {
            plane::tri_stress(&[self.xy[0], self.xy[1], self.xy[2]], pp, &[uv[0], uv[1], uv[2], uv[3], uv[4], uv[5]])
        } else {
            plane::quad_stress(&self.xy, pp, &uv, r, s)
        };
        let t = pp.thickness;
        [sigma[0] * t, sigma[1] * t, sigma[2] * t, 0.0, 0.0, 0.0, 0.0, 0.0]
    }

    fn triangle(&self) -> [[f64; 2]; 3] {
        [self.xy[0], self.xy[1], self.xy[2]]
    }

    /// Consistent nodal forces of a uniform load `g` per unit area, in global axes.
    pub fn uniform_load(&self, g: Vec3) -> [f64; 24] {
        let shares = self.nodal_areas();
        let mut f = [0.0; 24];
        for (n, a) in shares.iter().enumerate() {
            for k in 0..3 {
                f[6 * n + k] = g[k] * a;
            }
        }
        f
    }

    pub fn result(&self, name: &str, ue: &[f64; 24]) -> AreaResult {
        let local = self.to_local(ue);
        if let Some(pp) = &self.plane {
            // (0, 0) are the natural coordinates of both shapes: the centroid of
            // the triangle and the centre of the quadrilateral.
            let center = self.plane_stress(pp, &local, 0.0, 0.0);
            let corners = (0..self.nodes)
                .map(|k| match self.nodes {
                    3 => self.plane_stress(pp, &local, [0.0, 1.0, 0.0][k], [0.0, 0.0, 1.0][k]),
                    _ => self.plane_stress(pp, &local, [-1.0, 1.0, 1.0, -1.0][k], [-1.0, -1.0, 1.0, 1.0][k]),
                })
                .collect();
            return AreaResult { area: name.to_string(), center, corners };
        }
        if self.nodes == 3 {
            let tri = self.triangle();
            let at = |r: f64, s: f64| shell3::resultants(&tri, &self.props, &local, r, s);
            return AreaResult {
                area: name.to_string(),
                center: at(1.0 / 3.0, 1.0 / 3.0),
                corners: vec![at(0.0, 0.0), at(1.0, 0.0), at(0.0, 1.0)],
            };
        }
        let at = |r: f64, s: f64| shell4::resultants(&self.xy, &self.props, &local, r, s);
        AreaResult {
            area: name.to_string(),
            center: at(0.0, 0.0),
            corners: vec![at(-1.0, -1.0), at(1.0, -1.0), at(1.0, 1.0), at(-1.0, 1.0)],
        }
    }
}
