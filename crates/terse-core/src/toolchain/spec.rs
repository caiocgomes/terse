//! The managed-toolchain provisioning profile: where a reproducible TeX
//! Live for the export profile comes from and which packages it needs.
//! Embedded at compile time from `crates/terse-core/profiles/`; never
//! fetched or refreshed at runtime.

use serde::Deserialize;

const TOOLCHAIN_TEXLIVE_2025: &str = include_str!("../../profiles/toolchain-texlive-2025.toml");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct InstallTlArtifact {
    pub url: String,
    pub sha512: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct InstallTl {
    pub unix: InstallTlArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Closure {
    #[serde(rename = "derived-from")]
    pub derived_from: String,
    pub derived: Vec<String>,
    pub manual: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ToolchainSpec {
    #[serde(rename = "schema-version")]
    pub schema_version: u32,
    #[serde(rename = "texlive-year")]
    pub texlive_year: String,
    #[serde(rename = "export-profile")]
    pub export_profile: String,
    pub repositories: Vec<String>,
    #[serde(rename = "install-tl")]
    pub install_tl: InstallTl,
    pub closure: Closure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownToolchainSpec(pub String);

/// Resolves the provisioning profile for a TeX Live year. Adding a new
/// year means adding a new `.toml` file and a new match arm, never
/// editing an existing profile's pins in place.
pub fn toolchain_spec_for(year: &str) -> Result<ToolchainSpec, UnknownToolchainSpec> {
    match year {
        "2025" => Ok(toml::from_str(TOOLCHAIN_TEXLIVE_2025)
            .expect("bundled toolchain-texlive-2025.toml must be valid at compile time")),
        other => Err(UnknownToolchainSpec(other.to_string())),
    }
}

impl ToolchainSpec {
    /// Everything `tlmgr install` receives after `install-tl` has laid
    /// down the scheme: derived and manual packages, sorted and unique,
    /// without the scheme itself (selected by `install-tl`).
    pub fn tlmgr_packages(&self) -> Vec<String> {
        let mut packages: Vec<String> = self
            .closure
            .derived
            .iter()
            .chain(self.closure.manual.iter())
            .filter(|p| !p.starts_with("scheme-"))
            .cloned()
            .collect();
        packages.sort();
        packages.dedup();
        packages
    }
}
