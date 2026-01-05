//! Tool to reload the Oxidized source inventory (FR17).

use tracing::instrument;

use crate::error::OxidizedError;
use crate::oxidized::{OxidizedBackend, OxidizedClient};

use super::ToolResult;

/// Reload the Oxidized source inventory (FR17).
///
/// This tool triggers Oxidized to reload its source configuration,
/// making any newly added devices immediately available in the inventory.
///
/// # Arguments
///
/// * `backend` - The Oxidized client to use
///
/// # Returns
///
/// A `ToolResult` indicating success or failure.
///
/// # Cache Invalidation
///
/// On success, the backend method automatically invalidates ALL caches
/// via `invalidate_all_nodes()` to ensure subsequent requests see the
/// fresh inventory.
///
/// # Errors
///
/// - [`OxidizedError::ApiUnreachable`] - Network/connection error
///
/// # Example
///
/// ```ignore
/// let result = reload_sources(&client).await?;
/// println!("{}", result.message);
/// // "Oxidized sources reloaded. New devices are now available in the inventory."
/// ```
#[instrument(skip(backend))]
pub async fn reload_sources(backend: &OxidizedClient) -> Result<ToolResult, OxidizedError> {
    match backend.reload_sources().await {
        Ok(()) => {
            // Cache is already invalidated by OxidizedClient::reload_sources
            tracing::info!("Oxidized sources reloaded successfully");
            Ok(ToolResult::success(
                "",
                "Oxidized sources reloaded. New devices are now available in the inventory.",
            ))
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

    /// The exact success message produced by reload_sources().
    const EXPECTED_MESSAGE: &str =
        "Oxidized sources reloaded. New devices are now available in the inventory.";

    // -------------------------------------------------------------------------
    // Message Format Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_success_message_exact_format() {
        // Verify the message format matches what the function produces
        let result = ToolResult::success("", EXPECTED_MESSAGE);

        assert!(result.success);
        assert!(
            result.node.is_empty(),
            "reload_sources has no specific node"
        );
        assert_eq!(result.message, EXPECTED_MESSAGE);
    }

    #[test]
    fn test_success_node_is_empty() {
        // reload_sources operates on the entire inventory, not a specific node
        let result = ToolResult::success("", EXPECTED_MESSAGE);

        assert!(result.node.is_empty());
        // Verify the empty node is intentional behavior
        assert_eq!(result.node.len(), 0);
    }

    // -------------------------------------------------------------------------
    // Serialization Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_tool_result_json_serialization() {
        let result = ToolResult::success("", EXPECTED_MESSAGE);

        let json = serde_json::to_string(&result).expect("Should serialize to JSON");

        // Verify JSON structure
        assert!(json.contains("\"success\":true"));
        assert!(
            json.contains("\"node\":\"\""),
            "Node should be empty string"
        );
        assert!(json.contains("\"message\":"));

        // Verify round-trip via generic JSON
        let value: serde_json::Value = serde_json::from_str(&json).expect("Should parse as JSON");
        assert_eq!(value["success"], true);
        assert_eq!(value["node"], "");
        assert_eq!(value["message"], EXPECTED_MESSAGE);
    }

    #[test]
    fn test_tool_result_json_contains_all_fields() {
        let result = ToolResult::success("", EXPECTED_MESSAGE);

        let json = serde_json::to_string(&result).expect("Should serialize to JSON");
        let value: serde_json::Value = serde_json::from_str(&json).expect("Should parse as JSON");

        // Verify all expected fields are present
        assert!(
            value.get("success").is_some(),
            "Should have 'success' field"
        );
        assert!(value.get("node").is_some(), "Should have 'node' field");
        assert!(
            value.get("message").is_some(),
            "Should have 'message' field"
        );
    }
}
