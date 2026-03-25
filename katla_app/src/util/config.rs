//! Shared configuration file utilities.
//!
//! Provides common functions for locating, loading, and saving TOML-based
//! configuration files in OS-appropriate locations.

use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use log::{debug, info, warn};

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

/// Load a TOML config file from the Katla config directory.
///
/// Reads the file at `{config_dir}/{filename}`, returning `None` if the file
/// does not exist, cannot be read, or the config directory cannot be determined.
pub fn load_config_file(filename: &str) -> Option<String> {
    let path = katla_config_file(filename)?;

    if !path.exists() {
        debug!("Config file not found: {:?}", path);
        return None;
    }

    let mut content = String::new();
    fs::File::open(&path)
        .and_then(|mut f| f.read_to_string(&mut content))
        .map_err(|e| {
            warn!("Failed to read config file {:?}: {}", path, e);
            e
        })
        .ok()?;

    Some(content)
}

/// Save content to a TOML config file in the Katla config directory.
///
/// Creates the config directory if it does not exist.
pub fn save_config_file(filename: &str, content: &str) -> io::Result<()> {
    let config_dir = katla_config_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not determine config directory",
        )
    })?;

    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
        info!("Created config directory: {:?}", config_dir);
    }

    let path = katla_config_file(filename).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not determine config file path",
        )
    })?;

    let mut file = fs::File::create(&path)?;
    file.write_all(content.as_bytes())?;

    debug!("Saved config file: {:?}", path);
    Ok(())
}
