//! Oxidized API client implementation with testable backend abstraction.
//!
//! This module provides the [`OxidizedBackend`] trait for abstracting Oxidized API
//! operations and [`OxidizedClient`] as the HTTP client implementation.
//!
//! # Architecture
//!
//! The trait-based design enables:
//! - Dependency injection for testing
//! - Mock backend for unit tests
//! - Future extensions (e.g., direct Git backend)
//!
//! # Example
//!
//! ```ignore
//! use mcp_oxidized::oxidized::{OxidizedBackend, OxidizedClient};
//! use mcp_oxidized::config::Config;
//!
//! let config = Config::load()?;
//! let client = OxidizedClient::new(&config);
//!
//! // List all nodes
//! let nodes = client.get_nodes().await?;
//! for node in nodes {
//!     println!("{}: {}", node.name, node.status);
//! }
//! ```

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::time::Duration;
use tracing::instrument;

use crate::config::Config;
use crate::error::OxidizedError;

// ============================================================================
// Constants
// ============================================================================

/// Default HTTP connect timeout in seconds.
pub const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Default HTTP request timeout in seconds.
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

// ============================================================================
// Data Models
// ============================================================================

/// Represents a network device node in Oxidized inventory.
///
/// Contains metadata about the device including its name, IP address,
/// group classification, and backup status.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "SW-Core-01",
///   "full_name": "SW-Core-01.network.local",
///   "ip": "192.168.1.1",
///   "group": "switches",
///   "model": "cisco-ios",
///   "status": "success",
///   "last_status": "success",
///   "time": "2025-01-15 10:30:00 UTC",
///   "mtime": "2025-01-15 10:25:00 UTC"
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct Node {
    /// Short device name (e.g., "SW-Core-01")
    pub name: String,
    /// Fully qualified device name
    pub full_name: String,
    /// Device IP address
    pub ip: String,
    /// Device group/category (e.g., "switches", "routers")
    pub group: String,
    /// Device model/platform (e.g., "cisco-ios", "junos")
    pub model: String,
    /// Current backup status (e.g., "success", "failure", "never")
    pub status: String,
    /// Previous backup status
    pub last_status: String,
    /// Timestamp of last backup attempt
    pub time: Option<String>,
    /// Timestamp of last configuration modification
    pub mtime: Option<String>,
}

/// Represents a configuration version in Oxidized Git repository.
///
/// Each version corresponds to a Git commit containing a configuration snapshot.
///
/// # Example JSON
///
/// ```json
/// {
///   "oid": "abc123def456",
///   "date": "2025-01-15 10:30:00 UTC",
///   "author": "oxidized",
///   "message": "update SW-Core-01"
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct NodeVersion {
    /// Git object ID (commit hash)
    pub oid: String,
    /// Commit timestamp
    pub date: String,
    /// Commit author
    pub author: String,
    /// Commit message
    pub message: String,
}

/// Global Oxidized server statistics.
///
/// Provides an overview of the backup system's health and activity.
///
/// # Example JSON
///
/// ```json
/// {
///   "total_nodes": 150,
///   "success_count": 145,
///   "failure_count": 5,
///   "last_run": "2025-01-15 10:30:00 UTC"
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct Stats {
    /// Total number of managed nodes
    pub total_nodes: Option<u32>,
    /// Number of successful backups
    pub success_count: Option<u32>,
    /// Number of failed backups
    pub failure_count: Option<u32>,
    /// Timestamp of last backup run
    pub last_run: Option<String>,
}

// ============================================================================
// OxidizedBackend Trait
// ============================================================================

/// Trait for abstracting Oxidized API operations.
///
/// This trait defines the contract for interacting with the Oxidized backup system.
/// The primary implementation is [`OxidizedClient`], but the trait allows for
/// mock implementations in tests.
///
/// # Thread Safety
///
/// All implementations must be `Send + Sync` to allow use across async tasks.
///
/// # Error Handling
///
/// All methods return `Result<T, OxidizedError>` where errors are classified as
/// transient (retryable) or permanent. See [`OxidizedError`] for details.
///
/// # Example
///
/// ```ignore
/// async fn list_nodes<B: OxidizedBackend>(backend: &B) -> Result<(), OxidizedError> {
///     let nodes = backend.get_nodes().await?;
///     for node in nodes {
///         println!("{}: {}", node.name, node.status);
///     }
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait OxidizedBackend: Send + Sync {
    /// Retrieve all nodes from Oxidized inventory.
    ///
    /// Returns the complete list of managed network devices.
    ///
    /// # Errors
    ///
    /// - [`OxidizedError::ApiUnreachable`] - Network/connection error
    /// - [`OxidizedError::AuthFailed`] - Authentication failure
    /// - [`OxidizedError::ParseError`] - Invalid JSON response
    async fn get_nodes(&self) -> Result<Vec<Node>, OxidizedError>;

    /// Retrieve a specific node by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The node name to look up (e.g., "SW-Core-01")
    ///
    /// # Errors
    ///
    /// - [`OxidizedError::NodeNotFound`] - Node does not exist
    /// - [`OxidizedError::ApiUnreachable`] - Network/connection error
    /// - [`OxidizedError::AuthFailed`] - Authentication failure
    async fn get_node(&self, name: &str) -> Result<Node, OxidizedError>;

    /// Retrieve the current configuration for a node.
    ///
    /// Returns the latest configuration text from the device.
    ///
    /// # Arguments
    ///
    /// * `name` - The node name to fetch configuration for
    ///
    /// # Errors
    ///
    /// - [`OxidizedError::NodeNotFound`] - Node does not exist
    /// - [`OxidizedError::ApiUnreachable`] - Network/connection error
    async fn get_node_config(&self, name: &str) -> Result<String, OxidizedError>;

    /// Retrieve version history for a node.
    ///
    /// Returns a list of configuration versions (Git commits).
    ///
    /// # Arguments
    ///
    /// * `name` - The node name to get versions for
    ///
    /// # Errors
    ///
    /// - [`OxidizedError::NodeNotFound`] - Node does not exist
    /// - [`OxidizedError::ApiUnreachable`] - Network/connection error
    async fn get_node_versions(&self, name: &str) -> Result<Vec<NodeVersion>, OxidizedError>;

    /// Retrieve a specific configuration version.
    ///
    /// Returns the configuration text at a specific point in time.
    ///
    /// # Arguments
    ///
    /// * `name` - The node name
    /// * `oid` - The Git object ID (commit hash) of the version
    ///
    /// # Errors
    ///
    /// - [`OxidizedError::NodeNotFound`] - Node or version does not exist
    /// - [`OxidizedError::ApiUnreachable`] - Network/connection error
    async fn get_node_version(&self, name: &str, oid: &str) -> Result<String, OxidizedError>;

    /// Retrieve global Oxidized statistics.
    ///
    /// Returns server-wide backup statistics and health metrics.
    ///
    /// # Errors
    ///
    /// - [`OxidizedError::ApiUnreachable`] - Network/connection error
    /// - [`OxidizedError::ParseError`] - Invalid JSON response
    async fn get_stats(&self) -> Result<Stats, OxidizedError>;

    /// Trigger an immediate backup for a node.
    ///
    /// Requests Oxidized to prioritize and run backup for the specified node.
    ///
    /// # Arguments
    ///
    /// * `node` - The node name to backup
    ///
    /// # Errors
    ///
    /// - [`OxidizedError::NodeNotFound`] - Node does not exist
    /// - [`OxidizedError::ApiUnreachable`] - Network/connection error
    async fn trigger_backup(&self, node: &str) -> Result<(), OxidizedError>;

    /// Prioritize a node in the backup queue.
    ///
    /// Moves the node to the front of the backup queue.
    ///
    /// # Arguments
    ///
    /// * `node` - The node name to prioritize
    ///
    /// # Errors
    ///
    /// - [`OxidizedError::NodeNotFound`] - Node does not exist
    /// - [`OxidizedError::ApiUnreachable`] - Network/connection error
    async fn prioritize_node(&self, node: &str) -> Result<(), OxidizedError>;

    /// Reload the Oxidized source inventory.
    ///
    /// Triggers Oxidized to re-read its node inventory from the configured source.
    ///
    /// # Errors
    ///
    /// - [`OxidizedError::ApiUnreachable`] - Network/connection error
    async fn reload_sources(&self) -> Result<(), OxidizedError>;
}

// ============================================================================
// BasicAuth
// ============================================================================

/// HTTP Basic Authentication credentials.
#[derive(Clone)]
pub struct BasicAuth {
    username: String,
    password: String,
}

impl BasicAuth {
    /// Create new Basic Auth credentials.
    pub fn new(username: String, password: String) -> Self {
        Self { username, password }
    }
}

// ============================================================================
// OxidizedClient
// ============================================================================

/// HTTP client implementation of [`OxidizedBackend`].
///
/// Provides HTTP-based access to the Oxidized REST API with support for
/// Basic Authentication and configurable timeouts.
///
/// # Configuration
///
/// The client is configured via [`Config`] which reads from environment variables:
/// - `OXIDIZED_URL` - Base URL for the Oxidized server
/// - `OXIDIZED_USER` / `OXIDIZED_PASSWORD` - Optional authentication credentials
///
/// # Example
///
/// ```ignore
/// use mcp_oxidized::oxidized::OxidizedClient;
/// use mcp_oxidized::config::Config;
///
/// let config = Config::load()?;
/// let client = OxidizedClient::new(&config);
///
/// let nodes = client.get_nodes().await?;
/// ```
pub struct OxidizedClient {
    client: Client,
    base_url: String,
    auth: Option<BasicAuth>,
}

impl OxidizedClient {
    /// Create a new OxidizedClient from configuration.
    ///
    /// Initializes an HTTP client with appropriate timeouts and authentication
    /// settings based on the provided configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration including URL and optional credentials
    ///
    /// # Panics
    ///
    /// Panics if the reqwest client cannot be built (extremely rare, indicates
    /// TLS backend issues).
    pub fn new(config: &Config) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS))
            .build()
            .expect("Failed to build HTTP client");

        let auth = match (&config.oxidized_user, &config.oxidized_password) {
            (Some(user), Some(pass)) => Some(BasicAuth::new(user.clone(), pass.clone())),
            _ => None,
        };

        Self {
            client,
            base_url: config.oxidized_url.clone(),
            auth,
        }
    }

    /// Build an authenticated request to the given endpoint.
    fn build_request(&self, endpoint: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, endpoint);
        let mut request = self.client.get(&url);

        if let Some(auth) = &self.auth {
            request = request.basic_auth(&auth.username, Some(&auth.password));
        }

        request
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
    }

    /// Build an authenticated PUT request to the given endpoint.
    fn build_put_request(&self, endpoint: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, endpoint);
        let mut request = self.client.put(&url);

        if let Some(auth) = &self.auth {
            request = request.basic_auth(&auth.username, Some(&auth.password));
        }

        request
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
    }

    /// Handle HTTP response and map errors appropriately.
    async fn handle_json_response<T: serde::de::DeserializeOwned>(
        &self,
        response: Result<reqwest::Response, reqwest::Error>,
        context: &str,
    ) -> Result<T, OxidizedError> {
        let response = self.handle_request_error(response)?;
        let status = response.status();

        self.check_status(status, context)?;

        // Get response body first for error context
        let body = response
            .text()
            .await
            .map_err(|e| OxidizedError::ApiUnreachable {
                source: e,
                attempt: 1,
                last_success: None,
            })?;

        serde_json::from_str::<T>(&body).map_err(|e| OxidizedError::ParseError {
            context: context.to_string(),
            source: e,
        })
    }

    /// Handle HTTP response for text content.
    async fn handle_text_response(
        &self,
        response: Result<reqwest::Response, reqwest::Error>,
        context: &str,
    ) -> Result<String, OxidizedError> {
        let response = self.handle_request_error(response)?;
        let status = response.status();

        self.check_status(status, context)?;

        response
            .text()
            .await
            .map_err(|e| OxidizedError::ApiUnreachable {
                source: e,
                attempt: 1,
                last_success: None,
            })
    }

    /// Handle HTTP response for empty responses (PUT/POST operations).
    async fn handle_empty_response(
        &self,
        response: Result<reqwest::Response, reqwest::Error>,
        context: &str,
    ) -> Result<(), OxidizedError> {
        let response = self.handle_request_error(response)?;
        let status = response.status();

        self.check_status(status, context)?;

        Ok(())
    }

    /// Convert reqwest errors to OxidizedError.
    fn handle_request_error(
        &self,
        response: Result<reqwest::Response, reqwest::Error>,
    ) -> Result<reqwest::Response, OxidizedError> {
        response.map_err(|e| OxidizedError::ApiUnreachable {
            source: e,
            attempt: 1,
            last_success: None,
        })
    }

    /// Check HTTP status code and map to appropriate error.
    fn check_status(&self, status: StatusCode, context: &str) -> Result<(), OxidizedError> {
        if status == StatusCode::NOT_FOUND {
            return Err(OxidizedError::NodeNotFound(
                context.to_string(),
                vec![], // TODO(Story 1.6): Populate suggestions via fuzzy matching
            ));
        }

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(OxidizedError::AuthFailed);
        }

        // 5xx errors are server-side issues - map to HttpError (transient, retryable)
        if status.is_server_error() {
            return Err(OxidizedError::HttpError {
                status_code: status.as_u16(),
                context: context.to_string(),
            });
        }

        // Other 4xx errors (except 401/403/404 handled above)
        if status.is_client_error() {
            return Err(OxidizedError::HttpError {
                status_code: status.as_u16(),
                context: context.to_string(),
            });
        }

        Ok(())
    }
}

#[async_trait]
impl OxidizedBackend for OxidizedClient {
    #[instrument(skip(self), fields(url = %self.base_url))]
    async fn get_nodes(&self) -> Result<Vec<Node>, OxidizedError> {
        let response = self.build_request("/nodes.json").send().await;
        self.handle_json_response(response, "node list").await
    }

    #[instrument(skip(self), fields(url = %self.base_url, node = %name))]
    async fn get_node(&self, name: &str) -> Result<Node, OxidizedError> {
        let endpoint = format!("/node/show/{}.json", name);
        let response = self.build_request(&endpoint).send().await;
        self.handle_json_response(response, name).await
    }

    #[instrument(skip(self), fields(url = %self.base_url, node = %name))]
    async fn get_node_config(&self, name: &str) -> Result<String, OxidizedError> {
        let endpoint = format!("/node/fetch/{}", name);
        let response = self.build_request(&endpoint).send().await;
        self.handle_text_response(response, name).await
    }

    #[instrument(skip(self), fields(url = %self.base_url, node = %name))]
    async fn get_node_versions(&self, name: &str) -> Result<Vec<NodeVersion>, OxidizedError> {
        let endpoint = format!("/node/version?node={}", name);
        let response = self.build_request(&endpoint).send().await;
        self.handle_json_response(response, name).await
    }

    #[instrument(skip(self), fields(url = %self.base_url, node = %name, oid = %oid))]
    async fn get_node_version(&self, name: &str, oid: &str) -> Result<String, OxidizedError> {
        let endpoint = format!("/node/version?node={}&oid={}", name, oid);
        let response = self.build_request(&endpoint).send().await;
        self.handle_text_response(response, &format!("{}@{}", name, oid))
            .await
    }

    #[instrument(skip(self), fields(url = %self.base_url))]
    async fn get_stats(&self) -> Result<Stats, OxidizedError> {
        let response = self.build_request("/").send().await;
        self.handle_json_response(response, "stats").await
    }

    #[instrument(skip(self), fields(url = %self.base_url, node = %node))]
    async fn trigger_backup(&self, node: &str) -> Result<(), OxidizedError> {
        // Oxidized uses PUT /node/next/{name} to prioritize and trigger backup
        let endpoint = format!("/node/next/{}", node);
        let response = self.build_put_request(&endpoint).send().await;
        self.handle_empty_response(response, node).await
    }

    #[instrument(skip(self), fields(url = %self.base_url, node = %node))]
    async fn prioritize_node(&self, node: &str) -> Result<(), OxidizedError> {
        let endpoint = format!("/node/next/{}", node);
        let response = self.build_put_request(&endpoint).send().await;
        self.handle_empty_response(response, node).await
    }

    #[instrument(skip(self), fields(url = %self.base_url))]
    async fn reload_sources(&self) -> Result<(), OxidizedError> {
        let response = self.build_request("/reload?format=json").send().await;
        self.handle_empty_response(response, "reload").await
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Data Model Deserialization Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_node_deserialize() {
        let json = r#"{
            "name": "SW-Core-01",
            "full_name": "SW-Core-01.network.local",
            "ip": "192.168.1.1",
            "group": "switches",
            "model": "cisco-ios",
            "status": "success",
            "last_status": "success",
            "time": "2025-01-15 10:30:00 UTC",
            "mtime": "2025-01-15 10:25:00 UTC"
        }"#;

        let node: Node = serde_json::from_str(json).expect("Should deserialize Node");

        assert_eq!(node.name, "SW-Core-01");
        assert_eq!(node.full_name, "SW-Core-01.network.local");
        assert_eq!(node.ip, "192.168.1.1");
        assert_eq!(node.group, "switches");
        assert_eq!(node.model, "cisco-ios");
        assert_eq!(node.status, "success");
        assert_eq!(node.last_status, "success");
        assert_eq!(node.time, Some("2025-01-15 10:30:00 UTC".to_string()));
        assert_eq!(node.mtime, Some("2025-01-15 10:25:00 UTC".to_string()));
    }

    #[test]
    fn test_node_deserialize_without_optional_fields() {
        let json = r#"{
            "name": "SW-Core-01",
            "full_name": "SW-Core-01.network.local",
            "ip": "192.168.1.1",
            "group": "switches",
            "model": "cisco-ios",
            "status": "never",
            "last_status": "never"
        }"#;

        let node: Node =
            serde_json::from_str(json).expect("Should deserialize Node without optionals");

        assert_eq!(node.name, "SW-Core-01");
        assert_eq!(node.time, None);
        assert_eq!(node.mtime, None);
    }

    #[test]
    fn test_node_version_deserialize() {
        let json = r#"{
            "oid": "abc123def456",
            "date": "2025-01-15 10:30:00 UTC",
            "author": "oxidized",
            "message": "update SW-Core-01"
        }"#;

        let version: NodeVersion =
            serde_json::from_str(json).expect("Should deserialize NodeVersion");

        assert_eq!(version.oid, "abc123def456");
        assert_eq!(version.date, "2025-01-15 10:30:00 UTC");
        assert_eq!(version.author, "oxidized");
        assert_eq!(version.message, "update SW-Core-01");
    }

    #[test]
    fn test_stats_deserialize() {
        let json = r#"{
            "total_nodes": 150,
            "success_count": 145,
            "failure_count": 5,
            "last_run": "2025-01-15 10:30:00 UTC"
        }"#;

        let stats: Stats = serde_json::from_str(json).expect("Should deserialize Stats");

        assert_eq!(stats.total_nodes, Some(150));
        assert_eq!(stats.success_count, Some(145));
        assert_eq!(stats.failure_count, Some(5));
        assert_eq!(stats.last_run, Some("2025-01-15 10:30:00 UTC".to_string()));
    }

    #[test]
    fn test_stats_deserialize_partial() {
        let json = r#"{
            "total_nodes": 10
        }"#;

        let stats: Stats = serde_json::from_str(json).expect("Should deserialize partial Stats");

        assert_eq!(stats.total_nodes, Some(10));
        assert_eq!(stats.success_count, None);
        assert_eq!(stats.failure_count, None);
        assert_eq!(stats.last_run, None);
    }

    // -------------------------------------------------------------------------
    // OxidizedClient Construction Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_client_new_without_auth() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };

        let client = OxidizedClient::new(&config);

        assert_eq!(client.base_url, "http://localhost:8888");
        assert!(client.auth.is_none());
    }

    #[test]
    fn test_client_new_with_auth() {
        let config = Config {
            oxidized_url: "https://oxidized.example.com".to_string(),
            oxidized_user: Some("admin".to_string()),
            oxidized_password: Some("secret".to_string()),
        };

        let client = OxidizedClient::new(&config);

        assert_eq!(client.base_url, "https://oxidized.example.com");
        assert!(client.auth.is_some());
    }

    #[test]
    fn test_client_new_with_partial_auth_no_password() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: Some("admin".to_string()),
            oxidized_password: None,
        };

        let client = OxidizedClient::new(&config);

        // Should not set auth if password is missing
        assert!(client.auth.is_none());
    }

    #[test]
    fn test_client_new_with_partial_auth_no_user() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: Some("secret".to_string()),
        };

        let client = OxidizedClient::new(&config);

        // Should not set auth if user is missing
        assert!(client.auth.is_none());
    }

    // -------------------------------------------------------------------------
    // Error Mapping Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_check_status_not_found() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        let result = client.check_status(StatusCode::NOT_FOUND, "test-node");

        assert!(result.is_err());
        match result.unwrap_err() {
            OxidizedError::NodeNotFound(name, _) => {
                assert_eq!(name, "test-node");
            }
            _ => panic!("Expected NodeNotFound error"),
        }
    }

    #[test]
    fn test_check_status_unauthorized() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        let result = client.check_status(StatusCode::UNAUTHORIZED, "test");

        assert!(result.is_err());
        match result.unwrap_err() {
            OxidizedError::AuthFailed => {}
            _ => panic!("Expected AuthFailed error"),
        }
    }

    #[test]
    fn test_check_status_forbidden() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        let result = client.check_status(StatusCode::FORBIDDEN, "test");

        assert!(result.is_err());
        match result.unwrap_err() {
            OxidizedError::AuthFailed => {}
            _ => panic!("Expected AuthFailed error"),
        }
    }

    #[test]
    fn test_check_status_success() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        let result = client.check_status(StatusCode::OK, "test");

        assert!(result.is_ok());
    }

    // -------------------------------------------------------------------------
    // BasicAuth Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_basic_auth_new() {
        let auth = BasicAuth::new("user".to_string(), "pass".to_string());

        assert_eq!(auth.username, "user");
        assert_eq!(auth.password, "pass");
    }

    // -------------------------------------------------------------------------
    // Node List Deserialization Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_node_list_deserialize() {
        let json = r#"[
            {
                "name": "SW-Core-01",
                "full_name": "SW-Core-01.network.local",
                "ip": "192.168.1.1",
                "group": "switches",
                "model": "cisco-ios",
                "status": "success",
                "last_status": "success"
            },
            {
                "name": "RTR-Edge-01",
                "full_name": "RTR-Edge-01.network.local",
                "ip": "192.168.1.2",
                "group": "routers",
                "model": "junos",
                "status": "failure",
                "last_status": "success"
            }
        ]"#;

        let nodes: Vec<Node> = serde_json::from_str(json).expect("Should deserialize node list");

        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].name, "SW-Core-01");
        assert_eq!(nodes[1].name, "RTR-Edge-01");
    }

    // -------------------------------------------------------------------------
    // Fixture-based Deserialization Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_node_deserialize_from_fixture() {
        let json = std::fs::read_to_string("fixtures/node.json").expect("Should read fixture file");
        let node: Node = serde_json::from_str(&json).expect("Should deserialize Node from fixture");

        assert_eq!(node.name, "SW-Core-01");
        assert_eq!(node.full_name, "SW-Core-01.network.local");
        assert_eq!(node.ip, "192.168.1.1");
        assert_eq!(node.group, "switches");
        assert_eq!(node.model, "cisco-ios");
        assert_eq!(node.status, "success");
    }

    #[test]
    fn test_nodes_list_deserialize_from_fixture() {
        let json =
            std::fs::read_to_string("fixtures/nodes.json").expect("Should read fixture file");
        let nodes: Vec<Node> =
            serde_json::from_str(&json).expect("Should deserialize nodes from fixture");

        assert_eq!(nodes.len(), 5);
        assert_eq!(nodes[0].name, "SW-Core-01");
        assert_eq!(nodes[2].name, "RTR-Edge-01");
        assert_eq!(nodes[2].status, "failure");
        // Verify node without time/mtime (AP-Floor3-01)
        assert_eq!(nodes[4].name, "AP-Floor3-01");
        assert_eq!(nodes[4].time, None);
        assert_eq!(nodes[4].mtime, None);
    }

    #[test]
    fn test_stats_deserialize_from_fixture() {
        let json =
            std::fs::read_to_string("fixtures/stats.json").expect("Should read fixture file");
        let stats: Stats =
            serde_json::from_str(&json).expect("Should deserialize Stats from fixture");

        assert_eq!(stats.total_nodes, Some(150));
        assert_eq!(stats.success_count, Some(142));
        assert_eq!(stats.failure_count, Some(5));
        assert!(stats.last_run.is_some());
    }

    #[test]
    fn test_versions_deserialize_from_fixture() {
        let json =
            std::fs::read_to_string("fixtures/versions.json").expect("Should read fixture file");
        let versions: Vec<NodeVersion> =
            serde_json::from_str(&json).expect("Should deserialize versions from fixture");

        assert_eq!(versions.len(), 5);
        assert_eq!(versions[0].oid, "abc123def456789012345678901234567890abcd");
        assert_eq!(versions[0].author, "oxidized");
        assert!(versions[0].message.contains("SW-Core-01"));
    }

    // -------------------------------------------------------------------------
    // HTTP Error Status Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_check_status_server_error() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        let result = client.check_status(StatusCode::INTERNAL_SERVER_ERROR, "test");

        assert!(result.is_err());
        match result.unwrap_err() {
            OxidizedError::HttpError {
                status_code,
                context,
            } => {
                assert_eq!(status_code, 500);
                assert_eq!(context, "test");
            }
            _ => panic!("Expected HttpError"),
        }
    }

    #[test]
    fn test_check_status_bad_gateway() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        let result = client.check_status(StatusCode::BAD_GATEWAY, "proxy");

        assert!(result.is_err());
        match result.unwrap_err() {
            OxidizedError::HttpError { status_code, .. } => {
                assert_eq!(status_code, 502);
            }
            _ => panic!("Expected HttpError"),
        }
    }

    #[test]
    fn test_check_status_bad_request() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        let result = client.check_status(StatusCode::BAD_REQUEST, "invalid");

        assert!(result.is_err());
        match result.unwrap_err() {
            OxidizedError::HttpError { status_code, .. } => {
                assert_eq!(status_code, 400);
            }
            _ => panic!("Expected HttpError"),
        }
    }

    // -------------------------------------------------------------------------
    // Timeout Configuration Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_timeout_constants_are_reasonable() {
        // Connect timeout should be shorter than request timeout
        assert!(
            DEFAULT_CONNECT_TIMEOUT_SECS < DEFAULT_REQUEST_TIMEOUT_SECS,
            "Connect timeout should be less than request timeout"
        );

        // Connect timeout should be at least 5 seconds for slow networks
        assert!(
            DEFAULT_CONNECT_TIMEOUT_SECS >= 5,
            "Connect timeout should be at least 5 seconds"
        );

        // Request timeout should be at least 15 seconds for large responses
        assert!(
            DEFAULT_REQUEST_TIMEOUT_SECS >= 15,
            "Request timeout should be at least 15 seconds"
        );

        // Request timeout should not exceed 60 seconds (reasonable upper bound)
        assert!(
            DEFAULT_REQUEST_TIMEOUT_SECS <= 60,
            "Request timeout should not exceed 60 seconds"
        );
    }

    #[test]
    fn test_client_uses_timeout_constants() {
        // Verify the constants are what we expect (as documented in story)
        assert_eq!(
            DEFAULT_CONNECT_TIMEOUT_SECS, 10,
            "Connect timeout should be 10s"
        );
        assert_eq!(
            DEFAULT_REQUEST_TIMEOUT_SECS, 30,
            "Request timeout should be 30s"
        );
    }
}
