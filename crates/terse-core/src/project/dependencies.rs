//! Successful and attempted dependency collection.
//!
//! This is pure bookkeeping over already-resolved root-relative logical
//! paths handed in by the application layer (which does the actual
//! filesystem walking): includes, the manifest, themes, lock/overrides,
//! assets, and support files. Traversal order never leaks into the
//! serialized set, since both fields are ordered sets keyed by path.

use std::collections::BTreeSet;

/// The complete set of paths a build depended on, split into what was
/// actually read and what was referenced but could not be found. Watch
/// mode (task group 21) uses `attempted` to detect a later repair: a file
/// or an intermediate directory being created should trigger a rebuild
/// even though the previous build never opened it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencySet {
    pub resolved: BTreeSet<String>,
    pub attempted: BTreeSet<String>,
}

impl DependencySet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_resolved(&mut self, path: impl Into<String>) {
        self.resolved.insert(path.into());
    }

    /// Records `path` as attempted-but-unresolved, along with every
    /// intermediate directory on its route from the root, so creating a
    /// missing parent directory is also observable as a repair.
    pub fn add_attempted(&mut self, path: impl Into<String>) {
        let path = path.into();
        for parent in parents_of(&path) {
            self.attempted.insert(parent);
        }
        self.attempted.insert(path);
    }

    /// Merges another dependency set's entries into this one.
    pub fn merge(&mut self, other: DependencySet) {
        self.resolved.extend(other.resolved);
        self.attempted.extend(other.attempted);
    }
}

/// Every parent directory of a root-relative logical path, shallowest
/// first, excluding the root itself and the path's own final component.
fn parents_of(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split('/').collect();
    (1..parts.len()).map(|i| parts[..i].join("/")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attempted_dependency_includes_parent_directories() {
        let mut deps = DependencySet::new();
        deps.add_attempted("sections/missing.trs");
        assert!(deps.attempted.contains("sections"));
        assert!(deps.attempted.contains("sections/missing.trs"));
    }

    #[test]
    fn test_merge_is_order_independent() {
        let mut a = DependencySet::new();
        a.add_resolved("paper.trs");
        let mut b = DependencySet::new();
        b.add_resolved("sections/method.trs");
        a.merge(b);
        assert_eq!(
            a.resolved,
            BTreeSet::from(["paper.trs".to_string(), "sections/method.trs".to_string()])
        );
    }
}
