//! `kage-plugin` — Sandboxed Wasm plugin runtime and capability broker.
//!
//! # Architecture (KAGE-PLAT-002)
//!
//! Plugins execute inside an isolated Wasm sandbox. All interaction with the
//! browser, network, or host passes through the [`CapabilityBroker`].
//!
//! Manifest permissions are requested capabilities, not direct authorizations.

use serde::{Deserialize, Serialize};

/// Capability requested by a Wasm plugin manifest.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginCapability {
    #[serde(rename = "network:read")]
    NetworkRead,
    #[serde(rename = "dom:inspect")]
    DomInspect,
    #[serde(rename = "console:read")]
    ConsoleRead,
    #[serde(rename = "custom")]
    Custom(String),
}

/// Metadata and requested permissions declared in `plugin.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<PluginCapability>,
}
