//! Heavy end-to-end suite. Every case here needs a real XeLaTeX/Biber
//! toolchain and is `#[ignore]`d in the default `cargo test --workspace
//! --locked` lane; run explicitly with:
//!
//!   cargo test --locked -p terse-cli --test e2e -- --ignored

#[path = "e2e/themes.rs"]
mod themes;

#[path = "e2e/latex_generation.rs"]
mod latex_generation;

#[path = "e2e/citations.rs"]
mod citations;

#[path = "e2e/multi_file_projects.rs"]
mod multi_file_projects;

#[path = "e2e/arxiv_export.rs"]
mod arxiv_export;

#[path = "e2e/git_oriented_tooling.rs"]
mod git_oriented_tooling;

#[path = "e2e/toolchain.rs"]
mod toolchain;

#[path = "common/mod.rs"]
mod common;
