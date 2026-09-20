//! Pure toolchain data and computations: nothing here reads the
//! filesystem, the environment, or the clock. The application layer
//! (`terse-cli`) owns discovery, probing, and provisioning.

pub mod par;
pub mod spec;
pub mod tlpdb;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
