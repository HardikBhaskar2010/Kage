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
use crate::HostWindowHandle;

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
    pub profile_id: String,
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
    url: Option<String>,
    title: Option<String>,
    profile_id: Option<String>,
    payload: Option<CreateTabPayload>,
    tab_manager: State<'_, Arc<TabManager>>,
    cef_runtime: State<'_, Arc<CefRuntime>>,
    surface_manager: State<'_, Arc<NativeSurfaceManager>>,
    host_hwnd: State<'_, HostWindowHandle>,
    bridge: State<'_, Arc<kage_browser::CefTabBridge>>,
) -> Result<TabInfo, String> {
    let target_url = url
        .or_else(|| payload.as_ref().and_then(|p| p.url.clone()))
        .unwrap_or_else(|| "https://example.com".to_string());
    let display_title = title
        .or_else(|| payload.as_ref().and_then(|p| p.title.clone()))
        .unwrap_or_else(|| "New Tab".to_string());
    let profile = profile_id
        .or_else(|| payload.as_ref().and_then(|p| p.profile_id.clone()))
        .map(ProfileId::new)
        .unwrap_or_else(ProfileId::personal);

    let profile_id_str = profile.0.clone();
    let tab_id = tab_manager
        .create_tab(profile, &target_url)
        .await
        .map_err(|e| e.to_string())?;

    let surface_id = kage_browser::BrowserSurfaceId::new();
    let _ = tab_manager.bind_browser_surface(tab_id, surface_id).await;

    // Register pending tab with bridge so that on_after_created binds the real CEF browser ID
    bridge.register_pending_tab(tab_id, &tab_manager);

    // If host HWND is available and layout exists, dispatch CEF browser creation
    let parent_hwnd = host_hwnd.0.load(std::sync::atomic::Ordering::SeqCst);
    if parent_hwnd != 0 {
        let content_rect = surface_manager
            .last_layout()
            .map(|l| l.cef_content_rect)
            .unwrap_or_else(|| kage_engine::ViewportRect::new(320, 88, 1120, 812));

        tracing::info!(
            "IPC create_tab: Creating CEF child browser in HWND {} at {:?} for {}",
            parent_hwnd, content_rect, target_url
        );
        if let Err(e) = cef_runtime.create_browser(parent_hwnd, &content_rect, &target_url) {
            tracing::error!("IPC create_tab: Failed to create child CEF browser: {e}");
        }
    }

    Ok(TabInfo {
        id: tab_id.to_string(),
        url: target_url.clone(),
        title: display_title,
        favicon: Some(if target_url.is_empty() { "kage".into() } else { "globe".into() }),
        is_loading: !target_url.is_empty(),
        can_go_back: false,
        can_go_forward: false,
        is_secure: target_url.starts_with("https://") || target_url.is_empty(),
        profile_id: profile_id_str,
    })
}

#[tauri::command]
pub async fn close_tab(
    tab_id: String,
    tab_manager: State<'_, Arc<TabManager>>,
    cef_runtime: State<'_, Arc<CefRuntime>>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&tab_id).map_err(|e| e.to_string())?;
    let tid = TabId(uuid);

    if let Some(browser_id) = tab_manager.browser_id_for_tab(tid).await {
        let _ = cef_runtime.request_close_browser_by_id(browser_id, false);
    }

    tab_manager
        .close_tab(tid)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_tab(
    tab_id: String,
    tab_manager: State<'_, Arc<TabManager>>,
    cef_runtime: State<'_, Arc<CefRuntime>>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&tab_id).map_err(|e| e.to_string())?;
    let tid = TabId(uuid);

    let prev_tab_id = tab_manager.get_active_tab().await;
    if let Some(prev_id) = prev_tab_id {
        if prev_id != tid {
            if let Some(prev_browser_id) = tab_manager.browser_id_for_tab(prev_id).await {
                #[cfg(windows)]
                let _ = cef_runtime.set_browser_visible_by_id(prev_browser_id, false);
            }
        }
    }

    tab_manager
        .switch_tab(tid)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(new_browser_id) = tab_manager.browser_id_for_tab(tid).await {
        #[cfg(windows)]
        let _ = cef_runtime.set_browser_visible_by_id(new_browser_id, true);
    }

    Ok(())
}

#[tauri::command]
pub async fn navigate_to(
    tab_id: String,
    url: String,
    tab_manager: State<'_, Arc<TabManager>>,
    cef_runtime: State<'_, Arc<CefRuntime>>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&tab_id).map_err(|e| e.to_string())?;
    let tid = TabId(uuid);
    let tab = tab_manager
        .get_tab(tid)
        .await
        .map_err(|e| e.to_string())?;

    tab_manager
        .navigation()
        .navigate(&tab, &url, kage_browser::NavigationSource::Programmatic)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(browser_id) = tab_manager.browser_id_for_tab(tid).await {
        cef_runtime
            .load_url_by_browser_id(browser_id, &url)
            .map_err(|e| e.to_string())?;
    } else {
        tracing::warn!("navigate_to: tab {tid} has no bound CEF browser ID yet");
    }

    Ok(())
}

#[tauri::command]
pub async fn go_back(
    tab_id: String,
    tab_manager: State<'_, Arc<TabManager>>,
    cef_runtime: State<'_, Arc<CefRuntime>>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&tab_id).map_err(|e| e.to_string())?;
    let tid = TabId(uuid);
    let tab = tab_manager
        .get_tab(tid)
        .await
        .map_err(|e| e.to_string())?;

    let _ = tab_manager
        .navigation()
        .go_back(&tab)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(browser_id) = tab_manager.browser_id_for_tab(tid).await {
        let _ = cef_runtime.go_back_by_browser_id(browser_id);
    }

    Ok(())
}

#[tauri::command]
pub async fn go_forward(
    tab_id: String,
    tab_manager: State<'_, Arc<TabManager>>,
    cef_runtime: State<'_, Arc<CefRuntime>>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&tab_id).map_err(|e| e.to_string())?;
    let tid = TabId(uuid);
    let tab = tab_manager
        .get_tab(tid)
        .await
        .map_err(|e| e.to_string())?;

    let _ = tab_manager
        .navigation()
        .go_forward(&tab)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(browser_id) = tab_manager.browser_id_for_tab(tid).await {
        let _ = cef_runtime.go_forward_by_browser_id(browser_id);
    }

    Ok(())
}

#[tauri::command]
pub async fn reload_tab(
    tab_id: String,
    ignore_cache: Option<bool>,
    tab_manager: State<'_, Arc<TabManager>>,
    cef_runtime: State<'_, Arc<CefRuntime>>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&tab_id).map_err(|e| e.to_string())?;
    let tid = TabId(uuid);
    let tab = tab_manager
        .get_tab(tid)
        .await
        .map_err(|e| e.to_string())?;

    let ignore = ignore_cache.unwrap_or(false);
    let _ = tab_manager
        .navigation()
        .reload(&tab, ignore)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(browser_id) = tab_manager.browser_id_for_tab(tid).await {
        let _ = cef_runtime.reload_by_browser_id(browser_id, ignore);
    }

    Ok(())
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
pub async fn eval_js(
    command: String,
    tab_id: Option<String>,
    tab_manager: State<'_, Arc<TabManager>>,
    bus: State<'_, Arc<ToolBus>>,
) -> Result<serde_json::Value, String> {
    tracing::info!("IPC: eval_js routing via ToolBus: {command}");

    let target_tab_id = if let Some(id_str) = tab_id {
        Uuid::parse_str(&id_str)
            .map(TabId)
            .map_err(|e| format!("Invalid tab_id UUID: {e}"))?
    } else {
        tab_manager
            .get_active_tab()
            .await
            .ok_or_else(|| "No active tab available for eval_js".to_string())?
    };

    let tab = tab_manager.get_tab(target_tab_id).await.map_err(|e| e.to_string())?;

    // Determine authorization context from tab profile & session state
    // Per Rule 5: User profiles (Personal / Work) allow DevTools console evaluation;
    // AgentSandbox requires explicit session escalation grant.
    let (profile_id, session_granted) = {
        let ident = tab.identity.read().await;
        let is_granted = ident.profile_id != kage_browser::ProfileId::agent_sandbox();
        (ident.profile_id.0.clone(), is_granted)
    };

    let request_id = format!("req_eval_{}", Uuid::new_v4().simple());
    let request = ToolRequest {
        tool_id: "devtools.runtime.evaluate".to_string(),
        args: serde_json::json!({
            "tab_id": target_tab_id.to_string(),
            "expression": command,
            "return_by_value": true,
            "await_promise": true,
        }),
        request_id,
        reason: "DevTools V8 evaluation".to_string(),
    };

    let ctx = PartialPolicyContext::new(
        "devtools_console",
        format!("session_{profile_id}"),
        "default_workspace",
        session_granted,
    );

    let response = bus
        .dispatch(request, ctx, CancellationToken::new())
        .await
        .map_err(|e| e.to_string())?;

    Ok(response.output)
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
    cef_runtime: State<'_, Arc<CefRuntime>>,
) -> Result<(), String> {
    tracing::debug!("IPC: sync_viewport_bounds: {:?}", bounds);

    let layout = surface_manager
        .update_layout(bounds.width, bounds.height, bounds.scale_factor)
        .map_err(|e| e.to_string())?;

    tracing::info!(
        "sync_viewport_bounds: WebView2 top_chrome={:?}, CEF content={:?}",
        layout.top_chrome_rect,
        layout.cef_content_rect,
    );

    #[cfg(windows)]
    cef_runtime.resize_all_browsers(&layout.cef_content_rect);

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

// ---------------------------------------------------------------------------
// Profile & Permission Management IPC Commands (Phase 7, KAGE-SEC-004)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_profiles(
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<Vec<kage_browser::ProfileMetadata>, String> {
    Ok(tab_manager.profile_manager().list_metadata().await)
}

#[derive(Debug, Deserialize)]
pub struct CreateProfilePayload {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub color: Option<String>,
    pub icon: Option<String>,
}

#[tauri::command]
pub async fn create_profile(
    payload: CreateProfilePayload,
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<kage_browser::ProfileMetadata, String> {
    let kind = match payload.kind.to_lowercase().as_str() {
        "personal" => kage_browser::ProfileKind::Personal,
        "work" => kage_browser::ProfileKind::Work,
        "agent_sandbox" | "sandbox" => kage_browser::ProfileKind::AgentSandbox,
        _ => kage_browser::ProfileKind::Temporary,
    };
    let mut meta = kage_browser::ProfileMetadata::new(
        kage_browser::ProfileId::new(&payload.id),
        &payload.name,
        kind,
    );
    if let Some(c) = payload.color {
        meta.color = c;
    }
    if let Some(i) = payload.icon {
        meta.icon = i;
    }

    let profile = tab_manager
        .profile_manager()
        .create_profile(meta.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(profile.metadata.clone())
}

#[tauri::command]
pub async fn delete_profile(
    profile_id: String,
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<(), String> {
    let id = kage_browser::ProfileId::new(profile_id);
    tab_manager
        .profile_manager()
        .delete_profile(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_active_profile(
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<kage_browser::ProfileMetadata, String> {
    let active_id = tab_manager.get_active_tab().await;
    let profile_id = if let Some(tab_id) = active_id {
        if let Ok(tab) = tab_manager.get_tab(tab_id).await {
            let p_id = tab.identity.read().await.profile_id.clone();
            p_id
        } else {
            kage_browser::ProfileId::personal()
        }
    } else {
        kage_browser::ProfileId::personal()
    };

    let meta = tab_manager
        .profile_manager()
        .get_metadata(&profile_id)
        .await
        .unwrap_or_else(|| {
            kage_browser::ProfileMetadata::new(
                profile_id.clone(),
                &profile_id.0,
                kage_browser::ProfileKind::Personal,
            )
        });

    Ok(meta)
}

#[tauri::command]
pub async fn query_permission(
    profile_id: String,
    origin: String,
    permission_type: String,
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<String, String> {
    use std::str::FromStr;
    let p_type = kage_browser::PermissionType::from_str(&permission_type)
        .map_err(|e| e.to_string())?;
    let p_id = kage_browser::ProfileId::new(profile_id);
    let decision = tab_manager
        .permission_manager()
        .query(&p_id, &origin, p_type)
        .await;
    Ok(decision.to_string())
}

#[tauri::command]
pub async fn set_permission(
    profile_id: String,
    origin: String,
    permission_type: String,
    decision: String,
    tab_manager: State<'_, Arc<TabManager>>,
) -> Result<(), String> {
    use std::str::FromStr;
    let p_type = kage_browser::PermissionType::from_str(&permission_type)
        .map_err(|e| e.to_string())?;
    let p_dec = kage_browser::PermissionDecision::from_str(&decision)
        .map_err(|e| e.to_string())?;
    let p_id = kage_browser::ProfileId::new(profile_id);
    tab_manager
        .permission_manager()
        .set(p_id, &origin, p_type, p_dec)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
pub struct EscalateSessionPayload {
    pub tab_id: String,
    pub target_profile_id: String,
    pub reason: String,
    pub approved: bool,
}

#[tauri::command]
pub async fn escalate_session(
    payload: EscalateSessionPayload,
    tab_manager: State<'_, Arc<TabManager>>,
    audit_db: State<'_, Arc<kage_storage::AuditDb>>,
) -> Result<bool, String> {
    let tab_uuid = uuid::Uuid::parse_str(&payload.tab_id).map_err(|e| e.to_string())?;
    let tab_id = kage_browser::TabId(tab_uuid);
    let target_profile = kage_browser::ProfileId::new(&payload.target_profile_id);

    let current_profile = {
        let tab = tab_manager.get_tab(tab_id).await.map_err(|e| e.to_string())?;
        let p_id = tab.identity.read().await.profile_id.clone();
        p_id
    };

    let request_id = format!("req_esc_{}", uuid::Uuid::new_v4().simple());
    let audit_sink: Arc<dyn kage_core::audit::AuditSink> = audit_db.inner().clone();

    // 1. Two-stage fail-closed intent commitment (AuditStatus::Started)
    kage_browser::record_session_escalation_intent(
        &audit_sink,
        tab_id,
        &current_profile,
        &target_profile,
        &payload.reason,
        &request_id,
    )
    .await
    .map_err(|e| format!("Failed to record escalation intent: {e}"))?;

    // 2. If user rejected, commit Denied outcome and return false
    if !payload.approved {
        let _ = kage_browser::record_session_escalation_outcome(
            &audit_sink,
            tab_id,
            &target_profile,
            &request_id,
            false,
            5,
        )
        .await;
        return Ok(false);
    }

    // 3. User approved: escalate tab profile in TabManager
    tab_manager
        .escalate_tab_profile(tab_id, target_profile.clone())
        .await
        .map_err(|e| e.to_string())?;

    // 4. Commit Success audit log
    kage_browser::record_session_escalation_outcome(
        &audit_sink,
        tab_id,
        &target_profile,
        &request_id,
        true,
        10,
    )
    .await
    .map_err(|e| format!("Failed to commit escalation audit record: {e}"))?;

    Ok(true)
}



