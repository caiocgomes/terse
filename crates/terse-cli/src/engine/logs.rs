//! Engine result/log interpretation: distinguishes startup failures,
//! nonzero exits, and nonconvergence. None of these map to a `.trs`
//! source position, so their diagnostics carry no primary span rather
//! than inventing one.

use terse_core::diagnostic::{Diagnostic, Severity};

use crate::engine::{CompileFailure, PassKind};

fn pass_name(kind: PassKind) -> &'static str {
    match kind {
        PassKind::Xelatex => "xelatex",
        PassKind::Biber => "biber",
    }
}

pub fn interpret_failure(failure: &CompileFailure) -> Diagnostic {
    match failure {
        CompileFailure::ToolStartFailed { program } => Diagnostic {
            severity: Severity::Error,
            code: "E-LATEX-010",
            message: format!("could not start '{program}': the executable was not found or could not be run"),
            primary: None,
            related: Vec::new(),
            help: Some("install the documented TeX toolchain, or build with --tex-only".to_string()),
        },
        CompileFailure::NonZeroExit { kind, code } => Diagnostic {
            severity: Severity::Error,
            code: "E-LATEX-011",
            message: format!(
                "{} exited with a nonzero status ({})",
                pass_name(*kind),
                code.map(|c| c.to_string()).unwrap_or_else(|| "unknown".to_string())
            ),
            primary: None,
            related: Vec::new(),
            help: Some("inspect the engine log for the failing pass".to_string()),
        },
        CompileFailure::Timeout { kind, secs } => Diagnostic {
            severity: Severity::Error,
            code: "E-LATEX-014",
            message: format!(
                "{} did not finish within {secs}s and its process tree was terminated",
                pass_name(*kind)
            ),
            primary: None,
            related: Vec::new(),
            help: Some(
                "a hung engine usually means a stale biber PAR cache or a font cache rebuild; run `terse doctor`"
                    .to_string(),
            ),
        },
        CompileFailure::PassLimitExceeded => Diagnostic {
            severity: Severity::Error,
            code: "E-LATEX-012",
            message: format!(
                "references did not converge within {} engine passes",
                crate::engine::MAX_ENGINE_PASSES
            ),
            primary: None,
            related: Vec::new(),
            help: None,
        },
        CompileFailure::MissingGlyph => Diagnostic {
            severity: Severity::Error,
            code: "E-LATEX-013",
            message: "the selected font has no glyph for a character used in the document".to_string(),
            primary: None,
            related: Vec::new(),
            help: Some("use a character present in the theme's fonts, or declare a font that covers it".to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_start_failure_has_no_primary_span() {
        let failure = CompileFailure::ToolStartFailed {
            program: "xelatex".to_string(),
        };
        let diag = interpret_failure(&failure);
        assert_eq!(diag.code, "E-LATEX-010");
        assert!(diag.primary.is_none());
        assert!(diag.help.is_some());
    }
}
