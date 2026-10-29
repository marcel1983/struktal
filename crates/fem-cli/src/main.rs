//! rust-fem command-line runner.
//!
//! Exit codes: 0 success; 1 verification failed or a case did not complete;
//! 2 invalid input (parse/validation); 3 I/O error.

mod verify;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use fem_analysis::{Completion, Options, RunResult};

#[derive(Parser)]
#[command(
    name = "rust-fem",
    version,
    about = "Linear 3D frame FEM engine (subset of the rust-fem SAP2000 compatibility profile)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Validate a model file and print issues (entity / field / cause).
    Validate { model: PathBuf },
    /// Validate and solve all load cases; write results JSON.
    Solve {
        model: PathBuf,
        /// Output file (stdout if omitted).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Output stations per frame, including both ends.
        #[arg(long, default_value_t = 3)]
        stations: usize,
        /// Memory budget for the factorization, in MiB.
        #[arg(long, default_value_t = 1024)]
        memory_budget_mib: usize,
    },
    /// Print engine and solver backend capabilities as JSON.
    Capabilities,
    /// Run verification cases: solve each case's model and compare with its reference.
    Verify {
        #[arg(required = true)]
        cases: Vec<PathBuf>,
        /// Write the full JSON report here.
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Compare an existing results file with a verification case.
    Compare { results: PathBuf, case: PathBuf },
}

enum Fail {
    Input(String),
    Io(String),
}

fn load_snapshot(path: &Path) -> Result<fem_core::Snapshot, Fail> {
    let text = std::fs::read_to_string(path).map_err(|e| Fail::Io(format!("{}: {e}", path.display())))?;
    let model = fem_core::model_from_json(&text).map_err(|e| Fail::Input(format!("{}: {e}", path.display())))?;
    fem_core::validate(&model).map_err(|issues| {
        Fail::Input(format!(
            "{}: {} validation issue(s):\n{}",
            path.display(),
            issues.len(),
            issues.iter().map(|i| format!("  - {i}")).collect::<Vec<_>>().join("\n")
        ))
    })
}

fn stations_ok(n: usize) -> Result<usize, Fail> {
    if n < 2 { Err(Fail::Input("--stations must be >= 2".into())) } else { Ok(n) }
}

fn solve(path: &Path, stations: usize, budget_mib: usize) -> Result<RunResult, Fail> {
    let snap = load_snapshot(path)?;
    let opts = Options { stations: stations_ok(stations)?, memory_budget: budget_mib.saturating_mul(1 << 20) };
    Ok(fem_analysis::run(&snap, &opts))
}

fn json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|e| format!("{{\"serialization_error\": \"{e}\"}}"))
}

fn write_out(path: Option<&Path>, text: &str) -> Result<(), Fail> {
    match path {
        None => {
            println!("{text}");
            Ok(())
        }
        Some(p) => {
            // Write to a temporary file then rename, so a crash never leaves a
            // truncated results file under the final name.
            let tmp = p.with_extension("tmp");
            std::fs::write(&tmp, text).map_err(|e| Fail::Io(format!("{}: {e}", tmp.display())))?;
            std::fs::rename(&tmp, p).map_err(|e| Fail::Io(format!("{}: {e}", p.display())))
        }
    }
}

fn run_verify(cases: &[PathBuf], report: Option<&Path>, existing: Option<&Path>) -> Result<bool, Fail> {
    let mut reports = Vec::new();
    for case_path in cases {
        let (cf, dir) = verify::load_case(case_path).map_err(Fail::Input)?;
        let tol = verify::load_tolerances(&cf, &dir).map_err(Fail::Input)?;
        let model_path = dir.join(&cf.model);
        let hash = verify::file_sha256(&model_path).map_err(|e| Fail::Io(format!("{}: {e}", model_path.display())))?;
        let result = match existing {
            Some(p) => {
                let text = std::fs::read_to_string(p).map_err(|e| Fail::Io(format!("{}: {e}", p.display())))?;
                let r: RunResult =
                    serde_json::from_str(&text).map_err(|e| Fail::Input(format!("{}: {e}", p.display())))?;
                if r.manifest.model_sha256 != load_snapshot(&model_path)?.hash {
                    return Err(Fail::Input("results file was produced from a different model snapshot".into()));
                }
                r
            }
            None => solve(&model_path, 5, 1024)?,
        };
        let rep = verify::compare(&cf, &tol, &result, &hash);
        let failed: Vec<_> = rep.rows.iter().filter(|r| !r.pass).collect();
        println!(
            "{:<14} {:<16} {:>3} checks, {} failed  [{}]",
            rep.case_id,
            rep.outcome,
            rep.rows.len(),
            failed.len(),
            rep.oracle.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join(", ")
        );
        for f in failed {
            println!(
                "    FAIL {}: ref {:?} got {:?} err {:?} allowed {:?} {}",
                f.what, f.reference, f.computed, f.abs_error, f.allowed, f.note
            );
        }
        reports.push(rep);
    }
    let all_pass = reports.iter().all(|r| r.outcome == "pass");
    if let Some(p) = report {
        write_out(Some(p), &json(&reports))?;
    }
    Ok(all_pass)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let res: Result<bool, Fail> = match cli.cmd {
        Cmd::Validate { model } => load_snapshot(&model).map(|s| {
            println!(
                "valid: {} joints, {} frames, {} load cases, {} combinations; snapshot sha256 {}",
                s.model.joints.len(),
                s.model.frames.len(),
                s.model.load_cases.len(),
                s.model.combinations.len(),
                s.hash
            );
            true
        }),
        Cmd::Solve { model, output, stations, memory_budget_mib } => solve(&model, stations, memory_budget_mib)
            .and_then(|r| {
                let ok = r.cases.iter().all(|c| c.status.completion() == Completion::Complete);
                for c in &r.cases {
                    if c.status.completion() != Completion::Complete {
                        eprintln!(
                            "case {}: {:?} / {:?}: {}",
                            c.name,
                            c.status.completion(),
                            c.status.physical_status(),
                            c.messages.join("; ")
                        );
                    }
                }
                write_out(output.as_deref(), &json(&r)).map(|_| ok)
            }),
        Cmd::Capabilities => {
            let caps = serde_json::json!({
                "engine": "rust-fem",
                "engine_version": env!("CARGO_PKG_VERSION"),
                "model_schema_version": fem_core::model::SCHEMA_VERSION,
                "results_schema_version": fem_analysis::results::RESULTS_SCHEMA_VERSION,
                "analysis": ["linear_static"],
                "combinations": ["linear_add"],
                "elements": ["frame3d_euler_bernoulli"],
                "loads": ["joint_force_moment", "frame_uniform_full_length", "frame_point_force", "self_weight"],
                "not_supported": ["releases", "offsets", "springs", "constraints", "shear_deformation", "areas", "solids", "modal", "response_spectrum", "time_history", "nonlinear", "staged_construction", "moving_loads", "design", "s2k_import", "s2k_export", "oapi"],
                "solver_backends": [fem_math::skyline::capabilities()],
                "compatibility": "specified_not_verified (see spec/sap_profile.yaml)"
            });
            println!("{}", json(&caps));
            Ok(true)
        }
        Cmd::Verify { cases, report } => run_verify(&cases, report.as_deref(), None),
        Cmd::Compare { results, case } => run_verify(&[case], None, Some(&results)),
    };
    match res {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(Fail::Input(m)) => {
            eprintln!("error: {m}");
            ExitCode::from(2)
        }
        Err(Fail::Io(m)) => {
            eprintln!("io error: {m}");
            ExitCode::from(3)
        }
    }
}
