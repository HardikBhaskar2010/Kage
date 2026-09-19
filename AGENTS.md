# AGENTS.md — KAGE Autonomous Agent Operating Guidelines

**Project:** KAGE (影) — Developer-First Autonomous Browser Control Plane  
**Architecture Version:** v0.3.0 · **Target Milestone:** v1.0.0 Alpha / MVP  
**Strategy Document:** [docs/research/KAGE_Browser_Control_Plane_Strategy.md](docs/research/KAGE_Browser_Control_Plane_Strategy.md)  
**Ground-Truth Audit:** [docs/KAGE_Ground_Truth_Audit.md](docs/KAGE_Ground_Truth_Audit.md)  
**Specification Hub:** [docs/README.md](docs/README.md) · **Design Document:** [docs/Design.md](docs/Design.md)

---

## 1. Project Reality & Architectural Mandate

```
╔═════════════════════════════════════════════════════════════════════════════════════╗
║                               PROJECT REALITY MANDATE                               ║
║                                                                                     ║
║        KAGE IS NOT A TAURI APP THAT HAPPENS TO CONTAIN A WEBVIEW.                  ║
║        KAGE IS A FULL BROWSER CONTROL PLANE BUILT AROUND NATIVE CEF                 ║
║        (CHROMIUM EMBEDDED FRAMEWORK VIA TAURI-APPS/CEF-RS).                         ║
║                                                                                     ║
║        THE REACT SHELL IS MERELY THE PRESENTATION SURFACE. THE RUST HOST            ║
║        COORDINATES MULTI-PROCESS CHROMIUM, GOVERNED TOOL-BUS CAPABILITIES,          ║
║        IMMUTABLE AUDIT LOGS, AND AN OBSERVABLE AGENT RUNTIME.                       ║
╚═════════════════════════════════════════════════════════════════════════════════════╝
```

* **No System WebViews:** Do not use Tauri platform-default webviews (WebView2, WKWebView, WebKitGTK) as the browsing engine. They diverge in semantics, DevTools support, and cookie/network control.
* **Native CEF Pinned:** The browser engine is **CEF 152.x** via `tauri-apps/cef-rs` across Windows (x86_64/ARM64), macOS (x86_64/ARM64), and Linux (x86_64/ARM64).
* **Windowed CEF First (No OSR):** Browsing surfaces run in native windowed child HWNDs to guarantee 60–120 FPS GPU compositing without off-screen rendering throughput penalties.
* **CDP is Internal Transport, Not the Architecture:** The UI and AI never touch raw WebSockets (`ws://127.0.0.1:...`). CDP is an internal gateway between the Browser Controller and CEF.
* **The Prime Directive:** *Every feature that gives KAGE more power must simultaneously add observability, policy, and user control.*

---

## 2. Inviolable Architectural Invariants & Contracts

All agents operating in this repository must uphold these **10 Inviolable Architecture Contracts** defined authoritatively in [`docs/02-architecture/Architecture_Contracts.md`](docs/02-architecture/Architecture_Contracts.md). These are automated CI/review gates:

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│                     THE 10 INVIOLABLE ARCHITECTURE CONTRACTS                     │
│                                                                                  │
│  INV-01: AI never directly accesses CEF.                                         │
│  INV-02: All browser actions pass through ToolBus.                               │
│  INV-03: Web content is data, never authority.                                   │
│  INV-04: Privileged mutations require policy approval.                            │
│  INV-05: Privileged mutations require successful audit commitment (Fail-Closed). │
│  INV-06: Secrets never enter LLM context (Credential Broker).                    │
│  INV-07: React never directly controls CDP.                                      │
│  INV-08: Every agent action has an observable result (Deterministic Verifier).   │
│  INV-09: "STOP" prevents subsequent actions (No retrospective rollback illusion).│
│  INV-10: Every browser tab has an explicit ProfileId + TargetId relationship.    │
└──────────────────────────────────────────────────────────────────────────────────┘
```

Detailed definitions, threat models, and automated CI test gates are documented in [docs/02-architecture/Architecture_Contracts.md](docs/02-architecture/Architecture_Contracts.md).

---

## 3. Mandatory Skill Consultation Invariant

```
╔═════════════════════════════════════════════════════════════════════════════════════╗
║                      MANDATORY SKILL CONSULTATION INVARIANT                         ║
║                                                                                     ║
║        AGENTS MUST READ AND CONSULT RELEVANT SKILLS IN `.agents/skills/`            ║
║        BEFORE MAKING ANY CODE CHANGES IN THE CODEBASE.                              ║
╚═════════════════════════════════════════════════════════════════════════════════════╝
```

Before modifying, creating, or refactoring code in this repository, agents **MUST consult the relevant skill documentation in `.agents/skills/`**:
* **UI Craft & Polish:** [`.agents/skills/emil-design-eng/SKILL.md`](.agents/skills/emil-design-eng/SKILL.md) (Emil Kowalski's standards on layout finesse, micro-interactions, click states, and invisible details. Always review using the `| Before | After | Why |` table).
* **Motion & Physics:** [`.agents/skills/animate/SKILL.md`](.agents/skills/animate/SKILL.md) and [`.agents/skills/apple-design/SKILL.md`](.agents/skills/apple-design/SKILL.md) (fluid spring dynamics, interruptible gestures, strict duration envelopes, no `transition: all`, optical motion blur on GPU layers).
* **Design Systems & Tokens:** [`.agents/skills/design-systems/SKILL.md`](.agents/skills/design-systems/SKILL.md) and [`.agents/skills/ui-design/SKILL.md`](.agents/skills/ui-design/SKILL.md).
* **Security & OWASP Defense:** [`.agents/skills/security-owasp/SKILL.md`](.agents/skills/security-owasp/SKILL.md) and [`.agents/skills/prompt-injection-defense/SKILL.md`](.agents/skills/prompt-injection-defense/SKILL.md).
* **Testing & Quality Architecture:** [`.agents/skills/test-architect/SKILL.md`](.agents/skills/test-architect/SKILL.md).

---

## 4. The Five-Plane Architecture

KAGE divides responsibilities across five isolated planes:

```
┌────────────────────────────────────────────────────────────────────────┐
│ 1. CHROMIUM PLANE (CEF Multi-Process)                                 │
│    Blink Layout • V8 JS Engine • GPU Compositor • Network / TLS Stack  │
│    Isolated Renderer Processes • Win32 / Cocoa Child HWND Surface      │
├────────────────────────────────────────────────────────────────────────┤
│ 2. BROWSER CONTROL PLANE (Rust Core)                                   │
│    BrowserController • TabManager • WindowManager • ProfileManager     │
│    NavigationManager • PermissionManager • DownloadManager • Session    │
├────────────────────────────────────────────────────────────────────────┤
│ 3. DEVELOPER PLANE (CDP Gateway & Telemetry)                           │
│    Multiplexed CDP Sessions • DOM / BoxModel • CSS Computed Styles     │
│    Network Interception (Fetch) • Console Stream • Tracing / Memory    │
├────────────────────────────────────────────────────────────────────────┤
│ 4. GOVERNANCE PLANE (Security & Audit)                                 │
│    ToolBus Dispatcher • Scoped Capability Engine • Secret Sanitizer     │
│    Immutable SHA-256 Hash-Chained Audit Ledger (security_audit.db)     │
├────────────────────────────────────────────────────────────────────────┤
│ 5. AGENT PLANE (Autonomous Reasoning & Verification)                  │
│    Intent Parser • Bounded Planner • BrowserObservation Model          │
│    Deterministic Verifier • Human Takeover Escape Hatch                │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 5. Profile Architecture & Session Isolation

KAGE separates user data into distinct profiles:
* **Personal Profile:** Persistent disk storage, personal cookies and sessions.
* **Work Profile:** Isolated workspace sessions and cookies.
* **Agent Sandbox Profile (Default for Agent):** Clean environment, zero pre-existing cookies or tokens. Web agents cannot access user logins without explicit escalation.
* **Temporary Profile:** In-memory RAM storage, discarded on close.

**Escalation Rule:** An agent requesting to run in the user's active session (`Agent Shared Session Mode`) requires an explicit modal confirmation declaring that the agent will inherit existing logged-in credentials.

---

## 6. Structured Observation Model (Context Engine)

Do not dump raw `document.documentElement.outerHTML` into LLM prompts. The Context Engine converts the active page state into a structured `BrowserObservation`:
```rust
pub struct BrowserObservation {
    pub tab: TabState,
    pub page: PageSummary,
    pub accessibility: Option<AxTree>,
    pub dom: Vec<RelevantNode>,
    pub screenshot: Option<ImageRef>,
    pub console: Vec<ConsoleEvent>,
    pub network: Vec<NetworkSummary>,
    pub permissions: PermissionState,
    pub security: SecurityState,
    pub provenance: Vec<ProvenanceTag>,
}
```
* **Token Budget:** Strict **4,000 tokens** baseline for general context packs.
* **Secret Redaction:** Bearer tokens, API keys, passwords, session cookies, and authorization headers must be sanitized before assembly.

---

## 7. Deterministic Agent Verifier

An autonomous agent must never declare success based on tool invocation alone. Every mutating action requires postcondition verification:
```
Action: page.click("button[type='submit']")
Verifier checks:
  1. Did URL transition as expected?
  2. Did success notification / authenticated element appear in AX tree?
  3. Did unhandled console exceptions or network errors fire?
```
The agent only proceeds when postconditions are satisfied.

---

## 8. Coding & Implementation Standards

### Rust (Core & Host Subsystems)
* Use `thiserror` for domain errors (`ToolError`, `CdpError`, `StorageError`, `EngineError`) and `anyhow` for top-level entry points.
* **Never use raw `.unwrap()` or `.expect()`** on fallible external inputs, IPC payloads, or CEF callbacks.
* All database queries must use parameterized statements (`params![]`) in `rusqlite` to prevent SQL injection.
* Support graceful cancellation across all asynchronous tasks via `tokio_util::sync::CancellationToken`.
* Audit log records must compute SHA-256 hash chains linked to the genesis hash.

### TypeScript / React (UI Chrome)
* Strict mode enabled (`strict: true`, `noImplicitAny: true`).
* No `any` types. Define explicit interfaces matching `docs/02-architecture/IPC_Protocol.md`.
* UI components must use the design tokens from `src-ui/src/tokens/tokens.css` (Liquid Glass theme, peach-to-burgundy gradient `#F9DBBD` → `#450920`, Inter for chrome, JetBrains Mono for data).
* Never ship `transition: all`. Animate only `transform` and `opacity` on GPU layers.
* Never use static placeholder data when real data flow is required.

---

## 9. Performance Budgets

* **Idle RAM (Clean Start, 1 Tab):** < 150 MB (Host + UI + CEF single page baseline).
* **Compositor Frame Rate:** 60 FPS minimum (120 FPS target on ProMotion/high-refresh displays).
* **Tool Bus Dispatch Latency:** < 100 ms from UI invocation to CDP execution start.
* **Context Assembly Time:** < 50 ms for DOM + Console + Network snapshot.
* **Resize / Viewport Sync:** Debounced via `requestAnimationFrame` + settle timer to eliminate Win32 HWND IPC spam.

---

## 10. Specification Directory Reference

Before modifying or creating any component, consult the authoritative specifications:

* **Control Plane Strategy:** [docs/research/KAGE_Browser_Control_Plane_Strategy.md](docs/research/KAGE_Browser_Control_Plane_Strategy.md)
* **Ground-Truth Audit:** [docs/KAGE_Ground_Truth_Audit.md](docs/KAGE_Ground_Truth_Audit.md)
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
* **Security & Threat Model:** [docs/08-security/Security_Model.md](docs/08-security/Security_Model.md)
* **Design System (Liquid Glass):** [docs/09-ui/Design_System.md](docs/09-ui/Design_System.md)
