//! Versioned artifact plan: the complete set of generated files for a
//! source-only build, plus a manifest with content hashes for determinism
//! checks.

pub mod assets;
pub mod export;
pub mod profile;

use std::collections::BTreeMap;

use crate::latex;
use crate::references::record::NormalizedRecord;
use crate::semantic::{has_bibliography, ParsedModule};
use crate::theme::ResolvedTheme;
use sha2::{Digest, Sha256};

pub const ARTIFACT_SCHEMA_VERSION: u32 = 1;

/// Reserved logical paths produced by `plan_source_artifacts` itself.
/// Declared local support files (`[latex] support-files`) may not reuse
/// any of these names.
pub const RESERVED_GENERATED_NAMES: &[&str] = &[
    "paper.tex",
    "terse-style.sty",
    "references.bib",
    "COMPILE.txt",
    "build-manifest.json",
    latex::source_map::SOURCE_MAP_FILE_NAME,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportFileCollision(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub logical_path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactManifest {
    pub schema_version: u32,
    pub files: Vec<GeneratedFile>,
}

/// Plans the complete source-only file set for a build: `paper.tex`,
/// `terse-style.sty`, an empty `references.bib` (no citations in this
/// milestone), `COMPILE.txt`, and a `build-manifest.json` describing them.
pub fn plan_source_artifacts(module: &ParsedModule, theme: &ResolvedTheme) -> ArtifactManifest {
    plan_source_artifacts_with_support(module, theme, &[], &[], &BTreeMap::new(), &BTreeMap::new())
        .expect("no support files declared")
}

/// Same as [`plan_source_artifacts`], additionally requiring declared
/// built-in packages (already validated by [`latex::validate_packages`])
/// in `terse-style.sty`, copying `support_files` (declaring-file-
/// relative bytes already read by the application layer) into the plan
/// under their own declared logical path, and serializing `cited` (the
/// intersection of authored citations and bound reference aliases,
/// computed by the caller) as `references.bib`. Rejects a support file
/// whose logical path collides with a generated name.
///
/// `file_paths` maps each module's [`crate::source::FileId`] to its
/// root-relative logical path, so `paper.map.json` can name its origins
/// the way a reader would. The core never learns paths by itself; the
/// application layer already holds this table and passes it in as data.
pub fn plan_source_artifacts_with_support(
    module: &ParsedModule,
    theme: &ResolvedTheme,
    extra_packages: &[&str],
    support_files: &[(String, Vec<u8>)],
    cited: &BTreeMap<String, NormalizedRecord>,
    file_paths: &BTreeMap<crate::source::FileId, String>,
) -> Result<ArtifactManifest, SupportFileCollision> {
    for (path, _) in support_files {
        if RESERVED_GENERATED_NAMES
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(path))
        {
            return Err(SupportFileCollision(path.clone()));
        }
    }

    let bibliography_language = if has_bibliography(module) {
        Some(module.metadata.as_ref().map(|m| m.language.as_str()).unwrap_or("en"))
    } else {
        None
    };
    // The mapped generator produces the same `paper.tex` bytes as the
    // plain one (asserted in `source_map`'s own tests); taking the map
    // here is what makes it a published deliverable rather than a
    // capability that exists only in tests.
    let (document, source_map) = latex::source_map::generate_document_with_map(module, theme);
    let mut files = vec![
        GeneratedFile {
            logical_path: "paper.tex".to_string(),
            bytes: document.into_bytes(),
        },
        GeneratedFile {
            logical_path: latex::source_map::SOURCE_MAP_FILE_NAME.to_string(),
            bytes: source_map.to_json(file_paths).into_bytes(),
        },
        GeneratedFile {
            logical_path: "terse-style.sty".to_string(),
            bytes: latex::generate_style(theme, extra_packages, bibliography_language).into_bytes(),
        },
        GeneratedFile {
            logical_path: "references.bib".to_string(),
            bytes: latex::bibliography::generate_bib(cited).into_bytes(),
        },
        GeneratedFile {
            logical_path: "COMPILE.txt".to_string(),
            bytes: latex::generate_compile_instructions(theme).into_bytes(),
        },
    ];
    for (path, bytes) in support_files {
        files.push(GeneratedFile {
            logical_path: path.clone(),
            bytes: bytes.clone(),
        });
    }
    files.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));

    let manifest_bytes = build_manifest_json(&files).into_bytes();
    files.push(GeneratedFile {
        logical_path: "build-manifest.json".to_string(),
        bytes: manifest_bytes,
    });
    files.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));

    Ok(ArtifactManifest {
        schema_version: ARTIFACT_SCHEMA_VERSION,
        files,
    })
}

fn build_manifest_json(files: &[GeneratedFile]) -> String {
    let mut entries = Vec::new();
    for f in files {
        let mut hasher = Sha256::new();
        hasher.update(&f.bytes);
        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        entries.push(format!(
            "    {{\"path\": \"{}\", \"size\": {}, \"sha256\": \"{}\"}}",
            f.logical_path,
            f.bytes.len(),
            hex
        ));
    }
    format!(
        "{{\n  \"schema-version\": {},\n  \"files\": [\n{}\n  ]\n}}\n",
        ARTIFACT_SCHEMA_VERSION,
        entries.join(",\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::ParsedModule;
    use crate::source::FileId;
    use crate::theme;

    #[test]
    fn test_plan_is_deterministic_and_complete() {
        let module = ParsedModule {
            file_id: FileId(0),
            metadata: None,
            references: vec![],
            blocks: vec![],
        };
        let theme = theme::academic();
        let plan1 = plan_source_artifacts(&module, &theme);
        let plan2 = plan_source_artifacts(&module, &theme);
        assert_eq!(plan1, plan2);

        let names: Vec<&str> = plan1
            .files
            .iter()
            .map(|f| f.logical_path.as_str())
            .collect();
        assert!(names.contains(&"paper.tex"));
        assert!(names.contains(&"terse-style.sty"));
        assert!(names.contains(&"references.bib"));
        assert!(names.contains(&"COMPILE.txt"));
        assert!(names.contains(&"build-manifest.json"));
    }
}
