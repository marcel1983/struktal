//! End-to-end CLI tests over the verification bench.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_struktal"))
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn case_files() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(repo().join("verification/cases"))
        .expect("cases dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("case.json"))
        .filter(|p| p.exists())
        .collect();
    v.sort();
    v
}

/// Copies a case to a temp dir laid out like verification/, so relative
/// paths (model, tolerances) keep working.
fn copy_case(id: &str, tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("struktal-test-{tag}-{}", std::process::id()));
    let dir = root.join("cases").join(id);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let src = repo().join("verification/cases").join(id);
    for f in ["case.json", "model.json"] {
        std::fs::copy(src.join(f), dir.join(f)).expect("copy");
    }
    std::fs::copy(repo().join("verification/tolerances.json"), root.join("tolerances.json")).expect("copy tol");
    dir
}

#[test]
fn all_verification_cases_pass_with_every_backend() {
    let cases = case_files();
    assert!(cases.len() >= 11, "expected the verification bench, found {}", cases.len());
    for backend in ["faer", "skyline"] {
        let out = bin().args(["verify", "--backend", backend]).args(&cases).output().expect("run");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{backend}: verify failed:\n{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    }
}

#[test]
fn unknown_backend_is_input_error() {
    let model = repo().join("verification/cases/V-LIN-001/model.json");
    let out = bin().args(["solve", "--backend", "nope"]).arg(&model).output().expect("run");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn section_change_is_detected_as_stale_reference() {
    let dir = copy_case("V-LIN-001", "section");
    let model = dir.join("model.json");
    let text = std::fs::read_to_string(&model).expect("read");
    std::fs::write(&model, text.replacen("\"i33\": 0.0001", "\"i33\": 0.0002", 1)).expect("write");
    let out = bin().arg("verify").arg(dir.join("case.json")).output().expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("stale_reference"), "{stdout}");
}

#[test]
fn unit_change_is_detected_as_stale_reference() {
    let dir = copy_case("V-LIN-001", "units");
    let model = dir.join("model.json");
    let text = std::fs::read_to_string(&model).expect("read");
    std::fs::write(&model, text.replacen("\"force\": \"N\"", "\"force\": \"kN\"", 1)).expect("write");
    let out = bin().arg("verify").arg(dir.join("case.json")).output().expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("stale_reference"), "{stdout}");
}

#[test]
fn wrong_reference_value_fails() {
    let dir = copy_case("V-LIN-002", "wrongref");
    let case = dir.join("case.json");
    let text = std::fs::read_to_string(&case).expect("read");
    // Corrupt the expected axial displacement by 1e-6 relative (above rtol 1e-8).
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    let mut v = v;
    let e = &mut v["expected"][0]["value"];
    let x = e.as_f64().expect("num");
    *e = serde_json::json!(x * (1.0 + 1e-6));
    std::fs::write(&case, serde_json::to_string_pretty(&v).expect("ser")).expect("write");
    let out = bin().arg("verify").arg(&case).output().expect("run");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("FAIL"));
}

#[test]
fn invalid_model_reports_entity_field_cause() {
    let dir = copy_case("V-LIN-001", "invalid");
    let model = dir.join("model.json");
    let text = std::fs::read_to_string(&model).expect("read");
    std::fs::write(&model, text.replacen("\"area\": 0.01", "\"area\": -0.01", 1)).expect("write");
    let out = bin().arg("validate").arg(&model).output().expect("run");
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("section B / area"), "{err}");
}

#[test]
fn unstable_release_combination_is_input_error() {
    let dir = copy_case("V-REL-001", "unstable");
    let model = dir.join("model.json");
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&model).expect("read")).expect("json");
    // V2 released at both ends of H1: the member can translate freely.
    v["frames"][0]["releases"] = serde_json::json!({"i": [false, true, false, false, false, false], "j": [false, true, false, false, false, false]});
    std::fs::write(&model, serde_json::to_string_pretty(&v).expect("ser")).expect("write");
    let out = bin().arg("validate").arg(&model).output().expect("run");
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("frame H1 / releases: unstable release combination: V2 released at both ends"), "{err}");
}

#[test]
fn future_schema_rejected() {
    let dir = copy_case("V-LIN-001", "future");
    let model = dir.join("model.json");
    let text = std::fs::read_to_string(&model).expect("read");
    std::fs::write(&model, text.replacen("\"schema_version\": 1", "\"schema_version\": 99", 1)).expect("write");
    let out = bin().arg("solve").arg(&model).output().expect("run");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("newer than the supported"));
}

#[test]
fn mechanism_solve_exits_nonzero_with_diagnosis() {
    let model = repo().join("verification/cases/V-MECH-002/model.json");
    let out = bin().arg("solve").arg(&model).output().expect("run");
    assert_eq!(out.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["cases"][0]["status"]["physical_status"], "mechanism");
    assert_eq!(v["cases"][0]["status"]["compatibility_status"], "not_verified");
    let st = &v["manifest"]["stability"];
    assert_eq!(st["nullity"], 1);
    assert_eq!(st["modes"].as_array().expect("modes").len(), 1);
    // The torsional mode moves only rotations about X.
    for e in st["modes"][0]["dominant"].as_array().expect("dominant") {
        assert_eq!(e["component"], "RX");
    }
}

#[test]
fn solve_writes_file_and_compare_accepts_it() {
    let dir = copy_case("V-LIN-003", "compare");
    let out_path = dir.join("results.json");
    let st = bin()
        .arg("solve")
        .arg(dir.join("model.json"))
        .args(["--stations", "5", "-o"])
        .arg(&out_path)
        .status()
        .expect("run");
    assert!(st.success());
    let out = bin().arg("compare").arg(&out_path).arg(dir.join("case.json")).output().expect("run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn capabilities_declares_unsupported_families() {
    let out = bin().arg("capabilities").output().expect("run");
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let ns = v["not_supported"].as_array().expect("array");
    assert!(ns.iter().any(|x| x == "modal"));
    let backends = v["solver_backends"].as_array().expect("array");
    assert_eq!(backends.len(), 2);
    for b in backends {
        assert_eq!(b["matrix_classes"], serde_json::json!(["spd"]));
    }
}
