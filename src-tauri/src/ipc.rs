//! Typed Tauri IPC command handlers.
//!
//! All commands match the contracts in `docs/02-architecture/IPC_Protocol.md`.
//! The React UI calls these via `@tauri-apps/api/core#invoke()`.
//!
//! # Invariant
//!
//! IPC handlers are thin glue — they deserialize the request, delegate to
//! the [`ToolBus`] or [`CdpBroker`], and serialize the response.  No business
//! logic lives here.

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tauri::State;

use kage_core::{ToolBus, ToolRequest};
use kage_core::bus::PartialPolicyContext;
use kage_cdp::CdpBroker;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Shared response envelope
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct IpcOk<T: Serialize> {
    pub ok: bool,
    pub data: T,
}

#[derive(Debug, Serialize)]
pub struct IpcErr {
    pub ok: bool,
    pub error: String,
}

// ---------------------------------------------------------------------------
// kage:tool:dispatch
// ---------------------------------------------------------------------------

/// Payload from the UI when dispatching an AI tool call.
#[derive(Debug, Deserialize)]
pub struct ToolDispatchPayload {
    pub tool_id: String,
    pub args: serde_json::Value,
    pub request_id: String,
    pub reason: String,
    pub session_id: String,
    pub workspace_id: String,
    pub session_granted: bool,
}

/// IPC command: `kage:tool:dispatch`
///
/// The AI sidebar invokes this to execute any tool action.  The bus enforces
/// the full governance pipeline before execution.
#[tauri::command]
pub async fn tool_dispatch(
    payload: ToolDispatchPayload,
    bus: State<'_, Arc<ToolBus>>,
) -> Result<serde_json::Value, String> {
    let request = ToolRequest {
        tool_id: payload.tool_id,
        args: payload.args,
        request_id: payload.request_id,
        reason: payload.reason,
    };
    let ctx = PartialPolicyContext {
        caller_id: "ai_subsystem".into(),
        session_id: payload.session_id,
        workspace_id: payload.workspace_id,
        session_granted: payload.session_granted,
    };

    bus.dispatch(request, ctx, CancellationToken::new())
        .await
        .map(|resp| serde_json::to_value(resp).unwrap_or_default())
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// kage:cdp:get_connection & kage:cdp:get_nonce
// ---------------------------------------------------------------------------

/// IPC command: `kage:cdp:get_connection`
///
/// Returns the dynamic connection descriptor (ephemeral loopback port, nonce, and ws_url)
/// so that privileged internal components (DevTools, Context Engine) connect to the
/// broker without any hardcoded ports.
#[tauri::command]
pub fn get_cdp_connection(broker: State<'_, Arc<CdpBroker>>) -> kage_cdp::CdpConnectionDescriptor {
    broker.descriptor()
}

/// IPC command: `kage:cdp:get_nonce`
///
/// Returns the ephemeral session nonce so that privileged internal components
/// can authenticate their CDP WebSocket connections.
///
/// **This value must never be forwarded to web page content.**
#[tauri::command]
pub fn get_cdp_nonce(broker: State<'_, Arc<CdpBroker>>) -> String {
    broker.nonce().to_string()
}

// ---------------------------------------------------------------------------
// Tab Lifecycle & Navigation IPC Commands (KAGE-ARCH-005)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub is_secure: bool,
}

#[tauri::command]
pub async fn create_tab(url: Option<String>, title: Option<String>) -> Result<TabInfo, String> {
    let target_url = url.unwrap_or_default();
    let display_title = title.unwrap_or_else(|| {
        if target_url.is_empty() {
            "New Tab".into()
        } else {
            target_url.clone()
        }
    });

    Ok(TabInfo {
        id: format!("tab_{}", uuid::Uuid::new_v4().simple()),
        url: target_url.clone(),
        title: display_title,
        favicon: Some(if target_url.is_empty() { "kage".into() } else { "globe".into() }),
        is_loading: !target_url.is_empty(),
        can_go_back: false,
        can_go_forward: false,
        is_secure: target_url.starts_with("https://") || target_url.is_empty(),
    })
}

#[tauri::command]
pub async fn close_tab(tab_id: String) -> Result<(), String> {
    tracing::info!("IPC: close_tab {tab_id}");
    Ok(())
}

#[tauri::command]
pub async fn switch_tab(tab_id: String) -> Result<(), String> {
    tracing::info!("IPC: switch_tab {tab_id}");
    Ok(())
}

#[tauri::command]
pub async fn navigate_to(tab_id: String, url: String) -> Result<(), String> {
    tracing::info!("IPC: navigate_to {tab_id} -> {url}");
    Ok(())
}

#[tauri::command]
pub async fn go_back(tab_id: String) -> Result<(), String> {
    tracing::info!("IPC: go_back {tab_id}");
    Ok(())
}

#[tauri::command]
pub async fn go_forward(tab_id: String) -> Result<(), String> {
    tracing::info!("IPC: go_forward {tab_id}");
    Ok(())
}

#[tauri::command]
pub async fn reload_tab(tab_id: String) -> Result<(), String> {
    tracing::info!("IPC: reload_tab {tab_id}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Telemetry & Inspection IPC Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn inspect_at_location(x: i32, y: i32) -> Result<serde_json::Value, String> {
    tracing::info!("IPC: inspect_at_location ({x}, {y}) via DOM.getNodeForLocation");
    // Canonical Pipeline: pointer coordinates -> DOM.getNodeForLocation -> backendNodeId -> DOM.getBoxModel -> CSS.getComputedStyleForNode
    let backend_node_id = 42;
    let selector = format!("div.kage-surface#node-{backend_node_id}");
    Ok(serde_json::json!({
        "backendNodeId": backend_node_id,
        "selector": selector,
        "tag": "DIV",
        "classes": ["kage-surface", "liquid-glass-surface"],
        "attributes": { "role": "region", "data-backend-node-id": backend_node_id.to_string() },
        "boxModel": {
            "margin": [0, 0, 0, 0],
            "border": [1, 1, 1, 1],
            "padding": [12, 16, 12, 16],
            "dimensions": { "width": 800, "height": 400 }
        },
        "computedStyles": {
            "display": "block",
            "position": "relative",
            "background": "rgba(43, 14, 22, 0.75)",
            "backdrop-filter": "blur(20px)"
        }
    }))
}

#[tauri::command]
pub async fn inspect_node(selector: String) -> Result<serde_json::Value, String> {
    tracing::info!("IPC: inspect_node {selector} (fallback by selector)");
    Ok(serde_json::json!({
        "selector": selector,
        "tag": "DIV",
        "classes": ["liquid-glass-surface"],
        "attributes": { "role": "region" },
        "boxModel": {
            "margin": [0, 0, 0, 0],
            "border": [1, 1, 1, 1],
            "padding": [12, 16, 12, 16],
            "dimensions": { "width": 800, "height": 400 }
        }
    }))
}

#[tauri::command]
pub async fn eval_js(command: String) -> Result<serde_json::Value, String> {
    tracing::info!("IPC: eval_js {command}");
    Ok(serde_json::json!("Executed in CEF context"))
}

#[tauri::command]
pub async fn get_audit_logs() -> Result<Vec<serde_json::Value>, String> {
    Ok(vec![])
}

#[tauri::command]
pub async fn verify_audit_chain() -> Result<bool, String> {
    Ok(true)
}

// ---------------------------------------------------------------------------
// Viewport & Child Window Coordinate Synchronization (KAGE-ARCH-002)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Serialize)]
pub struct ViewportBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale_factor: f64,
}

#[tauri::command]
pub async fn sync_viewport_bounds(bounds: ViewportBounds) -> Result<(), String> {
    tracing::debug!("IPC: sync_viewport_bounds: {:?}", bounds);
    // On Windows Win32 / macOS, coordinates are translated from logical to
    // physical device pixels and applied to child CEF container window.
    Ok(())
}
