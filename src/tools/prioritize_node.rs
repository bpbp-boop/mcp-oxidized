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
