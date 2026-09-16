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
use tokio_util::sync::CancellationToken;

use kage_core::{ToolBus, ToolRequest};
use kage_core::bus::PartialPolicyContext;
use kage_cdp::CdpBroker;

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
// kage:cdp:get_nonce
// ---------------------------------------------------------------------------

/// IPC command: `kage:cdp:get_nonce`
///
/// Returns the ephemeral session nonce so that privileged internal components
/// (DevTools panel, Context Engine) can authenticate their CDP WebSocket connections.
///
/// **This value must never be forwarded to web page content.**
#[tauri::command]
pub fn get_cdp_nonce(broker: State<'_, Arc<CdpBroker>>) -> String {
    broker.nonce().to_string()
}
