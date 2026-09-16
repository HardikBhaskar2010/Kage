# AGENTS.md — KAGE Autonomous Agent Operating Guidelines

**Project:** KAGE (影) — Developer-First Autonomous Browser  
**Architecture Version:** v0.2.1 · **Target Milestone:** v1.0.0 MVP  
**Specification Hub:** [docs/README.md](docs/README.md) · **Design Document:** [docs/Design.md](docs/Design.md)

---

## 1. The Prime Architectural Invariant

All agents operating in this repository must uphold this inviolable rule across every file edit, tool creation, or architectural implementation:

```
╔═════════════════════════════════════════════════════════════════════════════════════╗
║                            PRIME ARCHITECTURAL INVARIANT                            ║
║                                                                                     ║
║        AI NEVER GETS BROWSER AUTHORITY DIRECTLY. EVERY AI ACTION BECOMES            ║
║        A TYPED TOOL BUS REQUEST, AND THE TOOL BUS—NOT THE MODEL, PROMPT,            ║
║        PLUGIN, OR UI—OWNS VALIDATION, AUTHORIZATION, EXECUTION,                     ║
║        CANCELLATION, AND AUDITING.                                                  ║
╚═════════════════════════════════════════════════════════════════════════════════════╝
```

* **Never** allow the AI agent, LLM prompts, UI panels, or plugins to directly call CEF, CDP, or filesystem APIs.
* **Always** dispatch requests through the Rust `ToolBus` (`kage-core`), which enforces the 4-tier permission policy, validates schemas, generates cancellation tokens, and writes to the immutable `security_audit.db`.

---

## 2. Mandatory Skill Consultation Invariant (Read Skills Before Changes)

All agents working on KAGE must adhere strictly to this pre-execution requirement:

```
╔═════════════════════════════════════════════════════════════════════════════════════╗
║                      MANDATORY SKILL CONSULTATION INVARIANT                         ║
║                                                                                     ║
║        AGENTS MUST READ AND CONSULT RELEVANT SKILLS IN `.agents/skills/`            ║
║        BEFORE MAKING ANY CODE CHANGES IN THE CODEBASE.                              ║
╚═════════════════════════════════════════════════════════════════════════════════════╝
```

* **Inspect Skills Before Coding:** Before modifying, creating, or refactoring any code in this repository, agents **MUST read the relevant skill documentation** in `.agents/skills/`:
  * **UI Craft & Polish:** Read [`.agents/skills/emil-design-eng/SKILL.md`](file:///c:/Users/sneha/Videos/Kage/.agents/skills/emil-design-eng/SKILL.md) (Emil Kowalski's standards on layout finesse, micro-interactions, click states, and invisible details).
  * **Motion & Physics:** Read [`.agents/skills/animate/SKILL.md`](file:///c:/Users/sneha/Videos/Kage/.agents/skills/animate/SKILL.md) and [`.agents/skills/apple-design/SKILL.md`](file:///c:/Users/sneha/Videos/Kage/.agents/skills/apple-design/SKILL.md) (fluid spring dynamics, interruptible gestures, strict duration envelopes, no `transition: all`).
  * **Design Systems & Tokens:** Read [`.agents/skills/design-systems/SKILL.md`](file:///c:/Users/sneha/Videos/Kage/.agents/skills/design-systems/SKILL.md) and [`.agents/skills/ui-design/SKILL.md`](file:///c:/Users/sneha/Videos/Kage/.agents/skills/ui-design/SKILL.md).
  * **Security & OWASP Defense:** Read [`.agents/skills/security-owasp/SKILL.md`](file:///c:/Users/sneha/Videos/Kage/.agents/skills/security-owasp/SKILL.md) and [`.agents/skills/prompt-injection-defense/SKILL.md`](file:///c:/Users/sneha/Videos/Kage/.agents/skills/prompt-injection-defense/SKILL.md).
  * **Testing & Quality Architecture:** Read [`.agents/skills/test-architect/SKILL.md`](file:///c:/Users/sneha/Videos/Kage/.agents/skills/test-architect/SKILL.md).
* **Zero Sloppy / Generic Code:** No ad-hoc styles, raw placeholders, or uninspired solutions are permitted when an installed skill provides the canonical conventions, easing formulas, and quality bars.

---

## 3. Global Runtime Domain Boundaries

KAGE divides responsibilities across four distinct layers. Never blur these lines:

1. **CEF Runtime (C++ / Chromium):**
   * Handles web standards, HTML/CSS layout, V8 JS execution, network stack, GPU rendering, and web sandbox.
   * Do not reimplement Chromium capabilities.

2. **Host Process (Tauri 2.x / Rust):**
   * Owns native window handles, CEF message pump / lifecycle, SQLite databases (`kage_data.db`, `security_audit.db`), WebSocket CDP client, Context Engine sanitization, and the central Tool Bus.
   * All state mutation and permission checks happen in Rust.

3. **UI Chrome (React 18+ / TypeScript):**
   * Owns the user interface: tab strip, omnibox, DevTools panels, AI sidebar, Micro Inspect overlay, and workspaces.
   * Communicates with Rust exclusively via typed IPC commands and events defined in [docs/02-architecture/IPC_Protocol.md](docs/02-architecture/IPC_Protocol.md).

4. **CDP Gateway:**
   * Rust-managed WebSocket connection to CEF’s remote debugging port. Multiplexes CDP sessions for DevTools, Context Engine, and Tool Bus drivers.

---

## 4. Data & Security Rules (Untrusted Webpage Boundary)

* **Untrusted by Default:** All content retrieved from the web (DOM strings, attributes, console messages, network request/response headers and bodies) is **untrusted external data**, never instructions.
* **Prompt Injection Defense:** Web content must be wrapped in strict data delimiters (`<webpage_data>` / `<untrusted_content>`) before reaching an LLM. Never concatenate raw page text directly into system instructions.
* **Secret Redaction:** Before any context packet is assembled, the Context Engine must redact Bearer tokens, API keys, passwords, cookie values, and session IDs using the sanitization rules in [docs/03-core/Context_Engine.md](docs/03-core/Context_Engine.md).
* **Permission Tiers:** Enforce the 4-tier model from [docs/08-security/Security_Model.md](docs/08-security/Security_Model.md):
  * **Tier 1 (Read-Only Passive):** Auto-allowed (e.g., inspect DOM, read console logs).
  * **Tier 2 (State-Mutating Low Risk):** Session-scoped approval (e.g., click element, fill input).
  * **Tier 3 (External / High Risk):** Per-action modal confirmation (e.g., network replay, file download/export).
  * **Tier 4 (Dangerous / Blocked):** Strictly forbidden for AI and unprivileged plugins (e.g., bypass cert errors, modify browser binaries).

---

## 5. Coding & Implementation Standards

### Rust (Host Core & Subsystems)
* Use `thiserror` for domain-specific errors (e.g., `ToolError`, `CdpError`, `StorageError`) and `anyhow` for top-level application boundaries.
* Never use raw `.unwrap()` or `.expect()` on fallible external inputs, IPC messages, or CEF callbacks. Return typed errors with contextual detail.
* All database queries must use parameterized statements in `rusqlite` to prevent SQL injection.
* Support graceful cancellation across all asynchronous operations via `tokio_util::sync::CancellationToken`.

### TypeScript / React (UI Chrome)
* Strict mode enabled (`strict: true`, `noImplicitAny: true`).
* No `any` types. Define explicit types matching the IPC contracts in `docs/02-architecture/IPC_Protocol.md`.
* UI components must use the design tokens from [docs/09-ui/Design_System.md](docs/09-ui/Design_System.md) (Liquid Glass theme, peach-to-burgundy gradient `#F9DBBD` → `#450920`, Inter for chrome, JetBrains Mono for technical data).
* Never use static placeholder data when real data flow is required.

### Context Engine & AI Operations
* Respect the baseline token budget: **4,000 tokens** for general context packs.
* Prioritize element relevance: Active focused node, interactive elements, viewport-visible nodes, recent console errors.
* Agent loop is bounded to a **maximum of 10 autonomous steps** per user query. Loops exceeding 10 steps or repeating identical tool calls must halt and ask for user guidance.

---

## 6. Performance Budgets

Agents must verify that implementations stay within the performance envelopes specified in [docs/02-architecture/Performance_Benchmarks.md](docs/02-architecture/Performance_Benchmarks.md):

* **Idle RAM (Clean Start, 1 Tab):** < 150 MB (Host + Chrome UI).
* **UI Compositor Frame Rate:** 60 FPS minimum (120 FPS target on ProMotion/high-refresh displays).
* **Tool Bus Dispatch Latency:** < 100 ms from UI invocation to CDP execution start.
* **Context Pack Assembly Time:** < 50 ms for DOM + Console + Network snapshot.

---

## 7. Specification Directory Reference

Before creating or modifying any feature, consult the authoritative specification:

* **Architecture & Runtime:** [docs/02-architecture/Architecture.md](docs/02-architecture/Architecture.md)
* **CEF Integration:** [docs/02-architecture/CEF_Integration.md](docs/02-architecture/CEF_Integration.md)
* **Tauri Host:** [docs/02-architecture/Tauri_Architecture.md](docs/02-architecture/Tauri_Architecture.md)
* **Browser Shell:** [docs/02-architecture/Browser_Shell.md](docs/02-architecture/Browser_Shell.md)
* **IPC Protocol:** [docs/02-architecture/IPC_Protocol.md](docs/02-architecture/IPC_Protocol.md)
* **Tool System:** [docs/03-core/Tool_System.md](docs/03-core/Tool_System.md)
* **Context Engine:** [docs/03-core/Context_Engine.md](docs/03-core/Context_Engine.md)
* **AI Subsystem:** [docs/04-ai/AI_Architecture.md](docs/04-ai/AI_Architecture.md)
* **DevTools:** [docs/05-devtools/DevTools_Architecture.md](docs/05-devtools/DevTools_Architecture.md)
* **Testing Lab & Strategy:** [docs/06-testing/Testing_Lab.md](docs/06-testing/Testing_Lab.md) & [docs/06-testing/Testing_Strategy.md](docs/06-testing/Testing_Strategy.md)
* **Data Model (SQLite):** [docs/07-platform/Data_Model.md](docs/07-platform/Data_Model.md)
* **Plugin API (Wasm):** [docs/07-platform/Plugin_API.md](docs/07-platform/Plugin_API.md)
* **Security & Threat Model:** [docs/08-security/Security_Model.md](docs/08-security/Security_Model.md)
* **Design System (Liquid Glass):** [docs/09-ui/Design_System.md](docs/09-ui/Design_System.md)
