//! `tlmgr` output parsing.

use super::lock::LockedPackage;

/// Parses `tlmgr info --only-installed --data name,localrev` (CSV,
/// `name,revision` per line), skipping blank or malformed lines. The
/// result is sorted by name, matching the lock's fixed order.
pub fn parse_installed(text: &str) -> Vec<LockedPackage> {
    let mut packages: Vec<LockedPackage> = text
        .lines()
        .filter_map(|line| {
            let (name, rev) = line.trim().split_once(',')?;
            let name = name.trim();
            let rev = rev.trim();
            if name.is_empty() || rev.is_empty() || !rev.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            Some(LockedPackage { name: name.to_string(), revision: rev.to_string() })
        })
        .collect();
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    packages.dedup_by(|a, b| a.name == b.name);
    packages
}
