//! `kage-context` — Context Engine: relevance scoring, secret scrubbing, and token budgeting.
//!
//! # Responsibilities (KAGE-CTX-001)
//!
//! 1. **Scrubber** — strips secrets from all web-sourced strings before they reach the LLM.
//! 2. **Budget** — enforces the 4,000-token baseline context cap.
//! 3. **Pack** — assembles the final context packet (DOM snapshot + console errors +
//!    network summary) in priority order, truncating to budget.
//!
//! # Prompt injection defence
//!
//! All web-sourced strings are wrapped in `<webpage_data>` / `</webpage_data>`
//! delimiters before being included in a context pack.  The LLM system prompt
//! instructs the model to treat delimited content as **untrusted external data**,
//! never as instructions.

pub mod scrubber;
pub mod budget;
pub mod pack;

pub use pack::{ContextPack, ContextPackError};
pub use budget::TokenBudget;
