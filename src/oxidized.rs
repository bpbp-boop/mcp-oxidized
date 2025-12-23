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
use moka::future::Cache;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::instrument;

use crate::config::Config;
use crate::error::{Actionable, OxidizedError};

// ============================================================================
// Constants
// ============================================================================

/// Default HTTP connect timeout in seconds.
pub const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Default HTTP request timeout in seconds.
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

// ============================================================================
// Cache TTL Constants (FR28, FR29, FR30)
// ============================================================================

/// Cache TTL for nodes list in seconds (5 minutes).
/// Node inventory changes infrequently during a session.
pub const NODES_CACHE_TTL_SECS: u64 = 300;

/// Cache TTL for node configurations in seconds (2 minutes).
/// Balance between performance and freshness.
pub const CONFIG_CACHE_TTL_SECS: u64 = 120;

/// Cache TTL for statistics in seconds (30 seconds).
/// Provides near real-time feel for stats.
pub const STATS_CACHE_TTL_SECS: u64 = 30;

// ============================================================================
// Retry Configuration Constants (NFR11)
// ============================================================================

/// Maximum number of retry attempts (initial + 2 retries).
pub const MAX_RETRY_ATTEMPTS: u8 = 3;

/// Retry delays in milliseconds for exponential backoff.
/// Delay sequence: 200ms, 800ms (exponential progression).
pub const RETRY_DELAYS_MS: [u64; 2] = [200, 800];

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
// Cache Metadata (FR32)
// ============================================================================

/// Metadata indicating cache status for responses (FR32).
///
/// Included in cached responses to indicate whether the data came from
/// cache (hit) or was freshly fetched from the API (miss).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    /// True if the response was served from cache
    pub cache_hit: bool,
}

impl CacheMetadata {
    /// Create metadata for a cache hit.
    pub fn hit() -> Self {
        Self { cache_hit: true }
    }

    /// Create metadata for a cache miss.
    pub fn miss() -> Self {
        Self { cache_hit: false }
    }
}

/// Response wrapper for nodes list with cache metadata.
#[derive(Debug, Clone, Serialize)]
pub struct CachedNodes {
    /// The list of nodes
    pub nodes: Vec<Node>,
    /// Cache status metadata
    pub metadata: CacheMetadata,
}

/// Response wrapper for a single node with cache metadata.
#[derive(Debug, Clone, Serialize)]
pub struct CachedNode {
    /// The node data
    pub node: Node,
    /// Cache status metadata
    pub metadata: CacheMetadata,
}

/// Response wrapper for node configuration with cache metadata.
#[derive(Debug, Clone, Serialize)]
pub struct CachedConfig {
    /// The configuration text
    pub config: String,
    /// Cache status metadata
    pub metadata: CacheMetadata,
}

/// Response wrapper for statistics with cache metadata.
#[derive(Debug, Clone, Serialize)]
pub struct CachedStats {
    /// The statistics data
    pub stats: Stats,
    /// Cache status metadata
    pub metadata: CacheMetadata,
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
/// # Cache Metadata (FR32)
///
/// Cached read operations return tuples with [`CacheMetadata`] to indicate
/// cache hit/miss status for MCP response inclusion.
///
/// # Example
///
/// ```ignore
/// async fn list_nodes<B: OxidizedBackend>(backend: &B) -> Result<(), OxidizedError> {
///     let (nodes, metadata) = backend.get_nodes().await?;
///     println!("Cache hit: {}", metadata.cache_hit);
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
    /// Returns the complete list of managed network devices with cache metadata (FR32).
    ///
    /// # Errors
    ///
    /// - [`OxidizedError::ApiUnreachable`] - Network/connection error
    /// - [`OxidizedError::AuthFailed`] - Authentication failure
    /// - [`OxidizedError::ParseError`] - Invalid JSON response
    async fn get_nodes(&self) -> Result<(Vec<Node>, CacheMetadata), OxidizedError>;

    /// Retrieve a specific node by name.
    ///
    /// Returns node details with cache metadata (FR32).
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
    async fn get_node(&self, name: &str) -> Result<(Node, CacheMetadata), OxidizedError>;

    /// Retrieve the current configuration for a node.
    ///
    /// Returns the latest configuration text with cache metadata (FR32).
    ///
    /// # Arguments
    ///
    /// * `name` - The node name to fetch configuration for
    ///
    /// # Errors
    ///
    /// - [`OxidizedError::NodeNotFound`] - Node does not exist
    /// - [`OxidizedError::ApiUnreachable`] - Network/connection error
    async fn get_node_config(&self, name: &str) -> Result<(String, CacheMetadata), OxidizedError>;

    /// Retrieve version history for a node.
    ///
    /// Returns a list of configuration versions (Git commits).
    /// Note: Versions are not cached (historical data, rarely accessed repeatedly).
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
    /// Note: Version content is not cached (point-in-time data).
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
    /// Returns server-wide backup statistics with cache metadata (FR32).
    ///
    /// # Errors
    ///
    /// - [`OxidizedError::ApiUnreachable`] - Network/connection error
    /// - [`OxidizedError::ParseError`] - Invalid JSON response
    async fn get_stats(&self) -> Result<(Stats, CacheMetadata), OxidizedError>;

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
    // Integrated caches (FR28, FR29, FR30)
    nodes_cache: Cache<(), Vec<Node>>,
    config_cache: Cache<String, String>,
    stats_cache: Cache<(), Stats>,
    node_cache: Cache<String, Node>,
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

        // Initialize caches with appropriate TTLs (FR28, FR29, FR30)
        let nodes_cache = Cache::builder()
            .time_to_live(Duration::from_secs(NODES_CACHE_TTL_SECS))
            .build();

        let config_cache = Cache::builder()
            .time_to_live(Duration::from_secs(CONFIG_CACHE_TTL_SECS))
            .build();

        let stats_cache = Cache::builder()
            .time_to_live(Duration::from_secs(STATS_CACHE_TTL_SECS))
            .build();

        let node_cache = Cache::builder()
            .time_to_live(Duration::from_secs(NODES_CACHE_TTL_SECS))
            .build();

        Self {
            client,
            base_url: config.oxidized_url.clone(),
            auth,
            nodes_cache,
            config_cache,
            stats_cache,
            node_cache,
        }
    }

    /// Execute an operation with retry on transient errors (NFR11).
    ///
    /// Implements exponential backoff with delays [200ms, 800ms] for up to 3 total attempts.
    /// Only retries if the error's `is_transient()` returns true.
    ///
    /// # Arguments
    ///
    /// * `operation` - A closure that returns a Future producing a Result
    ///
    /// # Returns
    ///
    /// The result of the operation, or the final error after all retries exhausted.
    async fn execute_with_retry<T, F, Fut>(&self, operation: F) -> Result<T, OxidizedError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, OxidizedError>>,
    {
        let delays = [
            Duration::from_millis(RETRY_DELAYS_MS[0]),
            Duration::from_millis(RETRY_DELAYS_MS[1]),
        ];

        let mut last_error: Option<OxidizedError> = None;

        for attempt in 0..MAX_RETRY_ATTEMPTS {
            match operation().await {
                Ok(value) => return Ok(value),
                Err(e) if e.is_transient() && attempt < MAX_RETRY_ATTEMPTS - 1 => {
                    let delay = delays[attempt as usize];
                    tracing::warn!(
                        attempt = attempt + 1,
                        max_attempts = MAX_RETRY_ATTEMPTS,
                        delay_ms = delay.as_millis() as u64,
                        error_type = %e.error_type(),
                        "Request failed, retrying"
                    );
                    tokio::time::sleep(delay).await;
                    last_error = Some(e);
                }
                Err(e) => {
                    if attempt > 0 {
                        tracing::error!(
                            attempts = attempt + 1,
                            error_type = %e.error_type(),
                            "Request failed after all retries"
                        );
                    }
                    return Err(e);
                }
            }
        }

        // If we exhausted all retries, return the last transient error
        let error = last_error.expect("Should have error after max retries");
        tracing::error!(
            attempts = MAX_RETRY_ATTEMPTS,
            error_type = %error.error_type(),
            "Request failed after all retries"
        );
        Err(error)
    }

    /// Invalidate cache entries for a specific node (AC: 4).
    ///
    /// Clears the config_cache and node_cache entries for the specified node.
    /// Called after successful write operations that affect a single node.
    ///
    /// # Arguments
    ///
    /// * `name` - The node name to invalidate cache for
    pub async fn invalidate_node(&self, name: &str) {
        self.config_cache.invalidate(name).await;
        self.node_cache.invalidate(name).await;
    }

    /// Invalidate all cache entries (AC: 4).
    ///
    /// Clears all caches: nodes_cache, config_cache, node_cache, and stats_cache.
    /// Called after successful operations that may affect the entire inventory.
    pub async fn invalidate_all_nodes(&self) {
        self.nodes_cache.invalidate_all();
        self.config_cache.invalidate_all();
        self.node_cache.invalidate_all();
        self.stats_cache.invalidate_all();
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
    ///
    /// Note: NodeNotFound returns empty suggestions because this method runs at the
    /// HTTP layer without access to the node list. The resources layer enriches
    /// NodeNotFound errors with fuzzy-matched suggestions via `find_similar_nodes`.
    fn check_status(&self, status: StatusCode, context: &str) -> Result<(), OxidizedError> {
        if status == StatusCode::NOT_FOUND {
            // Empty suggestions here - enriched by resources::get_node with fuzzy matching
            return Err(OxidizedError::NodeNotFound(context.to_string(), vec![]));
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
    async fn get_nodes(&self) -> Result<(Vec<Node>, CacheMetadata), OxidizedError> {
        // Check cache first
        if let Some(cached) = self.nodes_cache.get(&()).await {
            tracing::debug!("Cache hit for nodes list");
            return Ok((cached, CacheMetadata::hit()));
        }

        // Cache miss - fetch with retry
        tracing::debug!("Cache miss for nodes list, fetching from API");
        let nodes: Vec<Node> = self
            .execute_with_retry(|| async {
                let response = self.build_request("/nodes.json").send().await;
                self.handle_json_response(response, "node list").await
            })
            .await?;

        // Store in cache
        self.nodes_cache.insert((), nodes.clone()).await;
        Ok((nodes, CacheMetadata::miss()))
    }

    #[instrument(skip(self), fields(url = %self.base_url, node = %name))]
    async fn get_node(&self, name: &str) -> Result<(Node, CacheMetadata), OxidizedError> {
        // Check cache first
        if let Some(cached) = self.node_cache.get(name).await {
            tracing::debug!(node = %name, "Cache hit for node");
            return Ok((cached, CacheMetadata::hit()));
        }

        // Cache miss - fetch with retry
        tracing::debug!(node = %name, "Cache miss for node, fetching from API");
        let endpoint = format!("/node/show/{}.json", name);
        let node: Node = self
            .execute_with_retry(|| async {
                let response = self.build_request(&endpoint).send().await;
                self.handle_json_response(response, name).await
            })
            .await?;

        // Store in cache
        self.node_cache.insert(name.to_string(), node.clone()).await;
        Ok((node, CacheMetadata::miss()))
    }

    #[instrument(skip(self), fields(url = %self.base_url, node = %name))]
    async fn get_node_config(&self, name: &str) -> Result<(String, CacheMetadata), OxidizedError> {
        // Check cache first
        if let Some(cached) = self.config_cache.get(name).await {
            tracing::debug!(node = %name, "Cache hit for config");
            return Ok((cached, CacheMetadata::hit()));
        }

        // Cache miss - fetch with retry
        tracing::debug!(node = %name, "Cache miss for config, fetching from API");
        let endpoint = format!("/node/fetch/{}", name);
        let config = self
            .execute_with_retry(|| async {
                let response = self.build_request(&endpoint).send().await;
                self.handle_text_response(response, name).await
            })
            .await?;

        // Store in cache
        self.config_cache
            .insert(name.to_string(), config.clone())
            .await;
        Ok((config, CacheMetadata::miss()))
    }

    #[instrument(skip(self), fields(url = %self.base_url, node = %name))]
    async fn get_node_versions(&self, name: &str) -> Result<Vec<NodeVersion>, OxidizedError> {
        // Versions are not cached (historical data, rarely accessed repeatedly)
        let endpoint = format!("/node/version?node={}", name);
        self.execute_with_retry(|| async {
            let response = self.build_request(&endpoint).send().await;
            self.handle_json_response(response, name).await
        })
        .await
    }

    #[instrument(skip(self), fields(url = %self.base_url, node = %name, oid = %oid))]
    async fn get_node_version(&self, name: &str, oid: &str) -> Result<String, OxidizedError> {
        // Version content is not cached (point-in-time data)
        let endpoint = format!("/node/version?node={}&oid={}", name, oid);
        let context = format!("{}@{}", name, oid);
        self.execute_with_retry(|| async {
            let response = self.build_request(&endpoint).send().await;
            self.handle_text_response(response, &context).await
        })
        .await
    }

    #[instrument(skip(self), fields(url = %self.base_url))]
    async fn get_stats(&self) -> Result<(Stats, CacheMetadata), OxidizedError> {
        // Check cache first
        if let Some(cached) = self.stats_cache.get(&()).await {
            tracing::debug!("Cache hit for stats");
            return Ok((cached, CacheMetadata::hit()));
        }

        // Cache miss - fetch with retry
        tracing::debug!("Cache miss for stats, fetching from API");
        let stats: Stats = self
            .execute_with_retry(|| async {
                let response = self.build_request("/").send().await;
                self.handle_json_response(response, "stats").await
            })
            .await?;

        // Store in cache
        self.stats_cache.insert((), stats.clone()).await;
        Ok((stats, CacheMetadata::miss()))
    }

    #[instrument(skip(self), fields(url = %self.base_url, node = %node))]
    async fn trigger_backup(&self, node: &str) -> Result<(), OxidizedError> {
        // Write operation with retry
        let endpoint = format!("/node/next/{}", node);
        let result = self
            .execute_with_retry(|| async {
                let response = self.build_put_request(&endpoint).send().await;
                self.handle_empty_response(response, node).await
            })
            .await;

        // Invalidate cache ONLY on success (AC: 4, 5)
        if result.is_ok() {
            self.invalidate_node(node).await;
        }

        result
    }

    #[instrument(skip(self), fields(url = %self.base_url, node = %node))]
    async fn prioritize_node(&self, node: &str) -> Result<(), OxidizedError> {
        // Write operation with retry
        let endpoint = format!("/node/next/{}", node);
        let result = self
            .execute_with_retry(|| async {
                let response = self.build_put_request(&endpoint).send().await;
                self.handle_empty_response(response, node).await
            })
            .await;

        // Invalidate cache ONLY on success (AC: 4, 5)
        if result.is_ok() {
            self.invalidate_node(node).await;
        }

        result
    }

    #[instrument(skip(self), fields(url = %self.base_url))]
    async fn reload_sources(&self) -> Result<(), OxidizedError> {
        // Write operation with retry
        let result = self
            .execute_with_retry(|| async {
                let response = self.build_request("/reload?format=json").send().await;
                self.handle_empty_response(response, "reload").await
            })
            .await;

        // Invalidate ALL caches ONLY on success (AC: 4, 5)
        if result.is_ok() {
            self.invalidate_all_nodes().await;
        }

        result
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

    // -------------------------------------------------------------------------
    // Cache Constants Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_nodes_cache_ttl_is_5_minutes() {
        assert_eq!(
            NODES_CACHE_TTL_SECS, 300,
            "Nodes cache TTL should be 5 minutes (300 seconds)"
        );
    }

    #[test]
    fn test_config_cache_ttl_is_2_minutes() {
        assert_eq!(
            CONFIG_CACHE_TTL_SECS, 120,
            "Config cache TTL should be 2 minutes (120 seconds)"
        );
    }

    #[test]
    fn test_stats_cache_ttl_is_30_seconds() {
        assert_eq!(
            STATS_CACHE_TTL_SECS, 30,
            "Stats cache TTL should be 30 seconds"
        );
    }

    // -------------------------------------------------------------------------
    // Retry Constants Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_max_retry_attempts_is_3() {
        assert_eq!(
            MAX_RETRY_ATTEMPTS, 3,
            "Max retry attempts should be 3 (initial + 2 retries)"
        );
    }

    #[test]
    fn test_retry_delays_are_exponential() {
        assert_eq!(
            RETRY_DELAYS_MS,
            [200, 800],
            "Retry delays should be [200ms, 800ms]"
        );
        // Verify exponential progression (each delay is 4x previous)
        assert!(
            RETRY_DELAYS_MS[1] > RETRY_DELAYS_MS[0],
            "Second delay should be greater than first"
        );
    }

    // -------------------------------------------------------------------------
    // CacheMetadata Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_cache_metadata_hit() {
        let meta = CacheMetadata::hit();
        assert!(
            meta.cache_hit,
            "CacheMetadata::hit() should set cache_hit to true"
        );
    }

    #[test]
    fn test_cache_metadata_miss() {
        let meta = CacheMetadata::miss();
        assert!(
            !meta.cache_hit,
            "CacheMetadata::miss() should set cache_hit to false"
        );
    }

    #[test]
    fn test_cache_metadata_serializes_correctly() {
        let hit = CacheMetadata::hit();
        let json = serde_json::to_string(&hit).expect("Should serialize CacheMetadata");
        assert!(
            json.contains("\"cache_hit\":true"),
            "Should serialize cache_hit field"
        );

        let miss = CacheMetadata::miss();
        let json = serde_json::to_string(&miss).expect("Should serialize CacheMetadata");
        assert!(
            json.contains("\"cache_hit\":false"),
            "Should serialize cache_hit field"
        );
    }

    // -------------------------------------------------------------------------
    // Cached Response Wrapper Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_cached_nodes_serializes() {
        let nodes = vec![Node {
            name: "SW-01".to_string(),
            full_name: "SW-01.local".to_string(),
            ip: "10.0.0.1".to_string(),
            group: "switches".to_string(),
            model: "cisco".to_string(),
            status: "success".to_string(),
            last_status: "success".to_string(),
            time: None,
            mtime: None,
        }];
        let cached = CachedNodes {
            nodes,
            metadata: CacheMetadata::hit(),
        };
        let json = serde_json::to_string(&cached).expect("Should serialize CachedNodes");
        assert!(json.contains("\"nodes\""), "Should contain nodes field");
        assert!(
            json.contains("\"metadata\""),
            "Should contain metadata field"
        );
        assert!(
            json.contains("\"cache_hit\":true"),
            "Should indicate cache hit"
        );
    }

    #[test]
    fn test_cached_config_serializes() {
        let cached = CachedConfig {
            config: "hostname SW-01\n".to_string(),
            metadata: CacheMetadata::miss(),
        };
        let json = serde_json::to_string(&cached).expect("Should serialize CachedConfig");
        assert!(json.contains("\"config\""), "Should contain config field");
        assert!(
            json.contains("\"cache_hit\":false"),
            "Should indicate cache miss"
        );
    }

    #[test]
    fn test_cached_stats_serializes() {
        let stats = Stats {
            total_nodes: Some(100),
            success_count: Some(95),
            failure_count: Some(5),
            last_run: Some("2025-01-15".to_string()),
        };
        let cached = CachedStats {
            stats,
            metadata: CacheMetadata::hit(),
        };
        let json = serde_json::to_string(&cached).expect("Should serialize CachedStats");
        assert!(json.contains("\"stats\""), "Should contain stats field");
        assert!(
            json.contains("\"total_nodes\":100"),
            "Should contain stats data"
        );
    }

    // -------------------------------------------------------------------------
    // Retry Logic Tests (execute_with_retry behavior)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_retry_succeeds_on_first_attempt() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        // Simulate a successful operation on first attempt
        let result: Result<String, OxidizedError> = client
            .execute_with_retry(|| async { Ok("success".to_string()) })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_retry_non_transient_error_fails_immediately() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        use std::sync::atomic::{AtomicU8, Ordering};
        let attempt_count = std::sync::Arc::new(AtomicU8::new(0));
        let counter = attempt_count.clone();

        // Non-transient error should not retry
        let result: Result<String, OxidizedError> = client
            .execute_with_retry(|| {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err(OxidizedError::NodeNotFound("test".to_string(), vec![]))
                }
            })
            .await;

        assert!(result.is_err());
        // Should only attempt once (no retry for non-transient errors)
        assert_eq!(
            attempt_count.load(Ordering::SeqCst),
            1,
            "Non-transient error should not retry"
        );
    }

    #[tokio::test]
    async fn test_retry_transient_error_retries_up_to_max() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        use std::sync::atomic::{AtomicU8, Ordering};
        let attempt_count = std::sync::Arc::new(AtomicU8::new(0));
        let counter = attempt_count.clone();

        // Transient error should retry
        let result: Result<String, OxidizedError> = client
            .execute_with_retry(|| {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err(OxidizedError::HttpError {
                        status_code: 503,
                        context: "test".to_string(),
                    })
                }
            })
            .await;

        assert!(result.is_err());
        // Should attempt MAX_RETRY_ATTEMPTS times
        assert_eq!(
            attempt_count.load(Ordering::SeqCst),
            MAX_RETRY_ATTEMPTS,
            "Should retry up to max attempts for transient errors"
        );
    }

    #[tokio::test]
    async fn test_retry_succeeds_after_transient_failure() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        use std::sync::atomic::{AtomicU8, Ordering};
        let attempt_count = std::sync::Arc::new(AtomicU8::new(0));
        let counter = attempt_count.clone();

        // Fail first attempt, succeed on second
        let result: Result<String, OxidizedError> = client
            .execute_with_retry(|| {
                let counter = counter.clone();
                async move {
                    let attempt = counter.fetch_add(1, Ordering::SeqCst);
                    if attempt == 0 {
                        Err(OxidizedError::HttpError {
                            status_code: 500,
                            context: "test".to_string(),
                        })
                    } else {
                        Ok("success after retry".to_string())
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success after retry");
        assert_eq!(
            attempt_count.load(Ordering::SeqCst),
            2,
            "Should succeed on second attempt"
        );
    }

    // -------------------------------------------------------------------------
    // Cache Initialization Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_client_initializes_all_caches() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        // Verify caches are initialized (they should be empty initially)
        // We can't directly check TTL, but we can verify the caches exist
        // by checking they don't contain any entries
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            assert!(
                client.nodes_cache.get(&()).await.is_none(),
                "nodes_cache should be empty"
            );
            assert!(
                client.node_cache.get("test").await.is_none(),
                "node_cache should be empty"
            );
            assert!(
                client.config_cache.get("test").await.is_none(),
                "config_cache should be empty"
            );
            assert!(
                client.stats_cache.get(&()).await.is_none(),
                "stats_cache should be empty"
            );
        });
    }

    // -------------------------------------------------------------------------
    // Cache Invalidation Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_invalidate_node_clears_node_and_config_cache() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        // Pre-populate caches
        let node = Node {
            name: "test-node".to_string(),
            full_name: "test-node.local".to_string(),
            ip: "10.0.0.1".to_string(),
            group: "test".to_string(),
            model: "test".to_string(),
            status: "success".to_string(),
            last_status: "success".to_string(),
            time: None,
            mtime: None,
        };
        client
            .node_cache
            .insert("test-node".to_string(), node)
            .await;
        client
            .config_cache
            .insert("test-node".to_string(), "config data".to_string())
            .await;

        // Verify cache is populated
        assert!(client.node_cache.get("test-node").await.is_some());
        assert!(client.config_cache.get("test-node").await.is_some());

        // Invalidate node
        client.invalidate_node("test-node").await;

        // Verify cache is cleared for this node
        assert!(
            client.node_cache.get("test-node").await.is_none(),
            "node_cache should be invalidated"
        );
        assert!(
            client.config_cache.get("test-node").await.is_none(),
            "config_cache should be invalidated"
        );
    }

    #[tokio::test]
    async fn test_invalidate_node_does_not_affect_other_nodes() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        // Pre-populate caches for two nodes
        let node1 = Node {
            name: "node1".to_string(),
            full_name: "node1.local".to_string(),
            ip: "10.0.0.1".to_string(),
            group: "test".to_string(),
            model: "test".to_string(),
            status: "success".to_string(),
            last_status: "success".to_string(),
            time: None,
            mtime: None,
        };
        let node2 = Node {
            name: "node2".to_string(),
            full_name: "node2.local".to_string(),
            ip: "10.0.0.2".to_string(),
            group: "test".to_string(),
            model: "test".to_string(),
            status: "success".to_string(),
            last_status: "success".to_string(),
            time: None,
            mtime: None,
        };
        client.node_cache.insert("node1".to_string(), node1).await;
        client.node_cache.insert("node2".to_string(), node2).await;

        // Invalidate only node1
        client.invalidate_node("node1").await;

        // node1 should be invalidated, node2 should remain
        assert!(
            client.node_cache.get("node1").await.is_none(),
            "node1 should be invalidated"
        );
        assert!(
            client.node_cache.get("node2").await.is_some(),
            "node2 should remain cached"
        );
    }

    #[tokio::test]
    async fn test_invalidate_all_nodes_clears_all_caches() {
        let config = Config {
            oxidized_url: "http://localhost:8888".to_string(),
            oxidized_user: None,
            oxidized_password: None,
        };
        let client = OxidizedClient::new(&config);

        // Pre-populate all caches
        let node = Node {
            name: "test".to_string(),
            full_name: "test.local".to_string(),
            ip: "10.0.0.1".to_string(),
            group: "test".to_string(),
            model: "test".to_string(),
            status: "success".to_string(),
            last_status: "success".to_string(),
            time: None,
            mtime: None,
        };
        let stats = Stats {
            total_nodes: Some(10),
            success_count: Some(10),
            failure_count: Some(0),
            last_run: None,
        };
        client.nodes_cache.insert((), vec![node.clone()]).await;
        client.node_cache.insert("test".to_string(), node).await;
        client
            .config_cache
            .insert("test".to_string(), "config".to_string())
            .await;
        client.stats_cache.insert((), stats).await;

        // Verify all caches are populated
        assert!(client.nodes_cache.get(&()).await.is_some());
        assert!(client.node_cache.get("test").await.is_some());
        assert!(client.config_cache.get("test").await.is_some());
        assert!(client.stats_cache.get(&()).await.is_some());

        // Invalidate all
        client.invalidate_all_nodes().await;

        // Verify all caches are cleared
        assert!(
            client.nodes_cache.get(&()).await.is_none(),
            "nodes_cache should be invalidated"
        );
        assert!(
            client.node_cache.get("test").await.is_none(),
            "node_cache should be invalidated"
        );
        assert!(
            client.config_cache.get("test").await.is_none(),
            "config_cache should be invalidated"
        );
        assert!(
            client.stats_cache.get(&()).await.is_none(),
            "stats_cache should be invalidated"
        );
    }
}
