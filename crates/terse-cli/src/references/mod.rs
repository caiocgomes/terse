//! Effectful reference-resolution adapters: HTTP transport, DOI/arXiv
//! provider adapters, and the arXiv rate-limited worker. Only the
//! explicit resolution path (group 15's `refs resolve` command) is
//! allowed to construct or call any of this — `check`/`build`/`fmt`/
//! `watch` never do.

pub mod arxiv;
pub mod clock;
pub mod doi;
pub mod resolve;
pub mod transport;
