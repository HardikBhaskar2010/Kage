//! Governed tools implemented in the host integration layer.

pub mod browser;
pub mod common;
pub mod dom;
pub mod download;
pub mod network;
pub mod page_interaction;
pub mod page_observation;
pub mod runtime;
pub mod runtime_evaluate;
pub mod storage;
pub mod tab;

pub use browser::*;
pub use common::{resolve_active_tab, resolve_tab_or_active};
pub use dom::*;
pub use download::*;
pub use network::*;
pub use page_interaction::*;
pub use page_observation::*;
pub use runtime::*;
pub use runtime_evaluate::RuntimeEvaluateTool;
pub use storage::*;
pub use tab::*;

use std::sync::Arc;
use serde_json::json;
use kage_browser::TabManager;
use kage_core::bus::ToolBus;
use kage_core::policy::PermissionTier;
use kage_core::registry::{ToolCategory, ToolMetadata};
use crate::cdp_session::CdpSessionManager;

/// Register all canonical Phase 8 agent tools into the central ToolBus.
pub async fn register_canonical_tools(
    tool_bus: &ToolBus,
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
) {
    // 1. Browser Lifecycle & Navigation
    tool_bus.register_with_metadata(
        BrowserNavigateTool::new(tab_manager.clone(), session_manager.clone()),
        BrowserNavigateTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        BrowserReloadTool::new(tab_manager.clone(), session_manager.clone()),
        BrowserReloadTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        BrowserGoBackTool::new(tab_manager.clone(), session_manager.clone()),
        BrowserGoBackTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        BrowserGoForwardTool::new(tab_manager.clone(), session_manager.clone()),
        BrowserGoForwardTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        BrowserStopTool::new(tab_manager.clone(), session_manager.clone()),
        BrowserStopTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        BrowserWaitForNavigationTool::new(tab_manager.clone(), session_manager.clone()),
        BrowserWaitForNavigationTool::metadata(),
    ).await;

    // 2. Tab Management (INV-10 explicit profile binding)
    tool_bus.register_with_metadata(
        TabCreateTool::new(tab_manager.clone()),
        TabCreateTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        TabCloseTool::new(tab_manager.clone()),
        TabCloseTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        TabSwitchTool::new(tab_manager.clone()),
        TabSwitchTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        TabFocusTool::new(tab_manager.clone(), session_manager.clone()),
        TabFocusTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        TabListTool::new(tab_manager.clone()),
        TabListTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        TabGetStateTool::new(tab_manager.clone()),
        TabGetStateTool::metadata(),
    ).await;

    // 3. Page Interaction (Synthetic CDP events)
    tool_bus.register_with_metadata(
        PageClickTool::new(tab_manager.clone(), session_manager.clone()),
        PageClickTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        PageTypeTool::new(tab_manager.clone(), session_manager.clone()),
        PageTypeTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        PageFillTool::new(tab_manager.clone(), session_manager.clone()),
        PageFillTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        PageSelectTool::new(tab_manager.clone(), session_manager.clone()),
        PageSelectTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        PagePressKeyTool::new(tab_manager.clone(), session_manager.clone()),
        PagePressKeyTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        PageSubmitTool::new(tab_manager.clone(), session_manager.clone()),
        PageSubmitTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        PageScrollTool::new(tab_manager.clone(), session_manager.clone()),
        PageScrollTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        PageHoverTool::new(tab_manager.clone(), session_manager.clone()),
        PageHoverTool::metadata(),
    ).await;

    // 4. Page Observation
    tool_bus.register_with_metadata(
        PageGetTextTool::new(tab_manager.clone(), session_manager.clone()),
        PageGetTextTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        PageGetElementTool::new(tab_manager.clone(), session_manager.clone()),
        PageGetElementTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        PageGetLinksTool::new(tab_manager.clone(), session_manager.clone()),
        PageGetLinksTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        PageGetFormsTool::new(tab_manager.clone(), session_manager.clone()),
        PageGetFormsTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        PageGetSemanticSnapshotTool::new(tab_manager.clone(), session_manager.clone()),
        PageGetSemanticSnapshotTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        PageGetAccessibilityTreeTool::new(tab_manager.clone(), session_manager.clone()),
        PageGetAccessibilityTreeTool::metadata(),
    ).await;

    // 5. Download Management
    tool_bus.register_with_metadata(
        DownloadListTool::new(),
        DownloadListTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        DownloadStartTool::new(tab_manager.clone(), session_manager.clone()),
        DownloadStartTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        DownloadWaitTool::new(),
        DownloadWaitTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        DownloadCancelTool::new(),
        DownloadCancelTool::metadata(),
    ).await;
}

/// Register all Phase 6 developer intelligence tools into the central ToolBus.
///
/// NOTE: `devtools.runtime.evaluate` is strictly registered as a developer-only capability
/// and cannot be executed by autonomous agent planners without Tier 3 escalation (GATE-08-H).
pub async fn register_developer_tools(
    tool_bus: &ToolBus,
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
) {
    // 1. Runtime Tools
    let eval_meta = ToolMetadata::new(
        "devtools.runtime.evaluate",
        ToolCategory::Developer,
        PermissionTier::StateMutating,
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "expression": { "type": "string" }
            },
            "required": ["tab_id", "expression"]
        }),
    )
    .with_description("Interactive DevTools JavaScript REPL evaluation (Developer Only)")
    .developer_only();

    tool_bus.register_with_metadata(
        RuntimeEvaluateTool::new(tab_manager.clone(), session_manager.clone()),
        eval_meta,
    ).await;

    tool_bus.register(RuntimeGetPropertiesTool::new(tab_manager.clone(), session_manager.clone())).await;
    tool_bus.register(RuntimeCallFunctionTool::new(tab_manager.clone(), session_manager.clone())).await;
    tool_bus.register(RuntimeAwaitPromiseTool::new(tab_manager.clone(), session_manager.clone())).await;

    // 2. DOM Tools
    tool_bus.register(DomGetDocumentTool::new(tab_manager.clone(), session_manager.clone())).await;
    tool_bus.register(DomQuerySelectorTool::new(tab_manager.clone(), session_manager.clone())).await;
    tool_bus.register(DomQuerySelectorAllTool::new(tab_manager.clone(), session_manager.clone())).await;
    tool_bus.register(DomGetOuterHtmlTool::new(tab_manager.clone(), session_manager.clone())).await;
    tool_bus.register(DomGetAttributesTool::new(tab_manager.clone(), session_manager.clone())).await;
    tool_bus.register(DomGetBoundsTool::new(tab_manager.clone(), session_manager.clone())).await;

    // 3. Storage Tools
    tool_bus.register(StorageGetCookiesTool::new(tab_manager.clone(), session_manager.clone())).await;
    tool_bus.register(StorageGetLocalStorageTool::new(tab_manager.clone(), session_manager.clone())).await;
    tool_bus.register(StorageGetSessionStorageTool::new(tab_manager.clone(), session_manager.clone())).await;

    // 4. Network Tools
    tool_bus.register(NetworkGetResponseBodyTool::new(tab_manager.clone(), session_manager.clone())).await;
}
