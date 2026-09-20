use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn tempdir(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "terse-toolchain-unit-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_executable(path: &Path) {
    std::fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn fake_prefix(root: &Path, year: &str) -> PathBuf {
    let prefix = root.join("prefix");
    let bin = prefix.join("bin").join("fake-platform");
    std::fs::create_dir_all(&bin).unwrap();
    for tool in ["xelatex", "biber", "kpsewhich"] {
        write_executable(&bin.join(tool));
    }
    let lock = ToolchainLock {
        version: lock::LOCK_VERSION,
        texlive_year: year.to_string(),
        repository: "https://example.invalid/tlnet-final/".to_string(),
        install_tl_sha512: "0".repeat(128),
        packages: vec![],
        terse_version: "0.0.0".to_string(),
        platform: "fake-platform".to_string(),
    };
    std::fs::write(prefix.join(lock::LOCK_FILE_NAME), lock.encode()).unwrap();
    prefix
}

fn linux_host(path_dir: &Path, xdg: &Path) -> HostEnv {
    let mut host = HostEnv::minimal(HostOs::Linux, path_dir.as_os_str().to_os_string());
    host.xdg_data_home = Some(xdg.to_path_buf());
    host
}

#[test]
fn test_windows_pathext_discovery() {
    let dir = tempdir("pathext");
    std::fs::write(dir.join("xelatex.exe"), b"MZ").unwrap();
    let mut host = HostEnv::minimal(HostOs::Windows, dir.as_os_str().to_os_string());
    host.pathext = vec![".COM".to_string(), ".EXE".to_string(), ".BAT".to_string()];

    let found = find_tool_in("xelatex", &[dir.clone()], &host).expect("xelatex.exe resolves");
    assert_eq!(found, dir.join("xelatex.exe"));

    // A directory named like the tool is never a match.
    let other = tempdir("pathext-dir");
    std::fs::create_dir_all(other.join("xelatex")).unwrap();
    assert!(find_tool_in("xelatex", &[other.clone()], &host).is_none());

    // An empty PATHEXT falls back to the documented default.
    host.pathext = host::DEFAULT_PATHEXT.iter().map(|s| s.to_string()).collect();
    assert_eq!(find_tool_in("xelatex", &[dir.clone()], &host), Some(dir.join("xelatex.exe")));
}

#[test]
fn test_managed_prefix_year_mismatch_is_skipped() {
    let root = tempdir("year-mismatch");
    let xdg = root.join("xdg");
    let prefix = paths::managed_prefix(&linux_host(&root, &xdg), "2025").unwrap();
    std::fs::create_dir_all(prefix.parent().unwrap()).unwrap();
    let built = fake_prefix(&root, "2026");
    std::fs::rename(&built, &prefix).unwrap();

    let path_dir = root.join("path");
    std::fs::create_dir_all(&path_dir).unwrap();
    write_executable(&path_dir.join("xelatex"));
    let host = linux_host(&path_dir, &xdg);

    let tc = resolve(&ToolchainSelector::Auto, "2025", &host).unwrap();
    assert_eq!(tc.source, ToolchainSource::System);
    assert!(tc.reason.contains("2026"), "reason names the lock year: {}", tc.reason);
    assert_eq!(tc.xelatex, Some(path_dir.join("xelatex")));

    // Explicitly asking for it is a configuration error, not a silent fallback.
    let err = resolve(&ToolchainSelector::Managed, "2025", &host).unwrap_err();
    assert_eq!(err.code(), "E-TOOL-016");
}

#[test]
fn test_managed_bin_dir_leads_child_path() {
    let root = tempdir("bin-first");
    let prefix = fake_prefix(&root, "2025");
    let host = HostEnv::minimal(HostOs::Linux, "/usr/bin:/bin");
    let tc = resolve(&ToolchainSelector::Dir(prefix.clone()), "2025", &host).unwrap();
    let texmf = TexmfDirs::under(&root.join("texmf"));
    let env = prepare_child_env(&tc, &host, &texmf);
    let path = env.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone()).unwrap();
    let first = std::env::split_paths(&path).next().unwrap();
    assert_eq!(first, prefix.join("bin").join("fake-platform"));
    assert!(path.ends_with("/usr/bin:/bin"));

    let system = resolve(&ToolchainSelector::System, "2025", &host).unwrap();
    let env = prepare_child_env(&system, &host, &texmf);
    let path = env.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone()).unwrap();
    assert_eq!(path, "/usr/bin:/bin");
}

#[test]
fn test_toolchain_lock_records_pins() {
    let lock = ToolchainLock {
        version: lock::LOCK_VERSION,
        texlive_year: "2025".to_string(),
        repository: "https://ftp.math.utah.edu/pub/tex/historic/systems/texlive/2025/tlnet-final/".to_string(),
        install_tl_sha512: "ab".repeat(64),
        packages: vec![
            lock::LockedPackage { name: "xetex".to_string(), revision: "70000".to_string() },
            lock::LockedPackage { name: "biblatex".to_string(), revision: "69999".to_string() },
        ],
        terse_version: "0.1.0".to_string(),
        platform: "x86_64-linux".to_string(),
    };
    let text = lock.encode();
    let keys: Vec<usize> = [
        "\"version\"",
        "\"texlive_year\"",
        "\"repository\"",
        "\"install_tl_sha512\"",
        "\"packages\"",
        "\"terse_version\"",
        "\"platform\"",
    ]
    .iter()
    .map(|k| text.find(k).unwrap_or_else(|| panic!("missing key {k}")))
    .collect();
    assert!(keys.windows(2).all(|w| w[0] < w[1]), "fixed field order");
    assert!(!text.contains("time"), "no timestamps");
    assert!(text.find("\"biblatex\"").unwrap() < text.find("\"xetex\"").unwrap(), "packages sorted by name");

    let decoded = ToolchainLock::decode(&text).unwrap();
    assert_eq!(decoded.encode(), text, "round trip is byte-identical");
    assert_eq!(decoded.packages.len(), 2);

    let bad = text.replace("\"version\": 1", "\"version\": 9");
    assert!(matches!(ToolchainLock::decode(&bad), Err(lock::LockError::UnsupportedVersion(9))));
}

#[test]
fn test_manifest_selector_rejects_bare_words() {
    let root = Path::new("/project");
    assert_eq!(ToolchainSelector::parse_manifest("auto", root).unwrap(), ToolchainSelector::Auto);
    assert_eq!(
        ToolchainSelector::parse_manifest("./tex", root).unwrap(),
        ToolchainSelector::Dir(PathBuf::from("/project/tex"))
    );
    let err = ToolchainSelector::parse_manifest("bogus", root).unwrap_err();
    assert_eq!(err.code(), "E-CONFIG-009");
}

#[test]
fn test_data_dir_per_os() {
    let mut host = HostEnv::minimal(HostOs::MacOs, "");
    host.home = Some(PathBuf::from("/Users/x"));
    // No space anywhere in the path: XeTeX runs its output driver through
    // `sh` using its own location, and `Application Support` breaks it.
    assert_eq!(paths::data_dir(&host), Some(PathBuf::from("/Users/x/Library/terse")));
    assert!(!paths::managed_prefix(&host, "2025").unwrap().to_string_lossy().contains(' '));
    let mut host = HostEnv::minimal(HostOs::Linux, "");
    host.home = Some(PathBuf::from("/home/x"));
    assert_eq!(paths::data_dir(&host), Some(PathBuf::from("/home/x/.local/share/terse")));
    host.xdg_data_home = Some(PathBuf::from("/data"));
    assert_eq!(paths::data_dir(&host), Some(PathBuf::from("/data/terse")));
    let mut host = HostEnv::minimal(HostOs::Windows, "");
    host.local_app_data = Some(PathBuf::from("C:\\Users\\x\\AppData\\Local"));
    assert_eq!(paths::data_dir(&host).unwrap().file_name().unwrap(), "terse");
    let host = HostEnv::minimal(HostOs::Linux, "");
    assert_eq!(paths::data_dir(&host), None);
}

#[test]
fn test_install_tl_profile_is_portable_and_infraonly() {
    let golden = include_str!("../../../../tests/fixtures/toolchain/install-tl.profile");
    let texmf = TexmfDirs::under(Path::new("/data/texmf"));
    let generated = install::install_profile_text(Path::new("/staging"), &texmf);
    assert_eq!(generated, golden, "generated install-tl profile matches the golden");
    assert!(generated.contains("selected_scheme scheme-infraonly\n"));
    assert!(generated.contains("instopt_portable 1\n"));
    assert!(generated.contains("instopt_adjustpath 0\n"));
    assert!(generated.contains("tlpdbopt_install_docfiles 0\n"));
    assert!(generated.contains("tlpdbopt_install_srcfiles 0\n"));
    assert!(generated.contains("tlpdbopt_autobackup 0\n"));
    assert!(generated.contains("TEXDIR /staging\n"), "TEXDIR is the staging root, which becomes the prefix");
    assert!(generated.contains("TEXMFHOME /data/texmf/home\n"));
}

#[test]
fn test_tlmgr_installed_list_parses_name_and_localrev() {
    let text = "amsfonts,77677\nbiber,75738\n\nxetex,73850\nmalformed-line\n";
    let packages = tlmgr::parse_installed(text);
    let names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["amsfonts", "biber", "xetex"]);
    assert_eq!(packages[1].revision, "75738");
}

#[test]
fn test_install_refuses_prefix_with_whitespace() {
    use terse_core::toolchain::spec::toolchain_spec_for;

    let root = tempdir("space-prefix");
    let spaced = root.join("Application Support").join("texlive-2025");
    let mut host = HostEnv::minimal(HostOs::MacOs, "");
    host.home = Some(root.clone());
    let spec = toolchain_spec_for("2025").unwrap();
    let opts = install::InstallOptions { prefix: spaced.clone(), offline_from: None, update: false };
    let mut runner = crate::engine::FakeProcessRunner::new(vec![]);
    let err = install::run_install(&opts, &spec, &host, &mut runner, None, &mut |_| {}).unwrap_err();
    assert!(matches!(err, install::InstallError::Usage(_)), "{err:?}");
    assert_eq!(err.exit_code(), 2);
    assert!(runner.invocations.is_empty());
    assert!(!spaced.exists());
}
