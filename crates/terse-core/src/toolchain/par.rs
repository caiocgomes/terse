//! Biber is a PAR-packed Perl executable: on first run it unpacks itself
//! into a per-user cache directory whose name is derived from the
//! username, and a stale cache (left by a previous Biber build after a
//! TeX Live upgrade) makes every later invocation hang without output.
//! The directory is computable without running Biber, so `doctor` can
//! name it as the probable cause and `--fix` can remove it.

use std::path::{Path, PathBuf};

/// `<tmpdir>/par-<lowercase hex of the username's bytes>`, the directory
/// PAR::Packer uses for its unpack cache.
pub fn par_cache_dir(tmpdir: &Path, username: &str) -> PathBuf {
    let hex: String = username.bytes().map(|b| format!("{b:02x}")).collect();
    tmpdir.join(format!("par-{hex}"))
}
