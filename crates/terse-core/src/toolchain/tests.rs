use std::path::{Path, PathBuf};

use super::par::par_cache_dir;

#[test]
fn test_par_cache_dir_hex_encodes_username() {
    assert_eq!(
        par_cache_dir(Path::new("/tmp"), "cgomes"),
        PathBuf::from("/tmp/par-63676f6d6573")
    );
    // Lowercase hex regardless of the username's case; bytes, not chars.
    assert_eq!(
        par_cache_dir(Path::new("/var/folders/x/T"), "Ana"),
        PathBuf::from("/var/folders/x/T/par-416e61")
    );
    assert_eq!(par_cache_dir(Path::new("/t"), "é"), PathBuf::from("/t/par-c3a9"));
    assert_eq!(par_cache_dir(Path::new("/t"), ""), PathBuf::from("/t/par-"));
}

const TOOLCHAIN_TLPDB_EXCERPT: &str = include_str!("../../../../tests/fixtures/toolchain/tlpdb-excerpt.txt");

#[test]
fn test_toolchain_spec_parses_and_pins_sha512() {
    use super::spec::{toolchain_spec_for, UnknownToolchainSpec};

    let spec = toolchain_spec_for("2025").expect("the 2025 spec is embedded");
    assert_eq!(spec.texlive_year, "2025");
    assert_eq!(spec.export_profile, "texlive-2025-xelatex");
    assert_eq!(spec.schema_version, 1);
    assert!(!spec.repositories.is_empty(), "at least one pinned repository");
    for repo in &spec.repositories {
        assert!(repo.starts_with("https://") && repo.ends_with('/'), "repository URLs are https directories: {repo}");
    }
    assert!(spec.install_tl.unix.url.starts_with("https://"));
    assert!(spec.install_tl.unix.url.ends_with("install-tl-unx.tar.gz"));
    let sha = &spec.install_tl.unix.sha512;
    assert_eq!(sha.len(), 128, "SHA-512 hex digest");
    assert!(sha.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));

    assert!(spec.closure.derived.contains(&"unicode-math".to_string()), "derived by compiling, not guessed");
    assert!(spec.closure.derived.contains(&"amsfonts".to_string()));
    assert!(spec.closure.manual.contains(&"biber".to_string()));
    assert!(spec.closure.manual.contains(&"scheme-infraonly".to_string()));
    assert!(!spec.closure.derived_from.is_empty());

    let mut sorted = spec.closure.derived.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted, spec.closure.derived, "derived list is sorted and unique");

    let install = spec.tlmgr_packages();
    assert!(install.iter().all(|p| !p.starts_with("scheme-")), "the scheme is selected by install-tl, not tlmgr");
    assert!(install.contains(&"biber".to_string()) && install.contains(&"booktabs".to_string()));
    assert!(install.windows(2).all(|w| w[0] < w[1]), "tlmgr package list is sorted and unique");

    assert_eq!(toolchain_spec_for("2019"), Err(UnknownToolchainSpec("2019".to_string())));
}

#[test]
fn test_owning_packages_maps_runfiles_to_package() {
    use super::tlpdb::owning_packages;

    let files = [
        "texmf-dist/tex/latex/booktabs/booktabs.sty",
        "texmf-dist/tex/generic/hyph-utf8/loadhyph/loadhyph-pt.tex",
        "bin/universal-darwin/biber",
        "texmf-dist/doc/latex/booktabs/booktabs.pdf",
        "texmf-dist/source/latex/booktabs/booktabs.dtx",
        "texmf-dist/tex/latex/nothing/here.sty",
    ];
    let owners = owning_packages(TOOLCHAIN_TLPDB_EXCERPT, &files);
    assert_eq!(owners.get("texmf-dist/tex/latex/booktabs/booktabs.sty").map(String::as_str), Some("booktabs"), "RELOC/ maps to texmf-dist/");
    assert_eq!(
        owners.get("texmf-dist/tex/generic/hyph-utf8/loadhyph/loadhyph-pt.tex").map(String::as_str),
        Some("hyphen-portuguese")
    );
    assert_eq!(owners.get("bin/universal-darwin/biber").map(String::as_str), Some("biber.universal-darwin"), "binfiles are owned");
    assert!(!owners.contains_key("texmf-dist/doc/latex/booktabs/booktabs.pdf"), "docfiles are not runtime ownership");
    assert!(!owners.contains_key("texmf-dist/source/latex/booktabs/booktabs.dtx"), "srcfiles are skipped");
    assert!(!owners.contains_key("texmf-dist/tex/latex/nothing/here.sty"), "unknown files are absent, never guessed");
    assert_eq!(owners.len(), 3);
}
