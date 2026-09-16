use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "method", content = "params")]
pub enum CdpEvent {
    #[serde(rename = "DOM.documentUpdated")]
    DomDocumentUpdated {},

    #[serde(rename = "DOM.childNodeInserted")]
    DomChildNodeInserted { parent_node_id: i64, node: Value },

    #[serde(rename = "Network.requestWillBeSent")]
    NetworkRequestWillBeSent {
        request_id: String,
        url: String,
        method: String,
    },

    #[serde(rename = "Network.responseReceived")]
    NetworkResponseReceived {
        request_id: String,
        status: i32,
        url: String,
    },

    #[serde(rename = "Console.messageAdded")]
    ConsoleMessageAdded { level: String, text: String },

    #[serde(rename = "Page.loadEventFired")]
    PageLoadEventFired { timestamp: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpRequest {
    pub id: u64,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpResponse {
    pub id: u64,
    pub result: Option<Value>,
    pub error: Option<Value>,
}
