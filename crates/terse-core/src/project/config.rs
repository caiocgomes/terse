//! Closed, versioned `terse.toml` manifest schema.

use serde::Deserialize;
use std::collections::BTreeMap;

pub const SUPPORTED_FORMAT_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigError {
    Toml(String),
    UnsupportedFormatVersion(i64),
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectSection {
    pub entry: String,
    #[serde(default = "default_output")]
    pub output: String,
}

fn default_output() -> String {
    "build".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PdfMode {
    Auto,
    TexOnly,
    RequirePdf,
}

impl Default for PdfMode {
    fn default() -> Self {
        PdfMode::Auto
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LatexSection {
    #[serde(default = "default_engine")]
    pub engine: String,
    #[serde(default)]
    pub pdf: PdfMode,
    /// `auto`, `system`, `managed`, or an explicit directory. Selects where
    /// engine executables are located; never affects generated text.
    #[serde(default)]
    pub toolchain: Option<String>,
    #[serde(default)]
    pub support_files: Vec<String>,
    #[serde(default)]
    pub packages: Vec<String>,
}

fn default_engine() -> String {
    "xelatex".to_string()
}

impl Default for LatexSection {
    fn default() -> Self {
        Self {
            engine: default_engine(),
            pdf: PdfMode::default(),
            toolchain: None,
            support_files: Vec::new(),
            packages: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReferencesSection {
    #[serde(default = "default_lockfile")]
    pub lockfile: String,
    #[serde(default)]
    pub overrides: Option<String>,
}

fn default_lockfile() -> String {
    "references.lock".to_string()
}

impl Default for ReferencesSection {
    fn default() -> Self {
        Self {
            lockfile: default_lockfile(),
            overrides: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArxivExportSection {
    pub profile: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ExportSection {
    #[serde(default)]
    pub arxiv: Option<ArxivExportSection>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(rename = "format-version")]
    pub format_version: i64,
    pub project: ProjectSection,
    #[serde(default)]
    pub themes: BTreeMap<String, String>,
    #[serde(default)]
    pub latex: LatexSection,
    #[serde(default)]
    pub references: ReferencesSection,
    #[serde(default)]
    pub export: ExportSection,
}

pub fn parse_manifest(text: &str) -> Result<Manifest, ConfigError> {
    let manifest: Manifest = toml::from_str(text).map_err(|e| ConfigError::Toml(e.to_string()))?;
    if manifest.format_version != SUPPORTED_FORMAT_VERSION {
        return Err(ConfigError::UnsupportedFormatVersion(
            manifest.format_version,
        ));
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parses_minimal_manifest() {
        let text = "format-version = 1\n\n[project]\nentry = \"paper.trs\"\n";
        let manifest = parse_manifest(text).expect("valid manifest");
        assert_eq!(manifest.project.entry, "paper.trs");
        assert_eq!(manifest.project.output, "build");
        assert_eq!(manifest.latex.engine, "xelatex");
        assert_eq!(manifest.latex.pdf, PdfMode::Auto);
    }

    #[test]
    fn test_rejects_unknown_key() {
        let text = "format-version = 1\n\n[project]\nentry = \"paper.trs\"\nbogus = true\n";
        assert!(matches!(parse_manifest(text), Err(ConfigError::Toml(_))));
    }

    #[test]
    fn test_rejects_unsupported_format_version() {
        let text = "format-version = 2\n\n[project]\nentry = \"paper.trs\"\n";
        assert!(matches!(
            parse_manifest(text),
            Err(ConfigError::UnsupportedFormatVersion(2))
        ));
    }

    #[test]
    fn test_rejects_malformed_toml() {
        let text = "not = [valid";
        assert!(matches!(parse_manifest(text), Err(ConfigError::Toml(_))));
    }
}
