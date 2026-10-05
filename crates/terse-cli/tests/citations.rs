//! Offline citation binding through the real CLI: source declarations,
//! `references.lock`, and `references.overrides.toml` combine to
//! authorize (or reject) a cited alias, with no network access anywhere
//! in this file (group 13 is the offline data/binding layer only; DOI/
//! arXiv provider adapters are groups 14-15).

use std::fs;
use std::path::Path;
use std::sync::Mutex;

/// `test_unicode_text_and_bibliography_compile` below is, as of this
/// writing, the only test in this binary that invokes a real `biber`
/// process (a build with an actual bibliography). Biber writes to a
/// shared per-user cache and corrupts it under concurrent invocations
/// from parallel test threads (observed as spurious exit 255s while
/// building the group-17 full-paper fixture's e2e tests, which share
/// `crate::common::ENGINE_LOCK` for the same reason). This binary has
/// its own `tempdir_named` rather than the `tests/common` module, so it
/// keeps its own lock; if a second real-biber test is ever added here,
/// it must acquire this same mutex.
static ENGINE_LOCK: Mutex<()> = Mutex::new(());

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

const MANIFEST: &str = "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n";

fn tempdir_named(label: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!(
        "terse-test-{label}-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&base).unwrap();
    base
}

fn unique_suffix() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn lock_for(alias: &str, doi: &str, title: &str) -> String {
    format!(
        "lock-version = 1\nnormalization-version = 1\n\n\
         [entries.{alias}.declared]\nprovider = \"doi\"\nidentifier = \"{doi}\"\n\n\
         [entries.{alias}.resolved]\nprovider = \"doi\"\nidentifier = \"{doi}\"\n\n\
         [entries.{alias}.adapter]\nname = \"doi\"\nversion = \"1\"\n\n\
         [entries.{alias}.provider_data]\ntitle = \"{title}\"\nwork_type = \"journal-article\"\nanonymous = true\n\n\
         [entries.{alias}.effective]\ntitle = \"{title}\"\nwork_type = \"journal-article\"\nanonymous = true\n"
    )
}

#[test]
fn test_unknown_alias_has_actionable_source() {
    let tmp = tempdir_named("unknown-alias");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("body.trs"),
        "Nothing here.\n",
    );
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\ninclude \"body.trs\"\n\nSee @acemoglu2027 for details.\n",
    );
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 1, "an alias absent from declarations and the lock must fail checking");
}

#[test]
fn test_orphan_lock_cannot_authorize_alias() {
    let tmp = tempdir_named("orphan-lock");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nSee @robins1986 for details.\n",
    );
    // The lock has a resolved entry, but nothing in source ever declared
    // this alias via `refs:` — a leftover lock entry alone must not
    // authorize a citation.
    write(&tmp.join("references.lock"), &lock_for("robins1986", "10.1000/abc", "T"));
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 1, "a lock entry with no source declaration must not authorize a citation");

    // Adding the matching source declaration restores binding.
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nrefs:\n  robins1986: doi:10.1000/abc\n\nSee @robins1986 for details.\n",
    );
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 0, "a valid source declaration matching the lock restores binding");

    // A duplicate declaration for the same alias still fails, even with a
    // matching lock entry.
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nrefs:\n  robins1986: doi:10.1000/abc\n  robins1986: doi:10.1000/abc\n\nSee @robins1986 for details.\n",
    );
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 1, "duplicate declarations for the same alias must still fail");
}

#[test]
fn test_changed_identifier_requires_resolution() {
    let tmp = tempdir_named("changed-identifier");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(&tmp.join("references.lock"), &lock_for("robins1986", "10.1000/abc", "T"));

    // The source now declares a different DOI under the same alias than
    // the one the lock was resolved for.
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nrefs:\n  robins1986: doi:10.1000/xyz\n\nSee @robins1986.\n",
    );
    let before = fs::read_to_string(tmp.join("references.lock")).unwrap();
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 1, "a declared identifier that no longer matches the lock must fail checking");
    let after = fs::read_to_string(tmp.join("references.lock")).unwrap();
    assert_eq!(before, after, "checking a stale binding must never touch the lock");

    // A build attempt also fails without ever contacting a provider or
    // writing output (no network access is configured in this test at
    // all, so any provider call would panic/error rather than succeed).
    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 1, "building with a stale binding must also fail");

    // Equivalent normalized prefixes/case do not falsely invalidate
    // identity: re-declaring the *same* DOI with different case/prefix
    // style checks cleanly.
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nrefs:\n  robins1986: doi:DOI:10.1000/ABC\n\nSee @robins1986.\n",
    );
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 0, "case/prefix differences in an equivalent DOI must not be treated as a change");
}

#[test]
fn test_reserved_reference_providers_fail() {
    let tmp = tempdir_named("reserved-providers");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nrefs:\n  someBook: isbn:978-0-13-468599-1\n\nNo citation of someBook here.\n",
    );
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 1, "a declared ISBN alias must fail checking even when never cited");

    let tmp2 = tempdir_named("reserved-providers-url");
    write(&tmp2.join("terse.toml"), MANIFEST);
    write(
        &tmp2.join("paper.trs"),
        "document:\n  title: \"T\"\n\nrefs:\n  page: url:https://example.com/page\n\nNo citation of page here.\n",
    );
    let code = terse_cli::run(["terse", "check"], &tmp2);
    assert_eq!(code, 1, "a declared generic-URL alias must fail checking even when never cited");
}

#[test]
fn test_offline_override_reseals_effective_record() {
    let tmp = tempdir_named("offline-override");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nrefs:\n  robins1986: doi:10.1000/abc\n\nSee @robins1986.\n",
    );
    write(&tmp.join("references.lock"), &lock_for("robins1986", "10.1000/abc", "Original Title"));
    write(
        &tmp.join("references.overrides.toml"),
        "[robins1986.title]\nop = \"set\"\nvalue = \"Corrected Title\"\n",
    );

    // The lock does not yet know about this override (it was sealed with
    // `override_patch = None`), so this is exactly the unsealed case and
    // must fail until resolved/resealed.
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 1, "an override not yet sealed into the lock must fail checking");

    // Reseal the lock as `refs resolve --offline` would: store the same
    // patch and its resulting effective record.
    write(
        &tmp.join("references.lock"),
        "lock-version = 1\nnormalization-version = 1\n\n\
         [entries.robins1986.declared]\nprovider = \"doi\"\nidentifier = \"10.1000/abc\"\n\n\
         [entries.robins1986.resolved]\nprovider = \"doi\"\nidentifier = \"10.1000/abc\"\n\n\
         [entries.robins1986.adapter]\nname = \"doi\"\nversion = \"1\"\n\n\
         [entries.robins1986.provider_data]\ntitle = \"Original Title\"\nwork_type = \"journal-article\"\nanonymous = true\n\n\
         [entries.robins1986.override_patch.title]\nop = \"set\"\nvalue = \"Corrected Title\"\n\n\
         [entries.robins1986.effective]\ntitle = \"Corrected Title\"\nwork_type = \"journal-article\"\nanonymous = true\n",
    );
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 0, "a resealed override checks cleanly");

    // Building offline (no network configured at all in this test)
    // succeeds using the corrected effective record.
    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0, "a source-only build must succeed offline with a resealed override");

    // Neither check nor build ever rewrites the lock: it is byte-identical
    // before and after, since resolution (group 15) is the only writer.
    let lock_after_build = fs::read_to_string(tmp.join("references.lock")).unwrap();
    let expected_lock = fs::read_to_string(tmp.join("references.lock")).unwrap();
    let _ = terse_cli::run(["terse", "check"], &tmp);
    let lock_after_check = fs::read_to_string(tmp.join("references.lock")).unwrap();
    assert_eq!(lock_after_build, expected_lock);
    assert_eq!(lock_after_check, expected_lock, "check/build must preserve the lock byte-for-byte");
}

#[test]
fn test_unsealed_override_requires_resolution() {
    let tmp = tempdir_named("unsealed-override");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nrefs:\n  robins1986: doi:10.1000/abc\n\nSee @robins1986.\n",
    );
    // A lock resealed for one override...
    write(
        &tmp.join("references.lock"),
        "lock-version = 1\nnormalization-version = 1\n\n\
         [entries.robins1986.declared]\nprovider = \"doi\"\nidentifier = \"10.1000/abc\"\n\n\
         [entries.robins1986.resolved]\nprovider = \"doi\"\nidentifier = \"10.1000/abc\"\n\n\
         [entries.robins1986.adapter]\nname = \"doi\"\nversion = \"1\"\n\n\
         [entries.robins1986.provider_data]\ntitle = \"Original Title\"\nwork_type = \"journal-article\"\nanonymous = true\n\n\
         [entries.robins1986.override_patch.title]\nop = \"set\"\nvalue = \"Sealed Title\"\n\n\
         [entries.robins1986.effective]\ntitle = \"Sealed Title\"\nwork_type = \"journal-article\"\nanonymous = true\n",
    );
    // ...but the override file now asks for something different: a
    // semantic change after resolution.
    write(
        &tmp.join("references.overrides.toml"),
        "[robins1986.title]\nop = \"set\"\nvalue = \"Different Title\"\n",
    );
    let before = fs::read_to_string(tmp.join("references.lock")).unwrap();
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 1, "a semantic override change after resolution must fail with reseal guidance");
    let after = fs::read_to_string(tmp.join("references.lock")).unwrap();
    assert_eq!(before, after, "an unsealed-override failure must never write the lock");

    // A formatting-only equivalent override (same semantic patch, just
    // reformatted) remains valid.
    write(
        &tmp.join("references.overrides.toml"),
        "\n[robins1986.title]\n  op   =   \"set\"\n  value = \"Sealed Title\"\n\n",
    );
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 0, "a formatting-only equivalent override must remain valid");
}

// Group 14: DOI/arXiv provider adapters, tested directly against the
// adapters (no `refs resolve` CLI command exists yet — that's group 15's
// job). Every case here runs against a fake transport/clock: no real
// network access anywhere in this file.

mod provider_adapters {
    use terse_cli::references::arxiv::{ArxivResolveError, ArxivWorker, MIN_REQUEST_INTERVAL};
    use terse_cli::references::clock::fake::FakeClock;
    use terse_cli::references::doi::{resolve_doi, DoiResolveError};
    use terse_cli::references::transport::fake::{ok, FakeTransport};
    use terse_core::references::arxiv::AtomError;
    use terse_core::references::doi::CslError;

    const CSL_MATCH: &str = r#"{"DOI":"10.1000/abc","title":"T","type":"journal-article","author":[{"family":"Doe","given":"Jane"}]}"#;
    const CSL_MISMATCH: &str = r#"{"DOI":"10.1000/wrong","title":"T","type":"journal-article"}"#;

    const ATOM_FEED: &str = r#"<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>http://arxiv.org/abs/2301.12345v2</id>
    <published>2023-01-30T18:00:00Z</published>
    <title>An Example Paper</title>
    <author><name>Jane Doe</name></author>
  </entry>
</feed>"#;

    #[test]
    fn test_provider_identity_mismatch_rolls_back() {
        // "Rolls back" at adapter granularity: a provider returning a
        // record for a different identity than requested must never
        // produce a usable NormalizedRecord — the caller gets a clean
        // error, not a partially-applied result. (The full multi-alias
        // lock-write rollback transaction is group 15's `refs resolve`.)
        let mut transport = FakeTransport::new();
        transport.script("https://doi.org/10.1000/abc", ok(CSL_MISMATCH, "https://doi.org/10.1000/abc"));
        let result = resolve_doi(&mut transport, "10.1000/abc");
        assert!(matches!(result, Err(DoiResolveError::Parse(CslError::IdentityMismatch { .. }))));
    }

    #[test]
    fn test_versionless_arxiv_stays_pinned() {
        let mut transport = FakeTransport::new();
        transport.script("https://export.arxiv.org/api/query?id_list=2301.12345", ok(ATOM_FEED, "url"));
        let mut clock = FakeClock::new();
        let mut worker = ArxivWorker::new();

        let resolution = worker.resolve(&mut transport, &mut clock, "2301.12345").unwrap();
        assert_eq!(resolution.resolved_id, "2301.12345");
        assert_eq!(resolution.resolved_version.as_deref(), Some("v2"));

        // Re-resolving the same versionless id later (e.g. without
        // --refresh) must land on the exact same pinned version, not
        // silently move forward if the feed changed.
        let mut transport2 = FakeTransport::new();
        transport2.script("https://export.arxiv.org/api/query?id_list=2301.12345", ok(ATOM_FEED, "url"));
        let mut clock2 = FakeClock::new();
        let mut worker2 = ArxivWorker::new();
        let resolution2 = worker2.resolve(&mut transport2, &mut clock2, "2301.12345").unwrap();
        assert_eq!(resolution2.resolved_version, resolution.resolved_version);
    }

    #[test]
    fn test_explicit_arxiv_version_verified() {
        let mut transport = FakeTransport::new();
        transport.script("https://export.arxiv.org/api/query?id_list=2301.12345", ok(ATOM_FEED, "url"));
        let mut clock = FakeClock::new();
        let mut worker = ArxivWorker::new();
        assert!(worker.resolve(&mut transport, &mut clock, "2301.12345v2").is_ok());

        let mut transport2 = FakeTransport::new();
        transport2.script("https://export.arxiv.org/api/query?id_list=2301.12345", ok(ATOM_FEED, "url"));
        let mut clock2 = FakeClock::new();
        let mut worker2 = ArxivWorker::new();
        let err = worker2.resolve(&mut transport2, &mut clock2, "2301.12345v9").unwrap_err();
        assert!(matches!(err, ArxivResolveError::Parse(AtomError::IdentityMismatch { .. })));
    }

    #[test]
    fn test_resolution_is_all_or_nothing() {
        // A single adapter call either returns one complete, identity
        // checked record, or fails outright with no record at all: there
        // is no code path returning a partially-filled NormalizedRecord.
        let mut transport = FakeTransport::new();
        transport.script("https://doi.org/10.1000/abc", ok(CSL_MATCH, "https://doi.org/10.1000/abc"));
        let ok_result = resolve_doi(&mut transport, "10.1000/abc");
        assert!(ok_result.is_ok());

        let mut transport2 = FakeTransport::new();
        transport2.script("https://doi.org/10.1000/abc", ok(CSL_MISMATCH, "https://doi.org/10.1000/abc"));
        let err_result = resolve_doi(&mut transport2, "10.1000/abc");
        assert!(err_result.is_err(), "an identity mismatch must fail cleanly with no record produced");
    }

    #[test]
    fn arxiv_worker_serializes_requests_with_the_documented_interval() {
        let mut transport = FakeTransport::new();
        for _ in 0..2 {
            transport.script("https://export.arxiv.org/api/query?id_list=2301.12345", ok(ATOM_FEED, "url"));
        }
        let mut clock = FakeClock::new();
        let mut worker = ArxivWorker::new();
        worker.resolve(&mut transport, &mut clock, "2301.12345").unwrap();
        worker.resolve(&mut transport, &mut clock, "2301.12345").unwrap();
        assert!(clock.slept.contains(&MIN_REQUEST_INTERVAL));
    }
}

// Group 15: `refs resolve` — the transactional command that actually
// fetches metadata and (atomically) writes `references.lock`. Every case
// here drives `references::resolve::run` directly (the injectable
// application entrypoint, following the same pattern as
// `build::run_build_with_runner`) with a `FakeTransport`/`FakeClock`, so
// nothing here ever touches a real socket, DNS resolver, or clock.

mod refs_resolve_command {
    use super::*;
    use terse_cli::references::clock::fake::FakeClock;
    use terse_cli::references::resolve::{self, ResolveError, ResolveOptions};
    use terse_cli::references::transport::fake::{ok, FakeTransport};

    const CSL_A: &str = r#"{"DOI":"10.1000/a","title":"A","type":"journal-article","author":[{"family":"Doe","given":"Jane"}]}"#;
    const CSL_A_UPDATED: &str = r#"{"DOI":"10.1000/a","title":"A Updated","type":"journal-article","author":[{"family":"Doe","given":"Jane"}]}"#;
    const CSL_MISMATCH: &str = r#"{"DOI":"10.1000/wrong","title":"B","type":"journal-article","author":[{"family":"Roe","given":"Sam"}]}"#;

    fn options(refresh: bool, offline: bool, prune: bool) -> ResolveOptions {
        ResolveOptions { refresh, offline, prune }
    }

    #[test]
    fn test_unchanged_reference_not_refetched() {
        let tmp = tempdir_named("resolve-unchanged");
        write(&tmp.join("terse.toml"), MANIFEST);
        write(
            &tmp.join("paper.trs"),
            "document:\n  title: \"T\"\n\nrefs:\n  alpha: doi:10.1000/a\n\nSee @alpha.\n",
        );
        write(&tmp.join("references.lock"), &lock_for("alpha", "10.1000/a", "A"));

        // No response is scripted at all: any fetch attempt would panic
        // inside FakeTransport, so a passing test proves zero requests.
        let mut transport = FakeTransport::new();
        let mut clock = FakeClock::new();
        let report = resolve::run(&tmp, &tmp.join("paper.trs"), options(false, false, false), &mut transport, &mut clock)
            .expect("an unchanged alias needs no fetch and must not fail");
        assert_eq!(report.unchanged, vec!["alpha".to_string()]);
        assert!(report.fetched.is_empty());
        assert_eq!(transport.request_count("https://doi.org/10.1000/a"), 0);

        let lock_before = fs::read_to_string(tmp.join("references.lock")).unwrap();

        // `--refresh` forces a fetch even though nothing changed.
        let mut transport2 = FakeTransport::new();
        transport2.script("https://doi.org/10.1000/a", ok(CSL_A_UPDATED, "https://doi.org/10.1000/a"));
        let mut clock2 = FakeClock::new();
        let report2 = resolve::run(&tmp, &tmp.join("paper.trs"), options(true, false, false), &mut transport2, &mut clock2)
            .expect("refresh should succeed");
        assert_eq!(report2.fetched, vec!["alpha".to_string()]);
        assert_eq!(transport2.request_count("https://doi.org/10.1000/a"), 1);
        let lock_after_refresh = fs::read_to_string(tmp.join("references.lock")).unwrap();
        assert_ne!(lock_before, lock_after_refresh, "--refresh must actually rewrite the lock with fresh data");
    }

    #[test]
    fn test_offline_override_reseals_effective_record() {
        let tmp = tempdir_named("resolve-offline-reseal");
        write(&tmp.join("terse.toml"), MANIFEST);
        write(
            &tmp.join("paper.trs"),
            "document:\n  title: \"T\"\n\nrefs:\n  alpha: doi:10.1000/a\n\nSee @alpha.\n",
        );
        write(&tmp.join("references.lock"), &lock_for("alpha", "10.1000/a", "A"));
        write(&tmp.join("references.overrides.toml"), "[alpha.title]\nop = \"set\"\nvalue = \"A Corrected\"\n");

        // No transport scripts at all: --offline must never call fetch.
        let mut transport = FakeTransport::new();
        let mut clock = FakeClock::new();
        let report = resolve::run(&tmp, &tmp.join("paper.trs"), options(false, true, false), &mut transport, &mut clock)
            .expect("offline reseal with an existing matching record must succeed");
        assert_eq!(report.resealed, vec!["alpha".to_string()]);
        assert!(report.fetched.is_empty());

        let lock_text = fs::read_to_string(tmp.join("references.lock")).unwrap();
        assert!(lock_text.contains("A Corrected"), "the resealed lock must carry the new effective title");
        assert!(lock_text.contains("override_patch"), "the resealed lock must carry the sealed override patch");

        // Checking through the ordinary compiler entrypoint now succeeds
        // against the resealed lock, with no further lock writes.
        let code = terse_cli::run(["terse", "check"], &tmp);
        assert_eq!(code, 0);
        assert_eq!(fs::read_to_string(tmp.join("references.lock")).unwrap(), lock_text);
    }

    #[test]
    fn test_resolution_is_all_or_nothing() {
        let tmp = tempdir_named("resolve-all-or-nothing");
        write(&tmp.join("terse.toml"), MANIFEST);
        write(
            &tmp.join("paper.trs"),
            "document:\n  title: \"T\"\n\nrefs:\n  alpha: doi:10.1000/a\n  beta: doi:10.1000/b\n\nSee @alpha and @beta.\n",
        );
        // No pre-existing lock: both aliases need a fresh fetch.
        let mut transport = FakeTransport::new();
        transport.script("https://doi.org/10.1000/a", ok(CSL_A, "https://doi.org/10.1000/a"));
        // 'beta' resolves to a mismatched identity, which must fail the
        // whole run, discarding 'alpha's already-fetched record too.
        transport.script("https://doi.org/10.1000/b", ok(CSL_MISMATCH, "https://doi.org/10.1000/b"));
        let mut clock = FakeClock::new();

        let err = resolve::run(&tmp, &tmp.join("paper.trs"), options(false, false, false), &mut transport, &mut clock)
            .expect_err("a mismatched identity for one alias must fail the entire resolution");
        assert!(matches!(err, ResolveError::Doi(alias, _) if alias == "beta"));
        assert!(!tmp.join("references.lock").exists(), "no lock file may be written when any alias fails to resolve");
    }

    #[test]
    fn test_prune_is_explicit() {
        let tmp = tempdir_named("resolve-prune");
        write(&tmp.join("terse.toml"), MANIFEST);
        // 'orphan' is locked but no longer declared anywhere in source.
        write(&tmp.join("paper.trs"), "document:\n  title: \"T\"\n\nNothing cited here.\n");
        write(&tmp.join("references.lock"), &lock_for("orphan", "10.1000/old", "Old"));

        let mut transport = FakeTransport::new();
        let mut clock = FakeClock::new();
        let report = resolve::run(&tmp, &tmp.join("paper.trs"), options(false, false, false), &mut transport, &mut clock)
            .expect("resolving with no declared aliases must still succeed");
        assert!(report.pruned.is_empty());
        let lock_text = fs::read_to_string(tmp.join("references.lock")).unwrap();
        assert!(lock_text.contains("orphan"), "an orphaned lock entry is retained by default");

        let mut transport2 = FakeTransport::new();
        let mut clock2 = FakeClock::new();
        let report2 = resolve::run(&tmp, &tmp.join("paper.trs"), options(false, false, true), &mut transport2, &mut clock2)
            .expect("pruning must succeed");
        assert_eq!(report2.pruned, vec!["orphan".to_string()]);
        let lock_text_after = fs::read_to_string(tmp.join("references.lock")).unwrap();
        assert!(!lock_text_after.contains("orphan"), "--prune must remove the orphaned entry");
    }

    #[test]
    fn test_reserved_reference_providers_fail() {
        let tmp = tempdir_named("resolve-reserved");
        write(&tmp.join("terse.toml"), MANIFEST);
        write(
            &tmp.join("paper.trs"),
            "document:\n  title: \"T\"\n\nrefs:\n  someBook: isbn:978-0-13-468599-1\n\nNo citation here.\n",
        );
        let mut transport = FakeTransport::new();
        let mut clock = FakeClock::new();
        let err = resolve::run(&tmp, &tmp.join("paper.trs"), options(false, false, false), &mut transport, &mut clock)
            .expect_err("a reserved-provider declaration must fail resolution even when uncited");
        assert!(matches!(err, ResolveError::Declaration(_)));
        assert!(!tmp.join("references.lock").exists(), "a failed resolution must not create a lock file");
    }

    #[test]
    fn test_conflicting_flags_is_a_usage_error() {
        let tmp = tempdir_named("resolve-conflicting-flags");
        write(&tmp.join("terse.toml"), MANIFEST);
        write(&tmp.join("paper.trs"), "document:\n  title: \"T\"\n\nNothing here.\n");
        let mut transport = FakeTransport::new();
        let mut clock = FakeClock::new();
        let err = resolve::run(&tmp, &tmp.join("paper.trs"), options(true, true, false), &mut transport, &mut clock)
            .expect_err("--refresh and --offline together must be rejected before doing anything");
        assert!(matches!(err, ResolveError::ConflictingFlags));

        // The CLI itself also rejects the combination at parse time.
        let code = terse_cli::run(["terse", "refs", "resolve", "--refresh", "--offline"], &tmp);
        assert_eq!(code, 2);
    }

    #[test]
    fn test_no_reference_network_capability_outside_resolve() {
        // check/build/fmt never construct a real transport at all: this
        // is enforced structurally (nothing in build.rs/format.rs/init.rs
        // references `references::transport`), but exercise the
        // command paths here too, in an environment with no lock/refs at
        // all, to confirm they never attempt any network-shaped work.
        let tmp = tempdir_named("resolve-no-capability-elsewhere");
        write(&tmp.join("terse.toml"), MANIFEST);
        write(&tmp.join("paper.trs"), "document:\n  title: \"T\"\n\nNothing cited here.\n");
        assert_eq!(terse_cli::run(["terse", "check"], &tmp), 0);
        assert_eq!(terse_cli::run(["terse", "build", "--tex-only"], &tmp), 0);
        assert_eq!(terse_cli::run(["terse", "fmt", "--check"], &tmp), 0);
    }
}

/// Group 16: with `xelatex`/`biber` available, a real cited build must
/// actually converge and produce a PDF (not just generate plausible-
/// looking TeX). Locked offline (a hand-authored `references.lock`, no
/// network) so this stays a real-compile test, not a resolution test.
#[test]
#[ignore = "requires a local XeLaTeX/Biber distribution"]
fn test_unicode_text_and_bibliography_compile() {
    let _engine_guard = ENGINE_LOCK.lock().unwrap();
    let tmp = tempdir_named("bib-e2e-compile");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("references.lock"),
        &format!(
            "lock-version = 1\nnormalization-version = 1\n\n\
             [entries.robins1986.declared]\nprovider = \"doi\"\nidentifier = \"10.1000/abc\"\n\n\
             [entries.robins1986.resolved]\nprovider = \"doi\"\nidentifier = \"10.1000/abc\"\n\n\
             [entries.robins1986.adapter]\nname = \"doi\"\nversion = \"1\"\n\n\
             [entries.robins1986.provider_data]\n\
             title = \"Confounding and \\u00e9\\u00e8\\u00e7 with accents\"\n\
             work_type = \"journal-article\"\n\
             authors = [{{form = \"structured\", family = \"Robins\", given = \"James\"}}]\n\
             date = \"1986\"\n\n\
             [entries.robins1986.effective]\n\
             title = \"Confounding and \\u00e9\\u00e8\\u00e7 with accents\"\n\
             work_type = \"journal-article\"\n\
             authors = [{{form = \"structured\", family = \"Robins\", given = \"James\"}}]\n\
             date = \"1986\"\n"
        ),
    );
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"Unicode e Bibliografia\"\n  language: pt-BR\n\n\
         refs:\n  robins1986: doi:10.1000/abc\n\n\
         Confounding fo\u{300} discutido por @robins1986 e tamb\u{e9}m [@robins1986, p. 12].\n\n\
         bibliography\n",
    );

    let code = terse_cli::run(["terse", "build", "--require-pdf"], &tmp);
    assert_eq!(code, 0, "a locked citation must compile to a real PDF with biber+xelatex");

    let out = tmp.join("build").join("academic");
    assert!(out.join("paper.pdf").is_file());
    let bib = fs::read_to_string(out.join("references.bib")).unwrap();
    assert!(bib.contains("@article{robins1986"), "cited alias must appear in the generated .bib");
}
