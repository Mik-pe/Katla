//! Native Metal encoder interfaces and portable submission metadata.

pub mod command;
#[cfg(target_os = "macos")]
pub mod resource;
#[cfg(target_os = "macos")]
pub mod traits;
