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
