//! Declaring-file-relative asset naming: closed-extension validation and
//! deterministic, portable, collision-free logical basenames under
//! `assets/`. Reading bytes, resolving a declared path against its
//! declaring file's directory, and confining reads to the project root are
//! the application layer's responsibility (this module never touches the
//! filesystem); it only makes naming decisions over already-validated
//! strings, and walks the semantic tree to find every referenced figure
//! path so only used assets are ever planned.

use crate::project::paths;
use crate::semantic::{ListItem, Node, NodeKind};

pub const SUPPORTED_ASSET_EXTENSIONS: &[&str] = &["pdf", "png", "jpg", "jpeg"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// Not a valid relative logical path: absolute, drive/UNC-rooted,
    /// traversing, or a remote `scheme://` reference (which also fails
    /// this same lexical check, since it always contains a `:`).
    Invalid(String),
    UnsupportedExtension(String),
}

/// Validates a declared asset path before any filesystem access: it must
/// be a plain relative path (no remote scheme, no traversal) with one of
/// the closed supported extensions. There is no implicit format
/// conversion: an unsupported extension is always rejected, never
/// transcoded.
pub fn validate_asset_path(path: &str) -> Result<(), AssetError> {
    paths::normalize_logical(path).map_err(|_| AssetError::Invalid(path.to_string()))?;
    let ext = path
        .rsplit('.')
        .next()
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if path.rsplit_once('.').is_none() || !SUPPORTED_ASSET_EXTENSIONS.contains(&ext.as_str()) {
        return Err(AssetError::UnsupportedExtension(path.to_string()));
    }
    Ok(())
}

/// Recursively collects every figure `path`, in authored document order,
/// including duplicates (the caller deduplicates while disambiguating).
pub fn collect_figure_paths(nodes: &[Node]) -> Vec<String> {
    let mut out = Vec::new();
    collect_into(nodes, &mut out);
    out
}

fn collect_into(nodes: &[Node], out: &mut Vec<String>) {
    for node in nodes {
        match &node.kind {
            NodeKind::Figure { path, .. } => out.push(path.clone()),
            NodeKind::List { items, .. } => collect_from_items(items, out),
            NodeKind::TheoremLike { body, .. } | NodeKind::Proof { body, .. } => collect_into(body, out),
            NodeKind::Heading { .. }
            | NodeKind::Paragraph { .. }
            | NodeKind::Equation { .. }
            | NodeKind::Table { .. }
            | NodeKind::RawTex { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::Bibliography => {}
        }
    }
}

fn collect_from_items(items: &[ListItem], out: &mut Vec<String>) {
    for item in items {
        collect_into(&item.continuation, out);
    }
}

/// Assigns each distinct declaring-file-relative source path a stable
/// `assets/<basename>` logical path, preserving first-reference order.
/// Two distinct source paths that share a basename (assets from different
/// directories) are disambiguated with a numeric suffix before the
/// extension; the same source path referenced more than once always
/// reuses its first assigned name.
pub fn disambiguate_assets(refs: &[String]) -> Vec<(String, String)> {
    let mut assigned: Vec<(String, String)> = Vec::new();
    let mut used = std::collections::HashSet::new();

    for src in refs {
        if assigned.iter().any(|(s, _)| s == src) {
            continue;
        }
        let base = src.rsplit('/').next().unwrap_or(src);
        let (stem, ext) = split_ext(base);
        let mut candidate = format!("assets/{base}");
        let mut n = 1u32;
        while used.contains(&candidate) {
            candidate = format!("assets/{stem}-{n}.{ext}");
            n += 1;
        }
        used.insert(candidate.clone());
        assigned.push((src.clone(), candidate));
    }
    assigned
}

fn split_ext(basename: &str) -> (&str, &str) {
    match basename.rsplit_once('.') {
        Some((stem, ext)) => (stem, ext),
        None => (basename, ""),
    }
}

/// Rewrites every figure `path` in place to its final logical asset path,
/// leaving any path absent from `map` untouched.
pub fn rewrite_figure_paths(nodes: &mut [Node], map: &std::collections::HashMap<String, String>) {
    for node in nodes {
        match &mut node.kind {
            NodeKind::Figure { path, .. } => {
                if let Some(logical) = map.get(path.as_str()) {
                    *path = logical.clone();
                }
            }
            NodeKind::List { items, .. } => {
                for item in items {
                    rewrite_figure_paths(&mut item.continuation, map);
                }
            }
            NodeKind::TheoremLike { body, .. } | NodeKind::Proof { body, .. } => {
                rewrite_figure_paths(body, map);
            }
            NodeKind::Heading { .. }
            | NodeKind::Paragraph { .. }
            | NodeKind::Equation { .. }
            | NodeKind::Table { .. }
            | NodeKind::RawTex { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::Bibliography => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_asset_path_rejects_remote_and_unsupported() {
        assert!(validate_asset_path("figs/diagram.png").is_ok());
        assert!(validate_asset_path("figs/diagram.PDF").is_ok());
        assert_eq!(
            validate_asset_path("https://example.org/x.png"),
            Err(AssetError::Invalid("https://example.org/x.png".to_string()))
        );
        assert_eq!(
            validate_asset_path("../outside.png"),
            Err(AssetError::Invalid("../outside.png".to_string()))
        );
        assert_eq!(
            validate_asset_path("figs/diagram.svg"),
            Err(AssetError::UnsupportedExtension("figs/diagram.svg".to_string()))
        );
    }

    #[test]
    fn test_colliding_asset_basenames_are_disambiguated() {
        let refs = vec![
            "figs/diagram.png".to_string(),
            "other/diagram.png".to_string(),
            "figs/diagram.png".to_string(),
        ];
        let assigned = disambiguate_assets(&refs);
        assert_eq!(assigned.len(), 2, "the repeated reference reuses its first assignment");
        assert_eq!(assigned[0], ("figs/diagram.png".to_string(), "assets/diagram.png".to_string()));
        assert_eq!(assigned[1], ("other/diagram.png".to_string(), "assets/diagram-1.png".to_string()));
    }
}
