//! Shared identity and validation helpers for governed KageTools.

use std::sync::Arc;
use kage_browser::{Tab, TabHealth, TabId, TabLifecycle, TabManager};
use kage_core::tool::ToolError;
use uuid::Uuid;

/// Validate tab existence, lifecycle state, health, and resolve explicit CDP target identity.
///
/// Enforces:
/// - `INV-10`: Tab must exist with explicit ProfileId and TargetId.
/// - `INV-11A`: Fails closed if tab is Closing, Closed, Recovering, RendererTerminated, or Unresponsive.
/// - `INV-12`: Fails if target identity is inferred or unbound.
pub async fn resolve_active_tab(
    tab_manager: &Arc<TabManager>,
    tool_id: &'static str,
    args: &serde_json::Value,
) -> Result<(TabId, Arc<Tab>, String), ToolError> {
    let tab_id_str = args["tab_id"].as_str().ok_or_else(|| ToolError::SchemaViolation {
        tool_id: tool_id.to_string(),
        reason: "Missing 'tab_id' string argument".into(),
    })?;

    let tab_uuid = Uuid::parse_str(tab_id_str).map_err(|e| ToolError::SchemaViolation {
        tool_id: tool_id.to_string(),
        reason: format!("'tab_id' is not a valid UUID: {e}"),
    })?;
    let tab_id = TabId(tab_uuid);

    let tab = tab_manager.get_tab(tab_id).await.map_err(|_| ToolError::NotFound {
        id: format!("Tab {tab_id} does not exist"),
    })?;

    let lifecycle = tab.lifecycle.read().await.clone();
    if lifecycle == TabLifecycle::Closing || lifecycle == TabLifecycle::Closed {
        return Err(ToolError::ExecutionFailed {
            tool_id: tool_id.to_string(),
            source: format!("Tab {tab_id} is in non-executable state: {lifecycle:?}").into(),
        });
    }

    let health = tab.health.read().await.clone();
    match health {
        TabHealth::Healthy => {}
        TabHealth::Recovering { recovery_epoch } => {
            return Err(ToolError::ExecutionFailed {
                tool_id: tool_id.to_string(),
                source: format!("Tab {tab_id} is recovering (epoch: {recovery_epoch}) - CDP execution unavailable").into(),
            });
        }
        TabHealth::RendererTerminated { status, .. } => {
            return Err(ToolError::ExecutionFailed {
                tool_id: tool_id.to_string(),
                source: format!("Renderer terminated (status: {status:?}) - fail-closed (INV-11A)").into(),
            });
        }
        TabHealth::Unresponsive => {
            return Err(ToolError::ExecutionFailed {
                tool_id: tool_id.to_string(),
                source: format!("Tab {tab_id} is unresponsive - CDP execution unavailable").into(),
            });
        }
    }

    let target_id = {
        let cdp_guard = tab.cdp.read().await;
        cdp_guard.as_ref().map(|c| c.target_id.clone()).ok_or_else(|| ToolError::ExecutionFailed {
            tool_id: tool_id.to_string(),
            source: format!("Tab {tab_id} has no bound CDP target session (INV-12)").into(),
        })?
    };

    Ok((tab_id, tab, target_id))
}
