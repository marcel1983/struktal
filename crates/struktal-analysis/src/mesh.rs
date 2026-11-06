//! Analytical mesh of frames with `auto_mesh` (decision 0015).
//!
//! A frame remains one object for the user. For the analysis it is replaced by
//! pieces between its end, generated and intermediate joints, with its loads cut
//! accordingly. Each piece has exact fixed-end forces, so subdivision by itself
//! does not change the results: it adds output points and connections.
//!
//! Generated names use `~`: frame and joint `F~1`, `F~2`...

use struktal_core::{
    AutoMesh, EndOffsets, EndReleases, Frame, FrameLoad, FrameLoadKind, Joint, Model, Snapshot, validate,
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

#[derive(Debug, Clone)]
pub struct Mesh {
    pub parents: Vec<Parent>,
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
        Mesh { parents, user_joints: s.model.joints.len(), messages: vec![] }
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
    if m.frames.iter().all(|f| f.auto_mesh.is_none()) {
        return Ok((None, Mesh::identity(s)));
    }
    let mut messages = Vec::new();
    let mut out = Model { frames: Vec::new(), frame_loads: Vec::new(), ..m.clone() };
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
    let mut expanded = validate(&out).map_err(|issues| {
        let text: Vec<String> = issues.iter().map(ToString::to_string).collect();
        format!("analysis mesh: {}", text.join("; "))
    })?;
    expanded.hash = s.hash.clone();
    expanded.original_units = s.original_units;
    let mesh = Mesh { parents, user_joints: m.joints.len(), messages };
    Ok((Some(expanded), mesh))
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
}
