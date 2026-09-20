//! Full-paper fixture e2e: multi-file expansion, cross-file references,
//! and resource limits, exercised through the real binary and a real
//! XeLaTeX/Biber toolchain.

use std::fs;

use crate::common::{full_paper_fixture_dir, tempdir};

fn copy_fixture_to(dest: &std::path::Path) {
    fn copy_dir(src: &std::path::Path, dest: &std::path::Path) {
        fs::create_dir_all(dest).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let target = dest.join(entry.file_name());
            if path.is_dir() {
                if path.file_name().unwrap() == "build" {
                    continue;
                }
                copy_dir(&path, &target);
            } else {
                fs::copy(&path, &target).unwrap();
            }
        }
    }
    copy_dir(&full_paper_fixture_dir(), dest);
}

#[test]
#[ignore = "requires a local XeLaTeX/Biber distribution"]
fn test_multi_file_equation_link_valid() {
    let _engine_guard = crate::common::ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempdir("full-paper-xref");
    copy_fixture_to(&tmp);

    let code = terse_cli::run(["terse", "build", "--require-pdf", "--theme", "academic"], &tmp);
    assert_eq!(code, 0, "the full-paper fixture must compile cleanly under academic");

    let pdf = tmp.join("build/academic/paper.pdf");
    let text = crate::common::pdf::extract_text(&pdf);
    assert!(
        !text.contains("??"),
        "an unresolved cross-file reference would render as a literal '??' placeholder:\n{text}"
    );
    // The forward reference from paper.trs (included before results.trs)
    // to `eq-euler` (defined in the later-included results.trs) must
    // resolve to a real number, not the raw label or a placeholder.
    assert!(
        text.contains("forward reference to 1"),
        "the cross-file forward reference to the equation must resolve to its number:\n{text}"
    );
}

#[test]
#[ignore = "requires a local XeLaTeX/Biber distribution"]
fn test_cross_file_equation_number_converges() {
    let _engine_guard = crate::common::ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempdir("full-paper-eq-converge");
    copy_fixture_to(&tmp);

    let code = terse_cli::run(["terse", "build", "--require-pdf", "--theme", "academic"], &tmp);
    assert_eq!(code, 0);

    let pdf = tmp.join("build/academic/paper.pdf");
    let text = crate::common::pdf::extract_text(&pdf);
    // The equation itself renders with a visible number, and the earlier
    // backward reference to the background section resolves to its
    // heading text via \hyperref rather than a bare label.
    assert!(text.contains("Building on Background"), "backward cross-file heading reference:\n{text}");
}

