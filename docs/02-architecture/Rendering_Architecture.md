# KAGE Rendering Architecture Specification

| Document Metadata | Details |
|---|---|
| **Document ID** | KAGE-ARCH-004 |
| **Status** | Approved Architecture Specification |
| **Version** | v0.2.0 |
| **Last Updated** | 2026-09-16 |
| **Target Milestone** | v1.0.0 (CEF/CDP) → Future (KAGE Chromium) |
| **Classification** | Rendering Pipeline & Engine Strategy |

---

## 1. Executive Summary & Rendering Strategy

A foundational long-term vision of KAGE is to transform how developers understand and interact with rendered web pages—providing semantic layer visualization, alternate layout box models, instant CSS experimentation, and deep rendering pipeline inspection.

However, writing a layout and rasterization engine from scratch or maintaining a full source fork of Chromium from day one is catastrophic for solo-developer velocity.

To balance **practical shipping reality** with **ambitious architectural depth**, KAGE establishes a phased two-era rendering roadmap:

```
                          KAGE RENDERING STRATEGY
                                     │
                 ┌───────────────────┴───────────────────┐
                 │                                       │
            ERA 1: V1 CEF / CDP                    ERA 2: FUTURE FORK
            (Production Architecture)              (KAGE Chromium)
                 │                                       │
        • DOM/CSS Live Modification              • Blink Layout Engine Extensions
        • CDP Pipeline Instrumentation           • Custom Flexbox/Grid Algorithms
        • Hardware GPU Layer Observation         • Sub-Pixel Paint Interception
        • Synthetic Micro Inspect Overlays       • Compositor / Viz Custom Shaders
        • Style Injection & Token Shims          • Native Semantic Layer Projections
```

This document explicitly defines what remains Chromium-native in v1, what KAGE can inspect and modify via CEF and CDP, the exact GPU and compositing boundaries, and the technical migration path to an eventual minimal Chromium fork.

---

## 2. Chromium Rendering Pipeline Overview

To understand what KAGE can modify versus what is strictly internal to Blink and Chromium, consider the canonical 7-stage Chromium rendering pipeline:

```
1. PARSING      HTML/XML/SVG Tokenization  ──► DOM Tree Construction
                   │
2. STYLE        CSS Tokenization & Cascade ──► Computed Style Tree (RenderStyle)
                   │
3. LAYOUT       Box Sizing & Positioning   ──► LayoutObject Tree (LayoutNG)
                   │
4. PAINT        Display Item Generation    ──► PaintArtifactCompositor
                   │
5. COMPOSITING  Layerization & Tile Matrix ──► Composited Layer Tree (cc)
                   │
6. RASTER       GPU Shader & Canvas Draw   ──► Raster Worker / Viz
                   │
7. DRAW         Quad Aggregation           ──► OS Window Surface / Swapchain
```

---

## 3. Era 1 (v1.0): What Remains Chromium-Native

In v1, KAGE treats the internal C++ execution of stages 3 through 7 as an authoritative, accelerated black box. The following components remain 100% Chromium-native:

1. **Blink Layout Engine (LayoutNG):**
   - The recursive layout algorithms for Block, Inline, Flexbox, CSS Grid, Table, and Multi-column layout.
   - Text shaping, font fallback, bidirectional text analysis via HarfBuzz.
   - Fragmentation (break token calculation for multicol and print).

2. **Paint & Display Item Generation:**
   - Generation of paint display item lists (`cc::DisplayItemList`).
   - Culling of off-screen primitives and clip-rect tree generation.

3. **Chromium Compositor (`cc`):**
   - Layerization heuristics (which DOM subtrees get promoted to composited GPU layers).
   - Tiling engine and scroll offset management on the compositor thread.

4. **GPU Rasterization & Viz Display Compositor:**
   - Skia/DirectWrite/CoreText rasterization of vector paths and glyphs.
   - Generation of draw quads (`viz::DrawQuad`) and submission to the native graphics swapchain (DirectX 11/12 via ANGLE on Windows, Metal on macOS, Vulkan/OpenGL on Linux).

---

## 4. Era 1 (v1.0): Capabilities via CEF & CDP

While the core layout and rasterization code is native Chromium, KAGE leverages CEF hooks and the Chrome DevTools Protocol (CDP) to achieve extraordinary control over the DOM, styles, network assets, and visual presentation.

```
┌────────────────────────────────────────────────────────────────────────┐
│                        KAGE v1 CONTROL MATRIX                          │
├───────────────────┬───────────────────┬────────────────────────────────┤
│  CONTROL SURFACE  │  MECHANISM        │  KAGE CAPABILITY               │
├───────────────────┼───────────────────┼────────────────────────────────┤
│  DOM Tree         │ CDP `DOM` Domain  │ • Live node insertion / delete │
│                   │ CEF Frame Exec    │ • Attribute mutation           │
│                   │                   │ • OuterHTML hot-swapping       │
├───────────────────┼───────────────────┼────────────────────────────────┤
│  CSS & Cascades   │ CDP `CSS` Domain  │ • Forced pseudo-states (:hover)│
│                   │ CEF Style Inject  │ • Inline style mutation        │
│                   │                   │ • Style rule replacement       │
│                   │                   │ • Live CSS variable injection  │
├───────────────────┼───────────────────┼────────────────────────────────┤
│  Viewport & Media │ CDP `Emulation`   │ • Device metric overrides      │
│                   │ CDP `Page` Domain │ • Dark/light media simulation  │
│                   │                   │ • High-contrast simulation     │
│                   │                   │ • Geolocation / Timezone       │
├───────────────────┼───────────────────┼────────────────────────────────┤
│  Visual Overlays  │ KAGE React Shell  │ • Box model highlighting       │
│                   │ CDP `Overlay`     │ • Distance measurement rulers  │
│                   │                   │ • Grid & Flexbox overlays      │
│                   │                   │ • Semantic accessibility rings │
├───────────────────┼───────────────────┼────────────────────────────────┤
│  Assets & Network │ CEF Resource Req  │ • CSS stylesheet redirection   │
│                   │ CDP `Fetch`       │ • Image placeholder swapping   │
│                   │                   │ • Script mock / interception   │
└───────────────────┴───────────────────┴────────────────────────────────┘
```

### 4.1 Live Style Manipulation Pipeline
When a developer edits a style property in Micro Inspect or instructs the AI agent to "make this banner responsive":
1. KAGE sends a `CSS.setStyleTexts` command over CDP specifying the style sheet ID and target range.
2. Blink invalidates only the computed styles affected by the selector change.
3. Chromium's incremental style resolver recalculates `RenderStyle` for the affected subtree.
4. Blink performs an incremental layout pass without reloading the document, updating the screen in `< 16 ms`.

### 4.2 Resource & Stylesheet Override
CEF's `CefResourceRequestHandler` allows KAGE to intercept network requests for stylesheets (`.css`) or scripts (`.js`):
- Local overrides: Developers can point a production CSS file to a local workspace file on their machine.
- Inject user-agent reset styles or custom debug utility classes into any page on load.

---

## 5. Compositing Limitations & The GPU Boundary

To build a stable product, developers must understand what CEF's architecture **cannot** do without modifying Chromium source code:

### 5.1 Native Windowed Embedding vs. In-Process Compositing
KAGE v1 embeds CEF as a native child window (`CefWindowInfo::SetAsChild`). This has major rendering implications:
- **Zero-Copy Performance:** CEF renders directly into its own OS window handle backed by its own DirectX/Metal swapchain. There is zero memory copying of pixel buffers into the Tauri host.
- **Occlusion & Clipping:** A native child window sits above or below other native surfaces according to OS Z-order. 
  - Standard HTML elements in the React shell **cannot** render behind a transparent CEF window without using Off-Screen Rendering (OSR).
  - OSR was evaluated and rejected for v1 because OSR forces CPU/GPU memory readbacks for every frame, reducing 120Hz displays to 45–60Hz under heavy DOM loads and consuming 300% more power.
- **KAGE's Floating Overlay Strategy:** 
  Micro Inspect tooltips, element highlights, and measurement rulers are drawn as **transparent native child overlays** or rendered inside the CEF viewport via CDP's native `Overlay` domain.

```
┌─────────────────────────────────────────────────────────────┐
│ Top-Level Native Window (Tauri)                             │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ CEF Browser Window (DirectX Swapchain - Z: 1)         │  │
│  │                                                       │  │
│  │   [ Web Page Document ]                               │  │
│  │                                                       │  │
│  │   ┌────────────────────────────────────────────────┐  │  │
│  │   │ CDP Overlay Layer (Compositor Level - Z: 2)    │  │  │
│  │   │  • Margin / Padding Box Highlights             │  │  │
│  │   │  • CSS Grid Track Lines                        │  │  │
│  │   └────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ Floating Micro Inspect Card (React Native Popover Z: 3│  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

---

## 6. Rendering Telemetry & Performance Instrumentation

KAGE extracts deep telemetry from Chromium's internal rendering pipeline to feed the Performance Panel and AI Context Engine:

### 6.1 Core Web Vitals & Paint Timings
Through CDP's `Performance` and `PerformanceTimeline` domains:
- **First Contentful Paint (FCP):** Exact timestamp from `PerformanceObserver`.
- **Largest Contentful Paint (LCP):** Active DOM element reference, load time, and render time.
- **Cumulative Layout Shift (CLS):** Real-time tracking of unstable DOM nodes, sources, and layout shift values.
- **Interaction to Next Paint (INP):** Event latency from user click/keypress to frame presentation.

### 6.2 Rendering Metrics via `Performance.getMetrics`
KAGE samples low-level pipeline statistics every 500ms during profiling:
- `LayoutCount` & `RecalcStyleCount`: Detect runaway CSS recalculations.
- `LayoutDuration` & `RecalcStyleDuration`: Measure CPU bottlenecks in Blink.
- `JSHeapUsedSize`: V8 memory pressure.
- `Frames`: Compositor frame delivery and dropped frame rate.

---

## 7. Era 2 (Future): The KAGE Chromium Minimal Fork

Once KAGE establishes market adoption and financial sustainability, the project will evaluate a minimal fork of Chromium to unlock capabilities impossible through CEF/CDP alone:

```
┌────────────────────────────────────────────────────────────────────────┐
│                   ERA 2: MINIMAL FORK CAPABILITIES                     │
├───────────────────────┬────────────────────────────────────────────────┤
│  SUBSYSTEM            │  PROPOSED CUSTOM EXTENSIONS                   │
├───────────────────────┼────────────────────────────────────────────────┤
│  Blink LayoutNG       │ • Custom developer layout modes (Wireframe,    │
│                       │   Depth-3D, Semantic-Only projection)          │
│                       │ • Custom CSS layout constraints without JS     │
│                       │ • True sub-pixel layout boundary debugging     │
├───────────────────────┼────────────────────────────────────────────────┤
│  Style Engine         │ • Live CSS dependency graph generated during   │
│                       │   cascade evaluation (zero-overhead tracing)   │
│                       │ • Native runtime CSS isolation for components  │
├───────────────────────┼────────────────────────────────────────────────┤
│  Paint / Raster       │ • Inverted paint pass for visual contrast      │
│                       │ • Visual heatmaps of paint cost directly in    │
│                       │   the compositor surface                       │
├───────────────────────┼────────────────────────────────────────────────┤
│  Compositor (`cc`)    │ • Full 3D exploded layer view directly inside  │
│                       │   the viewport without CDP latency             │
└───────────────────────┴────────────────────────────────────────────────┘
```

### 7.1 The "Minimal Fork" Philosophy
If a fork occurs, KAGE will strictly reject maintaining a monolithic divergent codebase:
- Maintain custom code as **isolated patch sets** applied via automated git rebase scripts against upstream Chromium tags.
- Keep 98% of Chromium untouched.
- Limit modifications strictly to `//third_party/blink/renderer/core/layout` and `//cc/layers`.

---

## 8. Migration Path: From CEF to KAGE Chromium

To ensure that code written for v1 does not have to be rewritten if KAGE transitions to a custom engine, all rendering interactions are abstracted behind a unified Rust trait:

```rust
// Unified Abstraction Layer
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

- **v1 Implementation:** `CefCdpEngineBackend` implements this trait using CEF native calls and CDP WebSocket commands.
- **Future Fork Implementation:** `KageChromiumEngineBackend` implements this trait using direct shared-memory or internal IPC channels.
- **Impact on UI & AI:** Zero changes to the React shell, Micro Inspect, or AI subsystem when the underlying engine backend evolves.
