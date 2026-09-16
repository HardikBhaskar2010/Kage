# KAGE Performance Benchmarks Specification

| Document Metadata | Details |
|---|---|
| **Document ID** | KAGE-ARCH-007 |
| **Status** | Approved Core Specification |
| **Version** | v0.2.1 |
| **Last Updated** | 2026-09-16 |
| **Target Milestone** | v1.0.0 MVP |
| **Classification** | Performance Engineering, Metrics & Benchmarking |

---

## 1. Executive Summary & Philosophy

A developer browser must never feel sluggish, bloated, or heavy. While traditional developer extensions and companion apps introduce substantial memory overhead and IPC lag, KAGE is engineered to run at near-native hardware speed.

> [!NOTE]
> **Engineering Benchmark Standard:** The metrics defined in this document represent **empirical benchmark targets** measured under standardized hardware conditions, rather than unconditional guarantees. They serve as continuous quality gates in automated performance testing.

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

## 2. Benchmark Target Matrix

All metrics are benchmarked on the reference hardware baseline:
*Reference System: Intel Core i7-12700H / Apple M2, 16GB RAM, NVMe SSD, Windows 11 / macOS 14.*

| Operational Domain | Benchmark Metric | Target Threshold | Measurement Boundary |
|---|---|---|---|
| **Startup Lifecycle** | Cold Launch to Interactive Shell | **< 1,200 ms** | Process spawn to first user input in Omnibox. |
| **Startup Lifecycle** | Warm Launch | **< 600 ms** | Process spawn with warm OS disk cache. |
| **Memory Footprint** | Idle Memory (Shell + 1 Empty Tab) | **< 220 MB** | Private working set across Host + Subprocesses. |
| **Memory Footprint** | Incremental Tab Overhead (Idle) | **< 45 MB / tab** | Background tab after timer throttling. |
| **Tool Bus Pipeline** | Tool Dispatch & Schema Validation | **< 5 ms** | Invocation request to driver execution start. |
| **Micro Inspect** | Hover Hit-Testing & Bounding Box | **< 8 ms** | Native mouse move to overlay quad calculation. |
| **Micro Inspect** | Compositor Scroll Tracking | **60 – 120 Hz** | Highlight frame delivery during native scroll. |
| **Context Engine** | Context Pack Synthesis (Pruned) | **< 25 ms** | Telemetry buffer read to completed token string. |
| **Context Engine** | Buffer Swap on Tab Switch | **< 2 ms** | Active ring buffer pointer swap. |
| **Workspace System**| Atomic Workspace Switching | **< 200 ms** | Click workspace pill to active tab render. |
| **AI Subsystem** | First-Token Latency (Fast Route) | **< 600 ms** | Prompt dispatch to first streamed token (broadband). |
| **Testing Lab** | Deterministic Action Replay Pacing | **< 15 ms / step** | Synthetic input dispatch to DOM event delivery. |

---

## 3. Benchmarking Methodology & Profiling Tools

To prevent subjective assessments, KAGE automates performance metrics via dedicated instrumentation:

### 3.1 Trace Points & Measurement Harness
- **Host Instrumentation:** High-resolution timers (`std::time::Instant`) instrument every phase of the startup pipeline and Tool Bus dispatch:
  ```rust
  let start = std::time::Instant::now();
  let result = tool_bus.dispatch(&tool_name, args).await?;
  let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
  telemetry::record_metric("tool_bus_dispatch_ms", duration_ms);
  ```
- **CEF / Chromium Tracing:** Automated trace capture using CDP `Tracing.start` with categories:
  - `blink`, `blink.user_timing`, `v8.execute`, `cc`, `viz`.
- **Memory Tracking:** System private working set queried via native OS APIs (`GetProcessMemoryInfo` on Windows, `task_info` on macOS).

---

## 4. Performance Degradation Safeguards

KAGE implements automated runtime safeguards when resource limits are approached:

1. **Tab Memory Discarding:**
   - Threshold: When total KAGE memory usage exceeds 1.5 GB, background tabs are ranked by `last_accessed_ms`.
   - Action: Oldest background tabs have their CEF browser instances destroyed and their state serialized to SQLite.
2. **Context Engine Token Throttling:**
   - Threshold: When a DOM tree exceeds 15,000 nodes, the Context Engine increases repetition collapsing aggressiveness to keep the Context Pack strictly within token budgets.
3. **Micro Inspect Frame Throttling:**
   - Threshold: If hover hit-testing takes `> 16 ms`, the inspector switches to bounding-quad cache mode and reduces CDP query frequency to 30 Hz until cursor movement stabilizes.

---

## 5. Continuous Performance Regression Gates (CI)

Every release candidate executes an automated benchmark suite:
- A regression of `> 15%` in cold startup time or idle memory triggers a build failure in CI.
- Automated benchmark summaries are appended to release notes for full developer transparency.
