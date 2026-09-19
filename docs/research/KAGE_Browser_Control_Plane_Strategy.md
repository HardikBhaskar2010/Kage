# KAGE — Browser Control Plane & CEF Architecture Strategy

**Document ID:** KAGE-RES-001  
**Status:** Canonical Architecture Strategy & Research Specification  
**Classification:** Core System Architecture & Engine Decision  
**Target Milestone:** v1.0.0 Alpha / MVP  
**Reference Audit:** [docs/KAGE_Ground_Truth_Audit.md](../KAGE_Ground_Truth_Audit.md) · **Operating Rules:** [AGENTS.md](../../AGENTS.md)

---

## 1. Executive Summary & Core Architectural Thesis

KAGE (影) is **not a Tauri app that happens to contain a webview**. It is a **developer-first browser control plane built around the Chromium Embedded Framework (CEF)**.

The [KAGE Ground-Truth Project Audit](../KAGE_Ground_Truth_Audit.md) accurately identified that while KAGE possessed a polished React Liquid Glass UI shell and isolated cryptographic/policy units in Rust, the actual browser engine, live navigation, live CDP telemetry, and LLM reasoning were absent or stubbed.

This strategy document establishes the concrete, uncompromised path forward:
* **Engine Decision:** We reject platform-divergent system webviews (WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux) in favor of **native CEF (Chromium 152+ via `tauri-apps/cef-rs`)**.
* **Mode:** **Windowed CEF rendering first**, utilizing hardware-accelerated DirectComposition/Vulkan compositor pipelines rather than CPU/GPU-stalled Off-Screen Rendering (OSR).
* **Control Plane:** The Rust host coordinates native multi-process Chromium, exposing governed, observable, user-controlled primitives across five distinct planes.
* **The Prime Mandate:** *Every feature that gives KAGE more power must simultaneously add observability, policy, and user control.*

```
                       KAGE CONTROL PLANE
┌─────────────────────────────────────────────────────────────┐
│                    USER EXPERIENCE                          │
│ Tabs • Omnibox • Sidebar • Downloads • Settings • DevTools │
└────────────────────────────┬────────────────────────────────┘
                             │
                      KAGE Control Plane
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
   Tab Manager         Policy Engine         Agent Runtime
        │                    │                    │
   Profile Manager      Tool Bus              Planner
        │                    │                 Executor
   Permission Manager  Audit Ledger           Verifier
        │                    │
        └──────────────┬─────┘
                       │
                 Browser Gateway
                       │
                 CDP / CEF Bridge
                       │
              ┌────────┴─────────┐
              │                  │
        CEF Browser Layer      Network
              │                  │
        Blink + V8 + GPU       HTTP/TLS
              │
      Renderer / Utility / GPU
              │
          Web Application
```

---

## 2. Deep Audit Analysis: The Five Architectural Findings

The Ground-Truth Audit identified the critical integration debt (`verify_audit_chain` returning `true`, `get_audit_logs` returning `[]`, uninstantiated `AuditDb`, stubbed CDP, fake webview templates, canned AI responses). To resolve this permanently, we establish five core architectural corrections:

### Finding A — CDP is an Internal Transport, Not the Architecture
CDP (Chrome DevTools Protocol) is the engine's internal inspection and control protocol. The KAGE UI and KAGE AI must **never** connect directly to raw WebSockets like `ws://127.0.0.1:xxxxx/devtools/page/...`.
* The architecture must strictly flow:
  `KAGE UI / AI → KAGE Browser API → Policy / Tool Bus → Browser Gateway → CDP Session Broker → CEF`.
* CDP tip-of-tree protocol changes frequently and has no backward compatibility guarantees for new capabilities.
* **Rule:** KAGE pins its generated CDP protocol definitions to the exact embedded CEF version.

### Finding B — System WebViews Are a Prototype Trap; CEF is the Product
Tauri's default model uses platform webviews: WebView2 (Windows), WKWebView (macOS), and WebKitGTK (Linux). Building a developer browser on these creates insurmountable divergence:
* Chrome DevTools Protocol is incomplete or non-existent on WebKitGTK and WKWebView.
* Storage, cookie isolation, request interception (`Fetch.requestPaused`), and network waterfall semantics diverge across platforms.
* **Rule:** CEF embeds consistent Chromium across Windows (x86_64/ARM64), macOS (x86_64/ARM64), and Linux (x86_64/ARM64). Developer tools behave identically on all platforms.

### Finding C — Full User Control: Capabilities + Scope + Provenance + Approval
Governance cannot be a static 4-tier dropdown. It requires a formal **Capability Grant System**:
```json
{
  "capability": "browser.click",
  "scope": {
    "profile": "work",
    "tab": "tab_17",
    "origins": ["https://github.com"]
  },
  "mode": "confirm",
  "expires": "session"
}
```
The execution chain is strictly enforced:
`User → AI → Requested Capability → Scope Check → Risk Check → User Confirmation → ToolBus → Native Execution → Verification → Hash-Chained Audit`.

### Finding D — The Untrusted Webpage Boundary & Indirect Prompt Injection Defense
Webpages are **external untrusted data**, never instructions. Malicious pages can embed indirect prompt injections in hidden CSS, comments, DOM nodes, metadata, or HTTP response headers designed to hijack the agent.
* **Strict Invariant:**
  ```
  USER INSTRUCTIONS  ───►  AUTHORITY
  POLICY RULES       ───►  AUTHORITY
  USER-APPROVED PLAN ───►  AUTHORITY
  WEB CONTENT        ───►  DATA ONLY (NEVER INSTRUCTION)
  TOOL OUTPUT        ───►  DATA ONLY (NEVER INSTRUCTION)
  ```
* Web content must always be tagged with provenance (`source = webpage`, `trust = untrusted`) and enclosed in structural data envelopes before reaching any model.

### Finding E — Structured Observation Model (Not Raw DOM Dumps)
Sending `document.documentElement.outerHTML` to an LLM wastes tokens, exceeds budget, and leaks script noise. The Context Engine must output a typed `BrowserObservation`:
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
This keeps general context packs within the **4,000-token budget** while providing richer, cleaner signals (accessibility roles, focused inputs, visible text, network errors).

---

## 3. The Five-Plane Architecture

KAGE divides operations across five isolated planes:

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
│    ToolBus Dispatcher • 4-Tier Policy Engine • Secret Sanitizer         │
│    Immutable SHA-256 Hash-Chained Audit Ledger (security_audit.db)     │
├────────────────────────────────────────────────────────────────────────┤
│ 5. AGENT PLANE (Autonomous Reasoning & Verification)                  │
│    Intent Parser • Bounded Planner • Observation Model Builder         │
│    Deterministic Verifier • Human Takeover Escape Hatch                │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Engine Decision & Native Window Embedding

### Binding & Distribution Choice
* **Engine:** Chromium Embedded Framework (CEF).
* **Rust Binding:** `tauri-apps/cef-rs`.
* **Version:** Pinned to **CEF 152.x** (September 2026 releases).
* **Supported Targets:** Windows (x86_64, ARM64), macOS (x86_64, ARM64), Linux (x86_64, ARM64).
* **Platform Sequence:** Windows first → macOS → Linux.
* **No Chromium Fork:** Forking Chromium is explicitly rejected for V1; standard CEF distribution via `cef-rs` provides full embedding control without custom engine maintenance overhead.

### Windowed CEF vs. Off-Screen Rendering (OSR)
* **Decision:** **Windowed CEF First**.
* **Rationale:** CEF documentation explicitly notes that Off-Screen Rendering (OSR) suffers throughput and frame rate penalties because accelerated compositing and DirectComposition surface sharing are restricted. Windowed rendering gives native 60–120 FPS hardware acceleration directly via the GPU process.
* **Native Surface Composition:**
  ```
  Tauri Native Window (HWND)
  │
  ├── React / Tauri UI Surface (Liquid Glass Chrome)
  │     ├── Tab strip, Omnibox, Sidebar, DevTools drawer
  │
  ├── Native CEF Browser Child Surface (HWND child)
  │     └── DirectComposition hardware-accelerated web page
  │
  └── Native Transparent Overlay Surface
        ├── Micro Inspect hover highlights
        ├── Tool Bus confirmation modals
        └── Human Takeover HUD
  ```
* **Overlay Engineering Gate:** The native child HWND and the Tauri webview surface are distinct native layers. Translating mouse events and ensuring click-through to CEF while rendering overlays is an explicit engineering gate (formalized from `spikes/01-cef-tauri-embedding`).

---

## 5. Profile Architecture & Session Isolation

Chromium enforces profile isolation across cache, cookies, local storage, and credentials. KAGE structures profiles as first-class primitives:

```
KAGE PROFILES
├── Personal Profile      (Persistent disk storage, personal cookies/logins)
├── Work Profile          (Persistent disk storage, enterprise workspace cookies)
├── Agent Sandbox Profile (Isolated disk or memory cache, zero pre-existing cookies)
└── Temporary Profile     (RAM-only in-memory storage, discarded on close)
```

### Agent Access Modes
1. **Agent Sandbox Mode (Default):**
   * Spawns within a clean `Agent Sandbox Profile`.
   * Possesses no pre-existing cookies, session tokens, or saved credentials.
   * Prevents web agents from accessing user bank accounts, email sessions, or sensitive SaaS tools without user awareness.
2. **Agent Shared Session Mode (Elevated Privilege):**
   * Attaches to the user's active `Personal` or `Work` profile.
   * Operates with existing authentication cookies and sessions.
   * **Requires explicit user authorization modal** declaring the elevated risk.

---

## 6. Granular Capability & Permission Firewall

The Tool Bus enforces capability checking before any native action executes:

| Capability | Scope | Default Agent State | Policy Verification |
|---|---|---|---|
| `page.read_visible` | Current Tab | **Allow** | Auto-allowed passive read |
| `page.read_accessibility` | Current Tab | **Allow** | Auto-allowed semantic tree read |
| `dom.read_node` | Current Tab | **Allow** | Auto-allowed (sanitized of secrets) |
| `console.read_logs` | Current Tab | **Allow** | Auto-allowed passive telemetry |
| `network.read_metadata` | Current Tab | **Allow** | Headers and URLs (Bearer tokens redacted) |
| `page.capture_screenshot` | Viewport | **Allow** | Visual observation |
| `page.scroll` | Current Tab | **Allow** | Viewport movement |
| `browser.open_url` | Any Tab | **Confirm Initially** | Confirmed on first navigation per origin |
| `page.click_element` | Current Tab | **Confirm Initially** | Session-scoped approval per domain |
| `page.type_text` | Non-sensitive input | **Confirm Initially** | Confirmed for forms |
| `page.type_sensitive` | Password / Credit Card | **Strong Confirmation** | Explicit per-action modal with secret mask |
| `page.submit_form` | Payment / Mutation | **Strong Confirmation** | Explicit per-action confirmation |
| `downloads.start` | Filesystem | **Confirm** | Destination folder validation |
| `file.upload` | Filesystem | **Strong Confirmation** | File path and content inspection |
| `clipboard.read` | System | **Strong Confirmation** | Prevent secret theft |
| `clipboard.write` | System | **Confirm** | Prevent clipboard hijacking |
| `page.eval_javascript` | Execution Context | **Strong Confirmation** | Arbitrary script evaluation barrier |
| `storage.modify` | Cookies / LocalStorage | **Strong Confirmation** | Session tampering barrier |
| `permissions.change` | Site Settings | **Strong Confirmation** | Origin permission modification |
| `extension.install` | Browser Engine | **Strong Confirmation** | Code execution barrier |
| `system.launch_app` | OS Process | **Strong Confirmation** | Process execution barrier |
| `data.clear_profile` | Storage | **Strong Confirmation** | Destructive action barrier |

---

## 7. Developer Plane & Native DevTools Architecture

The DevTools React UI is preserved and connected to live CDP domains via the Rust `CdpBroker`:

```
┌──────────────┬─────────────────────────────────────────────────────────┐
│ PANEL        │ AUTHORITATIVE CDP DOMAINS & TELEMETRY                   │
├──────────────┼─────────────────────────────────────────────────────────┤
│ Elements     │ DOM.getNodeForLocation, DOM.getBoxModel,                │
│              │ CSS.getComputedStyleForNode, DOM.highlightNode          │
│ Console      │ Runtime.consoleAPICalled, Log.entryAdded,               │
│              │ Runtime.exceptionThrown                                 │
│ Network      │ Network.requestWillBeSent, Network.responseReceived,     │
│              │ Network.loadingFinished, Fetch.requestPaused (Mock/Mod) │
│ Sources      │ Debugger.enable, Debugger.setBreakpoint,                │
│              │ Debugger.stepInto, Runtime.evaluate                     │
│ Performance  │ Performance.enable, Performance.getMetrics,             │
│              │ Tracing.start, Tracing.dataCollected                    │
│ Storage      │ Storage.getUsageAndQuota, Storage.clearDataForOrigin,   │
│              │ DOMStorage.getDOMStorageItems, Network.getCookies       │
│ Security     │ Security.enable, Security.visibleSecurityStateChanged    │
│ Access       │ Accessibility.getFullAXTree, Accessibility.queryAXTree  │
└──────────────┴─────────────────────────────────────────────────────────┘
```

### Micro Inspect Pipeline (Goodbye Stub 42)
1. Cursor coordinates `(x, y)` captured on native overlay.
2. Coordinate transform: physical device pixels $\leftrightarrow$ logical CSS coordinates.
3. Invocation: `DOM.getNodeForLocation(x, y, includeUserAgentShadowDOM = false)`.
4. Returns authentic `backendNodeId`.
5. Multi-fetch:
   * Box Model: `DOM.getBoxModel(backendNodeId)`.
   * Computed Styles: `CSS.getComputedStyleForNode(backendNodeId)`.
   * Accessibility: `Accessibility.getPartialAXTree(backendNodeId)`.
6. Highlighting: `DOM.highlightNode(backendNodeId, highlightConfig)`.
7. Floating HUD renders real margin, border, padding quads and CSS properties.

---

## 8. Agent Plane: Autonomous Loop, Verifier & Human Takeover

```
                    AUTONOMOUS AGENT PIPELINE
                              USER
                               │
                               ▼
                         Intent Parser
                               │
                               ▼
                           Planner
                               │
                               ▼
                       Observation Model
                               │
                               ▼
                        Tool Selection
                               │
                               ▼
                        Policy Engine
                               │
                      ┌────────┴────────┐
                      │                 │
                  Auto-Allow         Confirm (Modal)
                      │                 │
                      └────────┬────────┘
                               ▼
                            ToolBus
                               │
                               ▼
                        Browser Action
                               │
                               ▼
                         State Change
                               │
                               ▼
                      Deterministic Verifier
                               │
                        Success / Failure
                               │
                               ▼
                     Hash-Chained Audit Log
```

### Deterministic Verifier
An autonomous agent must never assume an action succeeded. Every mutating action requires a verification postcondition:
* Action: `page.click("button[type='submit']")`
* Verifier checks:
  1. Did URL change to `/dashboard` or expected target?
  2. Did expected element or authenticated avatar appear in accessibility tree?
  3. Did an unhandled network or console error fire?
* Only when postconditions pass does the agent mark the step `Done`.

### Human Takeover Escape Hatch
The UI features a persistent, hardware-level physical overlay:
```
╔═══════════════════════════════════════════════════════════════════════════════════╗
║ [ ⬤ AGENT ACTIVE ]  Step 3/10: Navigating to GitHub Checkout                     ║
║ [ ⏸ PAUSE ]   [ ⏹ STOP ]   [ ✋ TAKE CONTROL ]   [ 👁 VIEW PLAN ]   [ 📜 AUDIT ]  ║
╚═══════════════════════════════════════════════════════════════════════════════════╝
```
Clicking **Take Control** instantly halts the agent loop, revokes active cancellation tokens, suspends CDP dispatch, and restores full keyboard/mouse control to the user.

---

## 9. Phased Implementation Roadmap (Phases 0–12)

```
                    KAGE IMPLEMENTATION ROADMAP

PHASE 0: Architecture & Version Lock
         Pin cef-rs 152.x, CDP schemas, capability models, threat model
                     │
                     ▼
PHASE 1: Fix Security Foundations (P0)
         Wire AuditDb into ToolBus & Tauri; replace Ok(true) / Ok([]) stubs
                     │
                     ▼
PHASE 2: CEF Native Bootstrap (P0)
         crates/kage-engine: render https://example.com in native windowed CEF
                     │
                     ▼
PHASE 3: Browser Lifecycle (P0)
         TabManager, WindowManager, Profiles, real navigate/back/forward/reload
                     │
                     ▼
PHASE 4: CDP Gateway & Session Router
         Real WebSocket transport replacing CdpClient::call() stub
                     │
           ┌─────────┴─────────┐
           ▼                   ▼
PHASE 5: DevTools Data  PHASE 7: Profiles & Site Permissions
         Real DOM/Net/Console    CookieManager, storage revocation
           │                   │
           └─────────┬─────────┘
                     ▼
PHASE 6: Real Micro Inspect
         DOM.getNodeForLocation coordinate pipeline replacing fake 42
                     │
                     ▼
PHASE 8: Production KageTool Registration
         page.*, dom.*, browser.* registered on ToolBus with risk schemas
                     │
                     ▼
PHASE 9: Real LLM Agent Loop
         Structured tool calling (Ollama / Anthropic) replacing regex
                     │
                     ▼
PHASE 10: Deterministic Verifier & User Takeover HUD
          Postcondition checks and hardware-level pause/takeover
                     │
                     ▼
PHASE 11: Agent Security & Indirect Prompt-Injection Hardening
          Delimited observation envelopes and untrusted provenance tags
                     │
                     ▼
PHASE 12: Testing Lab & Standards
          Headless replay runner, cross-crate integration tests, WebDriver BiDi
```

### The First Real Milestone (Alpha Gate)
KAGE is not alpha because it has an AI sidebar or pretty dark mode. KAGE achieves **Alpha** when:
* [ ] CEF windowed browser actually renders any valid URL.
* [ ] Tabs are backed by actual CEF browser handles with real navigation events.
* [ ] Cookies and storage persist across sessions per profile.
* [ ] CDP connects to the active tab over a real authenticated session.
* [ ] DevTools Elements, Console, and Network panels stream real live data.
* [ ] Micro Inspect resolves real nodes from cursor coordinates via CDP.
* [ ] `security_audit.db` cryptographically logs every tool dispatch with zero stubs.
* [ ] Human takeover button halts active agent execution immediately.
* [ ] Web content cannot directly acquire agent authority.

---

## 10. Target Repository Crate Structure

```
Kage/
├── src-ui/                   # React 18+ UI Chrome (Liquid Glass Shell)
│   ├── components/           # TabStrip, Omnibox, DevTools, AISidebar
│   ├── context/              # BrowserContext, CdpTelemetryProvider
│   ├── devtools/             # Elements, Console, Network, Performance panels
│   ├── agent/                # Agent HUD, Human Takeover Modal, Plan Viewer
│   └── ipc/                  # Typed Tauri IPC client
│
├── src-tauri/                # Tauri 2.x Native Host Process
│   ├── browser/              # BrowserController integration
│   ├── commands/             # Thin IPC handlers delegating to engines
│   ├── permissions/          # Native prompt modal & capability dialogs
│   ├── profiles/             # RequestContext & profile directory management
│   └── state/                # Shared Arc<ToolBus>, Arc<AuditDb>, Arc<CdpBroker>
│
├── crates/
│   ├── kage-engine/          # CEF C-FFI / cef-rs lifecycle & message pump
│   ├── kage-browser/         # TabManager, WindowManager, NavigationController
│   ├── kage-cdp/             # CDP connection multiplexer & target router
│   ├── kage-core/            # ToolBus, PolicyEngine, SecretSanitizer
│   ├── kage-context/         # BrowserObservation builder & token budgeting
│   ├── kage-agent/           # Intent parser, planner, tool executor, verifier
│   ├── kage-storage/         # SQLite workspace state & SHA-256 AuditDb
│   ├── kage-policy/          # Capability grant system & scope adjudicator
│   └── kage-events/          # Internal cross-subsystem event bus
│
├── native/
│   └── cef/                  # CEF binary distribution manifest & wrapper DLLs
│
└── docs/
    ├── research/             # Canonical strategy & architectural blueprints
    └── architecture/         # System specifications
```
