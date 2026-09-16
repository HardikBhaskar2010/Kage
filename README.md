# KAGE (影) — The Developer-First Autonomous Browser

> **A standalone, developer-first browser and instrumentation workstation built with Tauri (Rust), Chromium Embedded Framework (CEF), and a Liquid Glass React UI.**

KAGE combines a production-grade web rendering engine with native developer tooling, an action recorder/testing lab, an active Context Engine, and a permission-governed AI agent.

---

## What is KAGE?

KAGE is not a browser extension or a companion app. It is a full desktop browser where:
1. **Chromium handles web standards:** Standard compliance, JS execution, WebGL/WebGPU, and sandboxing via CEF.
2. **KAGE owns the shell and developer experience:** Tab management, workspaces, omnibox, native DevTools, and UI chrome.
3. **AI never gets direct browser authority:** Every AI action routes through a typed, permission-governed Rust Tool Bus with immutable audit logs.
4. **Context is assembled, not dumped:** An active Context Engine continuously extracts, redacts, and token-budgets DOM, network, and console state.
5. **Liquid Glass Aesthetic:** An anime-futuristic visual identity crafted with spacious layouts and modern translucency.

---

## Core System Architecture

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
┌─────────────────────────────┐    ┌──────────────────────────────────────────┐
│      CEF RUNTIME TREE       │    │       KAGE UI CHROME (React / TS)        │
│  (Blink / V8 Web Content)   │    │  (Omnibox, DevTools, AI Sidebar, Glass)  │
└─────────────────────────────┘    └──────────────────────────────────────────┘
```

---

## Specification Suite (25 Modules)

The architecture and implementation specifications are organized in the [`docs/`](docs/) directory:

* 🧭 **[Master Documentation Hub & Graph](docs/README.md)**
* 📐 **[Technical Design Document](docs/Design.md)**
* 📋 **[Product Requirements Document (PRD)](docs/01-product/PRD.md)** & **[Requirements Specification](docs/01-product/Requirements.md)**
* 🏗️ **Core Architecture:** [System Architecture](docs/02-architecture/Architecture.md) · [CEF Integration](docs/02-architecture/CEF_Integration.md) · [Tauri Architecture](docs/02-architecture/Tauri_Architecture.md) · [Browser Shell](docs/02-architecture/Browser_Shell.md) · [Rendering Architecture](docs/02-architecture/Rendering_Architecture.md) · [IPC Protocol](docs/02-architecture/IPC_Protocol.md) · [Performance Benchmarks](docs/02-architecture/Performance_Benchmarks.md) · [Migration to Chromium Fork](docs/02-architecture/Migration_to_Chromium_Fork.md)
* 🧠 **Core Subsystems:** [Context Engine](docs/03-core/Context_Engine.md) · [Tool Bus](docs/03-core/Tool_System.md) · [Micro Inspect](docs/03-core/Micro_Inspect.md) · [Workspace System](docs/03-core/Workspace_System.md)
* 🤖 **AI Subsystem:** [AI Architecture](docs/04-ai/AI_Architecture.md)
* 🛠️ **Developer Tools:** [DevTools Architecture](docs/05-devtools/DevTools_Architecture.md)
* 🧪 **Testing & Quality:** [Testing Lab](docs/06-testing/Testing_Lab.md) · [Testing Strategy](docs/06-testing/Testing_Strategy.md)
* 💾 **Platform & Extensibility:** [Data Model (SQLite)](docs/07-platform/Data_Model.md) · [Plugin API (Wasm)](docs/07-platform/Plugin_API.md)
* 🛡️ **Security:** [Security & Threat Model](docs/08-security/Security_Model.md)
* 🎨 **User Interface:** [Liquid Glass Design System](docs/09-ui/Design_System.md)
* 🚀 **Build & Release:** [Build System](docs/10-build-release/Build_System.md) · [Release Strategy](docs/10-build-release/Release_Strategy.md)

---

## Tech Stack

| Layer | Technology |
|---|---|
| **Host Runtime** | Tauri 2.x (Rust) |
| **Browser Engine** | Chromium Embedded Framework (CEF) |
| **Instrumentation** | Chrome DevTools Protocol (CDP) over WebSocket |
| **Frontend Chrome** | React 18+ / TypeScript / Vanilla CSS + Tailwind |
| **Plugin Sandbox** | WebAssembly (Wasmtime) |
| **Persistence** | SQLite (`rusqlite` + WAL mode) |
| **Design Language** | Liquid Glass (Anime-Futuristic Peach/Burgundy) |

---

## License

All rights reserved © 2026 KAGE Project.
