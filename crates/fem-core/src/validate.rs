//! Validation of an editable model into an immutable, indexed snapshot.
//!
//! Validation is all-or-nothing: either every check passes and a [`Snapshot`]
//! is produced, or the full list of issues (entity, field, cause) is returned
//! and nothing is transformed.

use std::collections::{HashMap, HashSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::model::*;

/// Relative tolerance for coincident joints / zero-length frames.
pub const LENGTH_TOL_REL: f64 = 1e-9;

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
    FutureSchema { found: u32, supported: u32 },
    UnsupportedSchema { found: u32 },
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
            LoadError::UnsupportedSchema { found } => write!(f, "schema_version {found} is not supported"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Reads a model from JSON, checking the schema version before the body.
/// Migrations: v1 → v2 adds `as2 = as3 = 0` (no shear deformation, the v1
/// behavior); missing fields already default to that, so only the version
/// number changes.
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
    if ver == 1 {
        // Shear areas did not exist in v1; reject them instead of silently
        // giving a v1 file v2 semantics.
        let has_shear = raw
            .get("sections")
            .and_then(|v| v.as_array())
            .is_some_and(|secs| secs.iter().any(|s| s.get("as2").is_some() || s.get("as3").is_some()));
        if has_shear {
            return Err(LoadError::UnsupportedSchema { found: ver });
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

pub fn validate(input: &Model) -> Result<Snapshot, Vec<Issue>> {
    let mut issues = Vec::new();
    if input.schema_version != SCHEMA_VERSION {
        issues.push(issue("model", "schema_version", format!("expected {SCHEMA_VERSION}")));
    }
    let m = input.to_si();

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
        if let (Some(a), Some(b)) = (ji, jj) {
            let (pa, pb) = (&m.joints[a], &m.joints[b]);
            let len = ((pb.x - pa.x).powi(2) + (pb.y - pa.y).powi(2) + (pb.z - pa.z).powi(2)).sqrt();
            if a == b || len <= len_tol {
                issues.push(issue(&e, "i/j", format!("zero length (L = {len:e} m)")));
            }
            connected.insert(a);
            connected.insert(b);
        }
        let mk = sk.and_then(|s| section_material.get(s).copied()).unwrap_or(usize::MAX);
        frame_refs.push((ji.unwrap_or(usize::MAX), jj.unwrap_or(usize::MAX), sk.unwrap_or(usize::MAX), mk));
    }

    for (k, j) in m.joints.iter().enumerate() {
        if !connected.contains(&k) && !j.restraint.iter().all(|&r| r) {
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
    for (k, fl) in m.frame_loads.iter().enumerate() {
        let e = format!("frame load #{k}");
        if !pattern_index.contains_key(&fl.pattern) {
            issues.push(issue(&e, "pattern", format!("unknown load pattern '{}'", fl.pattern)));
        }
        if !frame_index.contains_key(&fl.frame) {
            issues.push(issue(&e, "frame", format!("unknown frame '{}'", fl.frame)));
        }
        match fl.load {
            FrameLoadKind::Uniform { value, .. } => check_finite(value, &e, "value", &mut issues),
            FrameLoadKind::Point { rel, value, .. } => {
                check_finite(value, &e, "value", &mut issues);
                if !(0.0..=1.0).contains(&rel) {
                    issues.push(issue(&e, "rel", format!("must be in [0, 1], got {rel}")));
                }
            }
        }
    }
    for c in &m.load_cases {
        let LoadCase::LinearStatic { name, loads } = c;
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
        let Combination::LinearAdd { name, cases: terms } = c;
        for t in terms {
            if !cases.contains(t.case.as_str()) && !combos.contains_key(t.case.as_str()) {
                issues.push(issue(format!("combination {name}"), "cases", format!("unknown case '{}'", t.case)));
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
        if let Some(Combination::LinearAdd { cases, .. }) = combos.get(n) {
            for t in cases {
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
    use crate::units::{ForceUnit, LengthUnit, UnitSystem};

    fn cantilever() -> Model {
        Model {
            schema_version: SCHEMA_VERSION,
            title: "t".into(),
            units: UnitSystem::default(),
            materials: vec![Material { name: "S".into(), e: 200e9, nu: 0.3, g: None, unit_weight: 0.0 }],
            sections: vec![FrameSection {
                name: "B".into(),
                material: "S".into(),
                area: 0.01,
                i33: 1e-4,
                i22: 1e-4,
                j: 2e-4,
                as2: 0.0,
                as3: 0.0,
            }],
            joints: vec![
                Joint { name: "1".into(), x: 0.0, y: 0.0, z: 0.0, restraint: [true; 6] },
                Joint { name: "2".into(), x: 3.0, y: 0.0, z: 0.0, restraint: [false; 6] },
            ],
            frames: vec![Frame {
                name: "F1".into(),
                i: "1".into(),
                j: "2".into(),
                section: "B".into(),
                angle_deg: 0.0,
            }],
            load_patterns: vec![LoadPattern { name: "P".into(), self_weight_multiplier: 0.0 }],
            joint_loads: vec![JointLoad {
                pattern: "P".into(),
                joint: "2".into(),
                values: [0.0, 0.0, -1e4, 0.0, 0.0, 0.0],
            }],
            frame_loads: vec![],
            load_cases: vec![LoadCase::LinearStatic {
                name: "LC".into(),
                loads: vec![ScaledPattern { pattern: "P".into(), scale: 1.0 }],
            }],
            combinations: vec![],
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
        assert!(matches!(model_from_json(&v.to_string()), Err(LoadError::UnsupportedSchema { found: 1 })));
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
    fn future_schema_rejected() {
        let text = model_to_json(&cantilever()).replacen("\"schema_version\": 2", "\"schema_version\": 99", 1);
        assert!(matches!(model_from_json(&text), Err(LoadError::FutureSchema { found: 99, .. })));
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

    #[test]
    fn hash_is_unit_invariant_and_section_sensitive() {
        let a = validate(&cantilever()).expect("ok");
        let mut mm = cantilever();
        mm.units = UnitSystem { force: ForceUnit::KN, length: LengthUnit::Mm };
        mm.materials[0].e = 200.0;
        mm.sections[0] = FrameSection {
            name: "B".into(),
            material: "S".into(),
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
}
