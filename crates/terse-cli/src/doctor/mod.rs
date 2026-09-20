//! `terse doctor`: every toolchain check runs through the bounded process
//! runner with a timeout, reports evidence and a probable cause with a
//! per-OS fix, and `--fix` applies only an enumerated allowlist of safe
//! repairs. Never contacts the network.

pub mod checks;
pub mod fix;
pub mod report;

use std::path::Path;

use crate::engine::ProcessRunner;
use crate::toolchain::{HostEnv, ToolchainSelector};

pub use report::{exit_code, render_human, render_json, Check, CheckStatus, DoctorReport, ToolchainSummary};

#[derive(Debug, Clone)]
pub struct DoctorOptions {
    pub fix: bool,
    pub json: bool,
    pub toolchain: Option<ToolchainSelector>,
}

/// Runs every check and returns the report without printing anything.
pub fn run_doctor(cwd: &Path, opts: &DoctorOptions, host: &HostEnv, runner: &mut dyn ProcessRunner) -> DoctorReport {
    checks::run_all(cwd, opts, host, runner)
}

/// The CLI entry: prints the report (JSON on stdout only, otherwise human
/// text on stdout) and returns the exit code.
pub fn run_doctor_cli(cwd: &Path, opts: &DoctorOptions, host: &HostEnv, runner: &mut dyn ProcessRunner) -> i32 {
    if opts.json {
        eprintln!("doctor: running checks (toolchain: {})", opts.toolchain.as_ref().map(|t| format!("{t:?}")).unwrap_or_else(|| "auto".to_string()));
    }
    let report = run_doctor(cwd, opts, host, runner);
    if opts.json {
        println!("{}", render_json(&report));
    } else {
        print!("{}", render_human(&report));
    }
    exit_code(&report)
}
