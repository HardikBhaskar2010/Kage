//! Typed CDP event vocabulary for the domains used by KAGE.
//!
//! Handles serialization and deserialization of CDP requests, responses, and domain events.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Typed CDP events relevant to KAGE subsystems.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "method", content = "params")]
pub enum CdpEvent {
    /// `DOM.documentUpdated` — the document root has changed.
    #[serde(rename = "DOM.documentUpdated")]
    DomDocumentUpdated {},

    /// `DOM.childNodeInserted` — child node inserted into DOM tree.
    #[serde(rename = "DOM.childNodeInserted")]
    DomChildNodeInserted {
        parent_node_id: i64,
        node: Value,
    },

    /// `Network.requestWillBeSent` — a new network request is about to be sent.
    #[serde(rename = "Network.requestWillBeSent")]
    NetworkRequestWillBeSent {
        request_id: String,
        url: String,
        method: String,
    },

    /// `Network.responseReceived` — a response has been received.
    #[serde(rename = "Network.responseReceived")]
    NetworkResponseReceived {
        request_id: String,
        status: i32,
        url: String,
    },

    /// `Console.messageAdded` — a new console message.
    #[serde(rename = "Console.messageAdded")]
    ConsoleMessageAdded {
        level: String,
        text: String,
    },

    /// `Page.loadEventFired` — page load event fired.
    #[serde(rename = "Page.loadEventFired")]
    PageLoadEventFired {
        timestamp: f64,
    },

    /// `Runtime.consoleAPICalled` — console API called in page.
    #[serde(rename = "Runtime.consoleAPICalled")]
    RuntimeConsoleApiCalled {
        #[serde(rename = "type")]
        console_type: String,
        #[serde(default)]
        args: Vec<Value>,
    },

    /// `Page.frameNavigated` — frame navigated.
    #[serde(rename = "Page.frameNavigated")]
    PageFrameNavigated {
        #[serde(default)]
        frame: Value,
    },

    /// `Target.targetCreated` — target created.
    #[serde(rename = "Target.targetCreated")]
    TargetCreated {
        target_info: TargetInfo,
    },

    /// `Target.targetDestroyed` — target destroyed.
    #[serde(rename = "Target.targetDestroyed")]
    TargetDestroyed {
        target_id: String,
    },

    /// `DOM.setChildNodes` — children set for parent node.
    #[serde(rename = "DOM.setChildNodes")]
    DomSetChildNodes {
        #[serde(rename = "parentId")]
        parent_id: i64,
        nodes: Vec<Value>,
    },

    /// `DOM.childNodeRemoved` — child node removed from parent.
    #[serde(rename = "DOM.childNodeRemoved")]
    DomChildNodeRemoved {
        #[serde(rename = "parentNodeId")]
        parent_node_id: i64,
        #[serde(rename = "nodeId")]
        node_id: i64,
    },

    /// `DOM.attributeModified` — element attribute modified.
    #[serde(rename = "DOM.attributeModified")]
    DomAttributeModified {
        #[serde(rename = "nodeId")]
        node_id: i64,
        name: String,
        value: String,
    },

    /// `DOM.attributeRemoved` — element attribute removed.
    #[serde(rename = "DOM.attributeRemoved")]
    DomAttributeRemoved {
        #[serde(rename = "nodeId")]
        node_id: i64,
        name: String,
    },

    /// `Network.loadingFinished` — network request completed.
    #[serde(rename = "Network.loadingFinished")]
    NetworkLoadingFinished {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "timestamp", default)]
        timestamp: Option<f64>,
    },

    /// `Network.loadingFailed` — network request failed.
    #[serde(rename = "Network.loadingFailed")]
    NetworkLoadingFailed {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "errorText")]
        error_text: String,
        #[serde(rename = "canceled", default)]
        canceled: Option<bool>,
    },

    /// `Runtime.exceptionThrown` — unhandled exception in V8 context.
    #[serde(rename = "Runtime.exceptionThrown")]
    RuntimeExceptionThrown {
        #[serde(rename = "timestamp")]
        timestamp: f64,
        #[serde(rename = "exceptionDetails")]
        exception_details: Value,
    },

    /// `Log.entryAdded` — logging entry added.
    #[serde(rename = "Log.entryAdded")]
    LogEntryAdded {
        entry: Value,
    },

    /// `Target.attachedToTarget` — attached to target session.
    #[serde(rename = "Target.attachedToTarget")]
    AttachedToTarget {
        session_id: String,
        target_info: TargetInfo,
        #[serde(default)]
        waiting_for_debugger: bool,
    },

    /// `Target.detachedFromTarget` — detached from target session.
    #[serde(rename = "Target.detachedFromTarget")]
    DetachedFromTarget {
        session_id: String,
        #[serde(default)]
        target_id: Option<String>,
    },

    /// Fallback for unhandled or arbitrary Chromium domain events.
    #[serde(other)]
    Unknown,
}

/// Information about a Chromium CDP target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetInfo {
    #[serde(rename = "targetId")]
    pub target_id: String,
    #[serde(rename = "type")]
    pub target_type: String,
    pub title: String,
    pub url: String,
    pub attached: bool,
    #[serde(rename = "browserContextId", skip_serializing_if = "Option::is_none")]
    pub browser_context_id: Option<String>,
}

/// Outgoing request sent over CDP WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CdpRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Incoming response received from CDP WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CdpResponse {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CdpResponseError>,
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Error detail in a CDP response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CdpResponseError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl std::fmt::Display for CdpResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CDP error {}: {}", self.code, self.message)
    }
}
