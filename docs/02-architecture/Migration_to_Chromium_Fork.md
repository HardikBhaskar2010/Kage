# KAGE Migration to Chromium Fork Specification

| Document Metadata | Details |
|---|---|
| **Document ID** | KAGE-ARCH-008 |
| **Status** | Approved Core Specification |
| **Version** | v0.2.1 |
| **Last Updated** | 2026-09-16 |
| **Target Milestone** | Future Horizon (Post-v1.0 Stability) |
| **Classification** | Strategic Architecture & Engine Migration |

---

## 1. Executive Summary & Strategic Context

In v1, KAGE deliberately chose **CEF (Chromium Embedded Framework)** over a direct Chromium source fork. This decision protected solo-developer velocity, ensured seamless access to upstream Chromium security patches, and enabled shipping a production-grade browser without the crippling maintenance overhead of compiling 35+ million lines of C++ code daily.

However, KAGE’s ultimate rendering vision—semantic layer visualization, custom layout constraints, and hardware-accelerated compositor debugging—will eventually encounter the hard ceiling of what CEF and CDP can expose.

This document defines the **strategic trigger criteria**, the **minimal fork philosophy**, and the **architectural migration path** to transition from CEF to **KAGE Chromium** without throwing away the React UI, the AI subsystem, the Tool Bus, or the Testing Lab.

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

---

## 2. Fork Trigger Conditions & Thresholds

A fork of Chromium must never be undertaken on ideological grounds. KAGE will only initiate a Chromium fork when **all three** of the following conditions are simultaneously met:

```
┌────────────────────────────────────────────────────────────────────────┐
│                        FORK DECISION THRESHOLDS                        │
├───────────────────┬────────────────────────────────────────────────────┤
│ 1. Feature Wall   │ A core product capability (e.g. real-time 3D       │
│                   │ layout layer projection, custom Blink flexbox/grid │
│                   │ debugging passes) is physically impossible via     │
│                   │ CDP or CEF C-APIs.                                 │
├───────────────────┼────────────────────────────────────────────────────┤
│ 2. Market & Rev   │ KAGE has achieved proven product-market fit,       │
│    Validation     │ active daily developer adoption, and dedicated     │
│                   │ funding or revenue to sustain full-time engine eng.│
├───────────────────┼────────────────────────────────────────────────────┤
│ 3. Engineering    │ Dedicated infrastructure (distributed build farms, │
│    Capacity       │ ccache clusters) and engineering capacity exist to │
│                   │ rebase upstream Chromium milestones every 4 weeks. │
└───────────────────┴────────────────────────────────────────────────────┘
```

---

## 3. The "Minimal Fork" Philosophy

If KAGE forks Chromium, it will strictly avoid the trap of maintaining a divergent, monolithic browser codebase. 

KAGE will maintain a **Minimal Patchset Architecture**:

```
                         Upstream Chromium Git Tags
                                     │
                                     ▼
                      Automated Rebase & Patch Pipeline
                                     │
                   ┌─────────────────┴─────────────────┐
                   ▼                                   ▼
        Blink Core Layout Patches           Chromium Compositor (cc)
        //third_party/blink/...             //cc/layers/...
        (Custom Layout Inspection)          (Hardware Layer Projection)
                   │                                   │
                   └─────────────────┬─────────────────┘
                                     ▼
                            KAGE Chromium Core
                        (98% Untouched Chromium)
```

- **98% Codebase Purity:** KAGE leaves the V8 engine, the network stack (Cronet), the GPU rasterizer (Viz), and Chromium's security sandbox completely untouched.
- **Isolated Patchsets:** Custom engine logic lives in isolated patch files (`patches/blink/`, `patches/cc/`) applied deterministically during compilation.
- **Upstream Sync:** Automated git rebase scripts pull upstream stable tags every 4 weeks, running KAGE's automated test suite to verify patch continuity.

---

## 4. The Abstraction Buffer: `RenderingEngineBackend`

The single most important architectural decision made in v1 is the complete decoupling of the KAGE application layer from CEF via the `RenderingEngineBackend` Rust trait:

```rust
#[async_trait]
pub trait RenderingEngineBackend: Send + Sync {
    async fn navigate(&self, url: &str) -> Result<(), EngineError>;
    async fn evaluate_script(&self, js: &str) -> Result<serde_json::Value, EngineError>;
    async fn set_style_text(&self, sheet_id: &str, range: SourceRange, text: &str) -> Result<(), EngineError>;
    async fn highlight_node(&self, node_id: NodeId, config: HighlightConfig) -> Result<(), EngineError>;
    async fn capture_screenshot(&self, clip: Option<ViewportRect>) -> Result<Vec<u8>, EngineError>;
    async fn get_rendering_metrics(&self) -> Result<RenderingMetrics, EngineError>;
}
```

### What Changes vs. What Stays Intact

```
┌─────────────────────────────────────────────────────────────┐
│ UNTOUCHED BY MIGRATION (100% REUSED)                        │
│  • React / TypeScript Browser Shell & Liquid Glass UI       │
│  • KAGE Tool Bus & Permission Policy Engine                 │
│  • AI Subsystem, Model Routing & ReAct Agent Loop           │
│  • Context Engine Ingestion & Pruning                       │
│  • Testing Lab Recorder, Assertions & Playwright Exporter   │
│  • Workspace Manager & SQLite Database                      │
│  • Plugin API & WASM Sandbox                                │
├─────────────────────────────────────────────────────────────┤
│ SWAPPED OUT UNDERNEATH                                      │
│  [ CefCdpEngineBackend ]  ──►  [ KageChromiumEngineBackend ]│
│  (CEF C-API / WebSocket)  ──►  (Direct Mojo IPC / SharedMem)│
└─────────────────────────────────────────────────────────────┘
```

---

## 5. Technical Migration Roadmap

The migration will follow a 4-phase execution plan:

```
Phase 1: Build Infrastructure
 ├── Setup distributed Goma/Reclient compilation cluster
 ├── Automate Chromium source sync via `gclient`
 └── Establish automated CI build pipelines for Win/Mac/Linux
      │
      ▼
Phase 2: Minimal Blink Patchset
 ├── Implement custom Blink LayoutObject inspection hooks
 ├── Implement 3D layer visualization compositor pass in `cc`
 └── Build headless qualification tests
      │
      ▼
Phase 3: Backend Trait Implementation
 ├── Author `KageChromiumEngineBackend` in Rust
 ├── Connect Tauri host directly to KAGE Chromium via Mojo IPC
 └── Validate Tool Bus commands against new backend
      │
      ▼
Phase 4: Staged Production Cutover
 ├── Deploy to internal team & Alpha channel
 ├── Run side-by-side performance benchmarks (CEF vs Fork)
 └── Deprecate CEF binaries in stable release
```

This phased approach guarantees that KAGE remains a stable, shipping product throughout any future engine evolution.
