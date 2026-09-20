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
        CompileFailure::UndefinedReferences => Diagnostic {
            severity: Severity::Error,
            code: "E-LATEX-015",
            message: "the document still has undefined references or citations after the engine converged".to_string(),
            primary: None,
            related: Vec::new(),
            help: Some(
                "check `\\ref`/`\\cite` targets in raw TeX blocks; authored cross-references and citations are validated before generation".to_string(),
            ),
        },
    }
}

/// Whether the *settled* document pass still reports unresolved
/// references or citations.
///
/// Only the final XeLaTeX pass is inspected, which is the one the rerun
/// loop stopped on. Every intermediate pass of an ordinary citation build
/// legitimately reports undefined references, because `.aux` and `.bbl`
/// have not been read back yet — scanning all passes (as the missing-glyph
/// check reasonably does, glyph coverage being pass-independent) would
/// fail every document that cites anything.
pub fn has_undefined_references(passes: &[crate::engine::PassResult]) -> bool {
    let Some(last) = passes
        .iter()
        .rev()
        .find(|p| p.kind == crate::engine::PassKind::Xelatex)
    else {
        return false;
    };
    let log = String::from_utf8_lossy(&last.outcome.stdout);
    // Matched per line, not across the whole log: a converged build's log
    // mentions citations in many places and "undefined" in unrelated
    // engine chatter, so a whole-log conjunction reports a failure for
    // every document that cites anything. The engine wraps long lines, so
    // the summary warnings are matched as whole phrases and the per-item
    // form is matched by its two markers co-occurring on one line.
    log.lines().any(|line| {
        line.contains("There were undefined references")
            || line.contains("There were undefined citations")
            || (line.contains("Citation") && line.contains("undefined"))
            || (line.contains("Reference") && line.contains("undefined"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{PassKind, PassResult, ProcessOutcome};

    fn pass(kind: PassKind, log: &str) -> PassResult {
        PassResult {
            kind,
            outcome: ProcessOutcome::success_with_log(log.as_bytes().to_vec()),
        }
    }

    #[test]
    fn test_undefined_references_scan_inspects_only_the_final_pass() {
        // Every intermediate XeLaTeX pass of a citation build reports
        // undefined references before `.aux`/`.bbl` are read back, so a
        // scan over all passes would fail every ordinary citation build.
        let converged = vec![
            pass(PassKind::Xelatex, "LaTeX Warning: There were undefined references.\nPlease (re)run\n"),
            pass(PassKind::Biber, "biber output"),
            pass(PassKind::Xelatex, "all resolved\n"),
        ];
        assert!(
            !has_undefined_references(&converged),
            "references that resolve by the final pass are not a failure"
        );

        let unresolved = vec![
            pass(PassKind::Xelatex, "first\n"),
            pass(PassKind::Xelatex, "LaTeX Warning: There were undefined references.\n"),
        ];
        assert!(has_undefined_references(&unresolved), "the settled pass still reports them");

        let undefined_citation = vec![pass(
            PassKind::Xelatex,
            "LaTeX Warning: Citation 'smith2020' on page 1 undefined on input line 4.\n",
        )];
        assert!(has_undefined_references(&undefined_citation));

        // A Biber pass running last (no XeLaTeX after it) must not be
        // mistaken for the settled document pass.
        let biber_last = vec![
            pass(PassKind::Xelatex, "clean\n"),
            pass(PassKind::Biber, "There were undefined references.\n"),
        ];
        assert!(!has_undefined_references(&biber_last));
    }


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
