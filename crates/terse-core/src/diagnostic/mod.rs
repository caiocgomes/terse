//! Shared diagnostic model returned by every compiler stage.

use crate::source::SourceSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedInfo {
    pub message: String,
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub primary: Option<SourceSpan>,
    pub related: Vec<RelatedInfo>,
    pub help: Option<String>,
}

#[cfg(test)]
mod tests;

impl Diagnostic {
    pub fn error(code: &'static str, message: impl Into<String>, primary: SourceSpan) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            primary: Some(primary),
            related: Vec::new(),
            help: None,
        }
    }

    pub fn warning(code: &'static str, message: impl Into<String>, primary: SourceSpan) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message: message.into(),
            primary: Some(primary),
            related: Vec::new(),
            help: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_related(mut self, message: impl Into<String>, span: Option<SourceSpan>) -> Self {
        self.related.push(RelatedInfo {
            message: message.into(),
            span,
        });
        self
    }
}

/// Versioned envelope format for `--json` diagnostic output. The version
/// number is part of the contract: a consumer parsing this JSON should
/// check it before assuming field shapes are stable across releases.
pub const JSON_ENVELOPE_VERSION: u32 = 1;
