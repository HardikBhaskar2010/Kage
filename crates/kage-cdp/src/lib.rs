//! `kage-cdp` — Authenticated loopback CDP broker, client, and session multiplexer.
//!
//! # Responsibilities & Architecture (Phase 4 — Developer Plane)
//!
//! * Establishes and manages the loopback WebSocket connection to CEF's remote debugging port.
//! * Authenticates handshakes using an ephemeral session nonce (`x-kage-session-nonce`).
//! * Strictly rejects non-loopback bindings (e.g. `0.0.0.0`) to guarantee security.
//! * Multiplexes multiple named CDP sessions (`"devtools"`, `"context"`, `"toolbus"`).
//! * Exposes a typed, zero-stub request-response client with monotonic request ID sequencing.
//! * Maps CDP `TargetId`s to KAGE `TabId`s without mutating or replacing authoritative browser identity.
//!
//! # Security Boundary (INV-07)
//!
//! The Rust CDP gateway is the **only** component that may speak to the raw CDP
//! WebSocket. React UI panels and the AI subsystem route through typed Tauri IPC
//! and the KAGE ToolBus — never directly to CDP.

pub mod broker;
pub mod client;
pub mod events;
pub mod target_router;

pub use broker::{CdpBroker, CdpConnectionDescriptor, CdpError, SessionId, NONCE_HEADER};
pub use client::CdpClient;
pub use events::{CdpEvent, CdpRequest, CdpResponse, CdpResponseError, TargetInfo};
pub use target_router::{TargetRouter, TargetSessionInfo};
