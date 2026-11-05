//! Records the git commit for the run manifest. Falls back to "unknown".
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=STRUKTAL_GIT_SHA");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
    if std::env::var("STRUKTAL_GIT_SHA").is_ok() {
        return;
    }
    let sha = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);
    if let Some(s) = sha {
        let s = if dirty { format!("{s}-dirty") } else { s };
        println!("cargo:rustc-env=STRUKTAL_GIT_SHA={s}");
    }
}
