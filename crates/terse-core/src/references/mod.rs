//! Offline reference-record model: normalized metadata, the versioned
//! lock file format, override patching, and citation binding.
//!
//! Nothing in this module performs I/O or contacts a network provider —
//! DOI/arXiv HTTP adapters are `terse-cli` concerns (groups 14-15) that
//! produce the [`record::NormalizedRecord`]s this module stores and binds.

pub mod arxiv;
pub mod bind;
pub mod doi;
pub mod lock;
pub mod overrides;
pub mod record;

#[cfg(test)]
mod tests;
