//! Acceptance gate C: recorded DOI resolution through the application
//! entrypoint writes a real lock, then the actual binary builds fully
//! offline from it — real Biber/XeLaTeX bibliography compilation, zero
//! network calls in the build step.

use std::fs;

use terse_cli::references::clock::fake::FakeClock;
use terse_cli::references::resolve::{self, ResolveOptions};
use terse_cli::references::transport::fake::{ok, FakeTransport};

use crate::common::tempdir;

const MANIFEST: &str = "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n";
const RECORD: &str = r#"{"DOI":"10.1000/abc","title":"An Example Work","type":"journal-article","author":[{"family":"Doe","given":"Jane"}],"container-title":"Journal of Examples","issued":{"date-parts":[[2024]]}}"#;

#[test]
#[ignore = "requires a local XeLaTeX/Biber distribution"]
fn test_doi_resolution_then_offline_build() {
    let _engine_guard = crate::common::ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempdir("doi-resolve-then-offline");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(
        tmp.join("paper.trs"),
        concat!(
            "document:\n  title: \"DOI Then Offline\"\n\n",
            "refs:\n  doe2024: doi:10.1000/abc\n\n",
            "See @doe2024 for details.\n",
        ),
    )
    .unwrap();

    // No references.lock exists yet. Resolve it through the application
    // entrypoint against a recorded transport — no live network access.
    let mut transport = FakeTransport::new();
    transport.script("https://doi.org/10.1000/abc", ok(RECORD, "https://doi.org/10.1000/abc"));
    let mut clock = FakeClock::new();
    let report = resolve::run(
        &tmp,
        &tmp.join("paper.trs"),
        ResolveOptions { refresh: false, offline: false, prune: false },
        &mut transport,
        &mut clock,
    )
    .expect("resolution against a recorded transport must succeed");

    assert_eq!(report.fetched, vec!["doe2024".to_string()]);
    assert_eq!(
        transport.request_count("https://doi.org/10.1000/abc"),
        1,
        "exactly one metadata request for the one declared DOI"
    );
    assert!(tmp.join("references.lock").is_file(), "resolution must write a real lock file");
    let lock_bytes_after_resolve = fs::read(tmp.join("references.lock")).unwrap();

    // Now run the actual binary, fully offline: no transport is
    // constructed by `build` at all (structurally, not just by
    // convention — `run_build`/`check_diagnostics` never import
    // `references::transport`).
    let code = terse_cli::run(["terse", "build", "--require-pdf"], &tmp);
    assert_eq!(code, 0, "the build must succeed offline using only the just-written lock");
    assert_eq!(
        fs::read(tmp.join("references.lock")).unwrap(),
        lock_bytes_after_resolve,
        "an ordinary build must never rewrite the lock"
    );

    let out = tmp.join("build/academic");
    assert!(out.join("paper.pdf").is_file());
    let bib = fs::read_to_string(out.join("references.bib")).unwrap();
    assert!(bib.contains("An Example Work"));
    assert!(bib.contains("Doe, Jane"));

    let text = crate::common::pdf::extract_text(&out.join("paper.pdf"));
    assert!(text.contains("Doe"), "the compiled bibliography must render the resolved author:\n{text}");
}

/// Task group 20's pinned-toolchain determinism contract explicitly
/// requires comparing Biber's generated `.bbl` byte-for-byte across two
/// clean builds (PDF byte identity is out of scope; the `.bbl` text
/// artifact is not). Build the same locked citation fixture twice into
/// independent output trees and diff the intermediate `.bbl` that Biber
/// produces for each, by re-running Biber directly against each build's
/// own published `references.bib`/`main.tex` pair.
#[test]
#[ignore = "requires a local XeLaTeX/Biber distribution"]
fn test_bbl_is_reproducible_across_clean_builds() {
    let _engine_guard = crate::common::ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // The engine and Biber come from the same resolution `terse build`
    // uses (managed prefix when installed and matching the profile year,
    // else PATH) with the prepared environment, so this gate runs in the
    // pinned image, which has no TeX on PATH at all.
    let host = terse_cli::toolchain::HostEnv::capture();
    let profile = terse_core::artifact::profile::resolve_profile("texlive-2025-xelatex").unwrap();
    let tc = terse_cli::toolchain::resolve(&terse_cli::toolchain::ToolchainSelector::Auto, &profile.texlive_year, &host)
        .expect("toolchain resolution");
    let xelatex = tc.xelatex.clone().expect("xelatex must be resolvable for this gate");
    let biber = tc.biber.clone().expect("biber must be resolvable for this gate");
    let env = terse_cli::build::child_env_for(&tc, &host);

    let run_once = |label: &str| -> Vec<u8> {
        let tmp = tempdir(label);
        fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
        fs::write(
            tmp.join("paper.trs"),
            concat!(
                "document:\n  title: \"BBL Determinism\"\n\n",
                "refs:\n  doe2024: doi:10.1000/abc\n\n",
                "See @doe2024 for details.\n",
            ),
        )
        .unwrap();

        let mut transport = FakeTransport::new();
        transport.script("https://doi.org/10.1000/abc", ok(RECORD, "https://doi.org/10.1000/abc"));
        let mut clock = FakeClock::new();
        resolve::run(
            &tmp,
            &tmp.join("paper.trs"),
            ResolveOptions { refresh: false, offline: false, prune: false },
            &mut transport,
            &mut clock,
        )
        .expect("resolution against a recorded transport must succeed");

        assert_eq!(terse_cli::run(["terse", "build", "--tex-only"], &tmp), 0);

        let out = tmp.join("build/academic");
        let work = tempdir(&format!("{label}-biber-work"));
        for name in ["paper.tex", "references.bib", "terse-style.sty"] {
            fs::copy(out.join(name), work.join(name)).unwrap();
        }
        assert!(
            std::process::Command::new(&xelatex)
                .args(["-interaction=nonstopmode", "-halt-on-error", "paper.tex"])
                .env_clear()
                .envs(env.iter().cloned())
                .current_dir(&work)
                .status()
                .unwrap()
                .success(),
            "the first xelatex pass (to produce paper.bcf for Biber) must succeed"
        );
        assert!(
            std::process::Command::new(&biber)
                .arg("paper")
                .env_clear()
                .envs(env.iter().cloned())
                .current_dir(&work)
                .status()
                .unwrap()
                .success(),
            "biber must succeed against the published sources"
        );
        fs::read(work.join("paper.bbl")).expect("biber must produce paper.bbl")
    };

    let first = run_once("bbl-determinism-a");
    let second = run_once("bbl-determinism-b");
    assert_eq!(
        first, second,
        "Biber's generated .bbl must be byte-identical across two independent clean builds of the same locked citation data"
    );
}
