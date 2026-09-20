//! Shared logical-path normalization and output-scope validation.
//!
//! These checks are purely lexical, over the `/`-separated logical paths
//! declared in the manifest. Canonical filesystem checks (existing
//! ancestors, symlink escapes) are the application layer's responsibility.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    Empty,
    AbsoluteOrDriveRoot,
    Traversal,
    OutputOverlapsSource,
}

/// Normalizes a `/`-separated logical path into its non-empty, non-`.`
/// components, rejecting absolute paths, drive/UNC prefixes, and `..`
/// traversal.
pub fn normalize_logical(path: &str) -> Result<Vec<String>, PathError> {
    if path.is_empty() {
        return Err(PathError::Empty);
    }
    if path.starts_with('/') || path.starts_with('\\') || path.contains(':') {
        return Err(PathError::AbsoluteOrDriveRoot);
    }
    let mut out = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => continue,
            ".." => return Err(PathError::Traversal),
            other => out.push(other.to_string()),
        }
    }
    if out.is_empty() {
        return Err(PathError::Empty);
    }
    Ok(out)
}

/// Resolves a declaring-file-relative path (an include or asset reference)
/// against the declaring file's own root-relative directory, producing
/// root-relative logical components. Purely lexical: `..` pops the
/// declaring directory rather than escaping the project, and popping past
/// an empty stack is a traversal error. This cannot detect a symlink
/// escape; the application layer canonicalizes before ever reading the
/// resolved path.
pub fn resolve_declared_path(base_dir: &[String], relative: &str) -> Result<Vec<String>, PathError> {
    if relative.is_empty() {
        return Err(PathError::Empty);
    }
    if relative.starts_with('/') || relative.starts_with('\\') || relative.contains(':') {
        return Err(PathError::AbsoluteOrDriveRoot);
    }
    let mut stack: Vec<String> = base_dir.to_vec();
    for part in relative.split('/') {
        match part {
            "" | "." => continue,
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Traversal);
                }
            }
            other => stack.push(other.to_string()),
        }
    }
    if stack.is_empty() {
        return Err(PathError::Empty);
    }
    Ok(stack)
}

fn is_prefix(prefix: &[String], of: &[String]) -> bool {
    prefix.len() <= of.len() && prefix.iter().zip(of.iter()).all(|(a, b)| a == b)
}

/// Rejects an output path that is the project root, that equals or
/// contains the entry's source directory, or that the entry lives inside.
pub fn validate_output_scope(entry: &str, output: &str) -> Result<(), PathError> {
    let entry_components = normalize_logical(entry)?;
    let output_components = normalize_logical(output)?;

    if is_prefix(&output_components, &entry_components) {
        return Err(PathError::OutputOverlapsSource);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_output_does_not_overlap_root_entry() {
        assert!(validate_output_scope("paper.trs", "build").is_ok());
    }

    #[test]
    fn test_output_equal_to_root_is_rejected() {
        assert_eq!(
            validate_output_scope("paper.trs", "."),
            Err(PathError::Empty)
        );
    }

    #[test]
    fn test_output_equal_to_source_directory_is_rejected() {
        assert_eq!(
            validate_output_scope("sections/paper.trs", "sections"),
            Err(PathError::OutputOverlapsSource)
        );
    }

    #[test]
    fn test_output_escaping_root_is_rejected() {
        assert_eq!(
            validate_output_scope("paper.trs", "../outside"),
            Err(PathError::Traversal)
        );
    }

    #[test]
    fn test_sibling_directory_asset_resolves_relative_to_declaring_dir() {
        assert_eq!(
            resolve_declared_path(&["sections".to_string()], "../figures/model.pdf"),
            Ok(vec!["figures".to_string(), "model.pdf".to_string()])
        );
    }

    #[test]
    fn test_declared_path_cannot_escape_the_root() {
        assert_eq!(
            resolve_declared_path(&[], "../outside.trs"),
            Err(PathError::Traversal)
        );
    }
}
