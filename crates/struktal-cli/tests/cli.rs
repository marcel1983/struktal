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

/// Displacements of every joint, by name, from a `solve` run.
fn solved_displacements(model: &Path) -> std::collections::HashMap<String, Vec<f64>> {
    let out = bin().arg("solve").arg(model).output().expect("run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    v["cases"][0]["joints"]
        .as_array()
        .expect("joints")
        .iter()
        .map(|j| {
            let d = j["displacement"].as_array().expect("disp").iter().map(|x| x.as_f64().expect("num")).collect();
            (j["joint"].as_str().expect("name").to_string(), d)
        })
        .collect()
}

#[test]
fn constraint_results_do_not_depend_on_the_reference_joint() {
    for id in ["V-DIA-001", "V-BODY-001", "V-EQ-001"] {
        let dir = copy_case(id, "reference");
        let model = dir.join("model.json");
        let base = solved_displacements(&model);
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&model).expect("read")).expect("json");
        for c in v["constraints"].as_array_mut().expect("constraints") {
            c["joints"].as_array_mut().expect("joints").reverse();
        }
        std::fs::write(&model, serde_json::to_string_pretty(&v).expect("ser")).expect("write");
        let other = solved_displacements(&model);
        assert_eq!(base.len(), other.len());
        for (name, a) in &base {
            for (x, y) in a.iter().zip(&other[name]) {
                assert!((x - y).abs() <= 1e-10 * x.abs().max(1e-6), "{id} joint {name}: {x} vs {y}");
            }
        }
    }
}

#[test]
fn subdivision_changes_nothing_but_the_output_points() {
    // Exact fixed-end forces in each piece: the same answer, cut or not.
    for (id, stations) in [("V-TRAP-002", "9"), ("V-MOM-001", "9"), ("V-OFF-002", "9"), ("V-REL-001", "9")] {
        let dir = copy_case(id, "subdivide");
        let model = dir.join("model.json");
        let solve = |m: &Path| -> serde_json::Value {
            let out = bin().arg("solve").arg(m).args(["--stations", stations]).output().expect("run");
            assert!(out.status.success(), "{id}: {}", String::from_utf8_lossy(&out.stderr));
            serde_json::from_slice(&out.stdout).expect("json")
        };
        let base = solve(&model);
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&model).expect("read")).expect("json");
        v["schema_version"] = 5.into();
        for f in v["frames"].as_array_mut().expect("frames") {
            f["auto_mesh"] = serde_json::json!({"segments": 3});
        }
        std::fs::write(&model, serde_json::to_string_pretty(&v).expect("ser")).expect("write");
        let cut = solve(&model);
        let close = |a: f64, b: f64, what: String, scale: f64| {
            assert!((a - b).abs() <= 1e-9 * scale, "{id} {what}: {a} vs {b}");
        };
        for (ca, cb) in base["cases"].as_array().expect("cases").iter().zip(cut["cases"].as_array().expect("cases")) {
            for ja in ca["joints"].as_array().expect("joints") {
                let name = &ja["joint"];
                let jb = cb["joints"].as_array().expect("joints").iter().find(|j| j["joint"] == *name).expect("joint");
                for d in 0..6 {
                    let (a, b) =
                        (ja["displacement"][d].as_f64().expect("u"), jb["displacement"][d].as_f64().expect("u"));
                    close(a, b, format!("{name} displacement {d}"), a.abs().max(1e-3));
                    if let Some(ra) = ja["reaction"][d].as_f64() {
                        let rb = jb["reaction"][d].as_f64().expect("r");
                        close(ra, rb, format!("{name} reaction {d}"), ra.abs().max(1.0));
                    }
                }
            }
            let (fa, fb) = (ca["frames"].as_array().expect("frames"), cb["frames"].as_array().expect("frames"));
            assert_eq!(fa.len(), fb.len(), "{id}: frames of the model, not pieces");
            for (a, b) in fa.iter().zip(fb) {
                assert_eq!(a["frame"], b["frame"]);
                for (sa, sb) in a["stations"].as_array().expect("st").iter().zip(b["stations"].as_array().expect("st"))
                {
                    for k in 0..6 {
                        let (x, y) = (sa["actions"][k].as_f64().expect("a"), sb["actions"][k].as_f64().expect("a"));
                        close(x, y, format!("{} station {} action {k}", a["frame"], sa["rel"]), x.abs().max(1.0));
                    }
                }
            }
        }
        assert!(!cut["manifest"]["analysis_mesh"].as_array().expect("mesh").is_empty());
    }
}

#[test]
fn joint_off_the_axis_of_a_split_frame_fails_with_the_reason() {
    let dir = copy_case("V-MESH-002", "offaxis");
    let model = dir.join("model.json");
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&model).expect("read")).expect("json");
    // C is the joint F1 runs through. Move it off the axis, and remove the bar
    // that also reaches it, so that nothing connects it any more.
    for j in v["joints"].as_array_mut().expect("joints") {
        if j["name"] == "C" {
            j["y"] = 0.002.into();
        }
    }
    v["frames"].as_array_mut().expect("frames").retain(|f| f["name"] != "F2");
    v["joints"].as_array_mut().expect("joints").retain(|j| j["name"] != "R");
    std::fs::write(&model, serde_json::to_string_pretty(&v).expect("ser")).expect("write");
    let out = bin().arg("solve").arg(&model).output().expect("run");
    assert_eq!(out.status.code(), Some(1));
    let r: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let messages = r["manifest"]["messages"].to_string();
    assert!(messages.contains("joint C is not connected") || messages.contains("joint C / connectivity"), "{messages}");
}

#[test]
fn imported_s2k_solves_to_the_closed_form_in_si() {
    let dir = std::env::temp_dir().join(format!("struktal-test-import-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let model = dir.join("portal.json");
    let out = bin()
        .arg("import")
        .arg(repo().join("verification/s2k/portal.s2k"))
        .arg("-o")
        .arg(&model)
        .output()
        .expect("run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stderr).contains("MODAL"), "the skipped case is reported");
    let r = bin().arg("solve").arg(&model).output().expect("run");
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let v: serde_json::Value = serde_json::from_slice(&r.stdout).expect("json");
    assert_eq!(v["cases"].as_array().expect("cases").len(), 1);
    let tip = v["cases"][0]["joints"].as_array().expect("joints").iter().find(|j| j["joint"] == "2").expect("joint");
    // Vertical cantilever: 10 kip along X bends with I33 (local 2 = +X).
    let delta_in = 10.0 * 120.0_f64.powi(3) / (3.0 * 29000.0 * 800.0);
    let got = tip["displacement"][0].as_f64().expect("ux");
    assert!((got - delta_in * 0.0254).abs() < 1e-12, "{got}");
}

#[test]
fn s2k_with_something_unsupported_is_refused_with_all_the_reasons() {
    let dir = std::env::temp_dir().join(format!("struktal-test-import-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let file = dir.join("bad.s2k");
    let text = std::fs::read_to_string(repo().join("verification/s2k/portal.s2k")).expect("read");
    let bad = format!(
        "{}\nTABLE:  \"FRAME LOADS - TEMPERATURE\"\n   Frame=C1   LoadPat=H   Temp=10\nTABLE:  \"AREA LOADS - UNIFORM\"\n   Area=1\n",
        text.replace("END TABLE DATA", "")
    );
    std::fs::write(&file, bad).expect("write");
    let out = bin().arg("import").arg(&file).output().expect("run");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty(), "no partial model");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("FRAME LOADS - TEMPERATURE") && err.contains("AREA LOADS - UNIFORM"), "{err}");
}

#[test]
fn restraint_on_a_tied_component_is_input_error() {
    let dir = copy_case("V-DIA-001", "restrained");
    let model = dir.join("model.json");
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&model).expect("read")).expect("json");
    let name = v["constraints"][0]["joints"][1].clone();
    for j in v["joints"].as_array_mut().expect("joints") {
        if j["name"] == name {
            j["restraint"][0] = true.into();
        }
    }
    std::fs::write(&model, serde_json::to_string_pretty(&v).expect("ser")).expect("write");
    let out = bin().arg("validate").arg(&model).output().expect("run");
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("constraint D1 / joints") && err.contains("restrains UX"), "{err}");
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
