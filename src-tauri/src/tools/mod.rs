//! Governed tools implemented in the host integration layer.

pub mod common;
pub mod dom;
pub mod network;
pub mod runtime;
pub mod runtime_evaluate;
pub mod storage;

pub use common::resolve_active_tab;
pub use dom::*;
pub use network::*;
pub use runtime::*;
pub use runtime_evaluate::RuntimeEvaluateTool;
pub use storage::*;

use std::sync::Arc;
use kage_browser::TabManager;
use kage_core::bus::ToolBus;
use crate::cdp_session::CdpSessionManager;

/// Register all Phase 6 developer intelligence tools into the central ToolBus.
pub async fn register_developer_tools(
    tool_bus: &ToolBus,
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
) {
    // 1. Runtime Tools
    tool_bus.register(RuntimeEvaluateTool::new(tab_manager.clone(), session_manager.clone())).await;
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
