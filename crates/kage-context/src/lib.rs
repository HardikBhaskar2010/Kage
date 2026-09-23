//! `kage-context` — Context Engine: relevance scoring, secret scrubbing, and token budgeting.
//!
//! # Responsibilities (KAGE-CTX-001 / KAGE-CTX-002 / KAGE-CTX-003)
//!
//! 1. **Telemetry Buffers** (`telemetry`) — Bounded ring buffers for console logs, network requests,
//!    and DOM tree cache with deduplication and binary media stripping.
//! 2. **Pruner** (`pruner`) — 4-stage DOM pruning pipeline (focus subtree, structural stripping,
//!    accessibility projection, and repetition collapsing).
//! 3. **Scrubber** (`scrubber`) — strips secrets from all web-sourced strings and enforces
//!    `<untrusted_web_content>` XML framing to protect against Indirect Prompt Injection.
//! 4. **Budget** (`budget`) — enforces dynamic token budgets (1,500 for micro inspect, 4,000 baseline).
//! 5. **Pack** (`pack`) — assembles `BrowserObservation` and `ContextPack`.

pub mod budget;
pub mod pack;
pub mod pruner;
pub mod scrubber;
pub mod telemetry;

pub use budget::{TokenBudget, DEFAULT_BUDGET};
pub use pack::{
    BrowserObservation, ContextPack, ContextPackAssembler, ContextPackError, ContextSection,
    ContextSignals, BUDGET_DEFAULT, BUDGET_ERROR_DIAGNOSIS, BUDGET_FULL_PAGE, BUDGET_MICRO_INSPECT,
};
pub use pruner::{DomPruner, DomPrunerConfig, DomPruningReport};
pub use scrubber::{ContextScrubber, UNTRUSTED_CLOSE_TAG, UNTRUSTED_OPEN_TAG};
pub use telemetry::{
    ConsoleEntry, ConsoleLogLevel, ConsoleRingBuffer, DomNode, DomTreeStore, NetworkEntry,
    NetworkRingBuffer, CONSOLE_RING_CAPACITY, MAX_PREVIEW_BYTES, NETWORK_RING_CAPACITY,
};
