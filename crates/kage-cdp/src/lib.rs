//! `kage-cdp` — Authenticated loopback CDP broker.
//!
//! # Responsibilities (KAGE-ARCH-001)
//!
//! * Establishes and holds the WebSocket connection to CEF's remote debugging port.
//! * Authenticates the connection with an ephemeral session nonce generated at startup.
//! * Multiplexes multiple named CDP sessions (DevTools, Context Engine, Tool Bus).
//! * Exposes a typed event subscription API for DOM, Network, and Console domains.
//!
//! # Security boundary
//!
//! The Rust CDP gateway is the **only** component that may speak to the raw CDP
//! WebSocket.  React UI panels and the AI subsystem must route through the
//! [`CdpBroker`] — never open their own debugging connections.

pub mod broker;
pub mod client;
pub mod events;

pub use broker::{CdpBroker, CdpError, SessionId};
pub use events::CdpEvent;
