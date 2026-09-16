//! Typed CDP event vocabulary for the domains used by KAGE.
//!
//! Each variant corresponds to a CDP event that the [`CdpBroker`] can receive and
//! route to subscribers.  Only events consumed by the Context Engine and Tool Bus
//! are represented here; raw CDP JSON is available via the broker's broadcast channel.

use serde::{Deserialize, Serialize};

/// Typed CDP events relevant to KAGE subsystems.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "domain", rename_all = "camelCase")]
pub enum CdpEvent {
    /// `DOM.documentUpdated` — the document root has changed.
    DomDocumentUpdated,

    /// `DOM.setChildNodes` — child nodes populated for a requested node.
    DomSetChildNodes {
        parent_id: u64,
        nodes: Vec<serde_json::Value>,
    },

    /// `Network.requestWillBeSent` — a new network request is about to be sent.
    NetworkRequestWillBeSent {
        request_id: String,
        url: String,
        method: String,
    },

    /// `Network.responseReceived` — a response has been received.
    NetworkResponseReceived {
        request_id: String,
        status: u16,
        mime_type: String,
    },

    /// `Console.messageAdded` — a new console message.
    ConsoleMessageAdded {
        level: ConsoleLevel,
        text: String,
        source: String,
    },

    /// Catch-all for unrecognized events — preserved as raw JSON.
    Unknown { method: String, params: serde_json::Value },
}

/// Console message severity levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsoleLevel {
    Log,
    Info,
    Warning,
    Error,
    Debug,
}
