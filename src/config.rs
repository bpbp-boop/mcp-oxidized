//! Configuration management for mcp-oxidized server.
//!
//! This module handles loading configuration from environment variables with a
//! precedence chain that supports MCP client configuration passthrough.
//!
//! # Environment Variables
//!
//! - `OXIDIZED_URL` - Oxidized server URL (default: `http://localhost:8888`)
//! - `OXIDIZED_USER` - Optional username for Basic Auth
//! - `OXIDIZED_PASSWORD` - Optional password for Basic Auth
//! - `OXIDIZED_PASSWORD_FILE` - Optional path to file containing password (takes precedence over `OXIDIZED_PASSWORD`)

use std::env;
use std::fs;
use thiserror::Error;

/// Configuration for mcp-oxidized server
#[derive(Debug, Clone)]
pub struct Config {
    pub oxidized_url: String,
    pub oxidized_user: Option<String>,
    #[allow(dead_code)] // Will be used in Story 1.4 for HTTP authentication
    pub oxidized_password: Option<String>,
}

/// Configuration errors with actionable context
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Invalid Oxidized URL: {0}. Must start with http:// or https://")]
    InvalidUrl(String),

    #[error("Failed to read password file at {path}: {source}")]
    PasswordFileError {
        path: String,
        source: std::io::Error,
    },

    #[allow(dead_code)] // Reserved for future use
    #[error("Environment variable error: {0}")]
    EnvVarError(String),
}

impl Config {
    /// Load configuration from environment variables with precedence chain
    ///
    /// Precedence (highest to lowest):
    /// 1. Environment variables (includes MCP client config passed via Claude Desktop JSON)
    /// 2. Default values
    ///
    /// Note: MCP clients like Claude Desktop pass their config as environment variables
    /// to child processes. From this binary's perspective, all env vars are read the same
    /// way - the MCP client handles the precedence by setting env vars before spawning.
    ///
    /// Default values (zero-config mode):
    /// - OXIDIZED_URL: "http://localhost:8888"
    /// - OXIDIZED_USER: None
    /// - OXIDIZED_PASSWORD: None
    pub fn load() -> Result<Self, ConfigError> {
        // Load URL with default
        let oxidized_url =
            env::var("OXIDIZED_URL").unwrap_or_else(|_| "http://localhost:8888".to_string());

        // Validate URL format
        Self::validate_url(&oxidized_url)?;

        // Load optional credentials
        let oxidized_user = env::var("OXIDIZED_USER").ok();

        // Check for password file first, then direct password env var
        let oxidized_password = if let Ok(password_file) = env::var("OXIDIZED_PASSWORD_FILE") {
            Some(Self::read_password_file(&password_file)?)
        } else {
            env::var("OXIDIZED_PASSWORD").ok()
        };

        Ok(Config {
            oxidized_url,
            oxidized_user,
            oxidized_password,
        })
    }

    /// Validate URL format - must start with http:// or https://
    fn validate_url(url: &str) -> Result<(), ConfigError> {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(ConfigError::InvalidUrl(url.to_string()));
        }
        Ok(())
    }

    /// Read password from file, trimming whitespace
    fn read_password_file(path: &str) -> Result<String, ConfigError> {
        fs::read_to_string(path)
            .map(|content| content.trim().to_string())
            .map_err(|source| ConfigError::PasswordFileError {
                path: path.to_string(),
                source,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Clear all OXIDIZED_* env vars before each test.
    /// SAFETY: Tests using this are marked #[serial] ensuring single-threaded execution.
    fn clear_env_vars() {
        for key in [
            "OXIDIZED_URL",
            "OXIDIZED_USER",
            "OXIDIZED_PASSWORD",
            "OXIDIZED_PASSWORD_FILE",
        ] {
            // SAFETY: #[serial] ensures no concurrent access to env vars
            unsafe { std::env::remove_var(key) };
        }
    }

    /// Set an environment variable for testing.
    /// SAFETY: Tests using this are marked #[serial] ensuring single-threaded execution.
    fn set_env(key: &str, value: &str) {
        // SAFETY: #[serial] ensures no concurrent access to env vars
        unsafe { std::env::set_var(key, value) };
    }

    #[test]
    #[serial]
    fn test_default_values_when_no_env_vars() {
        clear_env_vars();

        let config = Config::load().expect("Should load with defaults");

        assert_eq!(config.oxidized_url, "http://localhost:8888");
        assert_eq!(config.oxidized_user, None);
        assert_eq!(config.oxidized_password, None);
    }

    #[test]
    #[serial]
    fn test_env_var_precedence() {
        clear_env_vars();
        set_env("OXIDIZED_URL", "https://oxidized.example.com");
        set_env("OXIDIZED_USER", "admin");
        set_env("OXIDIZED_PASSWORD", "secret123");

        let config = Config::load().expect("Should load from env vars");

        assert_eq!(config.oxidized_url, "https://oxidized.example.com");
        assert_eq!(config.oxidized_user, Some("admin".to_string()));
        assert_eq!(config.oxidized_password, Some("secret123".to_string()));

        clear_env_vars();
    }

    #[test]
    fn test_url_validation_valid_http() {
        let result = Config::validate_url("http://localhost:8888");
        assert!(result.is_ok());
    }

    #[test]
    fn test_url_validation_valid_https() {
        let result = Config::validate_url("https://oxidized.example.com");
        assert!(result.is_ok());
    }

    #[test]
    fn test_url_validation_invalid_format() {
        let result = Config::validate_url("ftp://invalid.com");
        assert!(result.is_err());

        if let Err(ConfigError::InvalidUrl(url)) = result {
            assert_eq!(url, "ftp://invalid.com");
        } else {
            panic!("Expected InvalidUrl error");
        }
    }

    #[test]
    fn test_password_file_reading() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        writeln!(temp_file, "  my-secret-password  ").expect("Failed to write to temp file");

        let password = Config::read_password_file(temp_file.path().to_str().unwrap())
            .expect("Should read password file");

        assert_eq!(password, "my-secret-password");
    }

    #[test]
    fn test_password_file_not_found() {
        let result = Config::read_password_file("/nonexistent/password.txt");
        assert!(result.is_err());

        if let Err(ConfigError::PasswordFileError { path, .. }) = result {
            assert_eq!(path, "/nonexistent/password.txt");
        } else {
            panic!("Expected PasswordFileError");
        }
    }

    #[test]
    #[serial]
    fn test_password_file_precedence_over_env_var() {
        clear_env_vars();

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        writeln!(temp_file, "file-password").expect("Failed to write to temp file");

        set_env("OXIDIZED_PASSWORD_FILE", temp_file.path().to_str().unwrap());
        set_env("OXIDIZED_PASSWORD", "env-password");
        set_env("OXIDIZED_URL", "http://localhost:8888");

        let config = Config::load().expect("Should load config");

        // Password file should take precedence
        assert_eq!(config.oxidized_password, Some("file-password".to_string()));

        clear_env_vars();
    }
}
