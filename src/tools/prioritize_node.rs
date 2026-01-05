//! Tool to prioritize a node in the backup queue (FR16).

use tracing::instrument;

use crate::error::OxidizedError;
use crate::oxidized::{OxidizedBackend, OxidizedClient};

use super::{ToolResult, enrich_node_not_found};

/// Prioritize a node in the backup queue (FR16).
///
/// This tool moves the specified node to the front of the backup queue,
/// ensuring it will be processed before other pending nodes.
///
/// # Arguments
///
/// * `backend` - The Oxidized client to use
/// * `node` - The node name to prioritize
///
/// # Returns
///
/// A `ToolResult` indicating success or failure.
///
/// # Cache Invalidation
///
/// On success, the backend method automatically invalidates the cache
/// for the node.
///
/// # Errors
///
/// - [`OxidizedError::NodeNotFound`] - Node does not exist (includes suggestions)
/// - [`OxidizedError::ApiUnreachable`] - Network/connection error
///
/// # Example
///
/// ```ignore
/// let result = prioritize_node(&client, "SW-Core-01").await?;
/// println!("{}", result.message);
/// // "Node 'SW-Core-01' has been prioritized in the backup queue."
/// ```
#[instrument(skip(backend), fields(node = %node))]
pub async fn prioritize_node(
    backend: &OxidizedClient,
    node: &str,
) -> Result<ToolResult, OxidizedError> {
    match backend.prioritize_node(node).await {
        Ok(()) => {
            tracing::info!(node = %node, "Node prioritized in queue");
            Ok(ToolResult::success(
                node,
                format!("Node '{}' has been prioritized in the backup queue.", node),
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
    /// This mirrors the actual message format in prioritize_node().
    fn expected_message(node: &str) -> String {
        format!("Node '{}' has been prioritized in the backup queue.", node)
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
            "Node 'SW-Core-01' has been prioritized in the backup queue."
        );
    }

    #[test]
    fn test_success_message_with_special_characters() {
        // Node names can contain special characters like hyphens, underscores, dots
        let node = "switch-datacenter_2.corp.local";
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
        let node = "switch-中文";
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
        let node = "router-1";
        let result = ToolResult::success(node, expected_message(node));

        let json = serde_json::to_string(&result).expect("Should serialize to JSON");

        // Verify JSON structure
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"node\":\"router-1\""));
        assert!(json.contains("\"message\":"));

        // Verify round-trip via generic JSON
        let value: serde_json::Value = serde_json::from_str(&json).expect("Should parse as JSON");
        assert_eq!(value["success"], true);
        assert_eq!(value["node"], "router-1");
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
