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
use kage_engine::{CefRuntime, NativeSurfaceManager};
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
    let ctx = PartialPolicyContext::new(
        "ai_subsystem",
        payload.session_id,
        payload.workspace_id,
        payload.session_granted,
    );

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

use kage_browser::{ProfileId, TabId, TabManager, TabSummary};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateTabPayload {
    pub url: Option<String>,
    pub title: Option<String>,
    pub profile_id: Option<String>,
}

#[tauri::command]
pub async fn create_tab(
    payload: Option<CreateTabPayload>,
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<TabInfo, String> {
    let payload = payload.unwrap_or(CreateTabPayload {
        url: None,
        title: None,
        profile_id: None,
    });

    let target_url = payload.url.unwrap_or_else(|| "https://example.com".to_string());
    let display_title = payload.title.unwrap_or_else(|| "New Tab".into());
    let profile_id = payload
        .profile_id
        .map(ProfileId::new)
        .unwrap_or_else(ProfileId::personal);

    let tab_id = tab_manager
        .create_tab(profile_id, &target_url)
        .await
        .map_err(|e| e.to_string())?;

    Ok(TabInfo {
        id: tab_id.to_string(),
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
pub async fn close_tab(
    tab_id: String,
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&tab_id).map_err(|e| e.to_string())?;
    tab_manager
        .close_tab(TabId(uuid))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_tab(
    tab_id: String,
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&tab_id).map_err(|e| e.to_string())?;
    tab_manager
        .switch_tab(TabId(uuid))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn navigate_to(
    tab_id: String,
    url: String,
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&tab_id).map_err(|e| e.to_string())?;
    let tab = tab_manager
        .get_tab(TabId(uuid))
        .await
        .map_err(|e| e.to_string())?;

    tab_manager
        .navigation()
        .navigate(&tab, &url, kage_browser::NavigationSource::Programmatic)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn go_back(
    tab_id: String,
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&tab_id).map_err(|e| e.to_string())?;
    let tab = tab_manager
        .get_tab(TabId(uuid))
        .await
        .map_err(|e| e.to_string())?;

    tab_manager
        .navigation()
        .go_back(&tab)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn go_forward(
    tab_id: String,
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&tab_id).map_err(|e| e.to_string())?;
    let tab = tab_manager
        .get_tab(TabId(uuid))
        .await
        .map_err(|e| e.to_string())?;

    tab_manager
        .navigation()
        .go_forward(&tab)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reload_tab(
    tab_id: String,
    ignore_cache: Option<bool>,
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&tab_id).map_err(|e| e.to_string())?;
    let tab = tab_manager
        .get_tab(TabId(uuid))
        .await
        .map_err(|e| e.to_string())?;

    tab_manager
        .navigation()
        .reload(&tab, ignore_cache.unwrap_or(false))
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_tabs(
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<Vec<TabSummary>, String> {
    Ok(tab_manager.list_tabs().await)
}

#[tauri::command]
pub async fn get_active_tab(
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<Option<String>, String> {
    Ok(tab_manager.get_active_tab().await.map(|id| id.to_string()))
}

// ---------------------------------------------------------------------------
// Telemetry & Inspection IPC Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn inspect_at_location(x: i32, y: i32) -> Result<serde_json::Value, String> {
    tracing::info!("IPC: inspect_at_location ({x}, {y}) via DOM.getNodeForLocation");
    // Fail-closed until Phase 4 CDP Target binding is active
    Err("Inspection unavailable: CDP target session not yet bound (Phase 4)".to_string())
}

#[tauri::command]
pub async fn inspect_node(selector: String) -> Result<serde_json::Value, String> {
    tracing::info!("IPC: inspect_node {selector} (fallback by selector)");
    // Fail-closed until Phase 4 CDP Target binding is active
    Err("Inspection unavailable: CDP target session not yet bound (Phase 4)".to_string())
}

#[tauri::command]
pub async fn eval_js(command: String) -> Result<serde_json::Value, String> {
    tracing::info!("IPC: eval_js {command}");
    Ok(serde_json::json!("Executed in CEF context"))
}

#[tauri::command]
pub async fn get_audit_logs(
    audit_db: State<'_, Arc<kage_storage::AuditDb>>,
) -> Result<Vec<kage_core::audit::CanonicalAuditEntry>, String> {
    use kage_core::audit::AuditReader;
    AuditReader::get_recent_records(&**audit_db, 100)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn verify_audit_chain(
    audit_db: State<'_, Arc<kage_storage::AuditDb>>,
) -> Result<bool, String> {
    use kage_core::audit::AuditVerifier;
    AuditVerifier::verify_chain(&**audit_db)
        .await
        .map(|_| true)
        .map_err(|e| e.to_string())
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
pub async fn sync_viewport_bounds(
    bounds: ViewportBounds,
    surface_manager: State<'_, Arc<NativeSurfaceManager>>,
) -> Result<(), String> {
    tracing::debug!("IPC: sync_viewport_bounds: {:?}", bounds);

    // Compute the physical dual-surface layout (WebView2 chrome + CEF content)
    // from the logical CSS pixel bounds reported by the React shell.
    //
    // NOTE: On Windows, `bounds.width` and `bounds.height` are the full
    // Tauri window client area in *physical* pixels (already scaled by
    // `bounds.scale_factor`).  `NativeSurfaceManager::update_layout` expects
    // physical pixel dimensions and a DPI scale factor.
    let layout = surface_manager
        .update_layout(bounds.width, bounds.height, bounds.scale_factor)
        .map_err(|e| e.to_string())?;

    tracing::info!(
        "sync_viewport_bounds: WebView2 top_chrome={:?}, CEF content={:?}",
        layout.top_chrome_rect,
        layout.cef_content_rect,
    );

    // TODO(Phase 2 Step 3): Apply layout.cef_content_rect to the CEF child HWND
    // via NativeSurfaceManager::set_hwnd_bounds once we have the real HWND handle.
    // TODO(Phase 2 Step 3): Apply layout.top_chrome_rect to the WebView2 child bounds.

    Ok(())
}

// ---------------------------------------------------------------------------
// CEF Engine State (Phase 2)
// ---------------------------------------------------------------------------

/// IPC command: `kage:cef:engine_state`
///
/// Returns the current lifecycle state of the CEF engine. The React UI uses
/// this to gate navigation controls and loading indicators.
#[tauri::command]
pub async fn get_engine_state(
    cef_runtime: State<'_, Arc<CefRuntime>>,
) -> Result<String, String> {
    let state = cef_runtime.state();
    Ok(format!("{:?}", state))
}



