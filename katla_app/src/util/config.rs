//! Shared configuration file utilities.
//!
//! Provides common functions for locating and accessing configuration files
//! in OS-appropriate locations.

use std::path::PathBuf;

/// Get the Katla config directory path for the current OS.
///
/// Returns the following paths depending on platform:
/// - Windows: `C:\Users\<user>\AppData\Roaming\katla`
/// - macOS: `~/Library/Application Support/katla`
/// - Linux: `~/.config/katla`
pub fn katla_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("katla"))
}

/// Get a config file path within the Katla config directory.
///
/// # Arguments
/// * `filename` - Name of the config file (e.g., "preferences.toml")
///
/// # Returns
/// Full path to the config file, or None if config directory cannot be determined.
pub fn katla_config_file(filename: &str) -> Option<PathBuf> {
    katla_config_dir().map(|p| p.join(filename))
}
