//! LLM configuration with persistent storage.
//!
//! API keys support `$ENV_VAR` reference syntax to avoid storing keys in plaintext.
//! The config file `llm.toml` should not be committed to version control.

use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};

/// Configuration validation issue with a human-readable description.
#[derive(Debug, Clone)]
pub(crate) struct ConfigIssue {
    pub(crate) message: String,
}

impl fmt::Display for ConfigIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

const CONFIG_FILENAME: &str = "llm.toml";

/// LLM provider type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LlmProviderKind {
    /// LLM calls disabled. Co-creator uses local pattern matching.
    #[default]
    Disabled,
    /// OpenAI API (api.openai.com).
    OpenAi,
    /// Any OpenAI-compatible endpoint (Ollama, LM Studio, vLLM, etc.).
    OpenAiCompatible,
}

/// LLM configuration persisted as `llm.toml` in the Katla config directory.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    /// Which LLM provider to use.
    pub provider: LlmProviderKind,
    /// API key. Supports `$ENV_VAR` reference syntax resolved at runtime.
    pub api_key: String,
    /// Custom base URL for OpenAI-compatible endpoints (e.g. `http://localhost:11434/v1`).
    pub base_url: Option<String>,
    /// Model identifier (e.g. `"gpt-4o"`, `"llama3"`).
    pub model: String,
    /// Maximum response tokens.
    pub max_tokens: u32,
    /// Sampling temperature (0.0–2.0).
    pub temperature: f32,
    /// Minimum interval between consecutive LLM calls in milliseconds.
    pub rate_limit_min_interval_ms: u64,
    /// Maximum number of LLM calls per rolling 60-second window.
    pub rate_limit_max_calls_per_minute: u32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProviderKind::Disabled,
            api_key: String::new(),
            base_url: None,
            model: "gpt-4o".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
            rate_limit_min_interval_ms: 1000,
            rate_limit_max_calls_per_minute: 20,
        }
    }
}

impl fmt::Debug for LlmConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LlmConfig")
            .field("provider", &self.provider)
            .field("api_key", &redacted_key(&self.api_key))
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .field("temperature", &self.temperature)
            .field(
                "rate_limit_min_interval_ms",
                &self.rate_limit_min_interval_ms,
            )
            .field(
                "rate_limit_max_calls_per_minute",
                &self.rate_limit_max_calls_per_minute,
            )
            .finish()
    }
}

impl fmt::Display for LlmConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LlmConfig {{ provider: {:?}", self.provider)?;
        write!(f, ", model: {}", self.model)?;

        if let Some(ref url) = self.base_url {
            write!(f, ", base_url: {}", url)?;
        }

        write!(f, ", temperature: {}", self.temperature)?;
        write!(f, ", max_tokens: {}", self.max_tokens)?;
        write!(
            f,
            ", rate_limit: {}/min, {}ms interval",
            self.rate_limit_max_calls_per_minute, self.rate_limit_min_interval_ms
        )?;

        if self.api_key.is_empty() {
            write!(f, ", api_key: <not set>")?;
        } else if let Some(var_name) = self.api_key.strip_prefix('$') {
            let resolved = env::var(var_name);
            match resolved {
                Ok(_) => write!(f, ", api_key: ${} (resolved: ***)", var_name)?,
                Err(_) => write!(f, ", api_key: ${} (unresolved)", var_name)?,
            }
        } else {
            write!(f, ", api_key: ***")?;
        }

        write!(f, " }}")
    }
}

fn redacted_key(key: &str) -> String {
    if key.is_empty() {
        "<not set>".to_string()
    } else if let Some(var_name) = key.strip_prefix('$') {
        match env::var(var_name) {
            Ok(_) => format!("${} (***)", var_name),
            Err(_) => format!("${} (unresolved)", var_name),
        }
    } else if key.len() > 4 {
        format!("{}***", &key[..4])
    } else {
        "***".to_string()
    }
}

impl LlmConfig {
    /// Load configuration from disk, or return defaults if not found.
    ///
    /// Validates the loaded config and logs warnings for any issues before
    /// applying defaults to invalid fields.
    pub fn load() -> Self {
        let content = match load_config_file(CONFIG_FILENAME) {
            Some(c) => c,
            None => {
                debug!("No LLM config file found, using defaults");
                return Self::default();
            }
        };

        let mut config: Self = match toml::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to parse LLM config: {}", e);
                return Self::default();
            }
        };

        let issues = config.validate();
        for issue in &issues {
            warn!("LLM config issue: {}", issue);
        }

        config.sanitize();
        config
    }

    /// Save configuration to disk.
    pub fn save(&self) -> io::Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        save_config_file(CONFIG_FILENAME, &content)
    }

    /// Whether the provider is configured enough to make LLM calls.
    pub fn is_enabled(&self) -> bool {
        match self.provider {
            LlmProviderKind::Disabled => false,
            LlmProviderKind::OpenAi | LlmProviderKind::OpenAiCompatible => {
                !self.resolve_api_key().is_empty() && !self.model.is_empty()
            }
        }
    }

    /// Resolve the API key, expanding `$ENV_VAR` references to env var values.
    pub fn resolve_api_key(&self) -> String {
        if let Some(rest) = self.api_key.strip_prefix('$') {
            env::var(rest).unwrap_or_default()
        } else {
            self.api_key.clone()
        }
    }

    /// The effective base URL. Returns `None` for stock OpenAI (uses default endpoint).
    pub fn effective_base_url(&self) -> Option<&str> {
        match self.provider {
            LlmProviderKind::OpenAiCompatible => self.base_url.as_deref(),
            _ => None,
        }
    }

    /// Validate configuration and return a list of issues without modifying the config.
    ///
    /// Checks for missing required fields based on the selected provider and
    /// reports out-of-range values.
    pub(crate) fn validate(&self) -> Vec<ConfigIssue> {
        let mut issues = Vec::new();

        match self.provider {
            LlmProviderKind::Disabled => {}
            LlmProviderKind::OpenAi | LlmProviderKind::OpenAiCompatible => {
                if self.resolve_api_key().is_empty() {
                    issues.push(ConfigIssue {
                        message: format!(
                            "api_key is required for provider {:?} but is not set. \
                             Set api_key in llm.toml or use $ENV_VAR syntax (e.g. $OPENAI_API_KEY).",
                            self.provider
                        ),
                    });
                }
            }
        }

        if self.provider == LlmProviderKind::OpenAiCompatible && self.base_url.is_none() {
            issues.push(ConfigIssue {
                message: "base_url is required for provider open_ai_compatible \
                          (e.g. http://localhost:11434/v1)."
                    .to_string(),
            });
        }

        if self.model.is_empty() {
            issues.push(ConfigIssue {
                message: "model is empty, will default to \"gpt-4o\".".to_string(),
            });
        }

        if self.max_tokens == 0 {
            issues.push(ConfigIssue {
                message: "max_tokens is 0, will default to 4096.".to_string(),
            });
        }

        if !(0.0..=2.0).contains(&self.temperature) {
            issues.push(ConfigIssue {
                message: format!(
                    "temperature {} is outside valid range [0.0, 2.0], will be clamped.",
                    self.temperature
                ),
            });
        }

        if let Some(ref url) = self.base_url {
            if url.is_empty() {
                issues.push(ConfigIssue {
                    message: "base_url is set but empty.".to_string(),
                });
            }
        }

        if self.rate_limit_max_calls_per_minute == 0 {
            issues.push(ConfigIssue {
                message: "rate_limit_max_calls_per_minute is 0, will default to 20.".to_string(),
            });
        }

        issues
    }

    /// Apply default values to invalid fields.
    fn sanitize(&mut self) {
        self.temperature = self.temperature.clamp(0.0, 2.0);
        if self.max_tokens == 0 {
            self.max_tokens = 4096;
        }
        if self.model.is_empty() {
            self.model = "gpt-4o".to_string();
        }
        if self.rate_limit_max_calls_per_minute == 0 {
            self.rate_limit_max_calls_per_minute = 20;
        }
    }
}

fn katla_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("katla"))
}

fn load_config_file(filename: &str) -> Option<String> {
    let path = katla_config_dir()?.join(filename);

    if !path.exists() {
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

fn save_config_file(filename: &str, content: &str) -> io::Result<()> {
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

    let path = config_dir.join(filename);
    let mut file = fs::File::create(&path)?;
    file.write_all(content.as_bytes())?;

    // Restrict file permissions to owner-only on Unix (API keys may be stored here)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms)?;
    }

    debug!("Saved config file: {:?}", path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LlmConfig::default();
        assert_eq!(config.provider, LlmProviderKind::Disabled);
        assert!(config.api_key.is_empty());
        assert!(config.base_url.is_none());
        assert_eq!(config.model, "gpt-4o");
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_parse_toml() {
        let content = r#"
provider = "open_ai"
api_key = "sk-test-key"
model = "gpt-4o-mini"
max_tokens = 2048
temperature = 0.5
"#;
        let mut config: LlmConfig = toml::from_str(content).unwrap();
        assert!(config.validate().is_empty());
        config.sanitize();
        assert_eq!(config.provider, LlmProviderKind::OpenAi);
        assert_eq!(config.api_key, "sk-test-key");
        assert_eq!(config.model, "gpt-4o-mini");
        assert_eq!(config.max_tokens, 2048);
        assert_eq!(config.temperature, 0.5);
        assert!(config.is_enabled());
    }

    #[test]
    fn test_openai_compatible_with_base_url() {
        let content = r#"
provider = "open_ai_compatible"
api_key = "ollama"
base_url = "http://localhost:11434/v1"
model = "llama3"
"#;
        let config: LlmConfig = toml::from_str(content).unwrap();
        assert_eq!(
            config.effective_base_url(),
            Some("http://localhost:11434/v1")
        );
        assert!(config.is_enabled());
    }

    #[test]
    fn test_env_var_resolution() {
        unsafe { env::set_var("TEST_KATLA_KEY", "resolved-key-123") };
        let config = LlmConfig {
            api_key: "$TEST_KATLA_KEY".to_string(),
            ..Default::default()
        };
        assert_eq!(config.resolve_api_key(), "resolved-key-123");
        unsafe { env::remove_var("TEST_KATLA_KEY") };
    }

    #[test]
    fn test_env_var_missing_returns_empty() {
        let config = LlmConfig {
            api_key: "$NONEXISTENT_KATLA_VAR_XYZ".to_string(),
            ..Default::default()
        };
        assert!(config.resolve_api_key().is_empty());
    }

    #[test]
    fn test_validate_clamps_temperature() {
        let mut config = LlmConfig {
            temperature: 5.0,
            ..Default::default()
        };
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.message.contains("temperature")));
        config.sanitize();
        assert_eq!(config.temperature, 2.0);
    }

    #[test]
    fn test_validate_defaults_empty_model() {
        let mut config = LlmConfig {
            model: String::new(),
            ..Default::default()
        };
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.message.contains("model")));
        config.sanitize();
        assert_eq!(config.model, "gpt-4o");
    }

    #[test]
    fn test_disabled_not_enabled() {
        let config = LlmConfig {
            provider: LlmProviderKind::Disabled,
            api_key: "sk-key".to_string(),
            model: "gpt-4o".to_string(),
            ..Default::default()
        };
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_openai_no_base_url() {
        let config = LlmConfig {
            provider: LlmProviderKind::OpenAi,
            ..Default::default()
        };
        assert_eq!(config.effective_base_url(), None);
    }

    #[test]
    fn test_validate_missing_api_key() {
        let config = LlmConfig {
            provider: LlmProviderKind::OpenAi,
            api_key: String::new(),
            model: "gpt-4o".to_string(),
            ..Default::default()
        };
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.message.contains("api_key")));
    }

    #[test]
    fn test_validate_missing_base_url_for_compatible() {
        let config = LlmConfig {
            provider: LlmProviderKind::OpenAiCompatible,
            api_key: "test-key".to_string(),
            base_url: None,
            model: "llama3".to_string(),
            ..Default::default()
        };
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.message.contains("base_url")));
    }

    #[test]
    fn test_validate_disabled_no_issues() {
        let config = LlmConfig::default();
        let issues = config.validate();
        assert!(issues.is_empty());
    }

    #[test]
    fn test_validate_valid_config_no_issues() {
        let config = LlmConfig {
            provider: LlmProviderKind::OpenAi,
            api_key: "sk-key".to_string(),
            model: "gpt-4o".to_string(),
            max_tokens: 2048,
            temperature: 0.7,
            ..Default::default()
        };
        let issues = config.validate();
        assert!(issues.is_empty());
    }

    #[test]
    fn test_validate_zero_max_tokens() {
        let config = LlmConfig {
            max_tokens: 0,
            ..Default::default()
        };
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.message.contains("max_tokens")));
    }

    #[test]
    fn test_validate_empty_base_url() {
        let config = LlmConfig {
            provider: LlmProviderKind::OpenAiCompatible,
            api_key: "key".to_string(),
            base_url: Some(String::new()),
            model: "llama3".to_string(),
            ..Default::default()
        };
        let issues = config.validate();
        assert!(
            issues
                .iter()
                .any(|i| i.message.contains("base_url is set but empty"))
        );
    }

    #[test]
    fn test_validate_negative_temperature() {
        let config = LlmConfig {
            temperature: -1.0,
            ..Default::default()
        };
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.message.contains("temperature")));
    }
}
