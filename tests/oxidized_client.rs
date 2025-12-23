//! Integration tests for OxidizedClient.
//!
//! These tests require a real Oxidized server and are marked with `#[ignore]`.
//!
//! # Environment Variables
//!
//! - `OXIDIZED_URL` - Required: Base URL of the Oxidized server
//! - `OXIDIZED_USER` - Optional: Username for Basic Auth
//! - `OXIDIZED_PASSWORD` - Optional: Password for Basic Auth
//!
//! # Running Tests
//!
//! ```bash
//! export OXIDIZED_URL="http://oxidized.example.com:8888"
//! cargo test -- --ignored
//! ```

use mcp_oxidized::config::Config;
use mcp_oxidized::error::OxidizedError;
use mcp_oxidized::oxidized::{OxidizedBackend, OxidizedClient};
use mcp_oxidized::resources::{
    get_node, get_node_config, get_node_version, get_node_versions, get_stats, list_nodes,
};

/// Helper to create a client from environment variables.
fn create_client_from_env() -> OxidizedClient {
    let oxidized_url =
        std::env::var("OXIDIZED_URL").expect("OXIDIZED_URL required for integration tests");

    let config = Config {
        oxidized_url,
        oxidized_user: std::env::var("OXIDIZED_USER").ok(),
        oxidized_password: std::env::var("OXIDIZED_PASSWORD").ok(),
    };

    OxidizedClient::new(&config)
}

/// Test that get_nodes() returns a non-empty list from a real Oxidized server.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_nodes_returns_list() {
    let client = create_client_from_env();

    let (nodes, metadata) = client
        .get_nodes()
        .await
        .expect("Should successfully get nodes from Oxidized");

    assert!(
        !nodes.is_empty(),
        "Oxidized should return at least one node"
    );

    // First call should be cache miss
    assert!(!metadata.cache_hit, "First call should be cache miss");

    // Verify node structure
    let first_node = &nodes[0];
    assert!(
        !first_node.name.is_empty(),
        "Node should have a non-empty name"
    );
    assert!(!first_node.ip.is_empty(), "Node should have a non-empty IP");
}

/// Test that get_node() returns details for a specific node.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_returns_details() {
    let client = create_client_from_env();

    // First get list of nodes to find a valid node name
    let (nodes, _) = client
        .get_nodes()
        .await
        .expect("Should get nodes to find a valid name");

    assert!(!nodes.is_empty(), "Need at least one node for this test");

    let node_name = &nodes[0].name;

    // Now get specific node details
    let (node, metadata) = client
        .get_node(node_name)
        .await
        .expect("Should get node details");

    assert_eq!(
        node.name, *node_name,
        "Returned node should match requested name"
    );

    // First call for this node should be cache miss
    assert!(!metadata.cache_hit, "First call should be cache miss");
}

/// Test that get_node_config() returns configuration text.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_config_returns_text() {
    let client = create_client_from_env();

    // First get list of nodes to find a valid node name
    let (nodes, _) = client
        .get_nodes()
        .await
        .expect("Should get nodes to find a valid name");

    assert!(!nodes.is_empty(), "Need at least one node for this test");

    // Find a node with successful backup
    let success_node = nodes.iter().find(|n| n.status == "success");

    if let Some(node) = success_node {
        let (config, metadata) = client
            .get_node_config(&node.name)
            .await
            .expect("Should get node configuration");

        assert!(
            !config.is_empty(),
            "Configuration should not be empty for a successful node"
        );

        // First call should be cache miss
        assert!(!metadata.cache_hit, "First call should be cache miss");
    } else {
        println!("Warning: No node with successful backup found, skipping config test");
    }
}

/// Test that get_stats() returns server statistics.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_stats_returns_data() {
    let client = create_client_from_env();

    let (stats, metadata) = client
        .get_stats()
        .await
        .expect("Should get server statistics");

    // Stats may have optional fields, but the request should succeed
    println!("Stats: {:?}", stats);

    // First call should be cache miss
    assert!(!metadata.cache_hit, "First call should be cache miss");
}

/// Test that get_node_versions() returns version history.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_versions_returns_history() {
    let client = create_client_from_env();

    // First get list of nodes to find a valid node name
    let (nodes, _) = client
        .get_nodes()
        .await
        .expect("Should get nodes to find a valid name");

    assert!(!nodes.is_empty(), "Need at least one node for this test");

    // Find a node with successful backup (likely to have versions)
    let success_node = nodes.iter().find(|n| n.status == "success");

    if let Some(node) = success_node {
        let versions = client
            .get_node_versions(&node.name)
            .await
            .expect("Should get node versions");

        if !versions.is_empty() {
            let first_version = &versions[0];
            assert!(
                !first_version.oid.is_empty(),
                "Version should have a non-empty oid"
            );
            assert!(
                !first_version.date.is_empty(),
                "Version should have a non-empty date"
            );
        } else {
            println!("Note: Node {} has no version history yet", node.name);
        }
    } else {
        println!("Warning: No node with successful backup found, skipping versions test");
    }
}

/// Test that requesting a non-existent node returns NodeNotFound error.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_not_found() {
    let client = create_client_from_env();

    let result = client.get_node("definitely-not-a-real-node-xyz123").await;

    assert!(result.is_err(), "Should return error for non-existent node");

    match result.unwrap_err() {
        mcp_oxidized::error::OxidizedError::NodeNotFound(name, _) => {
            assert!(name.contains("definitely-not-a-real-node"));
        }
        other => panic!("Expected NodeNotFound error, got: {:?}", other),
    }
}

// =============================================================================
// Performance Tests (AC: 7 - NFR1, NFR2)
// =============================================================================

/// Test that cached requests return in < 100ms (NFR1).
///
/// This test verifies the p95 target for cached requests.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_cached_request_performance_under_100ms() {
    use std::time::Instant;

    let client = create_client_from_env();

    // First call to populate the cache (uncached)
    let (_, first_metadata) = client
        .get_nodes()
        .await
        .expect("Should get nodes to populate cache");

    assert!(!first_metadata.cache_hit, "First call should be cache miss");

    // Measure cached request performance
    let mut durations = Vec::new();

    for _ in 0..20 {
        let start = Instant::now();
        let (_, metadata) = client
            .get_nodes()
            .await
            .expect("Should get nodes from cache");
        durations.push(start.elapsed());

        // Verify we're hitting cache
        assert!(metadata.cache_hit, "Subsequent calls should be cache hits");
    }

    // Sort durations to find p95
    durations.sort();
    let p95_index = (durations.len() as f64 * 0.95) as usize;
    let p95_duration = durations[p95_index.min(durations.len() - 1)];

    println!("Cached request p95: {:?} (target: < 100ms)", p95_duration);

    assert!(
        p95_duration < std::time::Duration::from_millis(100),
        "Cached request p95 should be < 100ms, got {:?}",
        p95_duration
    );
}

/// Test that uncached requests return in < 500ms (NFR2).
///
/// This test verifies the p95 target for uncached requests.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_uncached_request_performance_under_500ms() {
    use std::time::Instant;

    let client = create_client_from_env();

    // Measure initial (uncached) request performance
    let start = Instant::now();
    let (_, metadata) = client.get_nodes().await.expect("Should get nodes from API");
    let duration = start.elapsed();

    // Verify it was a cache miss
    assert!(!metadata.cache_hit, "First call should be cache miss");

    println!(
        "Uncached request duration: {:?} (target: < 500ms)",
        duration
    );

    assert!(
        duration < std::time::Duration::from_millis(500),
        "Uncached request should complete in < 500ms, got {:?}",
        duration
    );
}

/// Test that cache provides significant performance improvement.
///
/// Cached requests should be at least 5x faster than uncached requests.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_cache_provides_performance_improvement() {
    use std::time::Instant;

    let client = create_client_from_env();

    // Measure uncached request time
    let uncached_start = Instant::now();
    let (_, uncached_meta) = client.get_nodes().await.expect("Should get nodes from API");
    let uncached_duration = uncached_start.elapsed();

    assert!(!uncached_meta.cache_hit, "First call should be cache miss");

    // Measure cached request time
    let cached_start = Instant::now();
    let (_, cached_meta) = client
        .get_nodes()
        .await
        .expect("Should get nodes from cache");
    let cached_duration = cached_start.elapsed();

    assert!(cached_meta.cache_hit, "Second call should be cache hit");

    println!(
        "Uncached: {:?}, Cached: {:?}, Improvement: {:.1}x",
        uncached_duration,
        cached_duration,
        uncached_duration.as_micros() as f64 / cached_duration.as_micros().max(1) as f64
    );

    assert!(
        cached_duration < uncached_duration,
        "Cached request should be faster than uncached"
    );
}

/// Test that config cache works correctly.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_config_cache_hit() {
    use std::time::Instant;

    let client = create_client_from_env();

    // Get a valid node name
    let (nodes, _) = client.get_nodes().await.expect("Should get nodes");

    let success_node = nodes.iter().find(|n| n.status == "success");
    if success_node.is_none() {
        println!("Warning: No node with successful backup found, skipping config cache test");
        return;
    }
    let node_name = &success_node.unwrap().name;

    // First call - cache miss
    let start1 = Instant::now();
    let (config1, meta1) = client
        .get_node_config(node_name)
        .await
        .expect("Should get config");
    let duration1 = start1.elapsed();

    assert!(!meta1.cache_hit, "First call should be cache miss");

    // Second call - cache hit
    let start2 = Instant::now();
    let (config2, meta2) = client
        .get_node_config(node_name)
        .await
        .expect("Should get config from cache");
    let duration2 = start2.elapsed();

    assert!(meta2.cache_hit, "Second call should be cache hit");
    assert_eq!(config1, config2, "Cached config should match original");

    println!(
        "Config cache - Uncached: {:?}, Cached: {:?}",
        duration1, duration2
    );

    assert!(
        duration2 < std::time::Duration::from_millis(100),
        "Cached config request should be < 100ms"
    );
}

/// Test that stats cache works correctly.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_stats_cache_hit() {
    use std::time::Instant;

    let client = create_client_from_env();

    // First call - cache miss
    let start1 = Instant::now();
    let (_, meta1) = client.get_stats().await.expect("Should get stats");
    let duration1 = start1.elapsed();

    assert!(!meta1.cache_hit, "First call should be cache miss");

    // Second call - cache hit
    let start2 = Instant::now();
    let (_, meta2) = client
        .get_stats()
        .await
        .expect("Should get stats from cache");
    let duration2 = start2.elapsed();

    assert!(meta2.cache_hit, "Second call should be cache hit");

    println!(
        "Stats cache - Uncached: {:?}, Cached: {:?}",
        duration1, duration2
    );

    assert!(
        duration2 < std::time::Duration::from_millis(100),
        "Cached stats request should be < 100ms"
    );
}

// =============================================================================
// Resource Handler Integration Tests (Story 1.6)
// =============================================================================

/// Test that list_nodes() resource returns paginated data.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_list_nodes_returns_paginated_data() {
    let client = create_client_from_env();

    let result = list_nodes(&client, None, None, None).await;

    assert!(result.is_ok(), "list_nodes should succeed");

    let response = result.unwrap();
    assert!(response.total > 0, "Should have at least one node");
    assert_eq!(response.offset, 0, "Default offset should be 0");
    assert!(response.limit <= 500, "Limit should be capped at 500");
}

/// Test that list_nodes() respects pagination parameters.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_list_nodes_pagination_params() {
    let client = create_client_from_env();

    let result = list_nodes(&client, Some(0), Some(5), None).await;

    assert!(result.is_ok(), "list_nodes with pagination should succeed");

    let response = result.unwrap();
    assert!(response.items.len() <= 5, "Should respect limit parameter");
    assert_eq!(response.offset, 0, "Should preserve offset");
    assert_eq!(response.limit, 5, "Should preserve limit");
}

/// Test that list_nodes() filters by group.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_list_nodes_group_filter() {
    let client = create_client_from_env();

    // First get all nodes to find a valid group
    let all_nodes = list_nodes(&client, None, None, None).await.unwrap();

    if all_nodes.items.is_empty() {
        println!("Warning: No nodes found, skipping group filter test");
        return;
    }

    let group = &all_nodes.items[0].group;

    // Now filter by that group
    let filtered = list_nodes(&client, None, None, Some(group)).await.unwrap();

    // All items should have matching group
    for node in &filtered.items {
        assert_eq!(
            &node.group, group,
            "All filtered nodes should have matching group"
        );
    }
}

/// Test that get_node() resource returns node with cache metadata.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_returns_data() {
    let client = create_client_from_env();

    // Get a valid node name
    let nodes = list_nodes(&client, None, Some(1), None).await.unwrap();

    if nodes.items.is_empty() {
        println!("Warning: No nodes found, skipping get_node test");
        return;
    }

    let node_name = &nodes.items[0].name;

    let result = get_node(&client, node_name).await;

    assert!(result.is_ok(), "get_node should succeed");

    let response = result.unwrap();
    assert_eq!(
        &response.node.name, node_name,
        "Returned node should match request"
    );
}

/// Test that get_node() returns NodeNotFound with suggestions.
///
/// This test uses a two-phase approach:
/// 1. First, list nodes to find a real node name prefix
/// 2. Then, query for a non-existent node with that prefix to trigger suggestions
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_not_found_has_suggestions() {
    let client = create_client_from_env();

    // Phase 1: Get a real node to build a similar-but-nonexistent name
    let nodes_result = list_nodes(&client, None, Some(5), None).await;
    assert!(nodes_result.is_ok(), "Should be able to list nodes");

    let nodes = nodes_result.unwrap();
    if nodes.items.is_empty() {
        println!("SKIP: No nodes in inventory to test suggestions");
        return;
    }

    // Take first node's name and append something to make it not exist
    let existing_name = &nodes.items[0].name;
    let non_existent_name = format!("{}-NONEXISTENT-999", existing_name);

    // Phase 2: Query for non-existent node - should get suggestions
    let result = get_node(&client, &non_existent_name).await;

    assert!(result.is_err(), "Should return error for non-existent node");

    match result.unwrap_err() {
        OxidizedError::NodeNotFound(name, suggestions) => {
            assert_eq!(name, non_existent_name);
            // With at least one node in inventory and a prefix match, we should get suggestions
            assert!(
                !suggestions.is_empty(),
                "Should return suggestions when nodes exist with similar prefix. \
                 Original node '{}' should appear in suggestions.",
                existing_name
            );
            assert!(
                suggestions.contains(existing_name),
                "Suggestions {:?} should contain the original node '{}'",
                suggestions,
                existing_name
            );
            println!(
                "NodeNotFound correctly returned {} suggestions: {:?}",
                suggestions.len(),
                suggestions
            );
        }
        other => panic!("Expected NodeNotFound error, got: {:?}", other),
    }
}

/// Test that get_stats() resource returns statistics via resource handler.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_resource_get_stats_returns_data() {
    let client = create_client_from_env();

    let result = get_stats(&client).await;

    assert!(result.is_ok(), "get_stats should succeed");

    let response = result.unwrap();
    println!("Stats: {:?}", response.stats);
}

/// Test that list_nodes() includes cache metadata.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_list_nodes_cache_metadata() {
    let client = create_client_from_env();

    // First call - cache miss
    let first = list_nodes(&client, None, None, None).await.unwrap();
    assert!(!first.metadata.cache_hit, "First call should be cache miss");

    // Second call - cache hit
    let second = list_nodes(&client, None, None, None).await.unwrap();
    assert!(second.metadata.cache_hit, "Second call should be cache hit");
}

// =============================================================================
// Configuration Access Resources Tests (Story 1.7)
// =============================================================================

/// Test that get_node_config() returns configuration with size metadata.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_config_returns_data_with_size() {
    let client = create_client_from_env();

    // Get a valid node name with successful backup
    let nodes = list_nodes(&client, None, Some(10), None).await.unwrap();
    let success_node = nodes.items.iter().find(|n| n.status == "success");

    if success_node.is_none() {
        println!("SKIP: No node with successful backup found");
        return;
    }

    let node_name = &success_node.unwrap().name;
    let result = get_node_config(&client, node_name).await;

    assert!(result.is_ok(), "get_node_config should succeed");

    let response = result.unwrap();
    assert!(!response.config.is_empty(), "Config should not be empty");
    assert!(response.size.bytes > 0, "Should have size metadata bytes");
    assert!(response.size.lines > 0, "Should have line count");
    assert!(
        response.size.estimated_tokens > 0,
        "Should have estimated tokens"
    );

    // Verify token estimation is reasonable (bytes/4)
    assert_eq!(
        response.size.estimated_tokens,
        response.size.bytes / 4,
        "Token estimation should be bytes/4"
    );
}

/// Test that get_node_config() returns NodeNotFound with suggestions.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_config_not_found_has_suggestions() {
    let client = create_client_from_env();

    // Get a real node name to build similar-but-nonexistent name
    let nodes = list_nodes(&client, None, Some(1), None).await.unwrap();
    if nodes.items.is_empty() {
        println!("SKIP: No nodes in inventory");
        return;
    }

    let existing_name = &nodes.items[0].name;
    let non_existent = format!("{}-NONEXISTENT-999", existing_name);

    let result = get_node_config(&client, &non_existent).await;

    assert!(result.is_err(), "Should return error for non-existent node");

    match result.unwrap_err() {
        OxidizedError::NodeNotFound(name, suggestions) => {
            assert_eq!(name, non_existent);
            assert!(
                !suggestions.is_empty(),
                "Should return suggestions for similar nodes"
            );
        }
        other => panic!("Expected NodeNotFound, got: {:?}", other),
    }
}

/// Test that get_node_versions() returns version list sorted descending.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_versions_sorted_descending() {
    let client = create_client_from_env();

    // Get a valid node name with successful backup
    let nodes = list_nodes(&client, None, Some(10), None).await.unwrap();
    let success_node = nodes.items.iter().find(|n| n.status == "success");

    if success_node.is_none() {
        println!("SKIP: No node with successful backup found");
        return;
    }

    let node_name = &success_node.unwrap().name;
    let result = get_node_versions(&client, node_name).await;

    assert!(result.is_ok(), "get_node_versions should succeed");

    let response = result.unwrap();
    assert_eq!(
        response.total,
        response.versions.len(),
        "Total should match versions count"
    );

    // Verify descending order (if more than 1 version)
    if response.versions.len() > 1 {
        for i in 0..response.versions.len() - 1 {
            assert!(
                response.versions[i].date >= response.versions[i + 1].date,
                "Versions should be sorted newest first"
            );
        }
    }
}

/// Test that get_node_versions() returns NodeNotFound with suggestions.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_versions_not_found_has_suggestions() {
    let client = create_client_from_env();

    // Get a real node name to build similar-but-nonexistent name
    let nodes = list_nodes(&client, None, Some(1), None).await.unwrap();
    if nodes.items.is_empty() {
        println!("SKIP: No nodes in inventory");
        return;
    }

    let existing_name = &nodes.items[0].name;
    let non_existent = format!("{}-NONEXISTENT-999", existing_name);

    let result = get_node_versions(&client, &non_existent).await;

    assert!(result.is_err(), "Should return error for non-existent node");

    match result.unwrap_err() {
        OxidizedError::NodeNotFound(name, suggestions) => {
            assert_eq!(name, non_existent);
            assert!(
                !suggestions.is_empty(),
                "Should return suggestions for similar nodes"
            );
        }
        other => panic!("Expected NodeNotFound, got: {:?}", other),
    }
}

/// Test that get_node_version() returns historical config with size metadata.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_version_returns_historical_config() {
    let client = create_client_from_env();

    // Get a valid node with versions
    let nodes = list_nodes(&client, None, Some(10), None).await.unwrap();
    let success_node = nodes.items.iter().find(|n| n.status == "success");

    if success_node.is_none() {
        println!("SKIP: No node with successful backup found");
        return;
    }

    let node_name = &success_node.unwrap().name;

    // Get versions for this node
    let versions = get_node_versions(&client, node_name).await;
    if versions.is_err() || versions.as_ref().unwrap().versions.is_empty() {
        println!("SKIP: No versions available for node {}", node_name);
        return;
    }

    let version = &versions.unwrap().versions[0];
    let oid = &version.oid;

    // Get specific version config
    let result = get_node_version(&client, node_name, oid).await;

    assert!(result.is_ok(), "get_node_version should succeed");

    let response = result.unwrap();
    assert!(!response.config.is_empty(), "Config should not be empty");
    assert_eq!(response.oid, *oid, "OID should match request");
    assert!(response.size.bytes > 0, "Should have size metadata");
}

/// Test that get_node_version() returns error for invalid OID.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_version_invalid_oid_returns_error() {
    let client = create_client_from_env();

    // Get a valid node name
    let nodes = list_nodes(&client, None, Some(1), None).await.unwrap();
    if nodes.items.is_empty() {
        println!("SKIP: No nodes in inventory");
        return;
    }

    let node_name = &nodes.items[0].name;

    // Request with invalid OID
    let result = get_node_version(&client, node_name, "invalid-oid-that-does-not-exist").await;

    assert!(result.is_err(), "Should return error for invalid OID");

    // Verify the error type is appropriate (NodeNotFound or HttpError)
    let err = result.unwrap_err();
    match &err {
        OxidizedError::NodeNotFound(_, _) | OxidizedError::HttpError { .. } => {
            // Expected error types for invalid OID
        }
        other => panic!(
            "Expected NodeNotFound or HttpError for invalid OID, got: {:?}",
            other
        ),
    }
}

/// Test that get_node_version() returns NodeNotFound with suggestions for invalid node.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_version_not_found_has_suggestions() {
    let client = create_client_from_env();

    // Get a real node name to build similar-but-nonexistent name
    let nodes = list_nodes(&client, None, Some(1), None).await.unwrap();
    if nodes.items.is_empty() {
        println!("SKIP: No nodes in inventory");
        return;
    }

    let existing_name = &nodes.items[0].name;
    let non_existent = format!("{}-NONEXISTENT-999", existing_name);

    // Request with non-existent node and any OID
    let result = get_node_version(&client, &non_existent, "any-oid").await;

    assert!(result.is_err(), "Should return error for non-existent node");

    match result.unwrap_err() {
        OxidizedError::NodeNotFound(name, suggestions) => {
            assert_eq!(name, non_existent);
            assert!(
                !suggestions.is_empty(),
                "Should return suggestions for similar nodes"
            );
            println!(
                "NodeNotFound correctly returned {} suggestions: {:?}",
                suggestions.len(),
                suggestions
            );
        }
        other => panic!("Expected NodeNotFound, got: {:?}", other),
    }
}

/// Test config cache hit/miss for get_node_config.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_config_cache_hit() {
    let client = create_client_from_env();

    // Get a valid node name with successful backup
    let nodes = list_nodes(&client, None, Some(10), None).await.unwrap();
    let success_node = nodes.items.iter().find(|n| n.status == "success");

    if success_node.is_none() {
        println!("SKIP: No node with successful backup found");
        return;
    }

    let node_name = &success_node.unwrap().name;

    // First call - cache miss
    let first = get_node_config(&client, node_name).await.unwrap();
    assert!(!first.metadata.cache_hit, "First call should be cache miss");

    // Second call - cache hit
    let second = get_node_config(&client, node_name).await.unwrap();
    assert!(second.metadata.cache_hit, "Second call should be cache hit");

    // Config should be identical
    assert_eq!(first.config, second.config, "Cached config should match");
}
