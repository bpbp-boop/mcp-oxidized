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

    let nodes = client
        .get_nodes()
        .await
        .expect("Should successfully get nodes from Oxidized");

    assert!(
        !nodes.is_empty(),
        "Oxidized should return at least one node"
    );

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
    let nodes = client
        .get_nodes()
        .await
        .expect("Should get nodes to find a valid name");

    assert!(!nodes.is_empty(), "Need at least one node for this test");

    let node_name = &nodes[0].name;

    // Now get specific node details
    let node = client
        .get_node(node_name)
        .await
        .expect("Should get node details");

    assert_eq!(
        node.name, *node_name,
        "Returned node should match requested name"
    );
}

/// Test that get_node_config() returns configuration text.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_config_returns_text() {
    let client = create_client_from_env();

    // First get list of nodes to find a valid node name
    let nodes = client
        .get_nodes()
        .await
        .expect("Should get nodes to find a valid name");

    assert!(!nodes.is_empty(), "Need at least one node for this test");

    // Find a node with successful backup
    let success_node = nodes.iter().find(|n| n.status == "success");

    if let Some(node) = success_node {
        let config = client
            .get_node_config(&node.name)
            .await
            .expect("Should get node configuration");

        assert!(
            !config.is_empty(),
            "Configuration should not be empty for a successful node"
        );
    } else {
        println!("Warning: No node with successful backup found, skipping config test");
    }
}

/// Test that get_stats() returns server statistics.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_stats_returns_data() {
    let client = create_client_from_env();

    let stats = client
        .get_stats()
        .await
        .expect("Should get server statistics");

    // Stats may have optional fields, but the request should succeed
    println!("Stats: {:?}", stats);
}

/// Test that get_node_versions() returns version history.
#[tokio::test]
#[ignore] // Requires real Oxidized server - run with: cargo test -- --ignored
async fn test_get_node_versions_returns_history() {
    let client = create_client_from_env();

    // First get list of nodes to find a valid node name
    let nodes = client
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
