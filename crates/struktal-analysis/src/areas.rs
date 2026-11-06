//! Area elements in the assembly: local axes, stiffness in global axes, loads
//! and section forces (decision 0019).

use struktal_core::{AreaBehavior, Snapshot};
use struktal_elements::frame3d::Vec3;
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
            let props = ShellProps {
                e: mat.e,
                nu: mat.nu,
                g,
                membrane_t: if sec.behavior == AreaBehavior::Plate { 0.0 } else { sec.thickness },
                bending_t: if sec.behavior == AreaBehavior::Membrane { 0.0 } else { bending },
                shear_t: if sec.behavior == AreaBehavior::Membrane { 0.0 } else { 5.0 / 6.0 * bending },
            };
            let local = if nodes == 3 {
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
        if self.nodes == 3 { shell3::nodal_areas(&self.triangle()) } else { shell4::nodal_areas(&self.xy) }
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
