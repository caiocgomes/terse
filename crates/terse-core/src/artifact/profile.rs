//! Versioned, offline export compatibility profiles: static statements of
//! what an external toolchain generation is assumed to provide (engine,
//! packages, fonts, babel locales, bibliography backend). Profiles are
//! embedded at compile time from `crates/terse-core/profiles/*.toml` --
//! there is no online update mechanism, and an unknown profile name is
//! always a hard failure rather than a best-effort guess.

use serde::Deserialize;

/// The only profile Terse currently ships. Adding a new toolchain
/// generation means adding a new `.toml` file and a new match arm here,
/// never editing an existing profile's guarantees in place.
const TEXLIVE_2025_XELATEX: &str = include_str!("../../profiles/texlive-2025-xelatex.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct ExportProfile {
    pub name: String,
    pub engine: String,
    #[serde(rename = "bibliography-backend")]
    pub bibliography_backend: String,
    pub packages: Vec<String>,
    pub fonts: Vec<String>,
    #[serde(rename = "babel-languages")]
    pub babel_languages: Vec<String>,
    /// The TeX Live distribution year this profile's guarantees describe.
    /// Purely informational -- distinguishing a genuinely
    /// profile-verified compilation from one that merely happened to
    /// work locally is group 24's job, not this schema's.
    #[serde(rename = "texlive-year")]
    pub texlive_year: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownProfile(pub String);

/// Resolves a profile by its exact versioned name. Never fetches, never
/// falls back to "closest match" -- an unrecognized name is always
/// [`UnknownProfile`].
pub fn resolve_profile(name: &str) -> Result<ExportProfile, UnknownProfile> {
    match name {
        "texlive-2025-xelatex" => Ok(toml::from_str(TEXLIVE_2025_XELATEX)
            .expect("bundled texlive-2025-xelatex.toml must be valid at compile time")),
        other => Err(UnknownProfile(other.to_string())),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileViolation {
    UnsupportedPackage(String),
    UnsupportedFont(String),
    UnsupportedLanguage(String),
}

/// Checks a document's actual package/font/language requirements against
/// a profile's guarantees, returning every violation found (not just the
/// first) so a single export attempt reports its complete incompatibility
/// set.
pub fn check_requirements(
    profile: &ExportProfile,
    required_packages: &[&str],
    font_token: &str,
    babel_language: Option<&str>,
) -> Vec<ProfileViolation> {
    let mut violations = Vec::new();
    for pkg in required_packages {
        if !profile.packages.iter().any(|p| p == pkg) {
            violations.push(ProfileViolation::UnsupportedPackage(pkg.to_string()));
        }
    }
    if !profile.fonts.iter().any(|f| f == font_token) {
        violations.push(ProfileViolation::UnsupportedFont(font_token.to_string()));
    }
    if let Some(lang) = babel_language {
        if !profile.babel_languages.iter().any(|l| l == lang) {
            violations.push(ProfileViolation::UnsupportedLanguage(lang.to_string()));
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_profile_resolves_with_full_schema() {
        let profile = resolve_profile("texlive-2025-xelatex").unwrap();
        assert_eq!(profile.name, "texlive-2025-xelatex");
        assert_eq!(profile.engine, "xelatex");
        assert_eq!(profile.bibliography_backend, "biber");
        assert!(profile.packages.iter().any(|p| p == "hyperref"));
        assert!(profile.fonts.iter().any(|f| f == "tgheros"));
        assert!(profile.babel_languages.iter().any(|l| l == "pt-BR"));
    }

    /// `managed-toolchain` scenario "Profile packages equal the emitted
    /// set": the profile lists exactly the packages the style generator can
    /// emit, as sets, so `compiled-profile` never vouches for a package the
    /// generator never loads and never misses one it does.
    #[test]
    fn test_profile_packages_equal_emitted_set() {
        use std::collections::BTreeSet;
        let profile = resolve_profile("texlive-2025-xelatex").unwrap();
        let declared: BTreeSet<&str> = profile.packages.iter().map(String::as_str).collect();
        let emitted: BTreeSet<&str> = crate::latex::STYLE_PACKAGES.iter().copied().collect();
        assert_eq!(
            declared, emitted,
            "profile packages and STYLE_PACKAGES differ; only in profile: {:?}, only emitted: {:?}",
            declared.difference(&emitted).collect::<Vec<_>>(),
            emitted.difference(&declared).collect::<Vec<_>>()
        );
        assert!(!declared.contains("transparent"));
        assert_eq!(profile.packages.len(), declared.len(), "profile lists a package twice");
    }

    #[test]
    fn test_unknown_profile_name_fails_without_fallback() {
        assert_eq!(
            resolve_profile("texlive-2099-imaginary").unwrap_err(),
            UnknownProfile("texlive-2099-imaginary".to_string())
        );
    }

    #[test]
    fn test_requirements_report_every_violation() {
        let profile = resolve_profile("texlive-2025-xelatex").unwrap();
        let violations = check_requirements(&profile, &["hyperref", "made-up-pkg"], "made-up-font", Some("klingon"));
        assert_eq!(
            violations,
            vec![
                ProfileViolation::UnsupportedPackage("made-up-pkg".to_string()),
                ProfileViolation::UnsupportedFont("made-up-font".to_string()),
                ProfileViolation::UnsupportedLanguage("klingon".to_string()),
            ]
        );
    }

    #[test]
    fn test_requirements_pass_for_supported_set() {
        let profile = resolve_profile("texlive-2025-xelatex").unwrap();
        let violations = check_requirements(&profile, &["hyperref", "graphicx"], "tgheros", Some("en"));
        assert!(violations.is_empty());
    }
}
