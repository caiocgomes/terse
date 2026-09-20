//! Shared test-support helpers for integration/e2e suites.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub mod pdf;

/// Real XeLaTeX/Biber invocations in this pinned environment share
/// per-user TeX cache state (`TEXMFVAR`/biber's own cache) that is not
/// safe for concurrent processes; parallel `cargo test` threads each
/// spawning `biber` at once produced sporadic nonzero exits with no
/// content-level cause. Every e2e test that runs a real engine pass
/// holds this lock for its invocation, serializing them within one test
/// binary without serializing the whole suite (log/text/parse-only
/// assertions elsewhere still run in parallel).
pub static ENGINE_LOCK: Mutex<()> = Mutex::new(());

pub fn tempdir(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir().join(format!(
        "terse-e2e-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&base).unwrap();
    base
}

/// Creates `dir` containing one `#!/bin/sh` script per name, each with the
/// executable bit set, so discovery treats them as present tools without
/// any real TeX. Returns `dir`.
pub fn executable_dir(dir: &std::path::Path, names: &[&str]) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    for name in names {
        let path = dir.join(name);
        fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    dir.to_path_buf()
}

/// Installs a copy of `tests/fixtures/toolchain/fake-prefix/` at `dest`
/// with the lock year replaced by `year`, setting executable bits
/// explicitly (file modes are not reliable across checkouts).
pub fn install_fake_prefix(dest: &std::path::Path, year: &str) -> PathBuf {
    let src = fixtures_dir().join("toolchain").join("fake-prefix");
    executable_dir(&dest.join("bin").join("fake-platform"), &["xelatex", "biber", "kpsewhich"]);
    let lock = fs::read_to_string(src.join("terse-toolchain.lock.json"))
        .unwrap()
        .replace("\"texlive_year\": \"2025\"", &format!("\"texlive_year\": \"{year}\""));
    fs::write(dest.join("terse-toolchain.lock.json"), lock).unwrap();
    fs::copy(src.join(".terse-owned.json"), dest.join(".terse-owned.json")).unwrap();
    dest.to_path_buf()
}

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

pub fn tiny_png() -> Vec<u8> {
    fs::read(fixtures_dir().join("assets/tiny.png")).expect("tiny.png fixture must exist")
}

pub fn themes_fixture_dir() -> PathBuf {
    fixtures_dir().join("themes")
}

pub fn full_paper_fixture_dir() -> PathBuf {
    fixtures_dir().join("full-paper")
}

/// The engine executables and prepared environment a direct engine
/// invocation in an e2e test must use: resolved exactly as `terse build`
/// resolves them (managed prefix when installed and matching the profile
/// year, else `PATH`), so the "compile without Terse" gates run in the
/// pinned image, which has no TeX on `PATH`. Tests that isolate the user
/// TeX tree override `TEXMF*` after applying `env`.
pub struct ResolvedEngine {
    pub xelatex: Option<PathBuf>,
    pub biber: Option<PathBuf>,
    pub env: Vec<(String, String)>,
}

pub fn resolved_engine() -> ResolvedEngine {
    let host = terse_cli::toolchain::HostEnv::capture();
    let profile = terse_core::artifact::profile::resolve_profile("texlive-2025-xelatex").unwrap();
    let tc = terse_cli::toolchain::resolve(&terse_cli::toolchain::ToolchainSelector::Auto, &profile.texlive_year, &host)
        .expect("automatic toolchain resolution never fails");
    let env = terse_cli::build::child_env_for(&tc, &host);
    ResolvedEngine { xelatex: tc.xelatex.clone(), biber: tc.biber.clone(), env }
}

/// A `Command` for the resolved engine executable with the prepared
/// environment applied and nothing inherited.
pub fn engine_command(program: &std::path::Path, env: &[(String, String)]) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    cmd.env_clear().envs(env.iter().cloned());
    cmd
}
