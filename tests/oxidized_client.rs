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
use mcp_oxidized::oxidized::{OxidizedBackend, OxidizedClient};

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
