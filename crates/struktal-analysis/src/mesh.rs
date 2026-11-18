//! Analytical mesh of frames with `auto_mesh` (decision 0015).
//!
//! A frame remains one object for the user. For the analysis it is replaced by
//! pieces between its end, generated and intermediate joints, with its loads cut
//! accordingly. Each piece has exact fixed-end forces, so subdivision by itself
//! does not change the results: it adds output points and connections.
//!
//! Generated names use `~`: frame and joint `F~1`, `F~2`...

use struktal_core::{
    Area, AreaLoad, AutoMesh, EndOffsets, EndReleases, Frame, FrameLoad, FrameLoadKind, Joint, Model, Snapshot,
    validate,
};

/// A joint within this fraction of the length from the flexible axis is on it.
pub const ON_AXIS_TOL_REL: f64 = 1e-6;
/// Joints closer than this but not on the axis are reported: likely modeling errors.
const NEAR_MISS_REL: f64 = 1e-3;

/// How the frames of the model map to the analysis frames.
#[derive(Debug, Clone)]
pub struct Parent {
    pub name: String,
    pub length: f64,
    /// Indices in the expanded model, from end I to end J.
    pub pieces: Vec<usize>,
    /// Distance from end I to the start of each piece.
    pub starts: Vec<f64>,
}

impl Parent {
    /// The piece holding the station at x, and x within it. A station at a
    /// cut belongs to the piece before it.
    pub fn locate(&self, x: f64) -> (usize, f64) {
        let k = (1..self.starts.len()).rev().find(|&k| self.starts[k] < x).unwrap_or(0);
        (self.pieces[k], x - self.starts[k])
    }
}

/// The areas of the model as their elements, when the analysis mesh split them.
#[derive(Debug, Clone)]
pub struct AreaParent {
    pub name: String,
    /// Names of the elements that came out of the object, in order.
    pub elements: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub parents: Vec<Parent>,
    pub area_parents: Vec<AreaParent>,
    /// Joints at or above this index were generated.
    pub user_joints: usize,
    pub messages: Vec<String>,
}

impl Mesh {
    fn identity(s: &Snapshot) -> Mesh {
        let parents = s
            .model
            .frames
            .iter()
            .enumerate()
            .map(|(f, fr)| {
                let (xi, xj) = s.frame_ends(f);
                Parent { name: fr.name.clone(), length: distance(xi, xj), pieces: vec![f], starts: vec![0.0] }
            })
            .collect();
        Mesh { parents, area_parents: Vec::new(), user_joints: s.model.joints.len(), messages: vec![] }
    }
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|k| (b[k] - a[k]).powi(2)).sum::<f64>().sqrt()
}

/// Relative positions of the cuts of one frame, each with the joint it lands on.
fn cuts(s: &Snapshot, f: usize, mesh: AutoMesh, messages: &mut Vec<String>) -> Vec<(f64, Option<usize>)> {
    let fr = &s.model.frames[f];
    let (i, j, _, _) = s.frame_refs[f];
    let (xi, xj) = s.frame_ends(f);
    let length = distance(xi, xj);
    let e1 = [0, 1, 2].map(|k| (xj[k] - xi[k]) / length);
    let tol = ON_AXIS_TOL_REL * length;
    let mut on_axis: Vec<(f64, Option<usize>)> = Vec::new();
    if mesh.at_joints {
        for (p, joint) in s.model.joints.iter().enumerate() {
            if p == i || p == j {
                continue;
            }
            let d = [joint.x - xi[0], joint.y - xi[1], joint.z - xi[2]];
            let along: f64 = (0..3).map(|k| d[k] * e1[k]).sum();
            if along <= tol || along >= length - tol {
                continue;
            }
            let off = (0..3).map(|k| (d[k] - along * e1[k]).powi(2)).sum::<f64>().sqrt();
            if off <= tol {
                on_axis.push((along / length, Some(p)));
            } else if off <= NEAR_MISS_REL * length {
                messages.push(format!(
                    "frame {}: joint {} is {off:e} m from the axis, beyond the tolerance {tol:e} m: not connected",
                    fr.name, joint.name
                ));
            }
        }
    }
    // A joint wins over a generated cut at the same place.
    let n = mesh.segments.max(1);
    let even: Vec<(f64, Option<usize>)> = (1..n)
        .map(|k| f64::from(k) / f64::from(n))
        .filter(|rel| on_axis.iter().all(|c| (c.0 - rel).abs() * length > tol))
        .map(|rel| (rel, None))
        .collect();
    on_axis.extend(even);
    on_axis.sort_by(|a, b| a.0.total_cmp(&b.0));
    on_axis
}

fn cut_load(kind: &FrameLoadKind, r0: f64, r1: f64, last: bool) -> Option<FrameLoadKind> {
    let span = r1 - r0;
    // A concentrated load exactly at a cut goes to the piece before it.
    let holds = |rel: f64| (rel > r0 || r0 == 0.0) && (rel <= r1 || last);
    let segment = |a: f64, b: f64, va: f64, vb: f64| {
        let (lo, hi) = (a.max(r0), b.min(r1));
        (lo < hi).then(|| {
            let at = |r: f64| va + (vb - va) * (r - a) / (b - a);
            ((lo - r0) / span, (hi - r0) / span, at(lo), at(hi))
        })
    };
    match *kind {
        FrameLoadKind::Uniform { .. } => Some(kind.clone()),
        FrameLoadKind::Point { direction, rel, value } => {
            holds(rel).then(|| FrameLoadKind::Point { direction, rel: (rel - r0) / span, value })
        }
        FrameLoadKind::PointMoment { direction, rel, value } => {
            holds(rel).then(|| FrameLoadKind::PointMoment { direction, rel: (rel - r0) / span, value })
        }
        FrameLoadKind::Trapezoidal { direction, rel_start, rel_end, value_start, value_end } => {
            segment(rel_start, rel_end, value_start, value_end).map(|(a, b, v0, v1)| FrameLoadKind::Trapezoidal {
                direction,
                rel_start: a,
                rel_end: b,
                value_start: v0,
                value_end: v1,
            })
        }
        FrameLoadKind::TrapezoidalMoment { direction, rel_start, rel_end, value_start, value_end } => {
            segment(rel_start, rel_end, value_start, value_end).map(|(a, b, v0, v1)| FrameLoadKind::TrapezoidalMoment {
                direction,
                rel_start: a,
                rel_end: b,
                value_start: v0,
                value_end: v1,
            })
        }
    }
}

/// The analysis model and its mapping. Without `auto_mesh` there is no new
/// model. Generated entities can clash with user names: that is an error.
pub fn expand(s: &Snapshot) -> Result<(Option<Snapshot>, Mesh), String> {
    let m = &s.model;
    let frames_mesh = m.frames.iter().any(|f| f.auto_mesh.is_some());
    let areas_mesh = m.areas.iter().any(|a| a.auto_mesh.is_some());
    if !frames_mesh && !areas_mesh {
        return Ok((None, Mesh::identity(s)));
    }
    let mut messages = Vec::new();
    let mut out = Model {
        frames: if frames_mesh { Vec::new() } else { m.frames.clone() },
        frame_loads: if frames_mesh { Vec::new() } else { m.frame_loads.clone() },
        ..m.clone()
    };
    let mut parents = Vec::with_capacity(m.frames.len());
    for (f, fr) in m.frames.iter().enumerate() {
        let cut_list = fr.auto_mesh.map_or_else(Vec::new, |a| cuts(s, f, a, &mut messages));
        let (xi, xj) = s.frame_ends(f);
        let length = distance(xi, xj);
        let first = out.frames.len();
        let loads: Vec<&FrameLoad> = m.frame_loads.iter().filter(|l| l.frame == fr.name).collect();
        if cut_list.is_empty() {
            out.frames.push(Frame { auto_mesh: None, ..fr.clone() });
            out.frame_loads.extend(loads.into_iter().cloned());
            parents.push(Parent { name: fr.name.clone(), length, pieces: vec![first], starts: vec![0.0] });
            continue;
        }
        let pieces = cut_list.len() + 1;
        let mut joints = Vec::with_capacity(cut_list.len());
        for (k, &(rel, joint)) in cut_list.iter().enumerate() {
            joints.push(match joint {
                Some(p) => m.joints[p].name.clone(),
                None => {
                    let name = format!("{}~{}", fr.name, k + 1);
                    out.joints.push(Joint {
                        name: name.clone(),
                        x: xi[0] + rel * (xj[0] - xi[0]),
                        y: xi[1] + rel * (xj[1] - xi[1]),
                        z: xi[2] + rel * (xj[2] - xi[2]),
                        restraint: [false; 6],
                        springs: [0.0; 6],
                        added_mass: [0.0; 6],
                    });
                    name
                }
            });
        }
        let bounds: Vec<f64> =
            std::iter::once(0.0).chain(cut_list.iter().map(|c| c.0)).chain(std::iter::once(1.0)).collect();
        let offsets = fr.offsets.unwrap_or_default();
        let releases = fr.releases.clone().unwrap_or_default();
        for k in 0..pieces {
            let name = format!("{}~{}", fr.name, k + 1);
            let is_first = k == 0;
            let is_last = k + 1 == pieces;
            let piece_offsets = EndOffsets {
                i: if is_first { offsets.i } else { [0.0; 3] },
                j: if is_last { offsets.j } else { [0.0; 3] },
            };
            let piece_releases = EndReleases {
                i: if is_first { releases.i } else { [false; 6] },
                j: if is_last { releases.j } else { [false; 6] },
                i_springs: if is_first { releases.i_springs } else { [0.0; 6] },
                j_springs: if is_last { releases.j_springs } else { [0.0; 6] },
            };
            out.frames.push(Frame {
                name: name.clone(),
                i: if is_first { fr.i.clone() } else { joints[k - 1].clone() },
                j: if is_last { fr.j.clone() } else { joints[k].clone() },
                section: fr.section.clone(),
                angle_deg: fr.angle_deg,
                releases: (piece_releases != EndReleases::default()).then_some(piece_releases),
                offsets: (piece_offsets != EndOffsets::default()).then_some(piece_offsets),
                auto_mesh: None,
            });
            for l in &loads {
                if let Some(load) = cut_load(&l.load, bounds[k], bounds[k + 1], is_last) {
                    out.frame_loads.push(FrameLoad { pattern: l.pattern.clone(), frame: name.clone(), load });
                }
            }
        }
        parents.push(Parent {
            name: fr.name.clone(),
            length,
            pieces: (first..first + pieces).collect(),
            starts: bounds[..pieces].iter().map(|r| r * length).collect(),
        });
    }
    let area_parents = if areas_mesh { expand_areas(s, &mut out, &mut messages)? } else { Vec::new() };

    let mut expanded = validate(&out).map_err(|issues| {
        let text: Vec<String> = issues.iter().map(ToString::to_string).collect();
        format!("analysis mesh: {}", text.join("; "))
    })?;
    expanded.hash = s.hash.clone();
    expanded.original_units = s.original_units;
    let mesh = Mesh { parents, area_parents, user_joints: m.joints.len(), messages };
    Ok((Some(expanded), mesh))
}

/// One point of an area in natural coordinates: (r, s) in the unit square for a
/// quadrilateral, barycentric (u, v) with u + v <= 1 for a triangle.
fn area_point(corners: &[[f64; 3]], r: f64, s: f64) -> [f64; 3] {
    let mut p = [0.0; 3];
    if corners.len() == 3 {
        for k in 0..3 {
            p[k] = corners[0][k] + r * (corners[1][k] - corners[0][k]) + s * (corners[2][k] - corners[0][k]);
        }
        return p;
    }
    for k in 0..3 {
        p[k] = (1.0 - r) * (1.0 - s) * corners[0][k]
            + r * (1.0 - s) * corners[1][k]
            + r * s * corners[2][k]
            + (1.0 - r) * s * corners[3][k];
    }
    p
}

/// Position key, so a point shared by two areas is one joint.
fn key_of(p: [f64; 3]) -> (i64, i64, i64) {
    let k = |v: f64| (v * 1e9).round() as i64;
    (k(p[0]), k(p[1]), k(p[2]))
}

/// The corner (or the two corners of the edge) a grid point sits on.
fn edge_corners(i: usize, j: usize, nu: usize, nv: usize, triangles: bool) -> Option<(usize, usize)> {
    if triangles {
        if i == 0 && j == 0 {
            Some((0, 0))
        } else if i == nu && j == 0 {
            Some((1, 1))
        } else if i == 0 && j == nu {
            Some((2, 2))
        } else if j == 0 {
            Some((0, 1))
        } else if i == 0 {
            Some((0, 2))
        } else if i + j == nu {
            Some((1, 2))
        } else {
            None
        }
    } else if i == 0 && j == 0 {
        Some((0, 0))
    } else if i == nu && j == 0 {
        Some((1, 1))
    } else if i == nu && j == nv {
        Some((2, 2))
    } else if i == 0 && j == nv {
        Some((3, 3))
    } else if j == 0 {
        Some((0, 1))
    } else if i == nu {
        Some((1, 2))
    } else if j == nv {
        Some((2, 3))
    } else if i == 0 {
        Some((3, 0))
    } else {
        None
    }
}

/// The joint of a grid point: the object's corner when it is one, the joint
/// already at that position when there is one, a new one otherwise.
///
/// A new point on the boundary inherits the components both corners of that
/// edge restrain, so meshing a supported slab does not quietly unsupport it.
fn mesh_node(
    out: &mut Model,
    corners: &[(String, [f64; 3])],
    restraints: &[Option<[bool; 6]>],
    p: [f64; 3],
    edge: Option<(usize, usize)>,
    index: &mut std::collections::HashMap<(i64, i64, i64), String>,
    prefix: &str,
) -> String {
    let key = key_of(p);
    if let Some(name) = index.get(&key) {
        return name.clone();
    }
    for (name, xyz) in corners {
        if key_of(*xyz) == key {
            index.insert(key, name.clone());
            return name.clone();
        }
    }
    let mut res = [false; 6];
    if let Some((a, b)) = edge.filter(|(a, b)| a != b) {
        let (ra, rb) = (restraints[a].unwrap_or([false; 6]), restraints[b].unwrap_or([false; 6]));
        for k in 0..6 {
            res[k] = ra[k] && rb[k];
        }
    }
    let name = format!("{}~{}", prefix, out.joints.len() + 1);
    out.joints.push(Joint {
        name: name.clone(),
        x: p[0],
        y: p[1],
        z: p[2],
        restraint: res,
        springs: [0.0; 6],
        added_mass: [0.0; 6],
    });
    index.insert(key, name.clone());
    name
}

/// Subdivides every area that asks for it, in its own natural coordinates, and
/// replaces it by its elements (decision 0031).
///
/// Two neighbouring areas share the nodes of their common edge, because a point
/// is reused when its position already exists: the mesh of a slab does not come
/// apart where two objects meet.
fn expand_areas(s: &Snapshot, out: &mut Model, messages: &mut Vec<String>) -> Result<Vec<AreaParent>, String> {
    let m = &s.model;
    let by_name: std::collections::HashMap<&str, &Joint> = m.joints.iter().map(|j| (j.name.as_str(), j)).collect();

    let mut parents = Vec::new();
    let mut areas = Vec::new();
    // The loads of the objects that are not meshed stay as they are.
    let mut loads: Vec<AreaLoad> = m
        .area_loads
        .iter()
        .filter(|l| m.areas.iter().all(|a| a.name != l.area || a.auto_mesh.is_none()))
        .cloned()
        .collect();
    let mut index: std::collections::HashMap<(i64, i64, i64), String> = std::collections::HashMap::new();

    for area in &m.areas {
        let Some(mesh) = area.auto_mesh else {
            areas.push(area.clone());
            continue;
        };
        let mut corners: Vec<(String, [f64; 3])> = Vec::with_capacity(area.joints.len());
        let mut restraints: Vec<Option<[bool; 6]>> = Vec::with_capacity(area.joints.len());
        for name in &area.joints {
            let j = by_name.get(name.as_str()).ok_or_else(|| format!("area {}: unknown joint {}", area.name, name))?;
            corners.push((name.clone(), [j.x, j.y, j.z]));
            restraints.push(Some(j.restraint));
        }
        let triangles = corners.len() == 3;
        // A triangle has a single division count: n, on every edge.
        let (nu, nv) = if triangles {
            (mesh.n.max(1) as usize, mesh.n.max(1) as usize)
        } else {
            (mesh.m.max(1) as usize, mesh.n.max(1) as usize)
        };

        let xyz: Vec<[f64; 3]> = corners.iter().map(|c| c.1).collect();
        let mut grid: Vec<Vec<String>> = vec![vec![String::new(); nv + 1]; nu + 1];
        for (i, row) in grid.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                if triangles && i + j > nu {
                    continue;
                }
                let p = area_point(&xyz, i as f64 / nu as f64, j as f64 / nv as f64);
                let edge = edge_corners(i, j, nu, nv, triangles);
                *cell = mesh_node(out, &corners, &restraints, p, edge, &mut index, &area.name);
            }
        }

        let mut elements = Vec::new();
        let mut k = 1;
        for i in 0..nu {
            for j in 0..nv {
                if triangles {
                    if i + j >= nu {
                        continue;
                    }
                    let mut tris = vec![[grid[i][j].clone(), grid[i + 1][j].clone(), grid[i][j + 1].clone()]];
                    if i + j + 2 <= nu {
                        tris.push([grid[i + 1][j].clone(), grid[i + 1][j + 1].clone(), grid[i][j + 1].clone()]);
                    }
                    for tri in tris {
                        let name = format!("{}#{}", area.name, k);
                        k += 1;
                        areas.push(Area {
                            name: name.clone(),
                            joints: tri.to_vec(),
                            section: area.section.clone(),
                            auto_mesh: None,
                        });
                        elements.push(name);
                    }
                } else {
                    let quad = vec![
                        grid[i][j].clone(),
                        grid[i + 1][j].clone(),
                        grid[i + 1][j + 1].clone(),
                        grid[i][j + 1].clone(),
                    ];
                    let name = format!("{}#{}", area.name, k);
                    k += 1;
                    areas.push(Area {
                        name: name.clone(),
                        joints: quad,
                        section: area.section.clone(),
                        auto_mesh: None,
                    });
                    elements.push(name);
                }
            }
        }

        // The load of the object goes to every element: the intensity is the
        // same, so the resultant is preserved exactly.
        for load in &m.area_loads {
            if load.area == area.name {
                for element in &elements {
                    loads.push(AreaLoad {
                        pattern: load.pattern.clone(),
                        area: element.clone(),
                        direction: load.direction,
                        value: load.value,
                    });
                }
            }
        }
        if elements.is_empty() {
            return Err(format!("area {}: the mesh produced no element", area.name));
        }
        messages.push(format!("area {}: {} elements", area.name, elements.len()));
        parents.push(AreaParent { name: area.name.clone(), elements });
    }

    out.areas = areas;
    out.area_loads = loads;
    Ok(parents)
}
#[cfg(test)]
mod tests {
    use super::*;
    use struktal_core::LoadDirection::Gravity;

    fn snapshot(frame: serde_json::Value, joints: serde_json::Value, loads: serde_json::Value) -> Snapshot {
        let text = serde_json::json!({
            "schema_version": 5,
            "materials": [{"name": "S", "e": 2e11, "nu": 0.3}],
            "sections": [{"name": "B", "material": "S", "area": 0.01, "i33": 1e-4, "i22": 1e-4, "j": 2e-4}],
            "joints": joints,
            "frames": [frame],
            "load_patterns": [{"name": "L"}],
            "frame_loads": loads,
        })
        .to_string();
        validate(&struktal_core::model_from_json(&text).expect("parse")).expect("valid")
    }

    fn straight(auto: serde_json::Value, extra: serde_json::Value) -> Snapshot {
        let mut joints = vec![
            serde_json::json!({"name": "A", "x": 0.0, "y": 0.0, "z": 0.0, "restraint": [true, true, true, true, true, true]}),
            serde_json::json!({"name": "B", "x": 10.0, "y": 0.0, "z": 0.0}),
        ];
        joints.extend(extra.as_array().cloned().unwrap_or_default());
        snapshot(
            serde_json::json!({"name": "F", "i": "A", "j": "B", "section": "B", "auto_mesh": auto}),
            serde_json::Value::Array(joints),
            serde_json::json!([]),
        )
    }

    #[test]
    fn no_auto_mesh_means_no_new_model() {
        let s = snapshot(
            serde_json::json!({"name": "F", "i": "A", "j": "B", "section": "B"}),
            serde_json::json!([{"name": "A", "x": 0.0, "y": 0.0, "z": 0.0}, {"name": "B", "x": 1.0, "y": 0.0, "z": 0.0}]),
            serde_json::json!([]),
        );
        let (model, mesh) = expand(&s).expect("ok");
        assert!(model.is_none());
        assert_eq!(mesh.parents[0].pieces, vec![0]);
    }

    #[test]
    fn segments_and_joints_make_the_cuts_and_a_joint_wins_at_the_same_place() {
        // Divisions at 2.5, 5, 7.5; joints at 5 and 6: the one at 5 replaces the division.
        let s = straight(
            serde_json::json!({"segments": 4, "at_joints": true}),
            serde_json::json!([{"name": "J5", "x": 5.0, "y": 0.0, "z": 0.0}, {"name": "J6", "x": 6.0, "y": 0.0, "z": 0.0}]),
        );
        let (model, mesh) = expand(&s).expect("ok");
        let model = model.expect("expanded");
        let names: Vec<(&str, &str)> = model.model.frames.iter().map(|f| (f.i.as_str(), f.j.as_str())).collect();
        assert_eq!(names, vec![("A", "F~1"), ("F~1", "J5"), ("J5", "J6"), ("J6", "F~4"), ("F~4", "B")]);
        assert_eq!(mesh.parents[0].starts, vec![0.0, 2.5, 5.0, 6.0, 7.5]);
        assert_eq!(mesh.user_joints, 4);
        assert_eq!(model.hash, s.hash);
    }

    #[test]
    fn locate_gives_a_cut_to_the_piece_before_it() {
        let p = Parent { name: "F".into(), length: 10.0, pieces: vec![7, 8], starts: vec![0.0, 4.0] };
        assert_eq!(p.locate(0.0), (7, 0.0));
        assert_eq!(p.locate(4.0), (7, 4.0));
        assert_eq!(p.locate(5.0), (8, 1.0));
    }

    #[test]
    fn near_miss_is_reported_and_not_connected() {
        let s = straight(
            serde_json::json!({"at_joints": true}),
            serde_json::json!([{"name": "J", "x": 5.0, "y": 0.005, "z": 0.0, "restraint": [true, true, true, true, true, true]}]),
        );
        let (model, mesh) = expand(&s).expect("ok");
        assert_eq!(model.expect("expanded").model.frames.len(), 1);
        assert!(mesh.messages[0].contains("joint J"), "{:?}", mesh.messages);
    }

    #[test]
    fn generated_name_clash_is_an_error() {
        let s = straight(
            serde_json::json!({"segments": 2}),
            serde_json::json!([{"name": "F~1", "x": 3.0, "y": 3.0, "z": 0.0, "restraint": [true, true, true, true, true, true]}]),
        );
        assert!(expand(&s).expect_err("clash").contains("duplicate"));
    }

    #[test]
    fn loads_are_cut_at_the_right_values() {
        let trap = FrameLoadKind::Trapezoidal {
            direction: Gravity,
            rel_start: 0.2,
            rel_end: 0.8,
            value_start: 0.0,
            value_end: 6.0,
        };
        // Piece [0.5, 1]: from rel 0.5 (value 3) to 0.8 (value 6).
        let Some(FrameLoadKind::Trapezoidal { rel_start, rel_end, value_start, value_end, .. }) =
            cut_load(&trap, 0.5, 1.0, true)
        else {
            panic!("no load")
        };
        assert!((rel_start - 0.0).abs() < 1e-14 && (rel_end - 0.6).abs() < 1e-14);
        assert!((value_start - 3.0).abs() < 1e-14 && (value_end - 6.0).abs() < 1e-14);
        // A piece beyond the load gets nothing; the cut point goes to the piece before.
        assert!(cut_load(&trap, 0.8, 1.0, true).is_none());
        let point = FrameLoadKind::Point { direction: Gravity, rel: 0.5, value: 1.0 };
        assert!(cut_load(&point, 0.0, 0.5, false).is_some());
        assert!(cut_load(&point, 0.5, 1.0, true).is_none());
        let at_i = FrameLoadKind::Point { direction: Gravity, rel: 0.0, value: 1.0 };
        assert!(cut_load(&at_i, 0.0, 0.5, false).is_some());
    }
    fn area_snapshot(areas: serde_json::Value, joints: serde_json::Value, loads: serde_json::Value) -> Snapshot {
        let text = serde_json::json!({
            "schema_version": 9,
            "materials": [{"name": "S", "e": 2e11, "nu": 0.3}],
            "sections": [],
            "area_sections": [{"name": "S1", "material": "S", "thickness": 0.1}],
            "joints": joints,
            "areas": areas,
            "load_patterns": [{"name": "L"}],
            "area_loads": loads,
        })
        .to_string();
        validate(&struktal_core::model_from_json(&text).expect("parse")).expect("valid")
    }

    #[test]
    fn a_quad_meshed_m_by_n_becomes_m_times_n_areas() {
        let s = area_snapshot(
            serde_json::json!([{
                "name": "A", "joints": ["1", "2", "3", "4"], "section": "S1",
                "auto_mesh": {"m": 2, "n": 3},
            }]),
            serde_json::json!([
                {"name": "1", "x": 0.0, "y": 0.0, "z": 0.0},
                {"name": "2", "x": 2.0, "y": 0.0, "z": 0.0},
                {"name": "3", "x": 2.0, "y": 1.0, "z": 0.0},
                {"name": "4", "x": 0.0, "y": 1.0, "z": 0.0},
            ]),
            serde_json::json!([]),
        );
        let (model, mesh) = expand(&s).expect("ok");
        let model = model.expect("expanded");
        assert_eq!(model.model.areas.len(), 6, "m x n = 2 x 3");
        assert_eq!(model.model.joints.len(), 12, "(m+1)(n+1) = 3 x 4");
        assert_eq!(mesh.area_parents.len(), 1);
        assert_eq!(mesh.area_parents[0].elements.len(), 6);
        // The area is still one object in the model: no area keeps auto_mesh.
        assert!(model.model.areas.iter().all(|a| a.auto_mesh.is_none()));
    }

    #[test]
    fn a_triangle_meshed_n_times_has_n_squared_elements() {
        let s = area_snapshot(
            serde_json::json!([{
                "name": "T", "joints": ["1", "2", "3"], "section": "S1",
                "auto_mesh": {"m": 2, "n": 2},
            }]),
            serde_json::json!([
                {"name": "1", "x": 0.0, "y": 0.0, "z": 0.0},
                {"name": "2", "x": 2.0, "y": 0.0, "z": 0.0},
                {"name": "3", "x": 0.0, "y": 2.0, "z": 0.0},
            ]),
            serde_json::json!([]),
        );
        let (model, _) = expand(&s).expect("ok");
        let model = model.expect("expanded");
        assert_eq!(model.model.areas.len(), 4, "n^2 = 2^2");
        assert!(model.model.areas.iter().all(|a| a.joints.len() == 3));
    }

    #[test]
    fn two_neighbouring_areas_share_the_edge_nodes() {
        // Two quads side by side, each meshed 2 x 2: the middle of the common
        // edge is one joint, not two, or the slab would come apart there.
        let s = area_snapshot(
            serde_json::json!([
                {"name": "A", "joints": ["1", "2", "3", "4"], "section": "S1", "auto_mesh": {"m": 2, "n": 2}},
                {"name": "B", "joints": ["2", "5", "6", "3"], "section": "S1", "auto_mesh": {"m": 2, "n": 2}},
            ]),
            serde_json::json!([
                {"name": "1", "x": 0.0, "y": 0.0, "z": 0.0},
                {"name": "2", "x": 2.0, "y": 0.0, "z": 0.0},
                {"name": "3", "x": 2.0, "y": 1.0, "z": 0.0},
                {"name": "4", "x": 0.0, "y": 1.0, "z": 0.0},
                {"name": "5", "x": 4.0, "y": 0.0, "z": 0.0},
                {"name": "6", "x": 4.0, "y": 1.0, "z": 0.0},
            ]),
            serde_json::json!([]),
        );
        let (model, _) = expand(&s).expect("ok");
        let model = model.expect("expanded");
        // 3 x 3 per area, less the 3 shared with the neighbour.
        assert_eq!(model.model.joints.len(), 15, "9 + 9 - 3");
        assert_eq!(model.model.areas.len(), 8, "4 + 4");
    }

    #[test]
    fn a_meshed_area_keeps_its_load_on_every_element() {
        let s = area_snapshot(
            serde_json::json!([{
                "name": "A", "joints": ["1", "2", "3", "4"], "section": "S1",
                "auto_mesh": {"m": 2, "n": 2},
            }]),
            serde_json::json!([
                {"name": "1", "x": 0.0, "y": 0.0, "z": 0.0},
                {"name": "2", "x": 2.0, "y": 0.0, "z": 0.0},
                {"name": "3", "x": 2.0, "y": 1.0, "z": 0.0},
                {"name": "4", "x": 0.0, "y": 1.0, "z": 0.0},
            ]),
            serde_json::json!([{"pattern": "L", "area": "A", "direction": "Gravity", "value": 1000.0}]),
        );
        let (model, _) = expand(&s).expect("ok");
        let model = model.expect("expanded");
        // One load per element, with the object's intensity: the resultant is
        // the same 1000 N/m over the same 2 m2.
        assert_eq!(model.model.area_loads.len(), 4);
        assert!(model.model.area_loads.iter().all(|l| l.value == 1000.0));
        let names: Vec<&str> = model.model.areas.iter().map(|a| a.name.as_str()).collect();
        for load in &model.model.area_loads {
            assert!(names.contains(&load.area.as_str()), "load on a missing element");
        }
    }
}
