//! Validation of an editable model into an immutable, indexed snapshot.
//!
//! Validation is all-or-nothing: either every check passes and a [`Snapshot`]
//! is produced, or the full list of issues (entity, field, cause) is returned
//! and nothing is transformed.

use std::collections::{HashMap, HashSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::laminate::laminate_abc;
use crate::model::*;

/// Relative tolerance for coincident joints / zero-length frames.
pub const LENGTH_TOL_REL: f64 = 1e-9;

const DOF_NAMES: [&str; 6] = ["UX", "UY", "UZ", "RX", "RY", "RZ"];

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub entity: String,
    pub field: String,
    pub cause: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} / {}: {}", self.entity, self.field, self.cause)
    }
}

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Parse(serde_json::Error),
    MissingSchemaVersion,
    FutureSchema {
        found: u32,
        supported: u32,
    },
    /// The file declares an older schema but carries a field that only a newer
    /// one defines. `field` names the offending JSON key.
    NewerField {
        found: u32,
        field: String,
    },
    UnsupportedSchema {
        found: u32,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "io error: {e}"),
            LoadError::Parse(e) => write!(f, "parse error: {e}"),
            LoadError::MissingSchemaVersion => write!(f, "missing integer field `schema_version`"),
            LoadError::FutureSchema { found, supported } => {
                write!(f, "schema_version {found} is newer than the supported version {supported}; refusing to read")
            }
            LoadError::NewerField { found, field } => {
                write!(f, "schema_version {found} cannot hold '{field}': that field belongs to a newer schema")
            }
            LoadError::UnsupportedSchema { found } => write!(f, "schema_version {found} is not supported"),
        }
    }
}

impl std::error::Error for LoadError {}

/// True when a key is present and not `null`. A key a writer emits as `null`
/// means the same as an absent one.
fn is_set(value: Option<&serde_json::Value>) -> bool {
    !matches!(value, None | Some(serde_json::Value::Null))
}

/// True when `key` holds something a newer schema would have to interpret. An
/// absent key, an explicit `null` and an empty array all mean "nothing here", so
/// a v1 file written by a serializer that always emits `"area_loads": []` still
/// loads instead of being refused as a v5 file.
fn has_value(raw: &serde_json::Value, key: &str) -> bool {
    match raw.get(key) {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Array(items)) => !items.is_empty(),
        Some(_) => true,
    }
}

/// The first of `keys` that `raw` carries, for the error message.
fn first_value<'a>(raw: &'a serde_json::Value, keys: &[&'a str]) -> Option<&'a str> {
    keys.iter().copied().find(|k| has_value(raw, k))
}

/// Reads a model from JSON, checking the schema version before the body.
/// Migrations: v1 → v2 adds `as2 = as3 = 0` (no shear deformation, the v1
/// behavior); v2 → v3 adds no offsets, constraints, springs or imposed
/// displacements; v3 → v4 only adds load kinds; v4 → v5 adds no subdivision;
/// v5 → v6 only allows areas of three joints; v6 → v7 only adds a mass source.
/// Missing fields already default to that, so only the version number changes.
///
/// A field of a newer schema is refused even when it is empty of meaning only
/// if it carries content: `constraints: []` is the same as no constraints at all.
pub fn model_from_json(text: &str) -> Result<Model, LoadError> {
    let raw: serde_json::Value = serde_json::from_str(text).map_err(LoadError::Parse)?;
    let ver = raw.get("schema_version").and_then(|v| v.as_u64()).ok_or(LoadError::MissingSchemaVersion)?;
    let ver = u32::try_from(ver).map_err(|_| LoadError::UnsupportedSchema { found: u32::MAX })?;
    if ver > SCHEMA_VERSION {
        return Err(LoadError::FutureSchema { found: ver, supported: SCHEMA_VERSION });
    }
    if ver < 1 {
        return Err(LoadError::UnsupportedSchema { found: ver });
    }
    let newer = |field: &str| LoadError::NewerField { found: ver, field: field.to_string() };
    if ver == 1 {
        // Shear areas did not exist in v1; reject them instead of silently
        // giving a v1 file v2 semantics.
        let has_shear = raw
            .get("sections")
            .and_then(|v| v.as_array())
            .is_some_and(|secs| secs.iter().any(|s| is_set(s.get("as2")) || is_set(s.get("as3"))));
        if has_shear {
            return Err(newer("sections[].as2 / sections[].as3"));
        }
    }
    if ver < 3 {
        // Same rule for the v3 fields: never give an older file v3 semantics.
        let has_offsets = raw
            .get("frames")
            .and_then(|v| v.as_array())
            .is_some_and(|frames| frames.iter().any(|f| is_set(f.get("offsets"))));
        let has_springs = raw
            .get("joints")
            .and_then(|v| v.as_array())
            .is_some_and(|joints| joints.iter().any(|j| is_set(j.get("springs"))));
        if has_offsets {
            return Err(newer("frames[].offsets"));
        }
        if has_springs {
            return Err(newer("joints[].springs"));
        }
        if has_value(&raw, "constraints") {
            return Err(newer("constraints"));
        }
        if has_value(&raw, "joint_displacements") {
            return Err(newer("joint_displacements"));
        }
    }
    if ver < 9 {
        let has_area_mesh = raw
            .get("areas")
            .and_then(|v| v.as_array())
            .is_some_and(|areas| areas.iter().any(|a| a.get("auto_mesh").is_some()));
        if has_area_mesh {
            return Err(newer("areas[].auto_mesh"));
        }
    }
    if ver < 10 {
        // Same rule for the laminated section: never give an older file v10
        // semantics.
        let has_layers = raw
            .get("area_sections")
            .and_then(|v| v.as_array())
            .is_some_and(|secs| secs.iter().any(|s| is_set(s.get("layers"))));
        if has_layers {
            return Err(newer("area_sections[].layers"));
        }
    }
    if ver < 11 {
        // Same rule for the plane behaviour: an older reader would fail on the
        // value, so it says which field is newer instead (decision 0033).
        let has_plane = raw
            .get("area_sections")
            .and_then(|v| v.as_array())
            .is_some_and(|secs| secs.iter().any(|s| s.get("behavior").and_then(|b| b.as_str()) == Some("Plane")));
        if has_plane {
            return Err(newer("area_sections[].behavior: Plane"));
        }
    }
    if ver < 12 {
        // Same rule for the axisymmetric behaviour and the centrifugal
        // direction: an older reader cannot parse either value (decision 0034).
        let has_axisym = raw.get("area_sections").and_then(|v| v.as_array()).is_some_and(|secs| {
            secs.iter().any(|s| s.get("behavior").and_then(|b| b.as_str()) == Some("Axisymmetric"))
        });
        if has_axisym {
            return Err(newer("area_sections[].behavior: Axisymmetric"));
        }
        let has_centrifugal = ["area_loads", "frame_loads"].iter().any(|key| {
            raw.get(*key).and_then(|v| v.as_array()).is_some_and(|loads| {
                loads.iter().any(|l| l.get("direction").and_then(|d| d.as_str()) == Some("Centrifugal"))
            })
        });
        if has_centrifugal {
            return Err(newer("loads[].direction: Centrifugal"));
        }
    }
    if ver < 7 && has_value(&raw, "mass_source") {
        return Err(newer("mass_source"));
    }
    let has_auto_seismic = raw
        .get("load_patterns")
        .and_then(|v| v.as_array())
        .is_some_and(|patterns| patterns.iter().any(|p| is_set(p.get("auto_seismic"))));
    if ver < 8 && has_auto_seismic {
        return Err(newer("load_patterns[].auto_seismic"));
    }
    if ver < 6 {
        let has_triangle = raw.get("areas").and_then(|v| v.as_array()).is_some_and(|areas| {
            areas.iter().any(|a| a.get("joints").and_then(|j| j.as_array()).is_some_and(|j| j.len() != 4))
        });
        if has_triangle {
            return Err(newer("areas[].joints with three joints"));
        }
    }
    if ver < 5 {
        let areas = first_value(&raw, &["area_sections", "areas", "area_loads"]);
        let has_auto_mesh = raw
            .get("frames")
            .and_then(|v| v.as_array())
            .is_some_and(|frames| frames.iter().any(|f| is_set(f.get("auto_mesh"))));
        let has_envelope = raw
            .get("combinations")
            .and_then(|v| v.as_array())
            .is_some_and(|combos| combos.iter().any(|c| c.get("type").and_then(|t| t.as_str()) == Some("Envelope")));
        let has_shape = raw
            .get("sections")
            .and_then(|v| v.as_array())
            .is_some_and(|secs| secs.iter().any(|s| is_set(s.get("shape")) || is_set(s.get("neglect_shear"))));
        let has_dynamics = has_value(&raw, "response_spectra")
            || raw.get("load_cases").and_then(|v| v.as_array()).is_some_and(|cases| {
                cases.iter().any(|c| c.get("type").and_then(|t| t.as_str()) != Some("LinearStatic"))
            })
            || raw
                .get("materials")
                .and_then(|v| v.as_array())
                .is_some_and(|ms| ms.iter().any(|x| is_set(x.get("mass_density"))))
            || raw
                .get("joints")
                .and_then(|v| v.as_array())
                .is_some_and(|js| js.iter().any(|x| is_set(x.get("added_mass"))));
        if let Some(field) = areas {
            return Err(newer(field));
        }
        if has_dynamics {
            return Err(newer("response_spectra / a load case of another type / mass_density / added_mass"));
        }
        if has_auto_mesh {
            return Err(newer("frames[].auto_mesh"));
        }
        if has_envelope {
            return Err(newer("combinations with type Envelope"));
        }
        if has_shape {
            return Err(newer("sections[].shape / sections[].neglect_shear"));
        }
    }
    if ver < 4 {
        let has_v4_load = raw.get("frame_loads").and_then(|v| v.as_array()).is_some_and(|loads| {
            loads.iter().any(|l| {
                l.pointer("/load/kind")
                    .and_then(|k| k.as_str())
                    .is_some_and(|k| matches!(k, "Trapezoidal" | "PointMoment" | "TrapezoidalMoment"))
            })
        });
        if has_v4_load {
            return Err(newer("frame_loads[].load.kind"));
        }
    }
    let mut model: Model = serde_json::from_value(raw).map_err(LoadError::Parse)?;
    model.schema_version = SCHEMA_VERSION;
    Ok(model)
}

pub fn model_to_json(model: &Model) -> String {
    // Serialization of plain data structures cannot fail.
    serde_json::to_string_pretty(model).unwrap_or_default()
}

/// Indexed, validated, SI-normalized model. Indices are compact `usize`
/// positions into the model vectors.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub model: Model,
    pub original_units: crate::units::UnitSystem,
    pub joint_index: HashMap<Name, usize>,
    pub frame_index: HashMap<Name, usize>,
    /// Per frame: (joint i, joint j, section index, material index).
    pub frame_refs: Vec<(usize, usize, usize, usize)>,
    pub section_material: Vec<usize>,
    pub pattern_index: HashMap<Name, usize>,
    pub case_index: HashMap<Name, usize>,
    /// SHA-256 of the canonical JSON of the SI model.
    pub hash: String,
}

impl Snapshot {
    pub fn joint_xyz(&self, j: usize) -> [f64; 3] {
        let p = &self.model.joints[j];
        [p.x, p.y, p.z]
    }

    /// Positions of the element ends of a frame: its joints plus its offsets.
    pub fn frame_ends(&self, f: usize) -> ([f64; 3], [f64; 3]) {
        let (i, j, _, _) = self.frame_refs[f];
        let o = self.model.frames[f].offsets.unwrap_or_default();
        let (xi, xj) = (self.joint_xyz(i), self.joint_xyz(j));
        ([0, 1, 2].map(|k| xi[k] + o.i[k]), [0, 1, 2].map(|k| xj[k] + o.j[k]))
    }
}

fn issue(entity: impl Into<String>, field: &str, cause: impl Into<String>) -> Issue {
    Issue { entity: entity.into(), field: field.into(), cause: cause.into() }
}

fn index_unique<'a>(kind: &str, names: impl Iterator<Item = &'a str>, issues: &mut Vec<Issue>) -> HashMap<Name, usize> {
    let mut map = HashMap::new();
    for (k, n) in names.enumerate() {
        if n.trim().is_empty() {
            issues.push(issue(format!("{kind} #{k}"), "name", "empty name"));
        } else if map.insert(n.to_string(), k).is_some() {
            issues.push(issue(format!("{kind} {n}"), "name", "duplicate name"));
        }
    }
    map
}

fn check_positive(v: f64, entity: &str, field: &str, issues: &mut Vec<Issue>) {
    if !v.is_finite() || v <= 0.0 {
        issues.push(issue(entity, field, format!("must be finite and > 0, got {v}")));
    }
}

fn check_finite(v: f64, entity: &str, field: &str, issues: &mut Vec<Issue>) {
    if !v.is_finite() {
        issues.push(issue(entity, field, format!("must be finite, got {v}")));
    }
}

/// Replaces each parametric shape by the properties it gives. The snapshot is
/// the analytical model: a shape and its properties are the same section there.
fn derive_section_properties(m: &mut Model, issues: &mut Vec<Issue>) {
    for s in &mut m.sections {
        let e = format!("section {}", s.name);
        let Some(shape) = s.shape.take() else {
            if s.neglect_shear {
                issues.push(issue(&e, "neglect_shear", "only applies to a section with a shape"));
            }
            continue;
        };
        for (field, v) in [("area", s.area), ("i33", s.i33), ("i22", s.i22), ("j", s.j), ("as2", s.as2), ("as3", s.as3)]
        {
            if v != 0.0 {
                issues.push(issue(&e, field, "given together with a shape, which already defines it"));
            }
        }
        let dims = shape.dimensions();
        for (field, v) in &dims {
            check_positive(*v, &e, &format!("shape.{field}"), issues);
        }
        if dims.iter().any(|(_, v)| !v.is_finite() || *v <= 0.0) {
            continue;
        }
        if let Some(why) = shape.geometry_issue() {
            issues.push(issue(&e, "shape", why));
            continue;
        }
        let p = shape.properties();
        (s.area, s.i33, s.i22, s.j) = (p.area, p.i33, p.i22, p.j);
        (s.as2, s.as3) = if s.neglect_shear { (0.0, 0.0) } else { (p.as2, p.as3) };
        s.neglect_shear = false;
    }
}

fn check_mass_and_spectra(m: &Model, issues: &mut Vec<Issue>) {
    for mat in &m.materials {
        if !mat.mass_density.is_finite() || mat.mass_density < 0.0 {
            issues.push(issue(format!("material {}", mat.name), "mass_density", "must be finite and >= 0"));
        }
    }
    for j in &m.joints {
        if j.added_mass.iter().any(|v| !v.is_finite() || *v < 0.0) {
            issues.push(issue(format!("joint {}", j.name), "added_mass", "must be finite and >= 0"));
        }
    }
    if let Some(source) = &m.mass_source {
        for p in &source.patterns {
            if !m.load_patterns.iter().any(|x| x.name == p.pattern) {
                issues.push(issue("mass_source", "patterns", format!("unknown load pattern '{}'", p.pattern)));
            }
            if m.load_patterns.iter().any(|x| x.name == p.pattern && x.auto_seismic.is_some()) {
                issues.push(issue(
                    "mass_source",
                    "patterns",
                    format!("'{}' is computed from the mass, so it cannot give mass", p.pattern),
                ));
            }
            if !p.scale.is_finite() || p.scale < 0.0 {
                issues.push(issue(
                    "mass_source",
                    "patterns",
                    format!("scale of '{}' must be finite and >= 0", p.pattern),
                ));
            }
        }
    }
    let functions = index_unique("response spectrum", m.response_spectra.iter().map(|f| f.name.as_str()), issues);
    for f in &m.response_spectra {
        let e = format!("response spectrum {}", f.name);
        if f.points.is_empty() {
            issues.push(issue(&e, "points", "needs at least one point"));
        }
        if f.points.iter().flatten().any(|v| !v.is_finite()) || f.points.iter().any(|p| p[0] < 0.0) {
            issues.push(issue(&e, "points", "periods must be >= 0, and all values finite"));
        }
        if f.points.windows(2).any(|w| w[1][0] <= w[0][0]) {
            issues.push(issue(&e, "points", "periods must be strictly increasing"));
        }
    }
    for c in &m.load_cases {
        match c {
            LoadCase::LinearStatic { .. } => {}
            LoadCase::Modal { name, modes } => {
                if *modes == 0 {
                    issues.push(issue(format!("load case {name}"), "modes", "must be at least 1"));
                }
            }
            LoadCase::ResponseSpectrum { name, modal_case, function, scale, damping, .. } => {
                let e = format!("load case {name}");
                if !m.load_cases.iter().any(|o| o.name() == modal_case && matches!(o, LoadCase::Modal { .. })) {
                    issues.push(issue(&e, "modal_case", format!("'{modal_case}' is not a modal case")));
                }
                if !functions.contains_key(function) {
                    issues.push(issue(&e, "function", format!("unknown response spectrum '{function}'")));
                }
                check_finite(*scale, &e, "scale", issues);
                if !(0.0..1.0).contains(damping) {
                    issues.push(issue(&e, "damping", format!("must be in [0, 1), got {damping}")));
                }
            }
        }
    }
    // Neither modes nor a spectrum, which has no sign, can enter a combination.
    for combo in &m.combinations {
        for t in combo.terms() {
            if m.load_cases.iter().any(|c| c.name() == t.case && !matches!(c, LoadCase::LinearStatic { .. })) {
                issues.push(issue(
                    format!("combination {}", combo.name()),
                    "cases",
                    format!("'{}' is not a static case: combining it is not supported yet", t.case),
                ));
            }
        }
    }
}

/// Largest out-of-plane distance of a corner allowed in a flat element, as a
/// fraction of its longer diagonal.
pub const AREA_WARP_TOL_REL: f64 = 1e-3;

/// Global axis perpendicular to the plane of `points`, or `None` when they are
/// not parallel to a global axis plane (decision 0033).
///
/// A plane area carries two degrees of freedom per node, so the analysis has to
/// hold the other four. That is only a fixed set of components when the plane
/// is one of the three coordinate planes; a tilted one would need a kinematic
/// constraint per joint instead. Exactly one coordinate has to stay constant:
/// two would mean the points are a line, which is not a plane.
pub fn plane_axis(points: &[[f64; 3]]) -> Option<usize> {
    if points.len() < 2 {
        return None;
    }
    let mut range = [0.0; 3];
    for a in 0..3 {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for p in points {
            min = min.min(p[a]);
            max = max.max(p[a]);
        }
        range[a] = max - min;
    }
    let extent = range.iter().copied().fold(0.0, f64::max);
    if !extent.is_finite() || extent <= 0.0 {
        return None;
    }
    let flat: Vec<usize> = (0..3).filter(|&a| range[a] <= 1e-9 * extent).collect();
    match flat.as_slice() {
        [axis] => Some(*axis),
        _ => None,
    }
}

/// Is the section of an area one of the two whose element carries two degrees
/// of freedom per node? The analysis holds the other four of their joints
/// (decisions 0033 and 0034).
fn is_2d_section(behavior: Option<AreaBehavior>) -> bool {
    matches!(behavior, Some(AreaBehavior::Plane | AreaBehavior::Axisymmetric))
}

/// Geometry an axisymmetric section has to have (decision 0034): four joints,
/// a plane that contains the z axis, and a radius of at least zero. Returns
/// whether the area can be registered as one of the two-degree sections.
fn axisymmetric_geometry(e: &str, axis: usize, pts: &[[f64; 3]], issues: &mut Vec<Issue>) -> bool {
    let mut ok = true;
    if pts.len() != 4 {
        issues.push(issue(
            e,
            "joints",
            "an axisymmetric area has four joints: its element is the quadrilateral (decision 0034)",
        ));
        ok = false;
    }
    if axis > 1 {
        issues.push(issue(
            e,
            "joints",
            "an axisymmetric area stands in a plane that contains the z axis: x = 0 or y = 0, not a horizontal plane (decision 0034)",
        ));
        return false;
    }
    let scale = pts.iter().flat_map(|p| p.iter().map(|v| v.abs())).fold(1.0, f64::max);
    let tol = 1e-9 * scale;
    if pts.iter().any(|p| p[axis].abs() > tol) {
        issues.push(issue(
            e,
            "joints",
            "the plane of an axisymmetric area has to pass through the z axis: its constant coordinate is 0 (decision 0034)",
        ));
        ok = false;
    }
    let radius = 1 - axis;
    if pts.iter().any(|p| p[radius] < -tol) {
        issues.push(issue(
            e,
            "joints",
            "the radius of an axisymmetric area has to be >= 0: it is measured from the z axis (decision 0034)",
        ));
        ok = false;
    }
    // The element integrates with the sign of the Jacobian of its (r, z)
    // coordinates, and its axes are fixed by the revolution, so the joint
    // order is the only thing that can flip it (decision 0034).
    if pts.len() == 4 {
        let signed: f64 = (0..4)
            .map(|i| {
                let j = (i + 1) % 4;
                pts[i][radius] * pts[j][2] - pts[j][radius] * pts[i][2]
            })
            .sum();
        if signed < 0.0 {
            issues.push(issue(
                e,
                "joints",
                "the joints of an axisymmetric area run clockwise in its (r, z) section: they have to go counterclockwise (decision 0034)",
            ));
            ok = false;
        }
    }
    ok
}

fn check_areas(
    m: &Model,
    materials: &HashMap<Name, usize>,
    joint_index: &HashMap<Name, usize>,
    connected: &mut HashSet<usize>,
    plane_axis_of: &mut HashMap<usize, usize>,
    issues: &mut Vec<Issue>,
) {
    let sections = index_unique("area section", m.area_sections.iter().map(|s| s.name.as_str()), issues);
    let area_index = index_unique("area", m.areas.iter().map(|a| a.name.as_str()), issues);
    let behavior: HashMap<&str, AreaBehavior> = m.area_sections.iter().map(|s| (s.name.as_str(), s.behavior)).collect();
    for s in &m.area_sections {
        let e = format!("area section {}", s.name);
        check_positive(s.thickness, &e, "thickness", issues);
        if let Some(t) = s.bending_thickness {
            check_positive(t, &e, "bending_thickness", issues);
        }
        if !materials.contains_key(&s.material) {
            issues.push(issue(&e, "material", format!("unknown material '{}'", s.material)));
        }
        if matches!(s.behavior, AreaBehavior::Plane | AreaBehavior::Axisymmetric) {
            // A plane or axisymmetric section has no plate: the element takes
            // E, nu and t (decisions 0033 and 0034).
            if s.bending_thickness.is_some() {
                issues.push(issue(&e, "bending_thickness", "a plane section has no bending thickness"));
            }
            if !s.layers.is_empty() {
                issues.push(issue(&e, "layers", "a plane section has no layers"));
            }
        }
        if !s.layers.is_empty() {
            if s.bending_thickness.is_some() {
                issues.push(issue(
                    &e,
                    "bending_thickness",
                    "a laminated section carries its bending stiffness in the layers: bending_thickness does not apply",
                ));
            }
            let mut total = 0.0;
            for layer in &s.layers {
                total += layer.thickness;
                if !layer.thickness.is_finite() || layer.thickness <= 0.0 {
                    issues.push(issue(
                        &e,
                        "layers",
                        format!("layer '{}': thickness must be positive and finite", layer.material),
                    ));
                }
                if !materials.contains_key(&layer.material) {
                    issues.push(issue(&e, "layers", format!("unknown material '{}'", layer.material)));
                }
            }
            if (total - s.thickness).abs() > 1e-9 * s.thickness.abs().max(1.0) {
                issues.push(issue(
                    &e,
                    "layers",
                    format!("the layers add up to {total}, not to the thickness {}", s.thickness),
                ));
            }
            // The flat element has no bending-extension coupling, so a stack
            // whose `b` is not zero is refused instead of being solved as if it
            // were zero (decision 0032).
            let stiffness: HashMap<Name, (f64, f64)> =
                m.materials.iter().map(|x| (x.name.clone(), (x.e, x.nu))).collect();
            if let Ok(abc) = laminate_abc(&s.layers, &stiffness) {
                let scale = abc.a[0][0].abs().max(1.0) * s.thickness;
                if abc.b.iter().flatten().any(|v| v.abs() > 1e-9 * scale) {
                    issues.push(issue(
                        &e,
                        "layers",
                        "the stack couples bending and extension (b is not zero), which the shell element does not support yet",
                    ));
                }
            }
        }
    }
    for a in &m.areas {
        let e = format!("area {}", a.name);
        if !sections.contains_key(&a.section) {
            issues.push(issue(&e, "section", format!("unknown area section '{}'", a.section)));
        }
        if let Some(mesh) = a.auto_mesh {
            // A triangle has one division count, on every edge.
            let (m, n) = (mesh.m.max(1), mesh.n.max(1));
            if mesh.m < 1 || mesh.n < 1 {
                issues.push(issue(&e, "auto_mesh", "m and n must be >= 1"));
            }
            if a.joints.len() == 3 && m != n {
                issues.push(issue(&e, "auto_mesh", "a triangle uses one division count: m must equal n"));
            }
        }
        if !matches!(a.joints.len(), 3 | 4) {
            issues.push(issue(&e, "joints", format!("an area has 3 or 4 joints, not {}", a.joints.len())));
            continue;
        }
        let mut found = Vec::with_capacity(4);
        for name in &a.joints {
            match joint_index.get(name) {
                Some(&k) => {
                    connected.insert(k);
                    found.push(k);
                }
                None => issues.push(issue(&e, "joints", format!("unknown joint '{name}'"))),
            }
        }
        if found.len() < a.joints.len() {
            continue;
        }
        let at = |k: usize| {
            let j = &m.joints[found[k]];
            [j.x, j.y, j.z]
        };
        let behavior_of = behavior.get(a.section.as_str()).copied();
        if is_2d_section(behavior_of) {
            // A plane or axisymmetric area is a 2D idealisation: two DOF per
            // node, with the other four held by the analysis (decisions 0033
            // and 0034). Its plane has to be parallel to a global axis so that
            // the held components are a fixed set, and two of them that share
            // a joint have to lie in the same plane, or that joint would be
            // held twice over.
            let pts: Vec<[f64; 3]> = (0..found.len()).map(&at).collect();
            match plane_axis(&pts) {
                None => issues.push(issue(
                    &e,
                    "joints",
                    "a plane area must be parallel to a global axis plane: one of x, y, z constant over its joints",
                )),
                Some(axis) => {
                    let usable = behavior_of != Some(AreaBehavior::Axisymmetric)
                        || axisymmetric_geometry(&e, axis, &pts, issues);
                    if usable {
                        let other = found.iter().find(|&&k| plane_axis_of.get(&k).is_some_and(|&a| a != axis));
                        if other.is_some() {
                            issues.push(issue(
                                &e,
                                "joints",
                                "a plane area shares a joint with a plane area in a different plane (decisions 0033 and 0034)",
                            ));
                        } else {
                            for &k in &found {
                                plane_axis_of.entry(k).or_insert(axis);
                            }
                        }
                    }
                }
            }
        }
        let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let cross = |a: [f64; 3], b: [f64; 3]| {
            [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
        };
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        if found.len() == 3 {
            // Three points are always flat and convex: only collinear ones fail.
            let (e1, e2, e3) = (sub(at(1), at(0)), sub(at(2), at(0)), sub(at(2), at(1)));
            let normal = cross(e1, e2);
            let longest = dot(e1, e1).max(dot(e2, e2)).max(dot(e3, e3));
            if dot(normal, normal).sqrt() <= 1e-12 * longest {
                issues.push(issue(&e, "joints", "degenerate area: its joints are collinear"));
            }
            continue;
        }
        let (d1, d2) = (sub(at(2), at(0)), sub(at(3), at(1)));
        let normal = cross(d1, d2);
        let size = dot(normal, normal).sqrt();
        if size == 0.0 {
            issues.push(issue(&e, "joints", "degenerate area: its diagonals are parallel"));
            continue;
        }
        let n = normal.map(|c| c / size);
        // Convex with this order: every corner turns the same way as the normal.
        let concave = (0..4).any(|k| {
            let (prev, here, next) = (at((k + 3) % 4), at(k), at((k + 1) % 4));
            dot(cross(sub(here, prev), sub(next, here)), n) <= 0.0
        });
        if concave {
            issues.push(issue(&e, "joints", "the joints do not make a convex area in this order"));
        }
        let longer = dot(d1, d1).max(dot(d2, d2)).sqrt();
        let warp = (dot(sub(at(1), at(0)), n)).abs() / 2.0;
        if warp > AREA_WARP_TOL_REL * longer {
            issues.push(issue(
                &e,
                "joints",
                format!("warped: corners {warp:e} m off the mean plane, and flat elements cannot represent it"),
            ));
        }
    }
    if !plane_axis_of.is_empty() {
        // The analysis holds the out-of-plane DOF of a plane area's joints, so
        // sharing them with a 3D element would hold that element too.
        for f in &m.frames {
            let ends = [joint_index.get(f.i.as_str()), joint_index.get(f.j.as_str())];
            if ends.iter().any(|k| k.is_some_and(|k| plane_axis_of.contains_key(k))) {
                issues.push(issue(
                    format!("frame {}", f.name),
                    "joints",
                    "a plane area shares a joint with a frame: a plane area is a 2D idealisation and cannot mix with 3D elements (decision 0033)",
                ));
            }
        }
        for a in &m.areas {
            if is_2d_section(behavior.get(a.section.as_str()).copied()) {
                continue;
            }
            for name in &a.joints {
                if joint_index.get(name).is_some_and(|&k| plane_axis_of.contains_key(&k)) {
                    issues.push(issue(
                        format!("area {}", a.name),
                        "joints",
                        "a plane area shares a joint with a 3D area (decision 0033)",
                    ));
                }
            }
        }
    }
    for (k, l) in m.area_loads.iter().enumerate() {
        let e = format!("area load #{k}");
        if !m.load_patterns.iter().any(|p| p.name == l.pattern) {
            issues.push(issue(&e, "pattern", format!("unknown load pattern '{}'", l.pattern)));
        }
        if !area_index.contains_key(&l.area) {
            issues.push(issue(&e, "area", format!("unknown area '{}'", l.area)));
        }
        check_finite(l.value, &e, "value", issues);
        let subject = m
            .areas
            .iter()
            .find(|a| a.name == l.area)
            .filter(|a| is_2d_section(behavior.get(a.section.as_str()).copied()));
        if l.value != 0.0 {
            // A load out of the plane of a plane or axisymmetric area reaches
            // an element with no stiffness there: it would be dropped without
            // a word (decisions 0033 and 0034).
            if let Some(a) = subject {
                let axis = a.joints.first().and_then(|n| joint_index.get(n)).and_then(|&k| plane_axis_of.get(&k));
                let out_of_plane = match l.direction {
                    LoadDirection::Local3 => true,
                    LoadDirection::Local1 | LoadDirection::Local2 => false,
                    LoadDirection::GlobalX => axis == Some(&0),
                    LoadDirection::GlobalY => axis == Some(&1),
                    LoadDirection::GlobalZ | LoadDirection::Gravity => axis == Some(&2),
                    // Radial by construction, so it is always in the plane.
                    LoadDirection::Centrifugal => false,
                };
                if out_of_plane {
                    issues.push(issue(
                        &e,
                        "direction",
                        "out of the plane of a plane or axisymmetric area: the load would not be carried (decisions 0033 and 0034)",
                    ));
                }
            }
        }
        // The centrifugal body force needs a radius and a density (0034).
        if l.direction == LoadDirection::Centrifugal {
            let section = m
                .areas
                .iter()
                .find(|a| a.name == l.area)
                .and_then(|a| m.area_sections.iter().find(|s| s.name == a.section));
            let axisym = section.is_some_and(|s| s.behavior == AreaBehavior::Axisymmetric);
            if !axisym {
                issues.push(issue(
                    &e,
                    "direction",
                    "a centrifugal load needs an area with an axisymmetric section (decision 0034)",
                ));
            } else if l.value < 0.0 {
                issues.push(issue(&e, "value", "the angular velocity must be >= 0"));
            } else {
                let density =
                    section.and_then(|s| materials.get(&s.material)).map_or(0.0, |&i| m.materials[i].mass_density);
                if density <= 0.0 {
                    issues.push(issue(
                        &e,
                        "value",
                        "the material of this area has no mass_density, which the centrifugal load needs (decision 0034)",
                    ));
                }
            }
        }
    }
    // Weight of a plane area in a horizontal plane: gravity is out of that
    // plane, so the element would carry none of it (decision 0033).
    if m.load_patterns.iter().any(|p| p.self_weight_multiplier != 0.0) {
        for a in &m.areas {
            if behavior.get(a.section.as_str()) != Some(&AreaBehavior::Plane) {
                continue;
            }
            let Some(&k) = a.joints.first().and_then(|n| joint_index.get(n)) else { continue };
            if plane_axis_of.get(&k) != Some(&2) {
                continue;
            }
            let sec = m.area_sections.iter().find(|s| s.name == a.section);
            let weight = sec.and_then(|s| materials.get(&s.material)).map_or(0.0, |&i| m.materials[i].unit_weight);
            if weight > 0.0 {
                issues.push(issue(
                    format!("area {}", a.name),
                    "section",
                    "its weight acts out of the plane of a plane area and would not be carried: use a shell or plate section",
                ));
            }
        }
    }
}

/// Loads, imposed displacements and springs on a component the analysis holds
/// for a plane or axisymmetric area (decisions 0033 and 0034): they would be
/// dropped without a word, because the element has no stiffness there.
fn check_plane_dofs(
    m: &Model,
    joint_index: &HashMap<Name, usize>,
    plane_axis_of: &HashMap<usize, usize>,
    issues: &mut Vec<Issue>,
) {
    let held = |j: usize| -> Vec<usize> {
        match plane_axis_of.get(&j) {
            Some(&axis) => vec![axis, 3, 4, 5],
            None => Vec::new(),
        }
    };
    for (k, jl) in m.joint_loads.iter().enumerate() {
        let Some(&j) = joint_index.get(&jl.joint) else { continue };
        for d in held(j) {
            if jl.values[d] != 0.0 {
                issues.push(issue(
                    format!("joint load #{k}"),
                    "values",
                    format!(
                        "{} of joint '{}' is out of the plane of a plane or axisymmetric area: the load would not be carried (decisions 0033 and 0034)",
                        DOF_NAMES[d], jl.joint
                    ),
                ));
            }
        }
    }
    for (k, jd) in m.joint_displacements.iter().enumerate() {
        let Some(&j) = joint_index.get(&jd.joint) else { continue };
        for d in held(j) {
            if jd.values[d] != 0.0 {
                issues.push(issue(
                    format!("joint displacement #{k}"),
                    "values",
                    format!(
                        "{} of joint '{}' is out of the plane of a plane or axisymmetric area: the displacement would not be imposed (decisions 0033 and 0034)",
                        DOF_NAMES[d], jd.joint
                    ),
                ));
            }
        }
    }
    for (j, joint) in m.joints.iter().enumerate() {
        for d in held(j) {
            if joint.springs[d] > 0.0 {
                issues.push(issue(
                    format!("joint {}", joint.name),
                    &format!("springs[{}]", DOF_NAMES[d]),
                    "out of the plane of a plane or axisymmetric area: the spring would not be used (decisions 0033 and 0034)",
                ));
            }
        }
    }
}

pub fn validate(input: &Model) -> Result<Snapshot, Vec<Issue>> {
    let mut issues = Vec::new();
    if input.schema_version != SCHEMA_VERSION {
        issues.push(issue("model", "schema_version", format!("expected {SCHEMA_VERSION}")));
    }
    let mut m = input.to_si();
    derive_section_properties(&mut m, &mut issues);

    let mat_idx = index_unique("material", m.materials.iter().map(|x| x.name.as_str()), &mut issues);
    let sec_idx = index_unique("section", m.sections.iter().map(|x| x.name.as_str()), &mut issues);
    let joint_index = index_unique("joint", m.joints.iter().map(|x| x.name.as_str()), &mut issues);
    let frame_index = index_unique("frame", m.frames.iter().map(|x| x.name.as_str()), &mut issues);
    let pattern_index = index_unique("load pattern", m.load_patterns.iter().map(|x| x.name.as_str()), &mut issues);
    // Cases and combinations share one namespace for result lookup.
    let case_index = index_unique(
        "load case/combination",
        m.load_cases.iter().map(|c| c.name()).chain(m.combinations.iter().map(|c| c.name())),
        &mut issues,
    );

    for mat in &m.materials {
        let e = format!("material {}", mat.name);
        check_positive(mat.e, &e, "e", &mut issues);
        if !mat.nu.is_finite() || mat.nu <= -1.0 || mat.nu >= 0.5 {
            issues.push(issue(&e, "nu", format!("must be in (-1, 0.5), got {}", mat.nu)));
        }
        if let Some(g) = mat.g {
            check_positive(g, &e, "g", &mut issues);
        }
        if !mat.unit_weight.is_finite() || mat.unit_weight < 0.0 {
            issues.push(issue(&e, "unit_weight", "must be finite and >= 0"));
        }
    }

    let mut section_material = Vec::with_capacity(m.sections.len());
    for s in &m.sections {
        let e = format!("section {}", s.name);
        check_positive(s.area, &e, "area", &mut issues);
        check_positive(s.i33, &e, "i33", &mut issues);
        check_positive(s.i22, &e, "i22", &mut issues);
        check_positive(s.j, &e, "j", &mut issues);
        for (v, f) in [(s.as2, "as2"), (s.as3, "as3")] {
            if !v.is_finite() || v < 0.0 {
                issues.push(issue(&e, f, format!("must be finite and >= 0 (0 = no shear deformation), got {v}")));
            }
        }
        match mat_idx.get(&s.material) {
            Some(&k) => section_material.push(k),
            None => {
                issues.push(issue(&e, "material", format!("unknown material '{}'", s.material)));
                section_material.push(usize::MAX);
            }
        }
    }

    let mut scale = 0.0_f64;
    for j in &m.joints {
        let e = format!("joint {}", j.name);
        check_finite(j.x, &e, "x", &mut issues);
        check_finite(j.y, &e, "y", &mut issues);
        check_finite(j.z, &e, "z", &mut issues);
        scale = scale.max(j.x.abs()).max(j.y.abs()).max(j.z.abs());
        for (d, dof) in DOF_NAMES.iter().enumerate() {
            let k = j.springs[d];
            let field = format!("springs[{dof}]");
            if !k.is_finite() || k < 0.0 {
                issues.push(issue(&e, &field, format!("must be finite and >= 0, got {k}")));
            } else if k > 0.0 && j.restraint[d] {
                issues.push(issue(&e, &field, "spring on a restrained component"));
            }
        }
    }
    let len_tol = LENGTH_TOL_REL * scale.max(1.0);

    let mut frame_refs = Vec::with_capacity(m.frames.len());
    let mut connected: HashSet<usize> = HashSet::new();
    for f in &m.frames {
        let e = format!("frame {}", f.name);
        let ji = joint_index.get(&f.i).copied();
        let jj = joint_index.get(&f.j).copied();
        let sk = sec_idx.get(&f.section).copied();
        if ji.is_none() {
            issues.push(issue(&e, "i", format!("unknown joint '{}'", f.i)));
        }
        if jj.is_none() {
            issues.push(issue(&e, "j", format!("unknown joint '{}'", f.j)));
        }
        if sk.is_none() {
            issues.push(issue(&e, "section", format!("unknown section '{}'", f.section)));
        }
        check_finite(f.angle_deg, &e, "angle_deg", &mut issues);
        if let Some(r) = &f.releases {
            for (end, rel, springs) in [("i", &r.i, &r.i_springs), ("j", &r.j, &r.j_springs)] {
                for c in 0..6 {
                    let k = springs[c];
                    let field = format!("releases.{end}_springs[{}]", RELEASE_COMPONENTS[c]);
                    if !k.is_finite() || k < 0.0 {
                        issues.push(issue(&e, &field, format!("must be finite and >= 0, got {k}")));
                    } else if k > 0.0 && !rel[c] {
                        issues.push(issue(&e, &field, "partial-fixity spring on a component that is not released"));
                    }
                }
            }
            for why in r.unstable_combinations() {
                issues.push(issue(&e, "releases", format!("unstable release combination: {why}")));
            }
        }
        if f.auto_mesh.is_some_and(|a| a.segments == 0) {
            issues.push(issue(&e, "auto_mesh.segments", "must be at least 1"));
        }
        let offsets = f.offsets.unwrap_or_default();
        for (end, v) in [("i", offsets.i), ("j", offsets.j)] {
            for c in v {
                check_finite(c, &e, &format!("offsets.{end}"), &mut issues);
            }
        }
        if let (Some(a), Some(b)) = (ji, jj) {
            let (pa, pb) = (&m.joints[a], &m.joints[b]);
            let d = [
                pb.x + offsets.j[0] - pa.x - offsets.i[0],
                pb.y + offsets.j[1] - pa.y - offsets.i[1],
                pb.z + offsets.j[2] - pa.z - offsets.i[2],
            ];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if a == b || len <= len_tol {
                issues.push(issue(&e, "i/j", format!("zero length between element ends (L = {len:e} m)")));
            }
            connected.insert(a);
            connected.insert(b);
        }
        let mk = sk.and_then(|s| section_material.get(s).copied()).unwrap_or(usize::MAX);
        frame_refs.push((ji.unwrap_or(usize::MAX), jj.unwrap_or(usize::MAX), sk.unwrap_or(usize::MAX), mk));
    }

    let mut plane_axis_of: HashMap<usize, usize> = HashMap::new();
    check_areas(&m, &mat_idx, &joint_index, &mut connected, &mut plane_axis_of, &mut issues);
    check_plane_dofs(&m, &joint_index, &plane_axis_of, &mut issues);
    index_unique("constraint", m.constraints.iter().map(|c| c.name()), &mut issues);
    let mut constrained: HashMap<usize, &str> = HashMap::new();
    for c in &m.constraints {
        let e = format!("constraint {}", c.name());
        if c.joints().len() < 2 {
            issues.push(issue(&e, "joints", "a constraint needs at least 2 joints"));
        }
        let tied = c.tied_components();
        if !tied.iter().any(|&t| t) {
            issues.push(issue(&e, "components", "no component selected"));
        }
        for name in c.joints() {
            let Some(&k) = joint_index.get(name) else {
                issues.push(issue(&e, "joints", format!("unknown joint '{name}'")));
                continue;
            };
            if let Some(other) = constrained.insert(k, c.name()) {
                issues.push(issue(&e, "joints", format!("joint '{name}' is already in constraint '{other}'")));
            }
            connected.insert(k);
            let held: Vec<&str> =
                (0..6).filter(|&d| tied[d] && m.joints[k].restraint[d]).map(|d| DOF_NAMES[d]).collect();
            if !held.is_empty() {
                issues.push(issue(
                    &e,
                    "joints",
                    format!("joint '{name}' restrains {} tied by the constraint; not supported", held.join(", ")),
                ));
            }
        }
    }

    // A frame split at joints can connect any joint on its axis. That is known
    // only once the analysis mesh exists, and its validation applies this rule.
    let joints_may_connect_later = m.frames.iter().any(|f| f.auto_mesh.is_some_and(|a| a.at_joints));
    for (k, j) in m.joints.iter().enumerate() {
        if j.springs.iter().any(|&s| s > 0.0) {
            connected.insert(k);
        }
        if !connected.contains(&k) && !j.restraint.iter().all(|&r| r) && !joints_may_connect_later {
            issues.push(issue(
                format!("joint {}", j.name),
                "connectivity",
                "joint is not connected to any element and is not fully restrained",
            ));
        }
    }

    for p in &m.load_patterns {
        check_finite(
            p.self_weight_multiplier,
            &format!("load pattern {}", p.name),
            "self_weight_multiplier",
            &mut issues,
        );
        if let Some(a) = &p.auto_seismic {
            let e = format!("load pattern {}", p.name);
            if a.direction == GlobalAxis::Z {
                issues.push(issue(&e, "auto_seismic.direction", "must be X or Y"));
            }
            for (v, field) in [(a.coefficient, "auto_seismic.coefficient"), (a.exponent, "auto_seismic.exponent")] {
                if !v.is_finite() || v < 0.0 {
                    issues.push(issue(&e, field, "must be finite and >= 0"));
                }
            }
        }
    }
    for (k, jl) in m.joint_loads.iter().enumerate() {
        let e = format!("joint load #{k}");
        if !pattern_index.contains_key(&jl.pattern) {
            issues.push(issue(&e, "pattern", format!("unknown load pattern '{}'", jl.pattern)));
        }
        if !joint_index.contains_key(&jl.joint) {
            issues.push(issue(&e, "joint", format!("unknown joint '{}'", jl.joint)));
        }
        for v in jl.values {
            check_finite(v, &e, "values", &mut issues);
        }
    }
    for (k, jd) in m.joint_displacements.iter().enumerate() {
        let e = format!("joint displacement #{k}");
        if !pattern_index.contains_key(&jd.pattern) {
            issues.push(issue(&e, "pattern", format!("unknown load pattern '{}'", jd.pattern)));
        }
        for v in jd.values {
            check_finite(v, &e, "values", &mut issues);
        }
        let Some(&j) = joint_index.get(&jd.joint) else {
            issues.push(issue(&e, "joint", format!("unknown joint '{}'", jd.joint)));
            continue;
        };
        let joint = &m.joints[j];
        for (d, dof) in DOF_NAMES.iter().enumerate() {
            if jd.values[d] != 0.0 && !joint.restraint[d] && joint.springs[d] <= 0.0 {
                issues.push(issue(
                    &e,
                    "values",
                    format!("{dof} of joint '{}' has neither a restraint nor a spring", jd.joint),
                ));
            }
        }
    }
    for (k, fl) in m.frame_loads.iter().enumerate() {
        let e = format!("frame load #{k}");
        if !pattern_index.contains_key(&fl.pattern) {
            issues.push(issue(&e, "pattern", format!("unknown load pattern '{}'", fl.pattern)));
        }
        if !frame_index.contains_key(&fl.frame) {
            issues.push(issue(&e, "frame", format!("unknown frame '{}'", fl.frame)));
        }
        // Every kind carries a direction, and none of them can be the
        // centrifugal body force of a revolution (decision 0034).
        let dir = match fl.load {
            FrameLoadKind::Uniform { direction, .. }
            | FrameLoadKind::Point { direction, .. }
            | FrameLoadKind::Trapezoidal { direction, .. }
            | FrameLoadKind::PointMoment { direction, .. }
            | FrameLoadKind::TrapezoidalMoment { direction, .. } => direction,
        };
        if dir == LoadDirection::Centrifugal {
            issues.push(issue(
                &e,
                "direction",
                "a centrifugal load needs an area with an axisymmetric section: a bar cannot take one (decision 0034)",
            ));
        }
        match fl.load {
            FrameLoadKind::Uniform { value, .. } => check_finite(value, &e, "value", &mut issues),
            FrameLoadKind::Point { rel, value, .. } => {
                check_finite(value, &e, "value", &mut issues);
                if !(0.0..=1.0).contains(&rel) {
                    issues.push(issue(&e, "rel", format!("must be in [0, 1], got {rel}")));
                }
            }
            FrameLoadKind::PointMoment { direction, rel, value } => {
                check_finite(value, &e, "value", &mut issues);
                if !(0.0..=1.0).contains(&rel) {
                    issues.push(issue(&e, "rel", format!("must be in [0, 1], got {rel}")));
                }
                if direction == LoadDirection::Gravity {
                    issues.push(issue(&e, "direction", "a moment cannot act about Gravity"));
                }
            }
            FrameLoadKind::Trapezoidal { direction, rel_start, rel_end, value_start, value_end }
            | FrameLoadKind::TrapezoidalMoment { direction, rel_start, rel_end, value_start, value_end } => {
                check_finite(value_start, &e, "value_start", &mut issues);
                check_finite(value_end, &e, "value_end", &mut issues);
                // Also rejects NaN, for which every comparison is false.
                if !(rel_start >= 0.0 && rel_start < rel_end && rel_end <= 1.0) {
                    issues.push(issue(
                        &e,
                        "rel_start/rel_end",
                        format!("need 0 <= rel_start < rel_end <= 1, got {rel_start} and {rel_end}"),
                    ));
                }
                if matches!(fl.load, FrameLoadKind::TrapezoidalMoment { .. }) && direction == LoadDirection::Gravity {
                    issues.push(issue(&e, "direction", "a moment cannot act about Gravity"));
                }
            }
        }
    }
    check_mass_and_spectra(&m, &mut issues);
    for c in &m.load_cases {
        let LoadCase::LinearStatic { name, loads } = c else { continue };
        for sp in loads {
            if !pattern_index.contains_key(&sp.pattern) {
                issues.push(issue(
                    format!("load case {name}"),
                    "loads",
                    format!("unknown load pattern '{}'", sp.pattern),
                ));
            }
            check_finite(sp.scale, &format!("load case {name}"), "scale", &mut issues);
        }
    }
    check_combination_graph(&m, &mut issues);

    if !issues.is_empty() {
        return Err(issues);
    }
    let hash = hex(&Sha256::digest(model_to_json(&m).as_bytes()));
    Ok(Snapshot {
        original_units: input.units,
        model: m,
        joint_index,
        frame_index,
        frame_refs,
        section_material,
        pattern_index,
        case_index,
        hash,
    })
}

/// Combinations may reference cases or other combinations; cycles are rejected.
fn check_combination_graph(m: &Model, issues: &mut Vec<Issue>) {
    let cases: HashSet<&str> = m.load_cases.iter().map(|c| c.name()).collect();
    let combos: HashMap<&str, &Combination> = m.combinations.iter().map(|c| (c.name(), c)).collect();
    for c in &m.combinations {
        let name = c.name();
        for t in c.terms() {
            if !cases.contains(t.case.as_str()) && !combos.contains_key(t.case.as_str()) {
                issues.push(issue(format!("combination {name}"), "cases", format!("unknown case '{}'", t.case)));
            }
            if matches!(combos.get(t.case.as_str()), Some(Combination::Envelope { .. })) {
                issues.push(issue(
                    format!("combination {name}"),
                    "cases",
                    format!("'{}' is an envelope and cannot be a term", t.case),
                ));
            }
            check_finite(t.scale, &format!("combination {name}"), "scale", issues);
        }
    }
    // Depth-first cycle detection.
    fn visit<'a>(n: &'a str, combos: &HashMap<&'a str, &'a Combination>, state: &mut HashMap<&'a str, u8>) -> bool {
        match state.get(n) {
            Some(1) => return true,
            Some(2) => return false,
            _ => {}
        }
        state.insert(n, 1);
        if let Some(c) = combos.get(n) {
            for t in c.terms() {
                if combos.contains_key(t.case.as_str()) && visit(t.case.as_str(), combos, state) {
                    return true;
                }
            }
        }
        state.insert(n, 2);
        false
    }
    let mut state = HashMap::new();
    for c in &m.combinations {
        if visit(c.name(), &combos, &mut state) {
            issues.push(issue(format!("combination {}", c.name()), "cases", "cyclic reference"));
            break;
        }
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::laminate::Layer;
    use crate::units::{ForceUnit, LengthUnit, UnitSystem};

    fn cantilever() -> Model {
        Model {
            schema_version: SCHEMA_VERSION,
            title: "t".into(),
            units: UnitSystem::default(),
            materials: vec![Material {
                name: "S".into(),
                e: 200e9,
                nu: 0.3,
                g: None,
                unit_weight: 0.0,
                mass_density: 0.0,
            }],
            sections: vec![FrameSection {
                name: "B".into(),
                material: "S".into(),
                shape: None,
                neglect_shear: false,
                area: 0.01,
                i33: 1e-4,
                i22: 1e-4,
                j: 2e-4,
                as2: 0.0,
                as3: 0.0,
            }],
            joints: vec![
                Joint {
                    name: "1".into(),
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    restraint: [true; 6],
                    springs: [0.0; 6],
                    added_mass: [0.0; 6],
                },
                Joint {
                    name: "2".into(),
                    x: 3.0,
                    y: 0.0,
                    z: 0.0,
                    restraint: [false; 6],
                    springs: [0.0; 6],
                    added_mass: [0.0; 6],
                },
            ],
            frames: vec![Frame {
                name: "F1".into(),
                i: "1".into(),
                j: "2".into(),
                section: "B".into(),
                angle_deg: 0.0,
                releases: None,
                offsets: None,
                auto_mesh: None,
            }],
            area_sections: vec![],
            areas: vec![],
            area_loads: vec![],
            response_spectra: vec![],
            mass_source: None,
            constraints: vec![],
            load_patterns: vec![LoadPattern { name: "P".into(), self_weight_multiplier: 0.0, auto_seismic: None }],
            joint_loads: vec![JointLoad {
                pattern: "P".into(),
                joint: "2".into(),
                values: [0.0, 0.0, -1e4, 0.0, 0.0, 0.0],
            }],
            joint_displacements: vec![],
            frame_loads: vec![],
            load_cases: vec![LoadCase::LinearStatic {
                name: "LC".into(),
                loads: vec![ScaledPattern { pattern: "P".into(), scale: 1.0 }],
            }],
            combinations: vec![],
        }
    }

    /// A serialized model as a file written before schema v3 would have it.
    fn strip_v3_fields(v: &mut serde_json::Value) {
        let o = v.as_object_mut().expect("obj");
        o.remove("constraints");
        o.remove("joint_displacements");
        for f in v["frames"].as_array_mut().expect("frames") {
            f.as_object_mut().expect("obj").remove("offsets");
        }
        for j in v["joints"].as_array_mut().expect("joints") {
            j.as_object_mut().expect("obj").remove("springs");
        }
    }

    #[test]
    fn valid_model_round_trips() {
        let m = cantilever();
        let text = model_to_json(&m);
        let back = model_from_json(&text).expect("parse");
        assert_eq!(m, back);
        assert!(validate(&back).is_ok());
    }

    #[test]
    fn v1_file_migrates_without_shear_deformation() {
        let mut v: serde_json::Value = serde_json::from_str(&model_to_json(&cantilever())).expect("json");
        v["schema_version"] = 1.into();
        strip_v3_fields(&mut v);
        for s in v["sections"].as_array_mut().expect("sections") {
            let o = s.as_object_mut().expect("obj");
            o.remove("as2");
            o.remove("as3");
        }
        let m = model_from_json(&v.to_string()).expect("migrates");
        assert_eq!(m.schema_version, SCHEMA_VERSION);
        assert_eq!((m.sections[0].as2, m.sections[0].as3), (0.0, 0.0));
        assert!(validate(&m).is_ok());
    }

    #[test]
    fn v1_file_with_shear_areas_rejected() {
        let mut v: serde_json::Value = serde_json::from_str(&model_to_json(&cantilever())).expect("json");
        v["schema_version"] = 1.into();
        v["sections"][0]["as2"] = 0.005.into();
        assert!(matches!(model_from_json(&v.to_string()), Err(LoadError::NewerField { found: 1, .. })));
    }

    #[test]
    fn negative_shear_area_rejected_and_units_converted() {
        let mut m = cantilever();
        m.sections[0].as3 = -1.0;
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "as3"));
        let mut m = cantilever();
        m.units = UnitSystem { force: ForceUnit::N, length: LengthUnit::Mm };
        m.sections[0].area = 1e4;
        m.sections[0].i33 = 1e8;
        m.sections[0].i22 = 1e8;
        m.sections[0].j = 2e8;
        m.sections[0].as2 = 5e3;
        m.joints[1].x = 3000.0;
        m.materials[0].e = 200e3;
        let s = validate(&m).expect("ok");
        assert!((s.model.sections[0].as2 - 5e-3).abs() < 1e-18);
    }

    #[test]
    fn release_validation() {
        let rel = |i: [bool; 6], j: [bool; 6]| EndReleases { i, j, ..Default::default() };
        let t = true;
        let f = false;
        let cases: [(EndReleases, bool); 9] = [
            (rel([f, f, f, f, f, t], [f; 6]), true),              // pin M3 at I
            (rel([f, f, f, f, t, t], [f, f, f, t, t, t]), true),  // truss-like, T at one end
            (rel([t, f, f, f, f, f], [t, f, f, f, f, f]), false), // P both ends
            (rel([f, f, f, t, f, f], [f, f, f, t, f, f]), false), // T both ends
            (rel([f, t, f, f, f, f], [f, t, f, f, f, f]), false), // V2 both ends
            (rel([f, f, f, f, f, t], [f, t, f, f, f, t]), false), // M3 both + V2
            (rel([f, f, f, f, t, f], [f, f, t, f, t, f]), false), // M2 both + V3
            (rel([f, f, f, f, f, t], [f, f, f, f, f, t]), true),  // M3 both ends only
            (
                EndReleases {
                    i: [f, t, f, f, f, f],
                    j: [f, t, f, f, f, f],
                    i_springs: [0.0, 1e6, 0.0, 0.0, 0.0, 0.0],
                    ..Default::default()
                },
                true,
            ), // V2 both ends but one has a spring
        ];
        for (k, (r, ok)) in cases.into_iter().enumerate() {
            let mut m = cantilever();
            m.frames[0].releases = Some(r);
            assert_eq!(validate(&m).is_ok(), ok, "case {k}");
        }
        let mut m = cantilever();
        m.frames[0].releases = Some(EndReleases { j_springs: [0.0, 0.0, 0.0, 0.0, 0.0, 5.0], ..Default::default() });
        let errs = validate(&m).expect_err("spring without release");
        assert!(errs.iter().any(|i| i.field == "releases.j_springs[M3]"));
    }

    #[test]
    fn release_spring_units_converted() {
        let mut m = cantilever();
        m.units = UnitSystem { force: ForceUnit::KN, length: LengthUnit::Mm };
        m.materials[0].e = 200.0;
        m.sections[0].area = 1e4;
        m.sections[0].i33 = 1e8;
        m.sections[0].i22 = 1e8;
        m.sections[0].j = 2e8;
        m.joints[1].x = 3000.0;
        m.frames[0].releases = Some(EndReleases {
            j: [false, true, false, false, false, true],
            j_springs: [0.0, 2.0, 0.0, 0.0, 0.0, 3.0],
            ..Default::default()
        });
        let s = validate(&m).expect("ok");
        let r = s.model.frames[0].releases.as_ref().expect("rel");
        assert!((r.j_springs[1] - 2.0e6).abs() < 1e-6); // kN/mm -> N/m
        assert!((r.j_springs[5] - 3.0).abs() < 1e-12); // 3 kN·mm = 3 N·m
    }

    #[test]
    fn future_schema_rejected() {
        let current = format!("\"schema_version\": {SCHEMA_VERSION}");
        let text = model_to_json(&cantilever()).replacen(&current, "\"schema_version\": 99", 1);
        assert!(matches!(model_from_json(&text), Err(LoadError::FutureSchema { found: 99, .. })));
    }

    fn with_diaphragm(joints: &[&str], restraint: [bool; 6]) -> Model {
        let mut m = cantilever();
        m.joints[1].restraint = restraint;
        m.constraints = vec![Constraint::Diaphragm {
            name: "D".into(),
            axis: GlobalAxis::Z,
            joints: joints.iter().map(|j| j.to_string()).collect(),
        }];
        m
    }

    #[test]
    fn older_file_with_v3_fields_rejected_and_without_them_migrated() {
        let plain: serde_json::Value = serde_json::from_str(&model_to_json(&cantilever())).expect("json");
        let mut v = plain.clone();
        v["schema_version"] = 2.into();
        strip_v3_fields(&mut v);
        let m = model_from_json(&v.to_string()).expect("migrates");
        assert_eq!(m.schema_version, SCHEMA_VERSION);
        assert!(m.constraints.is_empty() && m.frames[0].offsets.is_none());
        assert!(m.joint_displacements.is_empty() && m.joints[1].springs == [0.0; 6]);
        let rejected = |change: fn(&mut serde_json::Value)| {
            let mut w = v.clone();
            change(&mut w);
            matches!(model_from_json(&w.to_string()), Err(LoadError::NewerField { found: 2, .. }))
        };
        assert!(rejected(|w| w["frames"][0]["offsets"] = serde_json::json!({"i": [0.0, 0.0, 1.0]})));
        assert!(rejected(|w| w["joints"][1]["springs"] = serde_json::json!([0.0, 0.0, 1.0, 0.0, 0.0, 0.0])));
        // A field the v2 file does not use is still a v2 file: an empty array is
        // the same as an absent field.
        assert!(rejected(|w| w["constraints"] =
            serde_json::json!([{"kind": "Diaphragm", "name": "D", "axis": "Z", "joints": ["2"]}])));
        assert!(rejected(|w| w["joint_displacements"] =
            serde_json::json!([{"pattern": "P1", "joint": "2", "values": [0.0, 0.0, -0.001, 0.0, 0.0, 0.0]}])));
        let mut empty = v.clone();
        empty["constraints"] = serde_json::json!([]);
        empty["joint_displacements"] = serde_json::json!([]);
        empty["frames"][0]["offsets"] = serde_json::Value::Null;
        let loaded = model_from_json(&empty.to_string());
        assert!(loaded.is_ok(), "empty v3 fields load as a v2 file: {:?}", loaded.err());
    }

    #[test]
    fn offsets_validated_and_converted_to_si() {
        let mut m = cantilever();
        m.frames[0].offsets = Some(EndOffsets { i: [f64::NAN, 0.0, 0.0], j: [0.0; 3] });
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "offsets.i"));
        // The offsets close the whole 3 m between the joints.
        let mut m = cantilever();
        m.frames[0].offsets = Some(EndOffsets { i: [1.0, 0.0, 0.0], j: [-2.0, 0.0, 0.0] });
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "i/j"));
        let mut m = cantilever();
        m.units = UnitSystem { force: ForceUnit::KN, length: LengthUnit::Mm };
        m.materials[0].e = 200.0;
        m.sections[0].area = 1e4;
        m.sections[0].i33 = 1e8;
        m.sections[0].i22 = 1e8;
        m.sections[0].j = 2e8;
        m.joints[1].x = 3000.0;
        m.frames[0].offsets = Some(EndOffsets { i: [500.0, 0.0, 0.0], j: [0.0, 0.0, 250.0] });
        let s = validate(&m).expect("ok");
        let (xi, xj) = s.frame_ends(0);
        assert!((xi[0] - 0.5).abs() < 1e-12 && (xj[0] - 3.0).abs() < 1e-12 && (xj[2] - 0.25).abs() < 1e-12);
    }

    #[test]
    fn diaphragm_validation() {
        assert!(validate(&with_diaphragm(&["2"], [false; 6])).is_err());
        assert!(validate(&with_diaphragm(&["2", "nope"], [false; 6])).is_err());
        // Joint 1 is fully restrained, so it holds tied components.
        let errs = validate(&with_diaphragm(&["1", "2"], [false; 6])).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("restrains UX, UY, RZ")), "{errs:?}");

        // A joint without elements is valid inside a diaphragm, and restraints
        // on components the diaphragm does not tie are allowed.
        let mut m = with_diaphragm(&["2", "3"], [false, false, true, false, false, false]);
        m.joints.push(Joint {
            name: "3".into(),
            x: 1.0,
            y: 1.0,
            z: 0.0,
            restraint: [false; 6],
            springs: [0.0; 6],
            added_mass: [0.0; 6],
        });
        assert!(validate(&m).is_ok());
        m.constraints.push(Constraint::Diaphragm {
            name: "E".into(),
            axis: GlobalAxis::X,
            joints: vec!["2".into(), "3".into()],
        });
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("already in constraint 'D'")), "{errs:?}");
    }

    fn with_frame_load(load: FrameLoadKind) -> Model {
        let mut m = cantilever();
        m.frame_loads = vec![FrameLoad { pattern: "P".into(), frame: "F1".into(), load }];
        m
    }

    #[test]
    fn span_load_validation_and_units() {
        let trap = |rel_start, rel_end| FrameLoadKind::Trapezoidal {
            direction: LoadDirection::Gravity,
            rel_start,
            rel_end,
            value_start: 1.0,
            value_end: 2.0,
        };
        assert!(validate(&with_frame_load(trap(0.0, 1.0))).is_ok());
        for (a, b) in [(0.5, 0.5), (0.7, 0.2), (-0.1, 0.5), (0.5, 1.1), (f64::NAN, 0.5)] {
            let errs = validate(&with_frame_load(trap(a, b))).expect_err("fail");
            assert!(errs.iter().any(|i| i.field == "rel_start/rel_end"), "{a} {b}");
        }
        let point = |direction| FrameLoadKind::PointMoment { direction, rel: 0.5, value: 1.0 };
        assert!(validate(&with_frame_load(point(LoadDirection::Local3))).is_ok());
        assert!(validate(&with_frame_load(point(LoadDirection::Gravity))).is_err());
        let dist = FrameLoadKind::TrapezoidalMoment {
            direction: LoadDirection::Gravity,
            rel_start: 0.0,
            rel_end: 1.0,
            value_start: 1.0,
            value_end: 1.0,
        };
        assert!(validate(&with_frame_load(dist)).is_err());

        // kN and mm: force/length N/m x 1e6, moment N·m x 1, moment/length N x 1e3.
        let in_kn_mm = |load| {
            let mut m = with_frame_load(load);
            m.units = UnitSystem { force: ForceUnit::KN, length: LengthUnit::Mm };
            m.materials[0].e = 200.0;
            m.sections[0].area = 1e4;
            m.sections[0].i33 = 1e8;
            m.sections[0].i22 = 1e8;
            m.sections[0].j = 2e8;
            m.joints[1].x = 3000.0;
            m.joint_loads.clear();
            validate(&m).expect("ok").model.frame_loads[0].load.clone()
        };
        let FrameLoadKind::Trapezoidal { value_end, .. } = in_kn_mm(trap(0.0, 1.0)) else { panic!() };
        assert!((value_end - 2.0e6).abs() < 1e-6);
        let FrameLoadKind::PointMoment { value, .. } = in_kn_mm(point(LoadDirection::Local3)) else { panic!() };
        assert!((value - 1.0).abs() < 1e-12);
        let FrameLoadKind::TrapezoidalMoment { value_start, .. } = in_kn_mm(FrameLoadKind::TrapezoidalMoment {
            direction: LoadDirection::Local1,
            rel_start: 0.0,
            rel_end: 1.0,
            value_start: 1.0,
            value_end: 1.0,
        }) else {
            panic!()
        };
        assert!((value_start - 1.0e3).abs() < 1e-9);
    }

    #[test]
    fn auto_mesh_validation_and_schema() {
        let mut m = cantilever();
        m.frames[0].auto_mesh = Some(AutoMesh { segments: 0, at_joints: false });
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "auto_mesh.segments"));
        m.frames[0].auto_mesh = Some(AutoMesh { segments: 3, at_joints: true });
        assert!(validate(&m).is_ok());
        let mut v: serde_json::Value = serde_json::from_str(&model_to_json(&m)).expect("json");
        v["schema_version"] = 4.into();
        assert!(matches!(model_from_json(&v.to_string()), Err(LoadError::NewerField { found: 4, .. })));
    }

    #[test]
    fn older_file_with_v4_loads_rejected() {
        let m = with_frame_load(FrameLoadKind::PointMoment { direction: LoadDirection::Local3, rel: 0.5, value: 1.0 });
        let mut v: serde_json::Value = serde_json::from_str(&model_to_json(&m)).expect("json");
        v["schema_version"] = 3.into();
        assert!(matches!(model_from_json(&v.to_string()), Err(LoadError::NewerField { found: 3, .. })));
    }

    #[test]
    fn body_and_equal_validation() {
        let two = || vec!["2".to_string(), "3".to_string()];
        let mut m = cantilever();
        m.joints.push(Joint {
            name: "3".into(),
            x: 4.0,
            y: 1.0,
            z: 0.0,
            restraint: [false; 6],
            springs: [0.0; 6],
            added_mass: [0.0; 6],
        });
        m.constraints = vec![Constraint::Body { name: "B".into(), joints: two() }];
        assert!(validate(&m).is_ok());
        let mut none = m.clone();
        none.constraints = vec![Constraint::Equal { name: "E".into(), components: [false; 6], joints: two() }];
        assert!(validate(&none).expect_err("fail").iter().any(|i| i.field == "components"));
        // A restraint on a component the Equal does not select is allowed.
        let mut held = m.clone();
        held.joints[2].restraint[2] = true;
        held.constraints = vec![Constraint::Equal {
            name: "E".into(),
            components: [true, false, false, false, false, false],
            joints: two(),
        }];
        assert!(validate(&held).is_ok());
        held.constraints = vec![Constraint::Body { name: "B".into(), joints: two() }];
        assert!(validate(&held).expect_err("fail").iter().any(|i| i.cause.contains("restrains UZ")));
    }

    #[test]
    fn spring_validation_and_units() {
        let mut m = cantilever();
        m.joints[1].springs[2] = -1.0;
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "springs[UZ]"));
        let mut m = cantilever();
        m.joints[0].springs[0] = 1.0;
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.cause.contains("restrained component")));
        // A joint held only by springs is not an unconnected joint.
        let mut m = cantilever();
        m.joints.push(Joint {
            name: "3".into(),
            x: 9.0,
            y: 0.0,
            z: 0.0,
            restraint: [false; 6],
            springs: [1.0; 6],
            added_mass: [0.0; 6],
        });
        assert!(validate(&m).is_ok());
        let mut m = cantilever();
        m.units = UnitSystem { force: ForceUnit::KN, length: LengthUnit::Mm };
        m.materials[0].e = 200.0;
        m.sections[0].area = 1e4;
        m.sections[0].i33 = 1e8;
        m.sections[0].i22 = 1e8;
        m.sections[0].j = 2e8;
        m.joints[1].x = 3000.0;
        m.joints[1].springs = [2.0, 0.0, 0.0, 0.0, 0.0, 3.0];
        let s = validate(&m).expect("ok");
        assert!((s.model.joints[1].springs[0] - 2.0e6).abs() < 1e-6); // kN/mm -> N/m
        assert!((s.model.joints[1].springs[5] - 3.0).abs() < 1e-12); // 3 kN·mm = 3 N·m
    }

    #[test]
    fn imposed_displacement_validation_and_units() {
        let imposed = |joint: &str, values| JointDisplacement { pattern: "P".into(), joint: joint.into(), values };
        let mut m = cantilever();
        m.joint_displacements = vec![imposed("1", [0.0, 0.0, -0.01, 0.0, 0.002, 0.0])];
        assert!(validate(&m).is_ok());
        // Joint 2 is free: the value would otherwise be silently lost.
        m.joint_displacements = vec![imposed("2", [0.0, 0.0, -0.01, 0.0, 0.0, 0.0])];
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.cause.contains("UZ of joint '2'")));
        m.joints[1].springs[2] = 1e6;
        assert!(validate(&m).is_ok());
        m.joint_displacements = vec![imposed("nope", [0.0; 6])];
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "joint"));

        let mut m = cantilever();
        m.units = UnitSystem { force: ForceUnit::KN, length: LengthUnit::Mm };
        m.materials[0].e = 200.0;
        m.sections[0].area = 1e4;
        m.sections[0].i33 = 1e8;
        m.sections[0].i22 = 1e8;
        m.sections[0].j = 2e8;
        m.joints[1].x = 3000.0;
        m.joint_displacements = vec![imposed("1", [0.0, 0.0, -10.0, 0.0, 0.002, 0.0])];
        let s = validate(&m).expect("ok");
        assert!((s.model.joint_displacements[0].values[2] + 0.01).abs() < 1e-15); // mm -> m
        assert_eq!(s.model.joint_displacements[0].values[4], 0.002); // rad is not converted
    }

    #[test]
    fn diaphragm_ties_the_plane_of_its_axis() {
        let tied = |axis| Constraint::Diaphragm { name: "D".into(), axis, joints: vec![] }.tied_components();
        assert_eq!(tied(GlobalAxis::Z), [true, true, false, false, false, true]);
        assert_eq!(tied(GlobalAxis::Y), [true, false, true, false, true, false]);
        assert_eq!(tied(GlobalAxis::X), [false, true, true, true, false, false]);
    }

    #[test]
    fn unknown_field_rejected() {
        let text = model_to_json(&cantilever()).replacen("\"title\"", "\"bogus\": 1, \"title\"", 1);
        assert!(matches!(model_from_json(&text), Err(LoadError::Parse(_))));
    }

    #[test]
    fn invalid_reports_entity_field_cause_and_all_issues() {
        let mut m = cantilever();
        m.sections[0].area = -1.0;
        m.frames[0].j = "nope".into();
        let errs = validate(&m).expect_err("must fail");
        assert!(errs.iter().any(|i| i.entity == "section B" && i.field == "area"));
        assert!(errs.iter().any(|i| i.entity == "frame F1" && i.field == "j"));
    }

    #[test]
    fn zero_length_frame_rejected() {
        let mut m = cantilever();
        m.joints[1].x = 0.0;
        let errs = validate(&m).expect_err("must fail");
        assert!(errs.iter().any(|i| i.field == "i/j"));
    }

    #[test]
    fn combination_cycle_rejected() {
        let mut m = cantilever();
        m.combinations = vec![
            Combination::LinearAdd { name: "A".into(), cases: vec![ScaledCase { case: "B".into(), scale: 1.0 }] },
            Combination::LinearAdd { name: "B".into(), cases: vec![ScaledCase { case: "A".into(), scale: 1.0 }] },
        ];
        let errs = validate(&m).expect_err("must fail");
        assert!(errs.iter().any(|i| i.cause.contains("cyclic")));
    }

    fn with_area(corners: [[f64; 3]; 4]) -> Model {
        let mut m = cantilever();
        for (k, c) in corners.iter().enumerate() {
            m.joints.push(Joint {
                name: format!("A{k}"),
                x: c[0],
                y: c[1],
                z: c[2],
                restraint: [false; 6],
                springs: [0.0; 6],
                added_mass: [0.0; 6],
            });
        }
        m.area_sections = vec![AreaSection {
            name: "S1".into(),
            material: "S".into(),
            thickness: 0.2,
            bending_thickness: None,
            behavior: AreaBehavior::Shell,
            layers: Vec::new(),
        }];
        m.areas = vec![Area {
            name: "Q".into(),
            joints: vec!["A0".to_string(), "A1".to_string(), "A2".to_string(), "A3".to_string()],
            section: "S1".into(),
            auto_mesh: None,
        }];
        m
    }

    #[test]
    fn laminated_sections_carry_their_layers() {
        let mut m = with_area([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]]);
        for (name, e) in [("SKIN", 2e11), ("CORE", 10e9)] {
            m.materials.push(Material { name: name.into(), e, nu: 0.3, g: None, unit_weight: 0.0, mass_density: 0.0 });
        }
        let layer = |t: f64, name: &str| Layer { thickness: t, material: name.into() };
        m.area_sections[0].layers = vec![layer(0.01, "SKIN"), layer(0.18, "CORE"), layer(0.01, "SKIN")];
        assert!(validate(&m).is_ok());

        // The layers have to add up to the thickness.
        let mut short = m.clone();
        short.area_sections[0].layers[1].thickness = 0.17;
        assert!(validate(&short).expect_err("fail").iter().any(|i| i.field == "layers"));

        // A stack that couples bending and extension is refused, not approximated.
        let mut coupled = m.clone();
        coupled.area_sections[0].layers = vec![layer(0.1, "SKIN"), layer(0.1, "CORE")];
        let text = format!("{:?}", validate(&coupled).expect_err("must fail"));
        assert!(text.contains("couples"), "{text}");

        // With layers, bending_thickness has nothing to say.
        let mut both = m.clone();
        both.area_sections[0].bending_thickness = Some(0.15);
        assert!(validate(&both).expect_err("fail").iter().any(|i| i.field == "bending_thickness"));

        // A file from before the layers existed cannot carry them.
        let mut v: serde_json::Value = serde_json::from_str(&model_to_json(&m)).expect("json");
        v["schema_version"] = 9.into();
        assert!(matches!(model_from_json(&v.to_string()), Err(LoadError::NewerField { found: 9, .. })));
    }
    #[test]
    fn a_mass_source_needs_schema_7_and_known_patterns() {
        let mut m = cantilever();
        m.mass_source = Some(MassSource {
            elements: false,
            added_masses: true,
            patterns: vec![ScaledPattern { pattern: "NOPE".into(), scale: -1.0 }],
        });
        let text = format!("{:?}", validate(&m).expect_err("invalid"));
        assert!(text.contains("NOPE") && text.contains(">= 0"), "{text}");

        let mut v: serde_json::Value = serde_json::from_str(&model_to_json(&m)).expect("json");
        v["schema_version"] = 6.into();
        assert!(matches!(model_from_json(&v.to_string()), Err(LoadError::NewerField { found: 6, .. })));
    }

    #[test]
    fn auto_seismic_needs_schema_8_a_lateral_direction_and_no_mass_role() {
        let mut m = cantilever();
        m.load_patterns[0].auto_seismic =
            Some(AutoSeismic { direction: GlobalAxis::Z, coefficient: -0.1, exponent: f64::NAN });
        m.mass_source = Some(MassSource {
            elements: true,
            added_masses: true,
            patterns: vec![ScaledPattern { pattern: m.load_patterns[0].name.clone(), scale: 1.0 }],
        });
        let text = format!("{:?}", validate(&m).expect_err("invalid"));
        for cause in ["X or Y", "coefficient", "exponent", "cannot give mass"] {
            assert!(text.contains(cause), "{cause}: {text}");
        }

        m.mass_source = None;
        m.load_patterns[0].auto_seismic =
            Some(AutoSeismic { direction: GlobalAxis::X, coefficient: 0.1, exponent: 1.0 });
        validate(&m).expect("valid");
        let mut v: serde_json::Value = serde_json::from_str(&model_to_json(&m)).expect("json");
        v["schema_version"] = 7.into();
        assert!(matches!(model_from_json(&v.to_string()), Err(LoadError::NewerField { found: 7, .. })));
    }

    #[test]
    fn triangles_are_valid_and_need_schema_6() {
        let mut m = with_area([[0.0, 0.0, 5.0], [2.0, 0.0, 5.0], [2.0, 2.0, 5.0], [0.0, 2.0, 5.0]]);
        m.areas[0].joints.pop();
        m.joints.pop();
        validate(&m).expect("a triangle is valid");

        let mut v5 = serde_json::to_value(&m).expect("json");
        v5["schema_version"] = 5.into();
        assert!(matches!(model_from_json(&v5.to_string()), Err(LoadError::NewerField { found: 5, .. })));

        // In a line, or repeating a joint.
        let mut line = m.clone();
        line.joints.iter_mut().find(|j| j.name == "A2").expect("A2").y = 0.0;
        assert!(validate(&line).expect_err("fail").iter().any(|i| i.cause.contains("collinear")));
        let mut repeated = m.clone();
        repeated.areas[0].joints[2] = "A0".into();
        assert!(validate(&repeated).expect_err("fail").iter().any(|i| i.cause.contains("collinear")));
        let mut two = m;
        two.areas[0].joints.pop();
        assert!(validate(&two).expect_err("fail").iter().any(|i| i.cause.contains("3 or 4 joints")));
    }

    #[test]
    fn a_v5_file_with_quadrilaterals_still_loads() {
        let m = with_area([[0.0, 0.0, 5.0], [2.0, 0.0, 5.0], [2.0, 2.0, 5.0], [0.0, 2.0, 5.0]]);
        let mut v5 = serde_json::to_value(&m).expect("json");
        v5["schema_version"] = 5.into();
        let back = model_from_json(&v5.to_string()).expect("loads");
        assert_eq!(back.schema_version, SCHEMA_VERSION);
        assert_eq!(back.areas[0].joints.len(), 4);
    }

    #[test]
    fn modal_and_spectrum_cases_are_validated_and_converted() {
        let spectrum = |function: &str, scale: f64, damping: f64| LoadCase::ResponseSpectrum {
            name: "RS".into(),
            modal_case: "M".into(),
            function: function.into(),
            direction: GlobalAxis::X,
            scale,
            combination: ModalCombination::Cqc,
            damping,
        };
        let base = || {
            let mut m = cantilever();
            m.materials[0].mass_density = 7850.0;
            m.load_cases.push(LoadCase::Modal { name: "M".into(), modes: 2 });
            m.response_spectra = vec![SpectrumFunction { name: "F".into(), points: vec![[0.0, 1.0], [1.0, 2.0]] }];
            m.load_cases.push(spectrum("F", 9.8, 0.05));
            m
        };
        assert!(validate(&base()).is_ok());

        let mut m = base();
        m.load_cases[1] = LoadCase::Modal { name: "M".into(), modes: 0 };
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "modes"));
        let mut m = base();
        m.load_cases[2] = spectrum("NOPE", 9.8, 0.05);
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "function"));
        let mut m = base();
        m.load_cases[2] = spectrum("F", 9.8, 1.5);
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "damping"));
        let mut m = base();
        m.response_spectra[0].points = vec![[1.0, 1.0], [0.5, 2.0]];
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "points"));
        let mut m = base();
        m.joints[1].added_mass[0] = -1.0;
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "added_mass"));
        let mut m = base();
        m.combinations = vec![Combination::LinearAdd {
            name: "C".into(),
            cases: vec![ScaledCase { case: "RS".into(), scale: 1.0 }],
        }];
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.cause.contains("not a static case")));

        // kN, mm: density kN s²/mm⁴ → N s²/m⁴ × 1e15; acceleration mm/s² → m/s² × 1e-3.
        let mut m = base();
        m.units = UnitSystem { force: ForceUnit::KN, length: LengthUnit::Mm };
        m.materials[0].e = 200.0;
        m.materials[0].mass_density = 7.85e-12;
        m.sections[0].area = 1e4;
        m.sections[0].i33 = 1e8;
        m.sections[0].i22 = 1e8;
        m.sections[0].j = 2e8;
        m.joints[1].x = 3000.0;
        m.joints[1].added_mass = [2.0, 0.0, 0.0, 0.0, 0.0, 3.0];
        m.load_cases[2] = spectrum("F", 9800.0, 0.05);
        let s = validate(&m).expect("ok");
        assert!((s.model.materials[0].mass_density - 7850.0).abs() < 1e-6);
        assert!((s.model.joints[1].added_mass[0] - 2.0e6).abs() < 1e-6); // kN s²/mm → kg
        assert!((s.model.joints[1].added_mass[5] - 3.0).abs() < 1e-12); // kN mm s² → kg m²
        let LoadCase::ResponseSpectrum { scale, .. } = &s.model.load_cases[2] else { panic!() };
        assert!((scale - 9.8).abs() < 1e-12);
    }

    #[test]
    fn areas_are_validated() {
        let flat = [[0.0, 5.0, 0.0], [2.0, 5.0, 0.0], [2.0, 7.0, 0.0], [0.0, 7.0, 0.0]];
        assert!(validate(&with_area(flat)).is_ok());
        let crossed = [[0.0, 5.0, 0.0], [2.0, 7.0, 0.0], [2.0, 5.0, 0.0], [0.0, 7.0, 0.0]];
        assert!(validate(&with_area(crossed)).is_err());
        let concave = [[0.0, 5.0, 0.0], [2.0, 5.0, 0.0], [0.5, 5.5, 0.0], [0.0, 7.0, 0.0]];
        assert!(validate(&with_area(concave)).expect_err("fail").iter().any(|i| i.cause.contains("convex")));
        let warped = [[0.0, 5.0, 0.0], [2.0, 5.0, 0.2], [2.0, 7.0, 0.0], [0.0, 7.0, 0.2]];
        assert!(validate(&with_area(warped)).expect_err("fail").iter().any(|i| i.cause.contains("warped")));
        let mut m = with_area(flat);
        m.area_sections[0].thickness = 0.0;
        m.areas[0].section = "NOPE".into();
        m.area_loads = vec![AreaLoad {
            pattern: "NOPE".into(),
            area: "NOPE".into(),
            direction: LoadDirection::Gravity,
            value: f64::NAN,
        }];
        let errs = validate(&m).expect_err("fail");
        for field in ["thickness", "section", "pattern", "area", "value"] {
            assert!(errs.iter().any(|i| i.field == field), "{field}: {errs:?}");
        }
        // Thickness is a length and the load a stress.
        let mut m = with_area(flat);
        m.units = UnitSystem { force: ForceUnit::KN, length: LengthUnit::Mm };
        m.materials[0].e = 200.0;
        m.sections[0].area = 1e4;
        m.sections[0].i33 = 1e8;
        m.sections[0].i22 = 1e8;
        m.sections[0].j = 2e8;
        for j in &mut m.joints {
            (j.x, j.y) = (j.x * 1000.0, j.y * 1000.0);
        }
        m.area_sections[0].thickness = 200.0;
        m.area_loads =
            vec![AreaLoad { pattern: "P".into(), area: "Q".into(), direction: LoadDirection::Gravity, value: 2e-3 }];
        let s = validate(&m).expect("ok");
        assert!((s.model.area_sections[0].thickness - 0.2).abs() < 1e-14);
        assert!((s.model.area_loads[0].value - 2e6).abs() < 1e-3); // kN/mm² -> Pa
    }

    fn shaped(shape: SectionShape) -> Model {
        let mut m = cantilever();
        let s = &mut m.sections[0];
        s.shape = Some(shape);
        (s.area, s.i33, s.i22, s.j) = (0.0, 0.0, 0.0, 0.0);
        m
    }

    #[test]
    fn shapes_give_properties_in_si_and_are_validated() {
        let rect = SectionShape::Rectangle { depth: 500.0, width: 200.0 };
        let mut m = shaped(rect);
        m.units = UnitSystem { force: ForceUnit::KN, length: LengthUnit::Mm };
        m.materials[0].e = 200.0;
        m.joints[1].x = 3000.0;
        let s = validate(&m).expect("ok");
        let sec = &s.model.sections[0];
        assert!((sec.area - 0.1).abs() < 1e-12);
        assert!((sec.i33 - 0.2 * 0.5_f64.powi(3) / 12.0).abs() < 1e-14);
        assert!((sec.as2 - 0.1 * 5.0 / 6.0).abs() < 1e-12);
        assert!(sec.shape.is_none(), "the snapshot holds the properties");

        let mut m = shaped(SectionShape::Rectangle { depth: 0.5, width: 0.2 });
        m.sections[0].neglect_shear = true;
        let s = validate(&m).expect("ok");
        assert_eq!((s.model.sections[0].as2, s.model.sections[0].as3), (0.0, 0.0));

        let mut m = shaped(rect);
        m.sections[0].area = 0.1;
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "area"));
        let m = shaped(SectionShape::Circle { diameter: -1.0 });
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "shape.diameter"));
        let m = shaped(SectionShape::Pipe { diameter: 1.0, wall: 0.6 });
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "shape"));
        let mut m = cantilever();
        m.sections[0].neglect_shear = true;
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "neglect_shear"));
        // A section with neither a shape nor properties is still an error.
        let mut m = cantilever();
        m.sections[0].area = 0.0;
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "area"));
    }

    #[test]
    fn older_file_with_shape_rejected() {
        let mut v: serde_json::Value =
            serde_json::from_str(&model_to_json(&shaped(SectionShape::Circle { diameter: 1.0 }))).expect("json");
        v["schema_version"] = 4.into();
        assert!(matches!(model_from_json(&v.to_string()), Err(LoadError::NewerField { found: 4, .. })));
    }

    #[test]
    fn empty_newer_collections_do_not_make_an_older_file_newer() {
        // A writer that always emits its keys produces `"area_loads": []` in a
        // v1 file. An empty array means the same as an absent field, so the file
        // is a v1 file and loads (S-02).
        let plain: serde_json::Value = serde_json::from_str(&model_to_json(&cantilever())).expect("json");
        // A genuinely v1 file: no shear areas, which are the v2 fields, and no
        // v3 springs (all-zero ones included).
        let mut v1 = plain.clone();
        v1["schema_version"] = 1.into();
        for section in v1["sections"].as_array_mut().expect("sections") {
            if let Some(map) = section.as_object_mut() {
                map.remove("as2");
                map.remove("as3");
            }
        }
        for joint in v1["joints"].as_array_mut().expect("joints") {
            if let Some(map) = joint.as_object_mut() {
                map.remove("springs");
            }
        }
        for frame in v1["frames"].as_array_mut().expect("frames") {
            if let Some(map) = frame.as_object_mut() {
                map.remove("offsets");
            }
        }
        for key in ["area_sections", "areas", "area_loads"] {
            let mut v = v1.clone();
            v[key] = serde_json::json!([]);
            if let Err(e) = model_from_json(&v.to_string()) {
                panic!("empty {key} must not need a newer schema: {e}");
            }
        }
        // With content it is a newer file, and the error names the field.
        let mut v = v1.clone();
        v["area_loads"] = serde_json::json!([{"area": "A", "direction": "Gravity", "value": 1.0}]);
        let err = model_from_json(&v.to_string()).expect_err("must fail").to_string();
        assert!(err.contains("area_loads"), "{err}");
    }

    #[test]
    fn envelope_cannot_be_a_term() {
        let term = |case: &str| vec![ScaledCase { case: case.into(), scale: 1.0 }];
        let mut m = cantilever();
        m.combinations = vec![Combination::Envelope { name: "E".into(), cases: term("LC") }];
        assert!(validate(&m).is_ok());
        for outer in [
            Combination::LinearAdd { name: "A".into(), cases: term("E") },
            Combination::Envelope { name: "A".into(), cases: term("E") },
        ] {
            let mut n = m.clone();
            n.combinations.push(outer);
            let errs = validate(&n).expect_err("fail");
            assert!(errs.iter().any(|i| i.cause.contains("cannot be a term")), "{errs:?}");
        }
        let mut v: serde_json::Value = serde_json::from_str(&model_to_json(&m)).expect("json");
        v["schema_version"] = 4.into();
        assert!(matches!(model_from_json(&v.to_string()), Err(LoadError::NewerField { found: 4, .. })));
    }

    #[test]
    fn hash_is_unit_invariant_and_section_sensitive() {
        let a = validate(&cantilever()).expect("ok");
        let mut mm = cantilever();
        mm.units = UnitSystem { force: ForceUnit::KN, length: LengthUnit::Mm };
        mm.materials[0].e = 200.0;
        mm.sections[0] = FrameSection {
            name: "B".into(),
            material: "S".into(),
            shape: None,
            neglect_shear: false,
            area: 1e4,
            i33: 1e8,
            i22: 1e8,
            j: 2e8,
            as2: 0.0,
            as3: 0.0,
        };
        mm.joints[1].x = 3000.0;
        mm.joint_loads[0].values[2] = -10.0;
        let b = validate(&mm).expect("ok");
        // Same physics: identical SI snapshots up to float rounding of the
        // conversion; the hash is computed on exact bytes, so compare values.
        assert!((b.model.sections[0].i33 - 1e-4).abs() < 1e-18);
        assert!((b.model.materials[0].e - 200e9).abs() < 1e-3);
        let mut c = cantilever();
        c.sections[0].i33 = 2e-4;
        let c = validate(&c).expect("ok");
        assert_ne!(a.hash, c.hash);
    }
    /// The cantilever plus a flat area with a plane section.
    fn plane_area(corners: [[f64; 3]; 4]) -> Model {
        let mut m = with_area(corners);
        m.area_sections[0].behavior = AreaBehavior::Plane;
        m
    }

    /// The cantilever plus a wall in the plane y = 5.
    fn plane_wall() -> Model {
        plane_area([[0.0, 5.0, 0.0], [2.0, 5.0, 0.0], [2.0, 5.0, 3.0], [0.0, 5.0, 3.0]])
    }

    #[test]
    fn plane_axis_is_the_one_coordinate_that_does_not_move() {
        assert_eq!(plane_axis(&[[0.0, 0.0, 2.0], [1.0, 0.0, 2.0], [1.0, 1.0, 2.0]]), Some(2));
        assert_eq!(plane_axis(&[[0.0, 4.0, 0.0], [0.0, 4.0, 3.0], [2.0, 4.0, 3.0]]), Some(1));
        assert_eq!(plane_axis(&[[5.0, 0.0, 0.0], [5.0, 1.0, 0.0], [5.0, 1.0, 1.0]]), Some(0));
        // Tilted: no coordinate stays put.
        assert_eq!(plane_axis(&[[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]]), None);
        // A line is not a plane, and neither is a single point.
        assert_eq!(plane_axis(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]]), None);
        assert_eq!(plane_axis(&[[1.0, 1.0, 1.0]]), None);
    }

    #[test]
    fn a_plane_section_has_no_plate_and_its_area_has_to_be_flat() {
        let flat = [[0.0, 5.0, 0.0], [2.0, 5.0, 0.0], [2.0, 7.0, 0.0], [0.0, 7.0, 0.0]];
        assert!(validate(&plane_area(flat)).is_ok());

        let tilted = [[0.0, 5.0, 0.0], [2.0, 5.0, 0.0], [2.0, 7.0, 1.0], [0.0, 7.0, 1.0]];
        let errs = validate(&plane_area(tilted)).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("global axis plane")), "{errs:?}");

        let mut plate = plane_area(flat);
        plate.area_sections[0].bending_thickness = Some(0.1);
        assert!(validate(&plate).expect_err("fail").iter().any(|i| i.field == "bending_thickness"));

        let mut layered = plane_area(flat);
        layered.area_sections[0].layers = vec![Layer { thickness: 0.2, material: "S".into() }];
        assert!(validate(&layered).expect_err("fail").iter().any(|i| i.field == "layers"));
    }

    #[test]
    fn a_plane_area_does_not_mix_with_3d_elements_or_with_another_plane() {
        let flat = [[0.0, 5.0, 0.0], [2.0, 5.0, 0.0], [2.0, 7.0, 0.0], [0.0, 7.0, 0.0]];

        // The joint of a frame, held by the plane area, would hold the frame.
        let mut m = plane_area(flat);
        m.areas[0].joints[0] = "1".into();
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("shares a joint with a frame")), "{errs:?}");

        // Two planes through one joint: that joint would be held twice over.
        // A wall standing on the edge A0A1 of the horizontal area.
        let mut m = plane_area(flat);
        m.joints.push(Joint {
            name: "V".into(),
            x: 1.0,
            y: 5.0,
            z: 2.0,
            restraint: [false; 6],
            springs: [0.0; 6],
            added_mass: [0.0; 6],
        });
        m.area_sections.push(AreaSection {
            name: "PW".into(),
            material: "S".into(),
            thickness: 0.2,
            bending_thickness: None,
            behavior: AreaBehavior::Plane,
            layers: Vec::new(),
        });
        m.areas.push(Area {
            name: "W".into(),
            joints: vec!["A0".into(), "A1".into(), "V".into()],
            section: "PW".into(),
            auto_mesh: None,
        });
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("different plane")), "{errs:?}");
    }

    #[test]
    fn loads_out_of_the_plane_of_a_plane_area_are_refused() {
        let flat = [[0.0, 5.0, 0.0], [2.0, 5.0, 0.0], [2.0, 7.0, 0.0], [0.0, 7.0, 0.0]];
        let load =
            |d: LoadDirection, v: f64| AreaLoad { pattern: "P".into(), area: "Q".into(), direction: d, value: v };

        // Gravity is out of a horizontal plane.
        let mut m = plane_area(flat);
        m.area_loads = vec![load(LoadDirection::Gravity, 1e3)];
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.field == "direction"), "{errs:?}");

        // Local 3 is the normal, and a zero value carries nothing.
        let mut m = plane_area(flat);
        m.area_loads = vec![load(LoadDirection::Local3, 1e3)];
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "direction"));
        let mut m = plane_area(flat);
        m.area_loads = vec![load(LoadDirection::Local3, 0.0), load(LoadDirection::GlobalX, 1e3)];
        assert!(validate(&m).is_ok());

        // A wall keeps gravity in its plane and loses the load along its normal.
        let mut m = plane_wall();
        m.area_loads = vec![load(LoadDirection::Gravity, 1e3), load(LoadDirection::Local1, 1e3)];
        assert!(validate(&m).is_ok());
        let mut m = plane_wall();
        m.area_loads = vec![load(LoadDirection::GlobalY, 1e3)];
        assert!(validate(&m).expect_err("fail").iter().any(|i| i.field == "direction"));
    }

    #[test]
    fn the_weight_of_a_horizontal_plane_area_is_refused() {
        let flat = [[0.0, 5.0, 0.0], [2.0, 5.0, 0.0], [2.0, 7.0, 0.0], [0.0, 7.0, 0.0]];
        let mut m = plane_area(flat);
        m.materials[0].unit_weight = 78000.0;
        assert!(validate(&m).is_ok(), "no pattern applies it");
        m.load_patterns[0].self_weight_multiplier = 1.0;
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("weight")), "{errs:?}");

        // A wall carries its own weight: it acts in its plane.
        let mut m = plane_wall();
        m.materials[0].unit_weight = 78000.0;
        m.load_patterns[0].self_weight_multiplier = 1.0;
        assert!(validate(&m).is_ok());
    }

    #[test]
    fn joint_loads_and_springs_out_of_the_plane_are_refused() {
        let flat = [[0.0, 5.0, 0.0], [2.0, 5.0, 0.0], [2.0, 7.0, 0.0], [0.0, 7.0, 0.0]];

        // A load the element has no stiffness against would be dropped.
        let mut m = plane_area(flat);
        m.joint_loads.push(JointLoad {
            pattern: "P".into(),
            joint: "A0".into(),
            values: [0.0, 0.0, 5.0, 0.0, 0.0, 0.0],
        });
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("out of the plane")), "{errs:?}");

        // Same for a grounded spring: the analysis holds that component, so the
        // spring would never deform.
        let mut m = plane_area(flat);
        m.joints[2].springs[2] = 1e6;
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.field == "springs[UZ]"), "{errs:?}");

        // In plane, both are ordinary.
        let mut m = plane_area(flat);
        m.joint_loads.push(JointLoad {
            pattern: "P".into(),
            joint: "A0".into(),
            values: [5.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        });
        assert!(validate(&m).is_ok());
    }

    #[test]
    fn a_file_from_before_the_plane_behaviour_cannot_carry_it() {
        let flat = [[0.0, 5.0, 0.0], [2.0, 5.0, 0.0], [2.0, 7.0, 0.0], [0.0, 7.0, 0.0]];
        let m = plane_area(flat);
        let mut v: serde_json::Value = serde_json::from_str(&model_to_json(&m)).expect("json");
        v["schema_version"] = 10.into();
        assert!(matches!(model_from_json(&v.to_string()), Err(LoadError::NewerField { found: 10, .. })));
    }

    /// The cantilever plus an axisymmetric area with these corners: its
    /// section is the one the analysis revolves about the global z axis
    /// (decision 0034).
    fn axisym_at(corners: [[f64; 3]; 4]) -> Model {
        let mut m = with_area(corners);
        m.area_sections[0].behavior = AreaBehavior::Axisymmetric;
        m
    }

    /// The cantilever plus an axisymmetric area in the plane x = 0, between
    /// r = 1 m and r = 2 m and one metre tall.
    fn axisym_area() -> Model {
        axisym_at([[0.0, 1.0, 0.0], [0.0, 2.0, 0.0], [0.0, 2.0, 1.0], [0.0, 1.0, 1.0]])
    }

    #[test]
    fn an_axisymmetric_area_needs_a_section_through_z_and_a_radius() {
        // The right shape: a plane through the axis, radius >= 0, and the
        // joints running counterclockwise in (r, z).
        assert!(validate(&axisym_area()).is_ok());

        // Three joints: the element of an axisymmetric area is a quadrilateral.
        let mut m = axisym_area();
        m.areas[0].joints.pop();
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("four joints")), "{errs:?}");

        // A horizontal plane does not contain z, so it cannot be revolved.
        let m = axisym_at([[1.0, 1.0, 5.0], [2.0, 1.0, 5.0], [2.0, 2.0, 5.0], [1.0, 2.0, 5.0]]);
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("not a horizontal plane")), "{errs:?}");

        // Parallel to z but not through it: the plane x = 5 has no axis in it.
        let m = axisym_at([[5.0, 1.0, 0.0], [5.0, 2.0, 0.0], [5.0, 2.0, 1.0], [5.0, 1.0, 1.0]]);
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("pass through the z axis")), "{errs:?}");

        // The radius is measured from z, so it cannot be negative.
        let m = axisym_at([[0.0, -1.0, 0.0], [0.0, 2.0, 0.0], [0.0, 2.0, 1.0], [0.0, 1.0, 1.0]]);
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("radius")), "{errs:?}");

        // The axes of the element are fixed by the revolution, so the order of
        // the joints is the only thing that can turn the section over.
        let mut m = axisym_area();
        m.areas[0].joints.reverse();
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("clockwise")), "{errs:?}");
    }

    #[test]
    fn a_centrifugal_load_needs_an_axisymmetric_section_and_a_density() {
        let load = |value: f64| AreaLoad {
            pattern: "P".into(),
            area: "Q".into(),
            direction: LoadDirection::Centrifugal,
            value,
        };

        // On a shell area there is no radius to spin it about.
        let mut m = with_area([[0.0, 1.0, 0.0], [0.0, 2.0, 0.0], [0.0, 2.0, 1.0], [0.0, 1.0, 1.0]]);
        m.area_loads.push(load(1.0));
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("axisymmetric section")), "{errs:?}");

        // The ρ of ρ ω² r comes from the material of the area.
        let mut m = axisym_area();
        m.area_loads.push(load(1.0));
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("mass_density")), "{errs:?}");

        // A negative ω would square into a positive force: refuse it instead
        // of spinning the other way for free.
        let mut m = axisym_area();
        m.materials[0].mass_density = 1.0;
        m.area_loads.push(load(-3.0));
        let errs = validate(&m).expect_err("fail");
        assert!(errs.iter().any(|i| i.cause.contains("angular velocity")), "{errs:?}");

        // The real thing: ω = 2 rad/s on a material with a density.
        let mut m = axisym_area();
        m.materials[0].mass_density = 1.0;
        m.area_loads.push(load(2.0));
        assert!(validate(&m).is_ok());
    }

    #[test]
    fn a_file_from_before_the_axisymmetric_behaviour_cannot_carry_it() {
        let m = axisym_area();
        let mut v: serde_json::Value = serde_json::from_str(&model_to_json(&m)).expect("json");
        v["schema_version"] = 11.into();
        assert!(matches!(
            model_from_json(&v.to_string()),
            Err(LoadError::NewerField { found: 11, ref field }) if field.contains("Axisymmetric")
        ));
    }

    #[test]
    fn a_file_from_before_the_centrifugal_direction_cannot_carry_it() {
        // A shell area so that only the direction is newer than the schema.
        let mut m = with_area([[0.0, 1.0, 0.0], [0.0, 2.0, 0.0], [0.0, 2.0, 1.0], [0.0, 1.0, 1.0]]);
        m.area_loads.push(AreaLoad {
            pattern: "P".into(),
            area: "Q".into(),
            direction: LoadDirection::Centrifugal,
            value: 1.0,
        });
        let mut v: serde_json::Value = serde_json::from_str(&model_to_json(&m)).expect("json");
        v["schema_version"] = 11.into();
        assert!(matches!(
            model_from_json(&v.to_string()),
            Err(LoadError::NewerField { found: 11, ref field }) if field.contains("Centrifugal")
        ));
    }
}
