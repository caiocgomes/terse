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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    InvalidUtf8,
    BareCr { byte_offset: u32 },
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
