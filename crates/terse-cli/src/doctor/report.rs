//! The doctor report: a versioned, serializable list of checks. Checks
//! carry no source positions, so they are their own envelope rather than
//! diagnostics forced into `JsonDiagnostic`; the envelope version is the
//! shared one so consumers track a single schema number.

use serde::Serialize;

use crate::diagnostics::JSON_ENVELOPE_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub status: CheckStatus,
    pub summary: String,
    pub evidence: Vec<String>,
    pub probable_cause: Option<String>,
    pub fix_command: Option<String>,
    pub code: Option<&'static str>,
    pub auto_fixable: bool,
}

impl Check {
    pub fn new(id: &'static str, status: CheckStatus, summary: impl Into<String>) -> Self {
        Self {
            id,
            status,
            summary: summary.into(),
            evidence: Vec::new(),
            probable_cause: None,
            fix_command: None,
            code: None,
            auto_fixable: false,
        }
    }

    pub fn ok(id: &'static str, summary: impl Into<String>) -> Self {
        Self::new(id, CheckStatus::Ok, summary)
    }

    pub fn info(id: &'static str, summary: impl Into<String>) -> Self {
        Self::new(id, CheckStatus::Info, summary)
    }

    pub fn warn(id: &'static str, summary: impl Into<String>, code: &'static str) -> Self {
        let mut c = Self::new(id, CheckStatus::Warn, summary);
        c.code = Some(code);
        c
    }

    pub fn fail(id: &'static str, summary: impl Into<String>, code: &'static str) -> Self {
        let mut c = Self::new(id, CheckStatus::Fail, summary);
        c.code = Some(code);
        c
    }

    pub fn with_evidence(mut self, line: impl Into<String>) -> Self {
        self.evidence.push(line.into());
        self
    }

    pub fn with_cause(mut self, cause: impl Into<String>) -> Self {
        self.probable_cause = Some(cause.into());
        self
    }

    pub fn with_fix(mut self, command: impl Into<String>, auto_fixable: bool) -> Self {
        self.fix_command = Some(command.into());
        self.auto_fixable = auto_fixable;
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolchainSummary {
    pub source: &'static str,
    pub reason: String,
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub version: u32,
    pub terse_version: String,
    pub toolchain: ToolchainSummary,
    pub checks: Vec<Check>,
    /// Repairs applied under `--fix`, one line each.
    pub actions: Vec<String>,
}

impl DoctorReport {
    pub fn new(toolchain: ToolchainSummary) -> Self {
        Self {
            version: JSON_ENVELOPE_VERSION,
            terse_version: env!("CARGO_PKG_VERSION").to_string(),
            toolchain,
            checks: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub fn has_failure(&self) -> bool {
        self.checks.iter().any(|c| c.status == CheckStatus::Fail)
    }
}

/// `0` with no failing check, `1` otherwise. Usage errors (`2`) never
/// reach a report.
pub fn exit_code(report: &DoctorReport) -> i32 {
    if report.has_failure() {
        1
    } else {
        0
    }
}

pub fn render_json(report: &DoctorReport) -> String {
    serde_json::to_string(report).expect("doctor report is always serializable")
}

pub fn render_human(report: &DoctorReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "terse doctor (terse {}; toolchain: {}, {})\n",
        report.terse_version, report.toolchain.source, report.toolchain.reason
    ));
    for c in &report.checks {
        let tag = match c.status {
            CheckStatus::Ok => "[ok]  ",
            CheckStatus::Warn => "[warn]",
            CheckStatus::Fail => "[FAIL]",
            CheckStatus::Info => "[info]",
        };
        match c.code {
            Some(code) if c.status != CheckStatus::Ok => out.push_str(&format!("{tag} {}: {} ({code})\n", c.id, c.summary)),
            _ => out.push_str(&format!("{tag} {}: {}\n", c.id, c.summary)),
        }
        for e in &c.evidence {
            out.push_str(&format!("        {e}\n"));
        }
        if let Some(cause) = &c.probable_cause {
            out.push_str(&format!("        probable cause: {cause}\n"));
        }
        if let Some(fix) = &c.fix_command {
            let suffix = if c.auto_fixable { " (applied by --fix)" } else { "" };
            out.push_str(&format!("        fix: {fix}{suffix}\n"));
        }
    }
    for a in &report.actions {
        out.push_str(&format!("fixed: {a}\n"));
    }
    out
}
