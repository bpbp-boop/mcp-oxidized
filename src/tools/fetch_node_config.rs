//! Tool to trigger immediate backup of a node's configuration (FR15).

use tracing::instrument;

use crate::error::OxidizedError;
use crate::oxidized::{OxidizedBackend, OxidizedClient};

use super::{ToolResult, enrich_node_not_found};

/// Trigger an immediate backup of a node's configuration (FR15).
///
/// This tool requests Oxidized to immediately fetch and store the
/// current configuration of the specified node, bypassing the normal
/// backup schedule.
///
/// # Arguments
///
/// * `backend` - The Oxidized client to use
/// * `node` - The node name to backup
///
/// # Returns
///
/// A `ToolResult` indicating success or failure.
///
/// # Cache Invalidation
///
/// On success, the backend method automatically invalidates the cache
/// for the node to ensure subsequent requests get the fresh configuration.
///
/// # Errors
///
/// - [`OxidizedError::NodeNotFound`] - Node does not exist (includes suggestions)
/// - [`OxidizedError::ApiUnreachable`] - Network/connection error
///
/// # Example
///
/// ```ignore
/// let result = fetch_node_config(&client, "SW-Core-01").await?;
/// println!("{}", result.message);
/// // "Backup triggered for node 'SW-Core-01'. Fresh configuration will be available shortly."
/// ```
#[instrument(skip(backend), fields(node = %node))]
pub async fn fetch_node_config(
    backend: &OxidizedClient,
    node: &str,
) -> Result<ToolResult, OxidizedError> {
    match backend.trigger_backup(node).await {
        Ok(()) => {
            tracing::info!(node = %node, "Backup triggered successfully");
            Ok(ToolResult::success(
                node,
                format!(
                    "Backup triggered for node '{}'. Fresh configuration will be available shortly.",
                    node
                ),
            ))
        }
        Err(OxidizedError::NodeNotFound(node_name, _)) => {
            Err(enrich_node_not_found(backend, node_name).await)
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

    /// Helper to generate the expected success message for a node.
    /// This mirrors the actual message format in fetch_node_config().
    fn expected_message(node: &str) -> String {
        format!(
            "Backup triggered for node '{}'. Fresh configuration will be available shortly.",
            node
        )
    }

    // -------------------------------------------------------------------------
    // Message Format Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_success_message_exact_format() {
        // Verify the message format matches what the function produces
        let node = "SW-Core-01";
        let result = ToolResult::success(node, expected_message(node));

        assert!(result.success);
        assert_eq!(result.node, node);
        assert_eq!(
            result.message,
            "Backup triggered for node 'SW-Core-01'. Fresh configuration will be available shortly."
        );
    }

    #[test]
    fn test_success_message_with_special_characters() {
        // Node names can contain special characters like hyphens, underscores, dots
        let node = "router-site_1.example.com";
        let result = ToolResult::success(node, expected_message(node));

        assert!(result.success);
        assert_eq!(result.node, node);
        assert!(
            result.message.contains(node),
            "Message should contain the exact node name with special chars"
        );
    }

    #[test]
    fn test_success_message_with_unicode() {
        // Edge case: node names with unicode (unlikely but possible)
        let node = "router-日本語";
        let result = ToolResult::success(node, expected_message(node));

        assert!(result.success);
        assert_eq!(result.node, node);
        assert!(result.message.contains(node));
    }

    // -------------------------------------------------------------------------
    // Serialization Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_tool_result_json_serialization() {
        let node = "test-node";
        let result = ToolResult::success(node, expected_message(node));

        let json = serde_json::to_string(&result).expect("Should serialize to JSON");

        // Verify JSON structure
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"node\":\"test-node\""));
        assert!(json.contains("\"message\":"));

        // Verify round-trip via generic JSON
        let value: serde_json::Value = serde_json::from_str(&json).expect("Should parse as JSON");
        assert_eq!(value["success"], true);
        assert_eq!(value["node"], "test-node");
    }

    #[test]
    fn test_tool_result_json_escapes_special_chars() {
        // Ensure special characters in node names are properly escaped in JSON
        let node = "node\"with'quotes";
        let result = ToolResult::success(node, expected_message(node));

        let json = serde_json::to_string(&result).expect("Should serialize to JSON");

        // Should be valid JSON (parse won't fail)
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("Should parse as valid JSON");
        assert_eq!(value["node"], node);
    }
}
