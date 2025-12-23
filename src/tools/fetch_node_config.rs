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
