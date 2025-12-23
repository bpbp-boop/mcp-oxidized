//! Actionable error handling framework for mcp-oxidized.
//!
//! This module provides LLM-optimized error messages that enable AI assistants
//! to understand errors and take appropriate actions autonomously.
//!
//! # Key Differentiator
//!
//! The `Actionable` trait is the main differentiator of mcp-oxidized. All errors
//! implement this trait to provide:
//! - Clear error description
//! - Context about what was attempted
//! - Suggestions for alternatives
//! - Next step guidance
//!
//! # Example Output
//!
//! ```text
//! [Error] Node 'SW-Unknown' not found.
//! [Context] Search performed in Oxidized inventory.
//! [Suggestions] Similar nodes: SW-Core-01, SW-Access-02.
//! [Next Step] Use 'oxidized://nodes' to list all available nodes.
//! ```

use thiserror::Error;

use crate::config::ConfigError;

// ============================================================================
// Error Message Constants
// ============================================================================

/// Prefix for error description in LLM-optimized messages.
///
/// Used as the first line of error output to clearly identify the error type.
///
/// # Example
/// ```text
/// [Error] Node 'SW-Unknown' not found.
/// ```
pub const ERROR_PREFIX: &str = "[Error]";

/// Prefix for context information in LLM-optimized messages.
///
/// Provides background about what operation was attempted when the error occurred.
///
/// # Example
/// ```text
/// [Context] Search performed in Oxidized inventory.
/// ```
pub const CONTEXT_PREFIX: &str = "[Context]";

/// Prefix for suggestions in LLM-optimized messages.
///
/// Lists alternatives or similar items that might help resolve the error.
///
/// # Example
/// ```text
/// [Suggestions] Similar nodes: SW-Core-01, SW-Access-02.
/// ```
pub const SUGGESTIONS_PREFIX: &str = "[Suggestions]";

/// Prefix for next step guidance in LLM-optimized messages.
///
/// Provides actionable guidance on what to do next to resolve or work around the error.
///
/// # Example
/// ```text
/// [Next Step] Use 'oxidized://nodes' to list all available nodes.
/// ```
pub const NEXT_STEP_PREFIX: &str = "[Next Step]";

// ============================================================================
// Actionable Trait
// ============================================================================

/// Trait for errors that can provide LLM-optimized messages.
///
/// This is a key differentiator for mcp-oxidized - all errors implement this trait
/// to provide actionable guidance to AI assistants.
///
/// # Format
///
/// The `to_llm_message()` method returns a structured message:
///
/// ```text
/// [Error] description
/// [Context] what was attempted
/// [Suggestions] similar items or alternatives
/// [Next Step] actionable guidance
/// ```
///
/// # Retry Classification
///
/// The `is_transient()` method classifies errors for retry logic:
/// - `true` → timeout, connection error, 5xx status → RETRY
/// - `false` → 4xx status, parse error, auth failed → NO RETRY
///
/// # Example
///
/// ```ignore
/// use mcp_oxidized::error::{OxidizedError, Actionable};
///
/// let error = OxidizedError::NodeNotFound(
///     "SW-Unknown".to_string(),
///     vec!["SW-Core-01".to_string(), "SW-Access-02".to_string()],
/// );
///
/// // Get LLM-optimized message
/// let message = error.to_llm_message();
/// println!("{}", message);
///
/// // Check if error is retryable
/// if error.is_transient() {
///     // Retry the operation
/// }
/// ```
pub trait Actionable {
    /// Returns a formatted message optimized for LLM understanding.
    ///
    /// The message uses structured prefixes to help AI assistants parse
    /// and act on errors autonomously.
    fn to_llm_message(&self) -> String;

    /// Classifies error as transient (retryable) or permanent.
    ///
    /// - `true` → timeout, connection error, 5xx status → RETRY
    /// - `false` → 4xx status, parse error, auth failed → NO RETRY
    fn is_transient(&self) -> bool;
}

// ============================================================================
// OxidizedError Enum
// ============================================================================

/// Comprehensive error types for mcp-oxidized operations.
///
/// All variants implement the `Actionable` trait to provide LLM-optimized
/// error messages with context, suggestions, and next steps.
#[derive(Debug, Error)]
pub enum OxidizedError {
    /// Node not found in Oxidized inventory.
    ///
    /// Contains the requested node name and a list of similar node names
    /// as suggestions.
    #[error("Node '{0}' not found")]
    NodeNotFound(String, Vec<String>),

    /// Oxidized API is unreachable.
    ///
    /// Contains the underlying request error, retry attempt count,
    /// and timestamp of last successful connection (if available).
    #[error("Oxidized API unreachable: {source}")]
    ApiUnreachable {
        /// The underlying reqwest error
        #[source]
        source: reqwest::Error,
        /// Current retry attempt number (1-based)
        attempt: u8,
        /// ISO 8601 timestamp of last successful connection, if any
        last_success: Option<String>,
    },

    /// Invalid regex pattern provided.
    ///
    /// Contains the malformed pattern string.
    #[error("Invalid regex pattern: {0}")]
    InvalidRegex(String),

    /// Authentication failed (401/403 response).
    ///
    /// Does NOT contain credential details for security (NFR6).
    #[error("Authentication failed")]
    AuthFailed,

    /// Configuration error.
    ///
    /// Wraps errors from the config module.
    #[error("Configuration error: {0}")]
    ConfigError(#[from] ConfigError),

    /// JSON parsing failed.
    ///
    /// Contains context about what was being parsed and the underlying error.
    #[error("JSON parse error in {context}: {source}")]
    ParseError {
        /// Description of what was being parsed (e.g., "node list response")
        context: String,
        /// The underlying serde_json error
        #[source]
        source: serde_json::Error,
    },

    /// HTTP error response (4xx/5xx status codes not covered by specific variants).
    ///
    /// Used for server errors (5xx) which are transient and retryable,
    /// and other client errors (4xx) not covered by NodeNotFound or AuthFailed.
    #[error("HTTP error {status_code} for {context}")]
    HttpError {
        /// HTTP status code
        status_code: u16,
        /// Context describing the operation that failed
        context: String,
    },
}

// ============================================================================
// Actionable Implementation
// ============================================================================

impl OxidizedError {
    /// Returns a short, safe error type name for logging.
    ///
    /// This method is designed for tracing output where we need a concise
    /// error identifier that is safe to log (no credentials or sensitive data).
    pub fn error_type(&self) -> &'static str {
        match self {
            OxidizedError::NodeNotFound(_, _) => "NodeNotFound",
            OxidizedError::ApiUnreachable { .. } => "ApiUnreachable",
            OxidizedError::InvalidRegex(_) => "InvalidRegex",
            OxidizedError::AuthFailed => "AuthFailed",
            OxidizedError::ConfigError(_) => "ConfigError",
            OxidizedError::ParseError { .. } => "ParseError",
            OxidizedError::HttpError { .. } => "HttpError",
        }
    }
}

impl Actionable for OxidizedError {
    fn to_llm_message(&self) -> String {
        match self {
            OxidizedError::NodeNotFound(node_name, suggestions) => {
                let suggestions_str = if suggestions.is_empty() {
                    "No similar nodes found.".to_string()
                } else {
                    format!("Similar nodes: {}.", suggestions.join(", "))
                };

                format!(
                    "{} Node '{}' not found.\n{} Search performed in Oxidized inventory.\n{} {}\n{} Use 'oxidized://nodes' to list all available nodes.",
                    ERROR_PREFIX,
                    node_name,
                    CONTEXT_PREFIX,
                    SUGGESTIONS_PREFIX,
                    suggestions_str,
                    NEXT_STEP_PREFIX
                )
            }

            OxidizedError::ApiUnreachable {
                source,
                attempt,
                last_success,
            } => {
                let last_success_info = match last_success {
                    Some(timestamp) => format!("Last successful connection: {}.", timestamp),
                    None => "No previous successful connection recorded.".to_string(),
                };

                // Detect SSL certificate errors for better suggestions
                let error_string = source.to_string().to_lowercase();
                let is_ssl_error = error_string.contains("certificate")
                    || error_string.contains("ssl")
                    || error_string.contains("tls")
                    || error_string.contains("invalid peer certificate");

                let (error_details, suggestion) = if is_ssl_error {
                    (
                        "SSL/TLS certificate verification failed.",
                        "For self-signed certificates, set OXIDIZED_SSL_VERIFY=false. WARNING: Only do this if you trust the server.",
                    )
                } else if source.is_timeout() {
                    (
                        "Connection timed out.",
                        "Check if Oxidized server is running and accessible. Verify OXIDIZED_URL configuration.",
                    )
                } else if source.is_connect() {
                    (
                        "Connection refused or network unreachable.",
                        "Check if Oxidized server is running and accessible. Verify OXIDIZED_URL configuration.",
                    )
                } else {
                    (
                        "Network error occurred.",
                        "Check if Oxidized server is running and accessible. Verify OXIDIZED_URL configuration.",
                    )
                };

                format!(
                    "{} Oxidized API unreachable - {}\n{} Attempt {}/3 to connect to Oxidized server. {}\n{} {}\n{} Wait a moment and retry, or check network connectivity and server status.",
                    ERROR_PREFIX,
                    error_details,
                    CONTEXT_PREFIX,
                    attempt,
                    last_success_info,
                    SUGGESTIONS_PREFIX,
                    suggestion,
                    NEXT_STEP_PREFIX
                )
            }

            OxidizedError::InvalidRegex(pattern) => {
                format!(
                    "{} Invalid regex pattern: '{}'.\n{} Attempted to compile regex for node filtering.\n{} Check for unescaped special characters. Use simple wildcards or valid regex syntax.\n{} Refer to Rust regex documentation for valid patterns.",
                    ERROR_PREFIX, pattern, CONTEXT_PREFIX, SUGGESTIONS_PREFIX, NEXT_STEP_PREFIX
                )
            }

            OxidizedError::AuthFailed => {
                // SECURITY (NFR6): Never mention which credential failed
                format!(
                    "{} Authentication failed.\n{} Attempted to authenticate with Oxidized server using provided credentials.\n{} Verify OXIDIZED_USER and OXIDIZED_PASSWORD environment variables are correctly set.\n{} Check credentials with Oxidized administrator or test with curl.",
                    ERROR_PREFIX, CONTEXT_PREFIX, SUGGESTIONS_PREFIX, NEXT_STEP_PREFIX
                )
            }

            OxidizedError::ConfigError(config_error) => {
                // Delegate to ConfigError's own message, wrapped in our format
                format!(
                    "{} Configuration error: {}.\n{} Loading mcp-oxidized configuration from environment variables.\n{} Check environment variables: OXIDIZED_URL, OXIDIZED_USER, OXIDIZED_PASSWORD, OXIDIZED_PASSWORD_FILE.\n{} Review configuration documentation and verify environment variable values.",
                    ERROR_PREFIX,
                    config_error,
                    CONTEXT_PREFIX,
                    SUGGESTIONS_PREFIX,
                    NEXT_STEP_PREFIX
                )
            }

            OxidizedError::ParseError { context, source } => {
                format!(
                    "{} Failed to parse JSON in {}.\n{} Received response from Oxidized but could not parse it. Error: {}.\n{} Verify Oxidized API version compatibility. Response format may have changed.\n{} Check Oxidized server logs for errors or contact administrator.",
                    ERROR_PREFIX,
                    context,
                    CONTEXT_PREFIX,
                    source,
                    SUGGESTIONS_PREFIX,
                    NEXT_STEP_PREFIX
                )
            }

            OxidizedError::HttpError {
                status_code,
                context,
            } => {
                let error_type = if *status_code >= 500 {
                    "Server error"
                } else {
                    "Client error"
                };

                let suggestion = if *status_code >= 500 {
                    "This is a server-side issue. The request may succeed if retried."
                } else {
                    "Check the request parameters and resource existence."
                };

                format!(
                    "{} {} (HTTP {}) for {}.\n{} HTTP request to Oxidized API returned error status.\n{} {}\n{} Retry the request or check Oxidized server status.",
                    ERROR_PREFIX,
                    error_type,
                    status_code,
                    context,
                    CONTEXT_PREFIX,
                    SUGGESTIONS_PREFIX,
                    suggestion,
                    NEXT_STEP_PREFIX
                )
            }
        }
    }

    fn is_transient(&self) -> bool {
        match self {
            // Retryable: network issues may be temporary
            OxidizedError::ApiUnreachable { .. } => true,

            // Retryable: 5xx server errors are transient
            OxidizedError::HttpError { status_code, .. } => *status_code >= 500,

            // Not retryable: these are permanent errors
            OxidizedError::NodeNotFound(_, _) => false,
            OxidizedError::AuthFailed => false,
            OxidizedError::InvalidRegex(_) => false,
            OxidizedError::ConfigError(_) => false,
            OxidizedError::ParseError { .. } => false,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Error Constants Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_error_prefix_value() {
        assert_eq!(ERROR_PREFIX, "[Error]");
    }

    #[test]
    fn test_context_prefix_value() {
        assert_eq!(CONTEXT_PREFIX, "[Context]");
    }

    #[test]
    fn test_suggestions_prefix_value() {
        assert_eq!(SUGGESTIONS_PREFIX, "[Suggestions]");
    }

    #[test]
    fn test_next_step_prefix_value() {
        assert_eq!(NEXT_STEP_PREFIX, "[Next Step]");
    }

    // -------------------------------------------------------------------------
    // NodeNotFound Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_node_not_found_message_format() {
        let error = OxidizedError::NodeNotFound(
            "SW-Unknown".to_string(),
            vec!["SW-Core-01".to_string(), "SW-Access-02".to_string()],
        );

        let message = error.to_llm_message();

        // Verify all prefixes are used (not hardcoded strings)
        assert!(
            message.contains(ERROR_PREFIX),
            "Should contain ERROR_PREFIX"
        );
        assert!(
            message.contains(CONTEXT_PREFIX),
            "Should contain CONTEXT_PREFIX"
        );
        assert!(
            message.contains(SUGGESTIONS_PREFIX),
            "Should contain SUGGESTIONS_PREFIX"
        );
        assert!(
            message.contains(NEXT_STEP_PREFIX),
            "Should contain NEXT_STEP_PREFIX"
        );

        // Verify content
        assert!(message.contains("SW-Unknown"), "Should contain node name");
        assert!(
            message.contains("SW-Core-01"),
            "Should contain first suggestion"
        );
        assert!(
            message.contains("SW-Access-02"),
            "Should contain second suggestion"
        );
        assert!(
            message.contains("oxidized://nodes"),
            "Should contain resource URI"
        );
    }

    #[test]
    fn test_node_not_found_with_single_suggestion() {
        let error =
            OxidizedError::NodeNotFound("test-node".to_string(), vec!["similar-node".to_string()]);

        let message = error.to_llm_message();

        assert!(message.contains("similar-node"));
        assert!(!message.contains("No similar nodes found"));
    }

    #[test]
    fn test_node_not_found_handles_empty_suggestions_gracefully() {
        // Note: Empty suggestions can occur when no similar nodes exist in inventory.
        // The error should handle this gracefully with a clear message.
        let error = OxidizedError::NodeNotFound("orphan-node".to_string(), vec![]);

        let message = error.to_llm_message();

        // Should handle empty suggestions gracefully with fallback message
        assert!(message.contains("orphan-node"));
        assert!(
            message.contains("No similar nodes found"),
            "Should provide fallback when no suggestions available"
        );
    }

    #[test]
    fn test_node_not_found_is_not_transient() {
        let error = OxidizedError::NodeNotFound("test".to_string(), vec!["similar".to_string()]);

        assert!(
            !error.is_transient(),
            "NodeNotFound should not be transient"
        );
    }

    #[test]
    fn test_node_not_found_display() {
        let error = OxidizedError::NodeNotFound("my-node".to_string(), vec!["other".to_string()]);

        let display = format!("{}", error);
        assert_eq!(display, "Node 'my-node' not found");
    }

    // -------------------------------------------------------------------------
    // ApiUnreachable Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_api_unreachable_message_format() {
        // Create a mock reqwest error using builder
        let error = create_api_unreachable_error(2, Some("2025-01-15T10:30:00Z".to_string()));

        let message = error.to_llm_message();

        // Verify all prefixes are used
        assert!(message.contains(ERROR_PREFIX));
        assert!(message.contains(CONTEXT_PREFIX));
        assert!(message.contains(SUGGESTIONS_PREFIX));
        assert!(message.contains(NEXT_STEP_PREFIX));

        // Verify content
        assert!(message.contains("Attempt 2/3"));
        assert!(message.contains("2025-01-15T10:30:00Z"));
        assert!(message.contains("OXIDIZED_URL"));
    }

    #[test]
    fn test_api_unreachable_without_last_success() {
        let error = create_api_unreachable_error(1, None);

        let message = error.to_llm_message();

        assert!(message.contains("No previous successful connection recorded"));
    }

    #[test]
    fn test_api_unreachable_includes_attempt_count() {
        let error = create_api_unreachable_error(3, None);

        let message = error.to_llm_message();

        assert!(
            message.contains("Attempt 3/3"),
            "Should include attempt count in message"
        );
    }

    #[test]
    fn test_api_unreachable_is_transient() {
        let error = create_api_unreachable_error(1, None);

        assert!(error.is_transient(), "ApiUnreachable should be transient");
    }

    // -------------------------------------------------------------------------
    // InvalidRegex Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_invalid_regex_message_format() {
        let error = OxidizedError::InvalidRegex("[invalid".to_string());

        let message = error.to_llm_message();

        assert!(message.contains(ERROR_PREFIX));
        assert!(message.contains(CONTEXT_PREFIX));
        assert!(message.contains(SUGGESTIONS_PREFIX));
        assert!(message.contains(NEXT_STEP_PREFIX));
        assert!(message.contains("[invalid"));
        assert!(message.contains("regex"));
    }

    #[test]
    fn test_invalid_regex_is_not_transient() {
        let error = OxidizedError::InvalidRegex("bad-pattern".to_string());

        assert!(
            !error.is_transient(),
            "InvalidRegex should not be transient"
        );
    }

    #[test]
    fn test_invalid_regex_display() {
        let error = OxidizedError::InvalidRegex("(unclosed".to_string());

        let display = format!("{}", error);
        assert_eq!(display, "Invalid regex pattern: (unclosed");
    }

    // -------------------------------------------------------------------------
    // AuthFailed Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_auth_failed_message_format() {
        let error = OxidizedError::AuthFailed;

        let message = error.to_llm_message();

        assert!(message.contains(ERROR_PREFIX));
        assert!(message.contains(CONTEXT_PREFIX));
        assert!(message.contains(SUGGESTIONS_PREFIX));
        assert!(message.contains(NEXT_STEP_PREFIX));
        assert!(message.contains("OXIDIZED_USER"));
        assert!(message.contains("OXIDIZED_PASSWORD"));
    }

    #[test]
    fn test_auth_failed_no_credentials_in_message() {
        let error = OxidizedError::AuthFailed;

        let message = error.to_llm_message();

        // NFR6: Verify no actual credential values could leak
        // The message should mention env var names but never actual values
        assert!(!message.contains("password123"));
        assert!(!message.contains("secret"));
        assert!(!message.contains("admin_password"));

        // Should NOT specify which credential failed (security best practice)
        assert!(!message.contains("incorrect password"));
        assert!(!message.contains("invalid username"));
        assert!(!message.contains("wrong user"));
    }

    #[test]
    fn test_auth_failed_is_not_transient() {
        let error = OxidizedError::AuthFailed;

        assert!(!error.is_transient(), "AuthFailed should not be transient");
    }

    #[test]
    fn test_auth_failed_display() {
        let error = OxidizedError::AuthFailed;

        let display = format!("{}", error);
        assert_eq!(display, "Authentication failed");
    }

    // -------------------------------------------------------------------------
    // ConfigError Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_config_error_message_format() {
        let config_err = ConfigError::InvalidUrl("ftp://invalid".to_string());
        let error = OxidizedError::ConfigError(config_err);

        let message = error.to_llm_message();

        assert!(message.contains(ERROR_PREFIX));
        assert!(message.contains(CONTEXT_PREFIX));
        assert!(message.contains(SUGGESTIONS_PREFIX));
        assert!(message.contains(NEXT_STEP_PREFIX));
        assert!(message.contains("ftp://invalid"));
    }

    #[test]
    fn test_config_error_is_not_transient() {
        let config_err = ConfigError::InvalidUrl("bad-url".to_string());
        let error = OxidizedError::ConfigError(config_err);

        assert!(!error.is_transient(), "ConfigError should not be transient");
    }

    #[test]
    fn test_config_error_from_conversion() {
        let config_err = ConfigError::InvalidUrl("test".to_string());

        // Test #[from] attribute - should convert automatically
        let oxidized_err: OxidizedError = config_err.into();

        match oxidized_err {
            OxidizedError::ConfigError(_) => {}
            _ => panic!("Expected ConfigError variant"),
        }
    }

    // -------------------------------------------------------------------------
    // ParseError Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_error_message_format() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let error = OxidizedError::ParseError {
            context: "node list response".to_string(),
            source: json_err,
        };

        let message = error.to_llm_message();

        assert!(message.contains(ERROR_PREFIX));
        assert!(message.contains(CONTEXT_PREFIX));
        assert!(message.contains(SUGGESTIONS_PREFIX));
        assert!(message.contains(NEXT_STEP_PREFIX));
        assert!(message.contains("node list response"));
    }

    #[test]
    fn test_parse_error_is_not_transient() {
        let json_err = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let error = OxidizedError::ParseError {
            context: "test".to_string(),
            source: json_err,
        };

        assert!(!error.is_transient(), "ParseError should not be transient");
    }

    #[test]
    fn test_parse_error_display() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let error = OxidizedError::ParseError {
            context: "config file".to_string(),
            source: json_err,
        };

        let display = format!("{}", error);
        assert!(display.contains("config file"));
        assert!(display.contains("JSON parse error"));
    }

    // -------------------------------------------------------------------------
    // HttpError Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_http_error_5xx_message_format() {
        let error = OxidizedError::HttpError {
            status_code: 503,
            context: "get_nodes".to_string(),
        };

        let message = error.to_llm_message();

        assert!(message.contains(ERROR_PREFIX));
        assert!(message.contains(CONTEXT_PREFIX));
        assert!(message.contains(SUGGESTIONS_PREFIX));
        assert!(message.contains(NEXT_STEP_PREFIX));
        assert!(message.contains("503"));
        assert!(message.contains("Server error"));
        assert!(message.contains("get_nodes"));
    }

    #[test]
    fn test_http_error_4xx_message_format() {
        let error = OxidizedError::HttpError {
            status_code: 400,
            context: "bad_request".to_string(),
        };

        let message = error.to_llm_message();

        assert!(message.contains("400"));
        assert!(message.contains("Client error"));
        assert!(message.contains("bad_request"));
    }

    #[test]
    fn test_http_error_display() {
        let error = OxidizedError::HttpError {
            status_code: 502,
            context: "proxy".to_string(),
        };

        let display = format!("{}", error);
        assert_eq!(display, "HTTP error 502 for proxy");
    }

    // -------------------------------------------------------------------------
    // Security Tests (NFR6)
    // -------------------------------------------------------------------------

    #[test]
    fn test_no_credentials_in_any_error_message() {
        // Test all error variants to ensure no credentials leak

        let errors: Vec<OxidizedError> = vec![
            OxidizedError::NodeNotFound("node".to_string(), vec!["other".to_string()]),
            create_api_unreachable_error(1, None),
            OxidizedError::InvalidRegex("pattern".to_string()),
            OxidizedError::AuthFailed,
            OxidizedError::ConfigError(ConfigError::InvalidUrl("url".to_string())),
            OxidizedError::ParseError {
                context: "test".to_string(),
                source: serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
            },
            OxidizedError::HttpError {
                status_code: 500,
                context: "test".to_string(),
            },
        ];

        // Common credential patterns that should NEVER appear
        let forbidden_patterns = [
            "password:",
            "secret:",
            "token:",
            "api_key:",
            "Authorization:",
            "Basic ",
            "Bearer ",
        ];

        for error in errors {
            let message = error.to_llm_message();

            for pattern in &forbidden_patterns {
                assert!(
                    !message.to_lowercase().contains(&pattern.to_lowercase()),
                    "Error message should not contain '{}': {}",
                    pattern,
                    message
                );
            }
        }
    }

    #[test]
    fn test_api_unreachable_no_url_credentials() {
        // Even if URL contains credentials, they should not appear in message
        let error = create_api_unreachable_error(1, None);
        let message = error.to_llm_message();

        // The error message should reference OXIDIZED_URL but not expose actual URL content
        assert!(message.contains("OXIDIZED_URL"));
        assert!(!message.contains("user:pass@"));
        assert!(!message.contains("admin:secret"));
    }

    // -------------------------------------------------------------------------
    // Comprehensive is_transient Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_all_variants_is_transient_classification() {
        // Transient (retryable)
        assert!(create_api_unreachable_error(1, None).is_transient());

        // HttpError: 5xx is transient, 4xx is not
        assert!(
            OxidizedError::HttpError {
                status_code: 500,
                context: "test".to_string()
            }
            .is_transient()
        );
        assert!(
            OxidizedError::HttpError {
                status_code: 503,
                context: "test".to_string()
            }
            .is_transient()
        );
        assert!(
            !OxidizedError::HttpError {
                status_code: 400,
                context: "test".to_string()
            }
            .is_transient()
        );
        assert!(
            !OxidizedError::HttpError {
                status_code: 429,
                context: "test".to_string()
            }
            .is_transient()
        );

        // Not transient (permanent)
        assert!(!OxidizedError::NodeNotFound("n".to_string(), vec![]).is_transient());
        assert!(!OxidizedError::InvalidRegex("p".to_string()).is_transient());
        assert!(!OxidizedError::AuthFailed.is_transient());
        assert!(
            !OxidizedError::ConfigError(ConfigError::InvalidUrl("u".to_string())).is_transient()
        );
        assert!(
            !OxidizedError::ParseError {
                context: "c".to_string(),
                source: serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
            }
            .is_transient()
        );
    }

    // -------------------------------------------------------------------------
    // Error Constants Usage Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_all_messages_use_error_constants() {
        let errors: Vec<OxidizedError> = vec![
            OxidizedError::NodeNotFound("n".to_string(), vec!["s".to_string()]),
            create_api_unreachable_error(1, Some("ts".to_string())),
            OxidizedError::InvalidRegex("p".to_string()),
            OxidizedError::AuthFailed,
            OxidizedError::ConfigError(ConfigError::InvalidUrl("u".to_string())),
            OxidizedError::ParseError {
                context: "c".to_string(),
                source: serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
            },
            OxidizedError::HttpError {
                status_code: 502,
                context: "c".to_string(),
            },
        ];

        for error in errors {
            let message = error.to_llm_message();

            // All messages must contain all four prefixes
            assert!(
                message.contains(ERROR_PREFIX),
                "Message missing ERROR_PREFIX: {}",
                message
            );
            assert!(
                message.contains(CONTEXT_PREFIX),
                "Message missing CONTEXT_PREFIX: {}",
                message
            );
            assert!(
                message.contains(SUGGESTIONS_PREFIX),
                "Message missing SUGGESTIONS_PREFIX: {}",
                message
            );
            assert!(
                message.contains(NEXT_STEP_PREFIX),
                "Message missing NEXT_STEP_PREFIX: {}",
                message
            );
        }
    }

    // -------------------------------------------------------------------------
    // Helper Functions
    // -------------------------------------------------------------------------

    // Creates an ApiUnreachable error for testing.
    //
    // Note: reqwest::Error has no public constructor, so we must create a real
    // connection error. We use a minimal runtime and connect to port 0 which
    // is guaranteed to fail immediately. This approach is acceptable for tests
    // as the runtime creation overhead is negligible (~1ms per test).
    fn create_api_unreachable_error(attempt: u8, last_success: Option<String>) -> OxidizedError {
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create test runtime");
        let error =
            runtime.block_on(async { reqwest::get("http://127.0.0.1:0").await.unwrap_err() });

        OxidizedError::ApiUnreachable {
            source: error,
            attempt,
            last_success,
        }
    }
}
