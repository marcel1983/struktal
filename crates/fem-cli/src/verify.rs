//! Verification case format and comparator (guide §10, §12).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fem_analysis::{
    CaseResult, Completion, PhysicalStatus, RunResult, results::COMPONENTS, results::REACTION_COMPONENTS,
    results::STATION_COMPONENTS,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseFile {
    pub case_id: String,
    pub title: String,
    pub feature_ids: Vec<String>,
    pub model: String,
    pub model_sha256: String,
    pub tolerances_file: String,
    pub tolerances_id: String,
    pub reference: Reference,
    #[serde(default)]
    pub expected_status: Vec<ExpectedStatus>,
    #[serde(default)]
    pub expected: Vec<Expected>,
    pub oracle: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub kind: String,
    pub source: String,
    #[serde(default)]
    pub derivation: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedStatus {
    pub case: String,
    pub completion: Completion,
    pub physical_status: PhysicalStatus,
    #[serde(default)]
    pub message_contains: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    pub result: String,
    pub case: String,
    #[serde(default)]
    pub entity: String,
    #[serde(default)]
    pub component: String,
    #[serde(default)]
    pub rel: Option<f64>,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Deserialize)]
pub struct ToleranceSet {
    pub rtol: f64,
    /// Absolute tolerance per unit string.
    pub atol: BTreeMap<String, f64>,
    pub residual_max: f64,
    pub equilibrium_max: f64,
}

#[derive(Debug, Serialize)]
pub struct CheckRow {
    pub what: String,
    pub reference: Option<f64>,
    pub computed: Option<f64>,
    pub abs_error: Option<f64>,
    pub allowed: Option<f64>,
    pub pass: bool,
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct CaseReport {
    pub case_id: String,
    pub title: String,
    pub feature_ids: Vec<String>,
    pub outcome: String,
    pub reference: Reference,
    pub model_sha256_recorded: String,
    pub model_sha256_actual: String,
    pub tolerances_id: String,
    pub oracle: BTreeMap<String, String>,
    pub rows: Vec<CheckRow>,
}

pub fn file_sha256(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(fem_core::validate::hex(&Sha256::digest(&bytes)))
}

pub fn load_case(path: &Path) -> Result<(CaseFile, PathBuf), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let cf: CaseFile = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    Ok((cf, dir))
}

pub fn load_tolerances(cf: &CaseFile, dir: &Path) -> Result<ToleranceSet, String> {
    let p = dir.join(&cf.tolerances_file);
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut all: BTreeMap<String, ToleranceSet> =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
    all.remove(&cf.tolerances_id)
        .ok_or_else(|| format!("tolerance set '{}' not found in {}", cf.tolerances_id, p.display()))
}

fn find_case<'a>(r: &'a RunResult, name: &str) -> Option<&'a CaseResult> {
    r.cases.iter().find(|c| c.name == name)
}

fn lookup(r: &RunResult, e: &Expected) -> Result<f64, String> {
    let c = find_case(r, &e.case).ok_or_else(|| format!("case '{}' not in results", e.case))?;
    let comp = |names: &[&str]| {
        names.iter().position(|n| *n == e.component).ok_or_else(|| format!("unknown component '{}'", e.component))
    };
    match e.result.as_str() {
        "joint_displacement" | "joint_reaction" => {
            let j = c
                .joints
                .iter()
                .find(|j| j.joint == e.entity)
                .ok_or_else(|| format!("joint '{}' not in results", e.entity))?;
            if e.result == "joint_displacement" {
                Ok(j.displacement[comp(&COMPONENTS)?])
            } else {
                let k = comp(&REACTION_COMPONENTS)?;
                j.reaction.map(|r| r[k]).ok_or_else(|| format!("joint '{}' has no reaction", e.entity))
            }
        }
        "frame_station" => {
            let f = c
                .frames
                .iter()
                .find(|f| f.frame == e.entity)
                .ok_or_else(|| format!("frame '{}' not in results", e.entity))?;
            let rel = e.rel.ok_or("frame_station needs 'rel'")?;
            let st = f
                .stations
                .iter()
                .find(|s| (s.rel - rel).abs() < 1e-12)
                .ok_or_else(|| format!("no station at rel {rel}"))?;
            Ok(st.actions[comp(&STATION_COMPONENTS)?])
        }
        "external_work" => c.checks.as_ref().map(|k| k.external_work).ok_or_else(|| "no checks in case".into()),
        other => Err(format!("unknown result kind '{other}'")),
    }
}

pub fn compare(cf: &CaseFile, tol: &ToleranceSet, r: &RunResult, actual_hash: &str) -> CaseReport {
    let mut rows = Vec::new();
    let stale = actual_hash != cf.model_sha256;
    rows.push(CheckRow {
        what: "model hash matches recorded reference input".into(),
        reference: None,
        computed: None,
        abs_error: None,
        allowed: None,
        pass: !stale,
        note: if stale {
            "model changed after the reference was recorded (stale_reference)".into()
        } else {
            String::new()
        },
    });

    for es in &cf.expected_status {
        let row = match find_case(r, &es.case) {
            None => CheckRow {
                what: format!("status of {}", es.case),
                reference: None,
                computed: None,
                abs_error: None,
                allowed: None,
                pass: false,
                note: "case missing".into(),
            },
            Some(c) => {
                let msg_ok =
                    es.message_contains.as_ref().is_none_or(|m| c.messages.iter().any(|x| x.contains(m.as_str())));
                let ok = c.status.completion() == es.completion
                    && c.status.physical_status() == es.physical_status
                    && msg_ok;
                CheckRow {
                    what: format!("status of {}", es.case),
                    reference: None,
                    computed: None,
                    abs_error: None,
                    allowed: None,
                    pass: ok,
                    note: format!(
                        "expected {:?}/{:?}, got {:?}/{:?}; messages: {}",
                        es.completion,
                        es.physical_status,
                        c.status.completion(),
                        c.status.physical_status(),
                        c.messages.join(" | ")
                    ),
                }
            }
        };
        rows.push(row);
    }

    // Built-in sanity on every complete linear case.
    for c in &r.cases {
        if let Some(k) = &c.checks {
            rows.push(CheckRow {
                what: format!("{}: relative residual", c.name),
                reference: Some(0.0),
                computed: Some(k.residual_relative),
                abs_error: Some(k.residual_relative),
                allowed: Some(tol.residual_max),
                pass: k.residual_relative <= tol.residual_max,
                note: String::new(),
            });
            let eq = k.equilibrium_force_error.max(k.equilibrium_moment_error);
            rows.push(CheckRow {
                what: format!("{}: global equilibrium (relative)", c.name),
                reference: Some(0.0),
                computed: Some(eq),
                abs_error: Some(eq),
                allowed: Some(tol.equilibrium_max),
                pass: eq <= tol.equilibrium_max,
                note: String::new(),
            });
        }
    }

    for e in &cf.expected {
        let what = format!(
            "{} {} {} {}{}",
            e.case,
            e.result,
            e.entity,
            e.component,
            e.rel.map(|r| format!(" @rel {r}")).unwrap_or_default()
        );
        let row = match (lookup(r, e), tol.atol.get(&e.unit)) {
            (Err(msg), _) => CheckRow {
                what,
                reference: Some(e.value),
                computed: None,
                abs_error: None,
                allowed: None,
                pass: false,
                note: msg,
            },
            (_, None) => CheckRow {
                what,
                reference: Some(e.value),
                computed: None,
                abs_error: None,
                allowed: None,
                pass: false,
                note: format!("no atol for unit '{}'", e.unit),
            },
            (Ok(v), Some(&atol)) => {
                let err = (v - e.value).abs();
                let allowed = atol + tol.rtol * e.value.abs();
                CheckRow {
                    what,
                    reference: Some(e.value),
                    computed: Some(v),
                    abs_error: Some(err),
                    allowed: Some(allowed),
                    pass: err <= allowed,
                    note: e.unit.clone(),
                }
            }
        };
        rows.push(row);
    }

    let outcome = if stale {
        "stale_reference"
    } else if rows.iter().all(|r| r.pass) {
        "pass"
    } else {
        "fail"
    };
    CaseReport {
        case_id: cf.case_id.clone(),
        title: cf.title.clone(),
        feature_ids: cf.feature_ids.clone(),
        outcome: outcome.into(),
        reference: Reference {
            kind: cf.reference.kind.clone(),
            source: cf.reference.source.clone(),
            derivation: cf.reference.derivation.clone(),
        },
        model_sha256_recorded: cf.model_sha256.clone(),
        model_sha256_actual: actual_hash.to_string(),
        tolerances_id: cf.tolerances_id.clone(),
        oracle: cf.oracle.clone(),
        rows,
    }
}
