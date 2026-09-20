//! Original-byte source storage, file identity, and span types.
//!
//! The compiler core never reads files itself; callers construct a
//! [`SourceFile`] from bytes they already loaded and pass it in.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub file_id: FileId,
    pub byte_start: u32,
    pub byte_end: u32,
}

impl SourceSpan {
    pub fn new(file_id: FileId, byte_start: u32, byte_end: u32) -> Self {
        Self {
            file_id,
            byte_start,
            byte_end,
        }
    }
}

/// The largest single `.trs`/`.theme` source the compiler will load, in
/// bytes. A structured document source is prose plus markup: the complete
/// multi-file acceptance fixture is under 10 KiB, and a book-length work
/// split across modules stays far below this per file. 4 MiB is therefore
/// several orders of magnitude above any authored document while still
/// bounding what a single malformed or hostile input can make the
/// compiler allocate, decode, and index before any parsing begins.
pub const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    InvalidUtf8,
    BareCr { byte_offset: u32 },
    TooLarge { bytes: usize, limit: usize },
}

impl SourceError {
    /// Renders this load failure as a diagnostic against `file_id`. The
    /// span is empty: nothing inside a file that failed to load can be
    /// pointed at, and inventing an offset would be a fabricated position.
    pub fn into_diagnostic(self, file_id: FileId) -> crate::diagnostic::Diagnostic {
        let span = SourceSpan::new(file_id, 0, 0);
        match self {
            SourceError::InvalidUtf8 => {
                crate::diagnostic::Diagnostic::error("E-SOURCE-001", "source is not valid UTF-8", span)
            }
            SourceError::BareCr { byte_offset } => crate::diagnostic::Diagnostic::error(
                "E-SOURCE-002",
                "source contains a bare carriage return; use LF or CRLF line endings",
                SourceSpan::new(file_id, byte_offset, byte_offset),
            ),
            SourceError::TooLarge { bytes, limit } => crate::diagnostic::Diagnostic::error(
                "E-LIMIT-003",
                format!("source is {bytes} bytes, exceeding the {limit}-byte per-file limit"),
                span,
            ),
        }
    }
}

/// A single loaded `.trs`/`.theme` file: its original bytes, decoded text,
/// and BOM provenance. Byte offsets used by [`SourceSpan`] are relative to
/// `original_bytes`, including any BOM prefix.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: FileId,
    pub path: String,
    original_bytes: Vec<u8>,
    text: String,
    pub had_bom: bool,
    base_offset: u32,
}

impl SourceFile {
    pub fn new(id: FileId, path: impl Into<String>, bytes: Vec<u8>) -> Result<Self, SourceError> {
        // Checked before decoding: this is the single chokepoint where
        // bytes enter the compiler core, so refusing here bounds every
        // later stage without each of them needing its own guard.
        if bytes.len() > MAX_SOURCE_BYTES {
            return Err(SourceError::TooLarge {
                bytes: bytes.len(),
                limit: MAX_SOURCE_BYTES,
            });
        }
        let (had_bom, rest) = strip_bom(&bytes);
        let base_offset = (bytes.len() - rest.len()) as u32;
        let text = std::str::from_utf8(rest)
            .map_err(|_| SourceError::InvalidUtf8)?
            .to_string();
        check_no_bare_cr(rest)?;
        Ok(Self {
            id,
            path: path.into(),
            original_bytes: bytes,
            text,
            had_bom,
            base_offset,
        })
    }

    /// Decoded UTF-8 text after the BOM (if any). Line endings are preserved
    /// as authored (LF or CRLF).
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn original_bytes(&self) -> &[u8] {
        &self.original_bytes
    }

    /// Number of leading BOM bytes stripped from `original_bytes` to reach
    /// `text`. Add this to a byte offset within `text` to get an offset
    /// within `original_bytes` for span construction.
    pub fn base_offset(&self) -> u32 {
        self.base_offset
    }
}

fn strip_bom(bytes: &[u8]) -> (bool, &[u8]) {
    const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
    if bytes.len() >= 3 && bytes[0..3] == BOM {
        (true, &bytes[3..])
    } else {
        (false, bytes)
    }
}

fn check_no_bare_cr(bytes: &[u8]) -> Result<(), SourceError> {
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
            return Err(SourceError::BareCr {
                byte_offset: i as u32,
            });
        }
        i += 1;
    }
    Ok(())
}
