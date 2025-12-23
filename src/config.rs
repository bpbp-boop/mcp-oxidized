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
//! - `OXIDIZED_SSL_VERIFY` - SSL certificate verification (`true`/`false`, default: `true`)
//! - `OXIDIZED_HEADERS` - Custom HTTP headers (format: `Header1:Value1,Header2:Value2`)

use std::env;
use std::fs;
use thiserror::Error;

/// Configuration for mcp-oxidized server
#[derive(Debug, Clone)]
pub struct Config {
    /// Oxidized server base URL (e.g., `http://localhost:8888`)
    pub oxidized_url: String,
    /// Optional username for HTTP Basic Auth
    pub oxidized_user: Option<String>,
    /// Optional password for HTTP Basic Auth
    pub oxidized_password: Option<String>,
    /// Whether to verify SSL certificates (default: `true`)
    pub ssl_verify: bool,
    /// Custom HTTP headers to include in all requests
    pub custom_headers: Vec<(String, String)>,
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

    #[error("Environment variable error: {0}")]
    EnvVarError(String),

    #[error("Invalid header format: {0}. Expected format: Header1:Value1,Header2:Value2")]
    InvalidHeaderFormat(String),
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

        // SSL verify - default true, accept "false" to disable (case-insensitive)
        let ssl_verify = env::var("OXIDIZED_SSL_VERIFY")
            .map(|v| !v.eq_ignore_ascii_case("false"))
            .unwrap_or(true);

        // Custom headers - graceful degradation on parse error
        let custom_headers = match env::var("OXIDIZED_HEADERS") {
            Ok(raw) => Self::parse_headers(&raw).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Invalid OXIDIZED_HEADERS format, ignoring");
                vec![]
            }),
            Err(_) => vec![],
        };

        Ok(Config {
            oxidized_url,
            oxidized_user,
            oxidized_password,
            ssl_verify,
            custom_headers,
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

    /// Parse OXIDIZED_HEADERS env var format: "Header1:Value1,Header2:Value2"
    ///
    /// # Arguments
    ///
    /// * `raw` - The raw header string to parse
    ///
    /// # Returns
    ///
    /// A vector of (header_name, header_value) tuples.
    /// Invalid entries are logged as warnings and skipped (graceful degradation).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let headers = Config::parse_headers("X-Api-Key:secret,X-Custom:value")?;
    /// assert_eq!(headers.len(), 2);
    /// ```
    pub fn parse_headers(raw: &str) -> Result<Vec<(String, String)>, ConfigError> {
        if raw.trim().is_empty() {
            return Ok(vec![]);
        }

        let mut headers = Vec::new();
        for pair in raw.split(',') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }

            // Split on FIRST ':' only (value may contain ':')
            match pair.split_once(':') {
                Some((key, value)) => {
                    let key = key.trim().to_string();
                    let value = value.trim().to_string();
                    if key.is_empty() {
                        tracing::warn!(pair = %pair, "Skipping header with empty key");
                        continue;
                    }
                    headers.push((key, value));
                }
                None => {
                    tracing::warn!(
                        pair = %pair,
                        "Invalid header format, expected 'Key:Value', skipping"
                    );
                }
            }
        }

        Ok(headers)
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
            "OXIDIZED_SSL_VERIFY",
            "OXIDIZED_HEADERS",
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
        assert!(config.ssl_verify, "SSL verify should default to true");
        assert!(
            config.custom_headers.is_empty(),
            "Custom headers should be empty by default"
        );
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

    // -------------------------------------------------------------------------
    // SSL Verification Tests (Story 4-1, AC1)
    // -------------------------------------------------------------------------

    #[test]
    #[serial]
    fn test_ssl_verify_default_true() {
        clear_env_vars();

        let config = Config::load().expect("Should load config");
        assert!(config.ssl_verify, "SSL verify should default to true");

        clear_env_vars();
    }

    #[test]
    #[serial]
    fn test_ssl_verify_false_explicit() {
        clear_env_vars();
        set_env("OXIDIZED_SSL_VERIFY", "false");

        let config = Config::load().expect("Should load config");
        assert!(
            !config.ssl_verify,
            "SSL verify should be false when set to 'false'"
        );

        clear_env_vars();
    }

    #[test]
    #[serial]
    fn test_ssl_verify_false_case_insensitive() {
        clear_env_vars();
        set_env("OXIDIZED_SSL_VERIFY", "FALSE");

        let config = Config::load().expect("Should load config");
        assert!(
            !config.ssl_verify,
            "SSL verify should be false (case-insensitive)"
        );

        clear_env_vars();
    }

    #[test]
    #[serial]
    fn test_ssl_verify_true_explicit() {
        clear_env_vars();
        set_env("OXIDIZED_SSL_VERIFY", "true");

        let config = Config::load().expect("Should load config");
        assert!(
            config.ssl_verify,
            "SSL verify should be true when set to 'true'"
        );

        clear_env_vars();
    }

    #[test]
    #[serial]
    fn test_ssl_verify_any_other_value_is_true() {
        clear_env_vars();
        set_env("OXIDIZED_SSL_VERIFY", "yes");

        let config = Config::load().expect("Should load config");
        assert!(
            config.ssl_verify,
            "SSL verify should be true for non-'false' values"
        );

        clear_env_vars();
    }

    // -------------------------------------------------------------------------
    // Header Parsing Tests (Story 4-1, AC2)
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_headers_valid() {
        let headers = Config::parse_headers("X-Api-Key:secret,X-Custom:value").unwrap();

        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0], ("X-Api-Key".to_string(), "secret".to_string()));
        assert_eq!(headers[1], ("X-Custom".to_string(), "value".to_string()));
    }

    #[test]
    fn test_parse_headers_with_colon_in_value() {
        let headers = Config::parse_headers("Authorization:Bearer token:with:colons").unwrap();

        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0, "Authorization");
        assert_eq!(headers[0].1, "Bearer token:with:colons");
    }

    #[test]
    fn test_parse_headers_empty() {
        let headers = Config::parse_headers("").unwrap();
        assert!(headers.is_empty());
    }

    #[test]
    fn test_parse_headers_whitespace_only() {
        let headers = Config::parse_headers("   ").unwrap();
        assert!(headers.is_empty());
    }

    #[test]
    fn test_parse_headers_malformed_graceful() {
        // Invalid entry "invalid" is skipped, valid ones are kept
        let headers = Config::parse_headers("valid:header,invalid,also:valid").unwrap();

        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0], ("valid".to_string(), "header".to_string()));
        assert_eq!(headers[1], ("also".to_string(), "valid".to_string()));
    }

    #[test]
    fn test_parse_headers_trims_whitespace() {
        let headers =
            Config::parse_headers("  X-Api-Key : secret123 , X-Custom : value  ").unwrap();

        assert_eq!(headers.len(), 2);
        assert_eq!(
            headers[0],
            ("X-Api-Key".to_string(), "secret123".to_string())
        );
        assert_eq!(headers[1], ("X-Custom".to_string(), "value".to_string()));
    }

    #[test]
    fn test_parse_headers_empty_key_skipped() {
        // Entry with empty key after colon should be skipped
        let headers = Config::parse_headers(":value,valid:header").unwrap();

        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0], ("valid".to_string(), "header".to_string()));
    }

    #[test]
    fn test_parse_headers_empty_value_allowed() {
        // Empty value is valid (some headers may have no value)
        let headers = Config::parse_headers("X-Empty:").unwrap();

        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0], ("X-Empty".to_string(), "".to_string()));
    }

    #[test]
    fn test_parse_headers_trailing_comma() {
        let headers = Config::parse_headers("X-Api-Key:secret,").unwrap();

        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0], ("X-Api-Key".to_string(), "secret".to_string()));
    }

    #[test]
    #[serial]
    fn test_config_load_with_headers() {
        clear_env_vars();
        set_env("OXIDIZED_HEADERS", "X-Api-Key:secret123,X-Custom:value");

        let config = Config::load().expect("Should load config");

        assert_eq!(config.custom_headers.len(), 2);
        assert_eq!(
            config.custom_headers[0],
            ("X-Api-Key".to_string(), "secret123".to_string())
        );
        assert_eq!(
            config.custom_headers[1],
            ("X-Custom".to_string(), "value".to_string())
        );

        clear_env_vars();
    }

    #[test]
    #[serial]
    fn test_config_load_with_invalid_headers_graceful() {
        clear_env_vars();
        // All entries are malformed - should gracefully degrade to empty
        set_env("OXIDIZED_HEADERS", "invalid");

        let config = Config::load().expect("Should load config even with invalid headers");

        // Should have empty headers (graceful degradation)
        assert!(config.custom_headers.is_empty());

        clear_env_vars();
    }
}
