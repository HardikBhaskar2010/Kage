# KAGE (影) — The Developer-First Autonomous Browser & Instrumentation Workstation

## Executive Overview

KAGE is an independent, developer-first desktop browser and instrumentation workstation engineered from the ground up to fuse web standards execution, deep runtime DevTools, an automated Testing Lab, and an autonomous AI agent into a singular, cohesive developer environment. Built with Tauri v2 (Rust), Chromium Embedded Framework (CEF 152), and a bespoke Liquid Glass React 18 frontend, KAGE abandons the brittle extension model in favor of full ownership over the browser shell, tab lifecycle, omnibox, and execution policies.

Rather than bolting an ungrounded chat assistant onto a consumer browser, KAGE introduces an active **Context Engine** and a permission-adjudicated **Rust Tool Bus** that bridges the Chrome DevTools Protocol (CDP). Every developer query—from layout shifts and cascade anomalies to network race conditions—is resolved against live, verified browser state with zero hallucination.

---

## The Engineering Problem: The Fragmented Developer Loop

Modern web engineering forces developers to continually juggle four disjointed tools:
1. **The Consumer Browser:** Used strictly for visual inspection and manual navigation.
2. **Native DevTools:** Opened, docked, and closed constantly to inspect DOM trees, CSS cascades, network waterfalls, and console outputs.
3. **External AI Interfaces:** Generic chatbots (ChatGPT, Claude, Cursor) fed incomplete, copy-pasted HTML snippets or lossy screenshots stripped of actual runtime computed styles, execution context, and event listeners.
4. **End-to-End Automation Frameworks:** Standalone test runners (Playwright, Cypress) where reproducing a bug requires writing boilerplate scripts from scratch.

This disconnect throws away critical state at every boundary. KAGE solves this by recognizing that the browser is the only environment that simultaneously possesses the full DOM tree, box model, accessibility tree, network graph, and JavaScript runtime.

---

## Architectural Philosophy & Core Tenets

KAGE is constructed upon five foundational engineering invariants:
1. **Never Rebuild What Chromium Solved:** Chromium/Blink handles web standards, HTML5 parsing, V8 JavaScript execution, WebGPU/WebGL rendering, and sandboxed process isolation via CEF. KAGE owns the shell, chrome, window management, and developer experience.
2. **Everything Is a Governed Tool Call:** Neither built-in UI panels nor autonomous AI agents receive unfettered browser access. All inspections, DOM queries, script evaluations, and network throttles route through a centralized Rust Tool Bus governed by two-stage fail-closed policies and cryptographic SHA-256 audit logs.
3. **Context Is Assembled, Never Dumped:** Rather than serializing entire megabyte-scale DOM trees into prompt contexts, KAGE's Context Engine extracts, redacts, ranks, and token-budgets the minimal semantic slice required for accurate reasoning.
4. **Untrusted By Default:** Web page content, console strings, and HTTP response bodies are strictly classified as untrusted data surfaces to prevent prompt injection and side-channel leakage.
5. **Liquid Glass Aesthetic:** High spatial clarity, translucent visual depth, and anime-futuristic obsidian/crimson palettes replace crowded DevTools walls with progressive disclosure and focused workflows.

---

## System Architecture & Subsystems

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         HOST PROCESS (Tauri / Rust)                         │
│                                                                             │
│  ┌───────────────────────┐  ┌─────────────────────┐  ┌───────────────────┐  │
│  │   KAGE Window Host    │  │    KAGE Tool Bus    │  │  Context Engine   │  │
│  │ (Window Mgr, Tabs, OS)│  │ (Router & Policy)   │  │ (Sanitizer & Pack)│  │
│  └───────────┬───────────┘  └──────────┬──────────┘  └─────────┬─────────┘  │
│              │                         │                       │            │
│  ┌───────────┴───────────┐  ┌──────────┴──────────┐  ┌─────────┴─────────┐  │
│  │    CEF LifeSpan Host  │  │    AI Subsystem     │  │    Testing Lab    │  │
│  │  (C-FFI, Message Pump)│  │(LLM Router & Bounded│  │ (Recorder/Runner) │  │
│  └───────────┬───────────┘  └──────────┬──────────┘  └─────────┬─────────┘  │
│              │                         │                       │            │
│  ┌───────────┴───────────┐  ┌──────────┴──────────┐  ┌─────────┴─────────┐  │
│  │    CDP Rust Client    │  │   Plugin Runtime    │  │  SQLite Storage   │  │
│  │ (WebSocket / DevTools)│  │   (Wasmtime / Wasm) │  │(kage_data.db,     │  │
│  └───────────┬───────────┘  └─────────────────────┘  │ security_audit.db)│  │
│              │                                       └───────────────────┘  │
└──────────────┼──────────────────────────────┬───────────────────────────────┘
               │ Native Surface / OSR         │ Tauri IPC (Typed JSON Streams)
               ▼                              ▼
┌──────────────────────────────┐    ┌─────────────────────────────────────────┐
│      CEF RUNTIME TREE        │    │          KAGE UI CHROME (React / TS)    │
│  (Blink / V8 Web Content)    │    │  (Omnibox, DevTools, AI Sidebar, Glass) │
└──────────────────────────────┘    └─────────────────────────────────────────┘
```

### 1. Tauri v2 Desktop Engine & Native Window Host
- Built on Tauri v2 and Rust 2021 with strict zero-cost foreign function interfaces (FFI).
- Hosts the native parent window (`HWND` on Windows), coordinates native message loops with `CefPostTask(TID_UI)`, and exposes typed IPC command streams to the UI chrome.

### 2. Chromium Embedded Framework (CEF 152) Integration
- Direct C-FFI bindings embedding full Chromium web rendering into native child surface viewports.
- Inherits site isolation, sandboxed renderer helper processes, GPU hardware acceleration, and full HTML5/CSS/ECMAScript standards compliance.

### 3. High-Velocity Chrome DevTools Protocol (CDP) WebSocket Engine
- Native Rust WebSocket client communicating directly with CEF's internal developer port.
- Dispatches domain commands across `DOM`, `CSS`, `Network`, `Runtime`, `Page`, `Overlay`, and `Emulation` with sub-millisecond roundtrips.

### 4. Permission-Governed KAGE Tool Bus & Cryptographic Audit Ledger
- Two-stage execution architecture: Policy Evaluation (`Adjudicate`) followed by Execution (`Execute`).
- Enforces strict capability boundaries (`ReadDOM`, `CaptureNetwork`, `EvaluateScript`, `MutateState`).
- Appends every action to an append-only SQLite audit database (`security_audit.db`) secured with SHA-256 chain hashes.

### 5. Active Context Engine (Token-Budgeted Browser Grounding)
- Eliminates context-window blowup by condensing DOM subtrees to relevant CSS selectors, bounding boxes, accessibility nodes, and computed styles.
- Automatically redacts sensitive fields (passwords, auth cookies, tokens, PII) before delivering context to LLM backends.

### 6. Built-in Testing Lab & Action Recorder
- Captures interactive browser sessions into deterministic, replayable test flows.
- Automatically generates resilient semantic selectors and asserts runtime visual state without third-party dependencies.

### 7. Liquid Glass Design System
- Anime-futuristic aesthetic balancing obsidian backgrounds, translucent backdrop blurs, and neon crimson/peach accents.
- Features custom omnibox auto-complete, multi-profile isolation, and responsive side panels built in React 18 and TailwindCSS.

---

## Technology Stack

| Layer | Technology |
|---|---|
| **Host Runtime** | Tauri v2.x (Rust 2021) |
| **Browser Engine** | Chromium Embedded Framework (CEF 152 / Chromium 132+) |
| **Instrumentation** | Chrome DevTools Protocol (CDP) over WebSocket |
| **Frontend Chrome** | React 18+, TypeScript Strict, TailwindCSS, CSS Modules |
| **Plugin Sandbox** | WebAssembly (Wasmtime) |
| **Persistence** | SQLite (`rusqlite` with WAL mode) |
| **Design Language** | Liquid Glass (Obsidian / Crimson / Translucent Glassmorphism) |

---

## Milestone Status & Verification

- **Phase 1 (Governance Subsystem):** `SEALED (100%)` — Inviolable audit hashing, ToolBus policy adjudication, two-stage fail-closed execution, and LLM context sanitization (`INV-01..INV-06, INV-09`).
- **Phase 2 (CEF Engine & Composition):** `SEALED (100%)` — Real CEF 152 child HWND composition inside Tauri WebView2 chrome, `CefPostTask(TID_UI)` UI loop integration, live process tree verification, and formal 10-point release sandbox packaging gate `CEF-03b-D`.
- **Phase 3 (Browser Lifecycle & Control Plane):** `CONTROL-PLANE VERIFIED` — 24/24 deterministic control-plane integration tests pass across tab management, profile isolation, and session persistence.
