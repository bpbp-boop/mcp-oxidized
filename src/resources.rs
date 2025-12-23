//! MCP Resource handlers for Oxidized node discovery and configuration access.
//!
//! This module provides MCP Resources for discovering and viewing network nodes
//! in the Oxidized inventory, accessing configurations, and viewing version history.
//! Resources are read-only data endpoints that expose node information, statistics,
//! configurations, and version history.
//!
//! # Resources
//!
//! | URI | Description | FRs Covered |
//! |-----|-------------|-------------|
//! | `oxidized://nodes` | List all nodes (paginated if > 100) | FR1, FR2, FR31 |
//! | `oxidized://node/{name}` | Details of a specific node | FR3, FR4 |
//! | `oxidized://node/{name}/config` | Current configuration with size metadata | FR5, FR42 |
//! | `oxidized://node/{name}/versions` | Version history (sorted newest first) | FR6 |
//! | `oxidized://node/{name}/versions/{oid}` | Historical config at specific version | FR7, FR42 |
//! | `oxidized://stats` | Global statistics | FR33 |
//!
//! # Pagination
//!
//! Node listings are automatically paginated when the count exceeds 100 nodes.
//! Pagination parameters:
//! - `offset`: Starting index (default: 0)
//! - `limit`: Number of items per page (default: 100, max: 500)
//!
//! # Size Metadata (FR42)
//!
//! Configuration responses include size metadata for LLM context awareness:
//! - `bytes`: Configuration size in bytes
//! - `lines`: Number of lines
//! - `estimated_tokens`: Approximate token count (~4 chars per token)
//!
//! # Example
//!
//! ```ignore
//! use mcp_oxidized::resources::{list_nodes, get_node, get_stats, get_node_config};
//! use mcp_oxidized::oxidized::OxidizedClient;
//!
//! let client = OxidizedClient::new(&config);
//!
//! // List all nodes with pagination
//! let nodes = list_nodes(&client, None, None, None).await?;
//!
//! // Get a specific node
//! let node = get_node(&client, "SW-Core-01").await?;
//!
//! // Get node configuration with size metadata
//! let config = get_node_config(&client, "SW-Core-01").await?;
//! println!("Config size: {} bytes, ~{} tokens", config.size.bytes, config.size.estimated_tokens);
//!
//! // Get global statistics
//! let stats = get_stats(&client).await?;
//! ```

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tracing::instrument;

use crate::error::OxidizedError;
use crate::oxidized::{CacheMetadata, CachedStats, Node, NodeVersion, OxidizedBackend};

// ============================================================================
// Pagination Constants (FR31)
// ============================================================================

/// Default number of items per page.
pub const DEFAULT_PAGE_SIZE: usize = 100;

/// Maximum allowed items per page.
pub const MAX_PAGE_SIZE: usize = 500;

/// Maximum number of node name suggestions for NodeNotFound errors.
pub const MAX_SUGGESTIONS: usize = 5;

// ============================================================================
// Pagination Types
// ============================================================================

/// Parameters for pagination.
#[derive(Debug, Clone, Default)]
pub struct PaginationParams {
    /// Starting index (0-based).
    pub offset: usize,
    /// Maximum number of items to return.
    pub limit: usize,
}

impl PaginationParams {
    /// Create new pagination parameters with defaults.
    ///
    /// # Arguments
    ///
    /// * `offset` - Starting index (defaults to 0 if None)
    /// * `limit` - Maximum items per page (defaults to 100, capped at 500)
    ///
    /// # Example
    ///
    /// ```
    /// use mcp_oxidized::resources::PaginationParams;
    ///
    /// // Default values
    /// let params = PaginationParams::new(None, None);
    /// assert_eq!(params.offset, 0);
    /// assert_eq!(params.limit, 100);
    ///
    /// // Custom values (limit capped at MAX_PAGE_SIZE)
    /// let params = PaginationParams::new(Some(10), Some(1000));
    /// assert_eq!(params.offset, 10);
    /// assert_eq!(params.limit, 500); // Capped
    /// ```
    pub fn new(offset: Option<usize>, limit: Option<usize>) -> Self {
        Self {
            offset: offset.unwrap_or(0),
            limit: limit.unwrap_or(DEFAULT_PAGE_SIZE).min(MAX_PAGE_SIZE),
        }
    }
}

/// Paginated response wrapper with cache metadata (FR31, FR32).
#[derive(Debug, Clone, Serialize)]
pub struct PaginatedResponse<T> {
    /// The items for the current page.
    pub items: Vec<T>,
    /// Total number of items across all pages.
    pub total: usize,
    /// Current page offset.
    pub offset: usize,
    /// Maximum items per page.
    pub limit: usize,
    /// Whether more items exist beyond this page.
    pub has_more: bool,
    /// Cache metadata indicating hit/miss status.
    pub metadata: CacheMetadata,
}

/// Response wrapper for a single node with cache metadata.
#[derive(Debug, Clone, Serialize)]
pub struct NodeResponse {
    /// The node data.
    pub node: Node,
    /// Cache metadata indicating hit/miss status.
    pub metadata: CacheMetadata,
}

// ============================================================================
// Configuration Metadata Types (FR42)
// ============================================================================

/// Metadata about configuration size for LLM context awareness (FR42).
///
/// Provides size information to help LLMs understand the scale of
/// configuration data before processing it.
///
/// # Example
///
/// ```
/// use mcp_oxidized::resources::ConfigMetadata;
///
/// let config = "hostname router1\ninterface eth0\n  ip address 10.0.0.1/24";
/// let meta = ConfigMetadata::from_config(config);
///
/// assert_eq!(meta.lines, 3);
/// assert!(meta.bytes > 0);
/// assert_eq!(meta.estimated_tokens, meta.bytes / 4);
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct ConfigMetadata {
    /// Configuration size in bytes.
    pub bytes: usize,
    /// Number of lines in configuration.
    pub lines: usize,
    /// Estimated token count (~4 chars per token).
    pub estimated_tokens: usize,
}

impl ConfigMetadata {
    /// Calculate metadata for a configuration string.
    ///
    /// Token estimation uses a rough approximation of ~4 characters per token,
    /// which is typical for network device configurations.
    pub fn from_config(config: &str) -> Self {
        let bytes = config.len();
        let lines = config.lines().count();
        // Token estimation: ~4 characters per token (rough approximation)
        let estimated_tokens = bytes / 4;

        Self {
            bytes,
            lines,
            estimated_tokens,
        }
    }
}

/// Response wrapper for configuration with size and cache metadata (FR5, FR42).
#[derive(Debug, Clone, Serialize)]
pub struct ConfigResponse {
    /// The configuration text.
    pub config: String,
    /// Size metadata for LLM context awareness.
    pub size: ConfigMetadata,
    /// Cache status metadata.
    pub metadata: CacheMetadata,
}

/// Response wrapper for version list (FR6).
///
/// Note: The `metadata` field is included for API consistency with other responses,
/// but always returns `cache_hit: false` because version lists are not cached
/// (architectural decision: historical data rarely accessed repeatedly).
#[derive(Debug, Clone, Serialize)]
pub struct VersionsResponse {
    /// List of versions, sorted newest first.
    pub versions: Vec<NodeVersion>,
    /// Total number of versions.
    pub total: usize,
    /// Cache status metadata (always miss - versions not cached).
    pub metadata: CacheMetadata,
}

/// Response wrapper for historical version configuration (FR7, FR42).
#[derive(Debug, Clone, Serialize)]
pub struct VersionConfigResponse {
    /// The configuration text at this version.
    pub config: String,
    /// The version OID (Git commit hash).
    pub oid: String,
    /// Size metadata for LLM context awareness.
    pub size: ConfigMetadata,
}

// ============================================================================
// Request ID Generation (FR34)
// ============================================================================

/// Generate a unique request ID for log correlation (FR34).
///
/// Uses nanosecond timestamp to generate a hex-encoded request ID.
/// Format: `req-{hex_timestamp}`
///
/// This function is called by MCP server handlers (`list_resources`, `read_resource`)
/// to create a root span with a unique ID. Child spans (from `resources::*` functions)
/// automatically inherit this ID via tracing's hierarchical context.
///
/// # Example
///
/// ```ignore
/// let request_id = generate_request_id();
/// // Returns something like "req-1a2b3c4d5e6f"
/// ```
pub fn generate_request_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("req-{:x}", timestamp)
}

// ============================================================================
// Pagination Helper
// ============================================================================

/// Paginate a vector of items.
///
/// # Arguments
///
/// * `items` - The full list of items to paginate
/// * `offset` - Starting index (0-based)
/// * `limit` - Maximum items to return
/// * `metadata` - Cache metadata to include in response
///
/// # Returns
///
/// A `PaginatedResponse` containing the requested page of items.
pub fn paginate<T>(
    items: Vec<T>,
    offset: usize,
    limit: usize,
    metadata: CacheMetadata,
) -> PaginatedResponse<T> {
    let total = items.len();
    let limit = limit.min(MAX_PAGE_SIZE);

    let page_items: Vec<T> = items.into_iter().skip(offset).take(limit).collect();

    let has_more = offset + page_items.len() < total;

    PaginatedResponse {
        items: page_items,
        total,
        offset,
        limit,
        has_more,
        metadata,
    }
}

// ============================================================================
// Fuzzy Matching for Node Suggestions (FR20)
// ============================================================================

/// Find nodes similar to the target name for error suggestions.
///
/// Returns up to `max_suggestions` node names, sorted by relevance.
/// Matching priority:
/// 1. Exact prefix match (case-insensitive)
/// 2. Contains target substring
/// 3. Partial prefix match (first 3 characters)
///
/// # Arguments
///
/// * `nodes` - List of nodes to search
/// * `target` - The target node name to find similar matches for
/// * `max_suggestions` - Maximum number of suggestions to return
///
/// # Returns
///
/// A vector of similar node names, sorted by relevance.
pub fn find_similar_nodes(nodes: &[Node], target: &str, max_suggestions: usize) -> Vec<String> {
    let target_lower = target.to_lowercase();

    let mut candidates: Vec<(usize, &str)> = nodes
        .iter()
        .filter_map(|n| {
            let name_lower = n.name.to_lowercase();

            // Priority scoring: lower is better
            let score = if name_lower.starts_with(&target_lower) {
                0 // Exact prefix match - highest priority
            } else if name_lower.contains(&target_lower) {
                1 // Contains target
            } else if target_lower.len() >= 3 && name_lower.contains(&target_lower[..3]) {
                2 // Partial prefix match (first 3 chars)
            } else {
                return None; // No match
            };

            Some((score, n.name.as_str()))
        })
        .collect();

    // Sort by score first, then by name length (shorter names first)
    candidates.sort_by_key(|(score, name)| (*score, name.len()));

    candidates
        .into_iter()
        .take(max_suggestions)
        .map(|(_, name)| name.to_string())
        .collect()
}

// ============================================================================
// Resource Handlers
// ============================================================================

/// List all nodes with optional filtering and pagination (FR1, FR2, FR31).
///
/// Retrieves nodes from the Oxidized inventory, optionally filtered by group,
/// and paginates the results.
///
/// # Arguments
///
/// * `backend` - The Oxidized backend to fetch nodes from
/// * `offset` - Starting index for pagination (default: 0)
/// * `limit` - Maximum items per page (default: 100, max: 500)
/// * `group` - Optional group name to filter by
///
/// # Returns
///
/// A paginated response containing nodes and cache metadata.
///
/// # Errors
///
/// - [`OxidizedError::ApiUnreachable`] - Network/connection error
/// - [`OxidizedError::AuthFailed`] - Authentication failure
#[instrument(skip(backend), fields(group = ?group, offset = ?offset, limit = ?limit))]
pub async fn list_nodes<B: OxidizedBackend>(
    backend: &B,
    offset: Option<usize>,
    limit: Option<usize>,
    group: Option<&str>,
) -> Result<PaginatedResponse<Node>, OxidizedError> {
    let (nodes, cache_meta) = backend.get_nodes().await?;

    // Apply group filter (FR2)
    let filtered: Vec<Node> = match group {
        Some(g) => nodes.into_iter().filter(|n| n.group == g).collect(),
        None => nodes,
    };

    // Apply pagination (FR31)
    let params = PaginationParams::new(offset, limit);

    Ok(paginate(filtered, params.offset, params.limit, cache_meta))
}

/// Get a specific node by name (FR3, FR4).
///
/// Retrieves detailed information about a node. If the node is not found,
/// returns an error with suggestions of similar node names.
///
/// # Arguments
///
/// * `backend` - The Oxidized backend to fetch the node from
/// * `name` - The node name to look up
///
/// # Returns
///
/// The node details with cache metadata.
///
/// # Errors
///
/// - [`OxidizedError::NodeNotFound`] - Node does not exist (includes suggestions)
/// - [`OxidizedError::ApiUnreachable`] - Network/connection error
#[instrument(skip(backend), fields(node = %name))]
pub async fn get_node<B: OxidizedBackend>(
    backend: &B,
    name: &str,
) -> Result<NodeResponse, OxidizedError> {
    match backend.get_node(name).await {
        Ok((node, meta)) => Ok(NodeResponse {
            node,
            metadata: meta,
        }),
        Err(OxidizedError::NodeNotFound(node_name, _)) => {
            // Fetch nodes to generate suggestions (FR20)
            let suggestions = match backend.get_nodes().await {
                Ok((nodes, _)) => find_similar_nodes(&nodes, &node_name, MAX_SUGGESTIONS),
                Err(_) => vec![], // If we can't fetch nodes, provide empty suggestions
            };
            Err(OxidizedError::NodeNotFound(node_name, suggestions))
        }
        Err(e) => Err(e),
    }
}

/// Get global Oxidized statistics (FR4).
///
/// Retrieves server-wide backup statistics including total nodes,
/// success rate, and last run time.
///
/// # Arguments
///
/// * `backend` - The Oxidized backend to fetch stats from
///
/// # Returns
///
/// Statistics with cache metadata.
///
/// # Errors
///
/// - [`OxidizedError::ApiUnreachable`] - Network/connection error
#[instrument(skip(backend))]
pub async fn get_stats<B: OxidizedBackend>(backend: &B) -> Result<CachedStats, OxidizedError> {
    let (stats, cache_meta) = backend.get_stats().await?;
    Ok(CachedStats {
        stats,
        metadata: cache_meta,
    })
}

/// Get current configuration for a node (FR5, FR42).
///
/// Retrieves the latest configuration text for a node with size metadata
/// for LLM context awareness.
///
/// # Arguments
///
/// * `backend` - The Oxidized backend to fetch config from
/// * `name` - The node name to get configuration for
///
/// # Returns
///
/// Configuration text with size and cache metadata.
///
/// # Errors
///
/// - [`OxidizedError::NodeNotFound`] - Node does not exist (includes suggestions)
/// - [`OxidizedError::ApiUnreachable`] - Network/connection error
#[instrument(skip(backend), fields(node = %name))]
pub async fn get_node_config<B: OxidizedBackend>(
    backend: &B,
    name: &str,
) -> Result<ConfigResponse, OxidizedError> {
    match backend.get_node_config(name).await {
        Ok((config, cache_meta)) => {
            let size = ConfigMetadata::from_config(&config);
            Ok(ConfigResponse {
                config,
                size,
                metadata: cache_meta,
            })
        }
        Err(OxidizedError::NodeNotFound(node_name, _)) => {
            // Fetch nodes to generate suggestions (reuse pattern from get_node)
            let suggestions = match backend.get_nodes().await {
                Ok((nodes, _)) => find_similar_nodes(&nodes, &node_name, MAX_SUGGESTIONS),
                Err(_) => vec![],
            };
            Err(OxidizedError::NodeNotFound(node_name, suggestions))
        }
        Err(e) => Err(e),
    }
}

/// Get version history for a node (FR6).
///
/// Retrieves a list of configuration versions, sorted by date descending
/// (newest first).
///
/// # Arguments
///
/// * `backend` - The Oxidized backend to fetch versions from
/// * `name` - The node name to get versions for
///
/// # Returns
///
/// List of versions with total count and cache metadata.
///
/// # Errors
///
/// - [`OxidizedError::NodeNotFound`] - Node does not exist (includes suggestions)
/// - [`OxidizedError::ApiUnreachable`] - Network/connection error
#[instrument(skip(backend), fields(node = %name))]
pub async fn get_node_versions<B: OxidizedBackend>(
    backend: &B,
    name: &str,
) -> Result<VersionsResponse, OxidizedError> {
    match backend.get_node_versions(name).await {
        Ok(mut versions) => {
            // Sort by date descending (newest first)
            versions.sort_by(|a, b| b.date.cmp(&a.date));

            let total = versions.len();

            Ok(VersionsResponse {
                versions,
                total,
                // Versions are not cached (historical data)
                metadata: CacheMetadata::miss(),
            })
        }
        Err(OxidizedError::NodeNotFound(node_name, _)) => {
            // Fetch nodes to generate suggestions
            let suggestions = match backend.get_nodes().await {
                Ok((nodes, _)) => find_similar_nodes(&nodes, &node_name, MAX_SUGGESTIONS),
                Err(_) => vec![],
            };
            Err(OxidizedError::NodeNotFound(node_name, suggestions))
        }
        Err(e) => Err(e),
    }
}

/// Get configuration at a specific version (FR7, FR42).
///
/// Retrieves the configuration text at a specific point in time,
/// identified by the Git commit OID.
///
/// # Arguments
///
/// * `backend` - The Oxidized backend to fetch version config from
/// * `name` - The node name
/// * `oid` - The Git object ID (commit hash) of the version
///
/// # Returns
///
/// Configuration text with OID and size metadata.
///
/// # Errors
///
/// - [`OxidizedError::NodeNotFound`] - Node or version does not exist (includes suggestions)
/// - [`OxidizedError::ApiUnreachable`] - Network/connection error
#[instrument(skip(backend), fields(node = %name, oid = %oid))]
pub async fn get_node_version<B: OxidizedBackend>(
    backend: &B,
    name: &str,
    oid: &str,
) -> Result<VersionConfigResponse, OxidizedError> {
    match backend.get_node_version(name, oid).await {
        Ok(config) => {
            let size = ConfigMetadata::from_config(&config);
            Ok(VersionConfigResponse {
                config,
                oid: oid.to_string(),
                size,
            })
        }
        Err(OxidizedError::NodeNotFound(node_name, _)) => {
            // Fetch nodes to generate suggestions (reuse pattern from other handlers)
            let suggestions = match backend.get_nodes().await {
                Ok((nodes, _)) => find_similar_nodes(&nodes, &node_name, MAX_SUGGESTIONS),
                Err(_) => vec![],
            };
            Err(OxidizedError::NodeNotFound(node_name, suggestions))
        }
        Err(e) => Err(e),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Test Helper: Create test nodes
    // -------------------------------------------------------------------------

    fn create_test_node(name: &str, group: &str) -> Node {
        Node {
            name: name.to_string(),
            full_name: format!("{}.network.local", name),
            ip: "10.0.0.1".to_string(),
            group: group.to_string(),
            model: "cisco-ios".to_string(),
            status: "success".to_string(),
            last_status: "success".to_string(),
            time: Some("2025-01-15 10:30:00 UTC".to_string()),
            mtime: Some("2025-01-15 10:25:00 UTC".to_string()),
        }
    }

    // -------------------------------------------------------------------------
    // Pagination Constants Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_default_page_size_is_100() {
        assert_eq!(DEFAULT_PAGE_SIZE, 100);
    }

    #[test]
    fn test_max_page_size_is_500() {
        assert_eq!(MAX_PAGE_SIZE, 500);
    }

    #[test]
    fn test_max_suggestions_is_5() {
        assert_eq!(MAX_SUGGESTIONS, 5);
    }

    // -------------------------------------------------------------------------
    // PaginationParams Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_pagination_params_defaults() {
        let params = PaginationParams::new(None, None);
        assert_eq!(params.offset, 0);
        assert_eq!(params.limit, DEFAULT_PAGE_SIZE);
    }

    #[test]
    fn test_pagination_params_with_values() {
        let params = PaginationParams::new(Some(10), Some(50));
        assert_eq!(params.offset, 10);
        assert_eq!(params.limit, 50);
    }

    #[test]
    fn test_pagination_params_limit_capped_at_max() {
        let params = PaginationParams::new(Some(0), Some(1000));
        assert_eq!(params.limit, MAX_PAGE_SIZE);
    }

    // -------------------------------------------------------------------------
    // Paginate Function Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_paginate_empty_list() {
        let items: Vec<i32> = vec![];
        let result = paginate(items, 0, 10, CacheMetadata::miss());

        assert_eq!(result.items.len(), 0);
        assert_eq!(result.total, 0);
        assert_eq!(result.offset, 0);
        assert_eq!(result.limit, 10);
        assert!(!result.has_more);
    }

    #[test]
    fn test_paginate_first_page() {
        let items: Vec<i32> = (0..50).collect();
        let result = paginate(items, 0, 10, CacheMetadata::miss());

        assert_eq!(result.items, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(result.total, 50);
        assert_eq!(result.offset, 0);
        assert_eq!(result.limit, 10);
        assert!(result.has_more);
    }

    #[test]
    fn test_paginate_middle_page() {
        let items: Vec<i32> = (0..50).collect();
        let result = paginate(items, 5, 10, CacheMetadata::miss());

        assert_eq!(result.items, vec![5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);
        assert_eq!(result.total, 50);
        assert_eq!(result.offset, 5);
        assert!(result.has_more);
    }

    #[test]
    fn test_paginate_last_page() {
        let items: Vec<i32> = (0..25).collect();
        let result = paginate(items, 20, 10, CacheMetadata::miss());

        assert_eq!(result.items, vec![20, 21, 22, 23, 24]);
        assert_eq!(result.total, 25);
        assert!(!result.has_more);
    }

    #[test]
    fn test_paginate_beyond_end() {
        let items: Vec<i32> = (0..5).collect();
        let result = paginate(items, 10, 10, CacheMetadata::miss());

        assert_eq!(result.items.len(), 0);
        assert_eq!(result.total, 5);
        assert!(!result.has_more);
    }

    #[test]
    fn test_paginate_exact_fit() {
        let items: Vec<i32> = (0..10).collect();
        let result = paginate(items, 0, 10, CacheMetadata::miss());

        assert_eq!(result.items.len(), 10);
        assert_eq!(result.total, 10);
        assert!(!result.has_more);
    }

    #[test]
    fn test_paginate_limit_exceeds_max() {
        let items: Vec<i32> = (0..1000).collect();
        let result = paginate(items, 0, 1000, CacheMetadata::miss());

        // Should cap at MAX_PAGE_SIZE
        assert_eq!(result.items.len(), MAX_PAGE_SIZE);
        assert_eq!(result.limit, MAX_PAGE_SIZE);
        assert!(result.has_more);
    }

    #[test]
    fn test_paginate_preserves_cache_metadata() {
        let items: Vec<i32> = vec![1, 2, 3];

        let result_hit = paginate(items.clone(), 0, 10, CacheMetadata::hit());
        assert!(result_hit.metadata.cache_hit);

        let result_miss = paginate(items, 0, 10, CacheMetadata::miss());
        assert!(!result_miss.metadata.cache_hit);
    }

    // -------------------------------------------------------------------------
    // Group Filtering Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_filter_by_group_matches() {
        let nodes = vec![
            create_test_node("SW-01", "switches"),
            create_test_node("RTR-01", "routers"),
            create_test_node("SW-02", "switches"),
        ];

        let filtered: Vec<_> = nodes
            .into_iter()
            .filter(|n| n.group == "switches")
            .collect();

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|n| n.group == "switches"));
    }

    #[test]
    fn test_filter_by_group_no_matches() {
        let nodes = vec![
            create_test_node("SW-01", "switches"),
            create_test_node("RTR-01", "routers"),
        ];

        let filtered: Vec<_> = nodes
            .into_iter()
            .filter(|n| n.group == "firewalls")
            .collect();

        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_by_group_is_case_sensitive() {
        let nodes = vec![
            create_test_node("SW-01", "Switches"),
            create_test_node("RTR-01", "switches"),
        ];

        let filtered: Vec<_> = nodes
            .into_iter()
            .filter(|n| n.group == "switches")
            .collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "RTR-01");
    }

    #[test]
    fn test_filter_combined_with_pagination() {
        let nodes: Vec<Node> = (0..20)
            .map(|i| {
                let group = if i % 2 == 0 { "even" } else { "odd" };
                create_test_node(&format!("Node-{:02}", i), group)
            })
            .collect();

        let filtered: Vec<_> = nodes.into_iter().filter(|n| n.group == "even").collect();

        let result = paginate(filtered, 0, 5, CacheMetadata::miss());

        assert_eq!(result.items.len(), 5);
        assert_eq!(result.total, 10); // 10 even nodes
        assert!(result.has_more);
    }

    // -------------------------------------------------------------------------
    // Fuzzy Matching Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_find_similar_nodes_prefix_match() {
        let nodes = vec![
            create_test_node("SW-Core-01", "switches"),
            create_test_node("SW-Core-02", "switches"),
            create_test_node("RTR-Edge-01", "routers"),
        ];

        let suggestions = find_similar_nodes(&nodes, "SW-Core", 5);

        assert_eq!(suggestions.len(), 2);
        assert!(suggestions.contains(&"SW-Core-01".to_string()));
        assert!(suggestions.contains(&"SW-Core-02".to_string()));
    }

    #[test]
    fn test_find_similar_nodes_substring_match() {
        let nodes = vec![
            create_test_node("DC1-SW-Core-01", "switches"),
            create_test_node("DC2-SW-Core-02", "switches"),
            create_test_node("RTR-Edge-01", "routers"),
        ];

        let suggestions = find_similar_nodes(&nodes, "Core", 5);

        assert_eq!(suggestions.len(), 2);
        assert!(suggestions.iter().all(|s| s.contains("Core")));
    }

    #[test]
    fn test_find_similar_nodes_case_insensitive() {
        let nodes = vec![
            create_test_node("SW-CORE-01", "switches"),
            create_test_node("sw-core-02", "switches"),
        ];

        let suggestions = find_similar_nodes(&nodes, "sw-core", 5);

        assert_eq!(suggestions.len(), 2);
    }

    #[test]
    fn test_find_similar_nodes_max_limit() {
        let nodes: Vec<Node> = (0..10)
            .map(|i| create_test_node(&format!("SW-{:02}", i), "switches"))
            .collect();

        let suggestions = find_similar_nodes(&nodes, "SW", 5);

        assert_eq!(suggestions.len(), 5);
    }

    #[test]
    fn test_find_similar_nodes_empty_list() {
        let nodes: Vec<Node> = vec![];

        let suggestions = find_similar_nodes(&nodes, "anything", 5);

        assert_eq!(suggestions.len(), 0);
    }

    #[test]
    fn test_find_similar_nodes_no_matches() {
        let nodes = vec![
            create_test_node("SW-Core-01", "switches"),
            create_test_node("RTR-Edge-01", "routers"),
        ];

        let suggestions = find_similar_nodes(&nodes, "xyz-unknown", 5);

        assert_eq!(suggestions.len(), 0);
    }

    #[test]
    fn test_find_similar_nodes_partial_prefix_match() {
        let nodes = vec![
            create_test_node("SW-Core-01", "switches"),
            create_test_node("SWitch-02", "switches"),
        ];

        // "SWI" should match both via partial prefix
        let suggestions = find_similar_nodes(&nodes, "SWI", 5);

        assert!(suggestions.len() >= 1);
    }

    #[test]
    fn test_find_similar_nodes_priority_ordering() {
        let nodes = vec![
            create_test_node("DC-SW-Core", "switches"), // contains "SW"
            create_test_node("SW-Core-01", "switches"), // starts with "SW"
            create_test_node("SW-Access-01", "switches"), // starts with "SW"
        ];

        let suggestions = find_similar_nodes(&nodes, "SW", 5);

        // Prefix matches should come first
        assert!(suggestions[0].starts_with("SW") || suggestions[1].starts_with("SW"));
    }

    #[test]
    fn test_find_similar_nodes_short_target() {
        let nodes = vec![
            create_test_node("A1", "group"),
            create_test_node("A2", "group"),
            create_test_node("B1", "group"),
        ];

        // Short target (< 3 chars) should still work for prefix/contains
        let suggestions = find_similar_nodes(&nodes, "A", 5);

        assert_eq!(suggestions.len(), 2);
    }

    // -------------------------------------------------------------------------
    // Request ID Generation Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_generate_request_id_format() {
        let request_id = generate_request_id();

        assert!(request_id.starts_with("req-"));
        assert!(request_id.len() > 4);
    }

    #[test]
    fn test_generate_request_id_unique() {
        let id1 = generate_request_id();
        std::thread::sleep(std::time::Duration::from_nanos(1));
        let id2 = generate_request_id();

        assert_ne!(id1, id2);
    }

    // -------------------------------------------------------------------------
    // NodeResponse Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_node_response_serializes() {
        let response = NodeResponse {
            node: create_test_node("SW-01", "switches"),
            metadata: CacheMetadata::hit(),
        };

        let json = serde_json::to_string(&response).expect("Should serialize NodeResponse");
        assert!(json.contains("\"node\""));
        assert!(json.contains("\"metadata\""));
        assert!(json.contains("\"cache_hit\":true"));
    }

    // -------------------------------------------------------------------------
    // PaginatedResponse Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_paginated_response_serializes() {
        let response = PaginatedResponse {
            items: vec![create_test_node("SW-01", "switches")],
            total: 1,
            offset: 0,
            limit: 100,
            has_more: false,
            metadata: CacheMetadata::miss(),
        };

        let json = serde_json::to_string(&response).expect("Should serialize PaginatedResponse");
        assert!(json.contains("\"items\""));
        assert!(json.contains("\"total\":1"));
        assert!(json.contains("\"offset\":0"));
        assert!(json.contains("\"limit\":100"));
        assert!(json.contains("\"has_more\":false"));
        assert!(json.contains("\"cache_hit\":false"));
    }

    // -------------------------------------------------------------------------
    // ConfigMetadata Tests (FR42)
    // -------------------------------------------------------------------------

    #[test]
    fn test_config_metadata_empty() {
        let meta = ConfigMetadata::from_config("");

        assert_eq!(meta.bytes, 0);
        assert_eq!(meta.lines, 0);
        assert_eq!(meta.estimated_tokens, 0);
    }

    #[test]
    fn test_config_metadata_single_line() {
        let meta = ConfigMetadata::from_config("hostname router1");

        assert_eq!(meta.bytes, 16);
        assert_eq!(meta.lines, 1);
        assert_eq!(meta.estimated_tokens, 4); // 16/4
    }

    #[test]
    fn test_config_metadata_multiline() {
        let config = "hostname router1\ninterface eth0\n  ip address 10.0.0.1/24";
        let meta = ConfigMetadata::from_config(config);

        assert_eq!(meta.lines, 3);
        assert!(meta.bytes > 0);
        assert_eq!(meta.estimated_tokens, meta.bytes / 4);
    }

    #[test]
    fn test_config_metadata_token_estimation() {
        // Test edge cases: 0-3 chars all result in 0 tokens (bytes/4 rounds down)
        assert_eq!(ConfigMetadata::from_config("").estimated_tokens, 0);
        assert_eq!(ConfigMetadata::from_config("a").estimated_tokens, 0);
        assert_eq!(ConfigMetadata::from_config("ab").estimated_tokens, 0);
        assert_eq!(ConfigMetadata::from_config("abc").estimated_tokens, 0);

        // Test exact boundaries
        let config_4 = "a".repeat(4);
        assert_eq!(ConfigMetadata::from_config(&config_4).estimated_tokens, 1);

        let config_100 = "a".repeat(100);
        assert_eq!(
            ConfigMetadata::from_config(&config_100).estimated_tokens,
            25
        );

        let config_1000 = "a".repeat(1000);
        assert_eq!(
            ConfigMetadata::from_config(&config_1000).estimated_tokens,
            250
        );
    }

    #[test]
    fn test_config_metadata_serializes() {
        let meta = ConfigMetadata::from_config("test config\nline 2");
        let json = serde_json::to_string(&meta).expect("Should serialize ConfigMetadata");

        assert!(json.contains("\"bytes\":"));
        assert!(json.contains("\"lines\":"));
        assert!(json.contains("\"estimated_tokens\":"));
    }

    #[test]
    fn test_config_metadata_realistic_config() {
        // Simulate a realistic network config
        let config = r#"!
hostname SW-Core-01
!
interface GigabitEthernet0/1
  description Uplink to Router
  ip address 192.168.1.1 255.255.255.0
  no shutdown
!
interface GigabitEthernet0/2
  description Server VLAN
  switchport mode access
  switchport access vlan 100
!
vlan 100
  name Servers
!
end
"#;
        let meta = ConfigMetadata::from_config(config);

        assert!(meta.bytes > 200, "Should have significant byte count");
        assert!(meta.lines > 10, "Should have multiple lines");
        assert!(
            meta.estimated_tokens > 50,
            "Should have significant token count"
        );
    }

    // -------------------------------------------------------------------------
    // ConfigResponse Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_config_response_serializes() {
        let response = ConfigResponse {
            config: "hostname router1\n".to_string(),
            size: ConfigMetadata::from_config("hostname router1\n"),
            metadata: CacheMetadata::hit(),
        };

        let json = serde_json::to_string(&response).expect("Should serialize ConfigResponse");
        assert!(json.contains("\"config\":"));
        assert!(json.contains("\"size\":"));
        assert!(json.contains("\"metadata\":"));
        assert!(json.contains("\"cache_hit\":true"));
    }

    // -------------------------------------------------------------------------
    // VersionsResponse Tests
    // -------------------------------------------------------------------------

    fn create_test_version(oid: &str, date: &str) -> NodeVersion {
        NodeVersion {
            oid: oid.to_string(),
            date: date.to_string(),
            author: "oxidized".to_string(),
            message: format!("update node {}", oid),
        }
    }

    #[test]
    fn test_versions_sorted_newest_first() {
        let mut versions = vec![
            create_test_version("old", "2025-01-01 00:00:00 UTC"),
            create_test_version("new", "2025-01-15 00:00:00 UTC"),
            create_test_version("mid", "2025-01-10 00:00:00 UTC"),
        ];

        // Sort by date descending (newest first)
        versions.sort_by(|a, b| b.date.cmp(&a.date));

        assert_eq!(versions[0].oid, "new");
        assert_eq!(versions[1].oid, "mid");
        assert_eq!(versions[2].oid, "old");
    }

    #[test]
    fn test_versions_empty_list() {
        let versions: Vec<NodeVersion> = vec![];

        let response = VersionsResponse {
            versions: versions.clone(),
            total: versions.len(),
            metadata: CacheMetadata::miss(),
        };

        assert_eq!(response.total, 0);
        assert!(response.versions.is_empty());
    }

    #[test]
    fn test_versions_single_version() {
        let mut versions = vec![create_test_version("only", "2025-01-15 00:00:00 UTC")];

        versions.sort_by(|a, b| b.date.cmp(&a.date));

        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].oid, "only");
    }

    #[test]
    fn test_versions_same_timestamp_stable_sort() {
        let mut versions = vec![
            create_test_version("first", "2025-01-15 00:00:00 UTC"),
            create_test_version("second", "2025-01-15 00:00:00 UTC"),
            create_test_version("third", "2025-01-15 00:00:00 UTC"),
        ];

        // Stable sort should preserve order for equal elements
        versions.sort_by(|a, b| b.date.cmp(&a.date));

        // All have same date, so original order is preserved
        assert_eq!(versions.len(), 3);
    }

    #[test]
    fn test_versions_response_serializes() {
        let response = VersionsResponse {
            versions: vec![
                create_test_version("abc123", "2025-01-15 10:30:00 UTC"),
                create_test_version("def456", "2025-01-14 09:00:00 UTC"),
            ],
            total: 2,
            metadata: CacheMetadata::miss(),
        };

        let json = serde_json::to_string(&response).expect("Should serialize VersionsResponse");
        assert!(json.contains("\"versions\":"));
        assert!(json.contains("\"total\":2"));
        assert!(json.contains("\"abc123\""));
        assert!(json.contains("\"def456\""));
    }

    // -------------------------------------------------------------------------
    // VersionConfigResponse Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_version_config_response_serializes() {
        let response = VersionConfigResponse {
            config: "hostname router1\n".to_string(),
            oid: "abc123def456".to_string(),
            size: ConfigMetadata::from_config("hostname router1\n"),
        };

        let json =
            serde_json::to_string(&response).expect("Should serialize VersionConfigResponse");
        assert!(json.contains("\"config\":"));
        assert!(json.contains("\"oid\":\"abc123def456\""));
        assert!(json.contains("\"size\":"));
        assert!(json.contains("\"bytes\":"));
        assert!(json.contains("\"lines\":"));
        assert!(json.contains("\"estimated_tokens\":"));
    }

    #[test]
    fn test_version_config_response_no_cache_metadata() {
        let response = VersionConfigResponse {
            config: "config data".to_string(),
            oid: "abc123".to_string(),
            size: ConfigMetadata::from_config("config data"),
        };

        let json = serde_json::to_string(&response).expect("Should serialize");
        // VersionConfigResponse does NOT include cache metadata (historical data)
        assert!(!json.contains("cache_hit"));
    }
}
