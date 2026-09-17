# KAGE — Ground-Truth Project Audit
**Repository:** https://github.com/HardikBhaskar2010/Kage · **Commit audited:** `b5db66d` (9 commits total, `main`)
**Method:** Full clone, static inspection of all Rust crates and TypeScript/React source, `npm run build` + `npm run lint` executed live, Rust workspace test execution attempted (see §9 for why it's NOT VERIFIED).

---

## 1. Executive Summary

Kage is documented as a full Chromium-Embedded-Framework (CEF) desktop browser with a Rust host, a governed AI "Tool Bus," a CDP-based Context Engine, and a Wasm plugin runtime — 25 specification documents, 74 numbered functional requirements, `AGENTS.md` invariants.

The actual repository is a **React/Tauri UI shell with no browser engine behind it**, wrapping a set of small, isolated Rust crates. Of those crates, the **security/policy plumbing (permission tiers, secret redaction, hash-chained audit log) is genuinely well-built and unit-tested**. Everything that would make Kage a *browser* — CEF embedding, real page rendering, a live CDP connection, DOM/network telemetry, an actual LLM call, and a populated Tool Bus — **does not exist yet**. Several of these are explicitly marked `TODO` / "stub" / "scaffold" in the source comments themselves, so this isn't inference — the code says so.

The clearest single artifact: `BrowserWebview.tsx` does not load any web page. It pattern-matches the URL string (`isGitHub`, `isVercel`, `isDocs`) and renders a hand-authored fake replica of GitHub/Vercel/generic doc pages. There is no iframe, no webview, no CEF surface.

---

## 2. What Kage Is Supposed To Be

Per `README.md`, `docs/Design.md`, and the 25-document spec suite:

- A standalone desktop browser: **Tauri (Rust) host + CEF (Chromium) rendering + React "Liquid Glass" UI chrome.**
- A governed AI agent that never touches the browser directly — every action routes through a Rust **Tool Bus** with a 4-tier permission model (Tier 1 auto-allow → Tier 4 forbidden) and an immutable, hash-chained `security_audit.db`.
- An active **Context Engine** that pulls DOM/network/console state over the Chrome DevTools Protocol (CDP), redacts secrets, and packs it into a 4,000-token LLM context.
- A native DevTools reimplementation (Elements, Console, Network, Storage), a "Micro Inspect" hover overlay, a Testing Lab (recorder + headless replay), a Wasm plugin sandbox, and SQLite-backed workspaces.
- Performance budgets: <150MB idle RAM, 60–120 FPS, <100ms tool dispatch, <50ms context assembly.

## 3. What Kage Actually Is

> **Based on the current repository, Kage is currently a themed React/Tauri desktop-app scaffold with a convincing "browser chrome" UI (tab strip, omnibox, AI sidebar, DevTools-styled panels) sitting on top of a handful of small, independently well-written Rust crates that are not wired into the app.** Typing a URL does not load a web page — it swaps in one of a few hardcoded fake page templates. Asking the "AI" a question does not call any LLM — it regex-matches your text against five keywords and prints one of five pre-written sentences. The Tool Bus, Context Engine, CDP broker, and audit database are real, compiling, unit-tested Rust code, but none of them are connected to each other or to the UI in the shipped binary: the CDP client's `call()` method is a literal stub that returns `{"stub": true}`, the Tool Bus registers zero production tools, and the Wasm plugin crate has no Wasm runtime dependency at all.

## 4. Architecture & Repository Map

| Component | Location | Purpose (documented) | Implementation State |
|---|---|---|---|
| Workspace root | `Cargo.toml` | Rust workspace: 5 crates + Tauri host | Compiles as a workspace definition; `spikes/` is a *separate*, non-member workspace |
| Tauri host | `src-tauri/src/{lib,main,ipc}.rs` | Window/CEF lifecycle, typed IPC | Real Tauri app; IPC handlers are mostly hardcoded/no-op stubs (see §5) |
| `kage-core` | `crates/kage-core` (771 LOC) | Tool Bus, permission engine, sanitizer | **Real, tested logic** — but never registers any production tool, and audit persistence is a commented-out `TODO` |
| `kage-cdp` | `crates/kage-cdp` (292 LOC) | WebSocket CDP client to CEF's debug port | Broker/session bookkeeping is real; the actual `CdpClient::call()` is an explicit stub — no live WebSocket is ever opened |
| `kage-context` | `crates/kage-context` (378 LOC) | DOM/console/network context packing, token budget, secret scrubbing | Real, tested packing/budget logic, but has no live data source to consume (depends on `kage-cdp`, which is stubbed) |
| `kage-storage` | `crates/kage-storage` (491 LOC) | SQLite workspaces + hash-chained audit log | **Genuinely solid** — real schema, real SHA-256 hash chain, real tamper-detection tests. Not invoked anywhere in `src-tauri` |
| `kage-plugin` | `crates/kage-plugin` (32 LOC) | Wasm sandbox + capability broker | Two `enum`/`struct` type definitions only. No Wasmtime dependency, no loader, no execution path |
| `spikes/01-cef-tauri-embedding` | `spikes/` | CEF↔Tauri window embedding R&D | Win32 window creation using `windows-sys`; comment reads `"Simulated CEF surface"` — no CEF crate, no real Chromium embedding attempted |
| `src-ui/src/components/BrowserWebview` | `src-ui/src` | Render actual web content | Hardcoded fake page templates keyed off URL substring match (`isGitHub`/`isVercel`/`isDocs`/generic) |
| `src-ui/src/components/AISidebar` | `src-ui/src` | AI chat + agent loop | Keyword-regex → canned reply text; no LLM call of any kind |
| `src-ui/src/services/aiProviderDiscovery.ts` | `src-ui/src` | "Dynamic" provider/model discovery | Static hardcoded provider list (`isConfigured: true` is a literal, not a check) — except one real `fetch` to a local Ollama endpoint |
| `src-ui/src/context/BrowserContext.tsx` | `src-ui/src` | Tab/console/network/DOM state | In-memory React state; DOM tree is a fixed `SAMPLE_DOM_TREE`; console logs generated via `Math.random()` |
| `src-ui/src/components/DevTools` | `src-ui/src` (1,121 LOC) | Elements/Console/Network/Storage panels | UI is built out reasonably; the data it displays is synthetic (see `BrowserContext`), not from CDP |
| `.agents/skills/*` | repo root | Design/animation/security "skill" packs referenced by `AGENTS.md` | Present on disk, but `AGENTS.md` links to them via `file:///c:/Users/sneha/Videos/Kage/...` — a foreign machine's path, unrelated to this repo's location, left over from template scaffolding |

---

## 5. Requirement → Implementation Matrix (Representative Sample)

Requirements.md defines 74 items across 16 categories (`FR-SHELL`, `FR-CORE`, `FR-MICRO`, `FR-EXPLAIN`, `FR-AGENT`, `FR-DEVTOOLS`, `FR-TEST`, `FR-WORKSPACE`, `FR-AI-UI`, `FR-EXPLORE`, `FR-CMD`, `FR-DESIGN`, `FR-CUSTOM`, `FR-PLUGIN`, `FR-AI`, NFRs). Full line-by-line coverage of all 74 is not reproduced here to avoid padding; the following is representative of every category, verified against source.

| ID (representative) | Requirement | Status | Evidence | Remaining Work |
|---|---|---|---|---|
| KAGE-SHELL — Tab lifecycle | Create/close/switch tabs | **DIVERGENT** | `ipc.rs::create_tab` returns a `TabInfo` struct with no backing browser process; `close_tab`/`switch_tab` just `tracing::info!` and return `Ok(())` | Tabs exist only as UI state; needs a real content-hosting layer per tab |
| KAGE-SHELL — Navigation (URL load) | Navigate to a URL, back/forward/reload | **MISSING** | `navigate_to`, `go_back`, `go_forward`, `reload_tab` in `ipc.rs` are all one-line logs returning `Ok(())`; no fetch, no engine call | Requires an actual rendering surface (CEF, WRY webview, or similar) wired to these handlers |
| KAGE-CORE — Web content rendering | Chromium renders real pages via CEF | **MISSING** | No `cef` crate anywhere in `Cargo.lock`; `BrowserWebview.tsx` renders hardcoded fake page templates keyed by URL substring | This is the core product premise; needs the CEF (or alternative engine) integration from scratch |
| KAGE-ARCH-002 — CEF Integration | C-FFI binding, message pump, OSR | **MISSING** | `spikes/01-cef-tauri-embedding` only creates a plain Win32 child window (`windows-sys`), commented `"Simulated CEF surface"`; no CEF binary, no FFI | Entire subsystem unbuilt; the one "spike" toward it doesn't touch CEF |
| KAGE-CORE-002 — Tool Bus dispatch/governance | Typed dispatch, 4-tier policy, cancellation | **PARTIAL** (engine) / **MISSING** (population) | `bus.rs`, `policy.rs` are real, well-tested (7 unit tests total). But `grep` across the repo shows the *only* `.register()` calls are the test `EchoTool` — zero production tools are ever registered | Needs real `KageTool` impls (DOM read, navigation, network, etc.) registered at startup |
| KAGE-CORE-003 — Context Engine (DOM/console/network packing) | Priority-ordered, budget-bound context pack | **PARTIAL** | `pack.rs`/`budget.rs`/`scrubber.rs` implement real prioritization and token budgeting with tests, but the only input is `ContextSignals`, which nothing in production ever populates (would come from `kage-cdp`, which is a stub) | Wire real CDP signal capture into `ContextSignals` |
| KAGE-SEC-001 — Secret redaction | Redact bearer tokens/passwords/cookies before any LLM sees them | **DONE** (logic) | `sanitizer.rs`, exercised by `bus.rs::dispatch_sanitizes_secret_in_output` test — password fields verified `[REDACTED]` | Needs to actually receive real tool output once tools exist |
| KAGE-SEC-003 — Immutable audit log | SHA-256 hash-chained, append-only `security_audit.db` | **DONE** (as a library) / **MISSING** (integration) | `audit.rs` is fully implemented: genesis hash, per-row hash chaining, `verify_chain()`, and a dedicated tamper-detection test that mutates a row and confirms detection. **However**, `AuditDb` is never constructed or `.manage()`d in `src-tauri/src/lib.rs`, `bus.rs` line ~130 has `// TODO: Audit::append(record) — wired in Chunk 7`, and the IPC handlers `get_audit_logs` (always returns `[]`) / `verify_audit_chain` (always returns hardcoded `true`) never touch this database at all | Wire `ToolBus::dispatch` step 6 to actually call `AuditDb::append`; rewrite the two IPC handlers to query/verify the real DB instead of returning constants |
| KAGE-AI-001 — Multi-provider LLM routing | Route to Anthropic/OpenAI/DeepSeek/Ollama models | **DIVERGENT** | `aiProviderDiscovery.ts`: all providers except Ollama are static hardcoded objects with `isConfigured: true` baked in (not derived from any credential check); only Ollama has a real `fetch("http://127.0.0.1:11434/api/tags")` | "Discovery" needs actual API-key validation calls per provider; routing logic to send a real request doesn't exist |
| KAGE-AI-001 — Bounded agent loop (max 10 steps) | Agentic loop with step limit | **MISSING** | No LLM call exists anywhere in `AISidebar.tsx`; a single keyword-regex chooses a `toolId`, and the reply text is one of 5 hardcoded strings regardless of the (discarded) tool response | Loop bounding is moot until there is an actual model call and multi-step tool use to bound |
| KAGE-PLAT-002 — Wasm plugin sandbox | Wasmtime-sandboxed plugin execution | **MISSING** | `kage-plugin/Cargo.toml` has zero Wasm-related dependencies; `lib.rs` is 32 lines of `enum`/`struct` type definitions with no loader, no execution, no capability enforcement | Needs an actual Wasmtime host, module loader, and capability-gated host functions |
| KAGE-DEV-001 — DevTools panels (Elements/Console/Network/Storage) | Native panels backed by live CDP data | **DIVERGENT** | 1,121 LOC of real React UI exists and is reasonably built, but it's fed by `SAMPLE_DOM_TREE` (one fixed fake DOM tree) and `Math.random()`-generated console log entries in `BrowserContext.tsx`, not by CDP | UI shell can likely be kept; needs real CDP `DOM.*`/`Log.*`/`Network.*` event subscriptions to replace the synthetic generators |
| KAGE-CORE-001 — Micro Inspect | Hover-to-inspect via `DOM.getNodeForLocation` | **DIVERGENT** | `ipc.rs::inspect_at_location` hardcodes `backend_node_id = 42` and returns an identical fabricated box model/selector on every single call, regardless of `(x, y)` | Needs real CDP `DOM.getNodeForLocation` → `DOM.getBoxModel` → `CSS.getComputedStyleForNode` pipeline |
| KAGE-ARCH-005 — `eval_js` (JS execution passthrough) | Execute JS in the page via CDP `Runtime.evaluate` | **MISSING** | `ipc.rs::eval_js` logs the command and returns the literal string `"Executed in CEF context"` for any input | No execution occurs; needs real CDP `Runtime.evaluate` call once a live connection exists |
| KAGE-UI-001 — Liquid Glass Design System | Peach→burgundy gradient tokens, JetBrains Mono, translucency | **PARTIAL / plausible DONE** | `src-ui/src/tokens/tokens.css` defines a real token set; component CSS across `TabStrip`, `AISidebar`, etc. consumes them | Not exhaustively diffed against `Design_System.md`, but tokens exist and are used — best-supported UI claim in the repo |
| KAGE-TEST-001/002 — Testing Lab, unit/integration/E2E strategy | Recorder, headless replay, quality gates | **MISSING** | No test-recorder code found; `src-ui` has **zero** `*.test.*`/`*.spec.*` files; Rust side has 29 `#[test]`/`#[tokio::test]` functions total, all unit-level, confined to `kage-core`/`kage-cdp`/`kage-context`/`kage-storage` internals — no integration or E2E harness exists | Frontend needs a test runner + tests from scratch; Rust needs integration tests across crate boundaries |
| KAGE-BLD-001 — Build System (Cargo + Vite, CEF binary packaging) | Cross-platform packaging incl. CEF binaries | **PARTIAL** | Frontend build (`npm run build`) succeeds cleanly (VERIFIED, see §9); Rust workspace build could not be verified in this sandbox (see §9); no CEF binary distribution/caching logic exists because there's no CEF dependency to distribute | N/A until CEF integration exists |

---

## 6. Browser Audit

Most of Phase E's audit areas (rendering, JS execution, iframe/origin behavior, sandboxing, HTTP/cookies/TLS, storage/profiles) are **UNCERTAIN — insufficient repository evidence**, because the subsystems that would exhibit this behavior (CEF rendering, a live CDP connection) do not exist in the codebase. There is nothing to audit for XSS-in-rendered-content, cookie handling, cache behavior, or certificate validation, because no page is ever actually loaded by the application. This absence is itself the primary finding, not a gap in the audit.

The one navigation-adjacent thing that *is* real: `TabInfo.is_secure` in `ipc.rs` is computed as `url.starts_with("https://") || url.is_empty()` — a string-prefix check, not a certificate or connection-security check. This is a correct characterization of what little exists, not a vulnerability in a real TLS path (there is no real TLS path yet).

---

## 7. Security Findings

Concrete, evidence-backed findings only — no speculative vulnerabilities invented.

**Finding 1 — Severity: High — Audit-verification IPC always reports success**
- **Location:** `src-tauri/src/ipc.rs::verify_audit_chain`
- **Root cause:** The handler is `pub async fn verify_audit_chain() -> Result<bool, String> { Ok(true) }` — a hardcoded constant, never calling the real `AuditDb::verify_chain()` in `kage-storage`.
- **Impact:** If any UI surface in the future trusts this IPC command to represent the tamper-evidence guarantee described in `AGENTS.md`/`Security_Model.md`, it will always report the audit trail as intact even if it has been tampered with or never existed. This is not currently exploitable (nothing depends on it yet), but it's a landmine for whoever wires it up under time pressure and assumes the existing endpoint is real.
- **Fix direction:** Delete the stub; route to `AuditDb::verify_chain()` and propagate real errors.

**Finding 2 — Severity: Medium — `get_audit_logs` always returns empty**
- **Location:** `src-tauri/src/ipc.rs::get_audit_logs`
- **Root cause:** Returns `Ok(vec![])` unconditionally; never queries `security_audit.db`.
- **Impact:** Same class as Finding 1 — a security-relevant read path that silently reports "nothing happened" regardless of reality.
- **Fix direction:** Query `AuditDb` and return real rows.

**Finding 3 — Severity: Informational — Tool Bus governance is unreachable in practice**
- **Location:** `crates/kage-core/src/bus.rs`, `src-tauri/src/lib.rs`
- **Root cause:** `ToolBus::new()` starts with an empty registry, and no code path in `src-tauri` ever calls `.register()` with a real tool.
- **Impact:** Not a vulnerability today (there's nothing to exploit because there are no real tools), but it means the "Prime Architectural Invariant" (AI never gets direct browser authority; the Tool Bus mediates everything) is currently **untested in the one place that matters** — the moment a real tool is added, someone must remember to register it *through* the bus rather than wiring it directly from `AISidebar.tsx` to a new Tauri command, which would silently bypass the entire policy/audit pipeline this codebase built well.
- **Fix direction:** Treat "all browser-mutating capability must be added as a `KageTool`, never as a bare `#[tauri::command]`" as a hard PR-review gate before any real navigation/DOM tool lands.

No SQL injection, path traversal, unsafe deserialization, or CSRF/SSRF issues were found in the code that exists, because the code that exists doesn't yet touch a filesystem, execute arbitrary URLs, or accept web-origin input — `kage-storage`'s queries are correctly parameterized (`rusqlite` `params![]` throughout `audit.rs`/`workspace.rs`). No findings are asserted about IPC message validation, origin confusion, or privilege escalation in the browser sense, because those require a rendering/navigation layer this repo doesn't yet have: **UNCERTAIN — insufficient repository evidence**, not "clean."

---

## 8. Code / Architecture Findings

- **Real architectural discipline where it was applied.** `kage-core`'s `PolicyEngine`/`ToolBus`/`SecretSanitizer` and `kage-storage`'s `AuditDb` follow the documented design closely, use `thiserror` for domain errors, avoid raw `.unwrap()` on fallible paths, and have meaningful unit tests (not tautological "it compiles" tests) — including a test that actively tampers with a DB row to prove the hash chain catches it.
- **Integration debt, not code-quality debt, is the dominant problem.** Individually, `kage-cdp`, `kage-context`, and `kage-storage` are reasonably clean Rust. The gap is entirely in `src-tauri` (which never constructs `AuditDb`, never registers tools, never opens a real CDP socket) and in `src-ui` (which never calls a real LLM and never renders real pages). This is a wiring/integration problem, concentrated at the two seams (`lib.rs` and `BrowserWebview.tsx`/`AISidebar.tsx`), not a pervasive quality issue across the whole codebase.
- **`kage-plugin` is scaffolding, not a stub of an implementation.** It has no dependency that could ever execute Wasm (no `wasmtime`, no `wasmer`). This is a documentation/implementation mismatch (`Cargo.toml` docstring claims "Sandboxed Wasm plugin runtime") rather than a broken feature — the feature was never started.
- **Frontend has zero automated test coverage.** 1,931 modules build cleanly (`vite build`), and `oxlint` passes with only 9 stylistic warnings (unused param, effect purity, `setState`-in-effect) and 0 errors — genuinely healthy for a project this size. But there are no `*.test.*`/`*.spec.*` files anywhere under `src-ui/src`, so none of the simulated-data logic, tab-state reducer, or IPC client is regression-tested.
- **Documentation-vs-code drift is systemic, not occasional.** Nearly every subsystem doc describes the *target* architecture in present tense ("The Tool Bus enforces...", "Chromium handles web standards..."), with no visible marker in the docs suite distinguishing "built" from "designed." Someone reading only `README.md`/`AGENTS.md` would reasonably believe CEF is already embedded.
- **`AGENTS.md` contains a foreign, non-portable path.** Its skill-consultation links point to `file:///c:/Users/sneha/Videos/Kage/...` — a different machine's absolute path, not this repo's location. Harmless today (the files exist under `.agents/skills/` regardless), but the links are dead as written and indicate the file was copied from another project's scaffold without adjustment.

---

## 9. Testing & Build Verification

| Check | Result | Notes |
|---|---|---|
| `npm install` (src-ui) | **VERIFIED** | 37 packages, clean install |
| `npm run build` (src-ui, `tsc -b && vite build`) | **VERIFIED — PASS** | 1,931 modules transformed, built in 488ms, no TS errors |
| `npm run lint` (src-ui, oxlint) | **VERIFIED — PASS with warnings** | 9 warnings (React purity/effect hygiene, one unused param), 0 errors, across 39 files/116 rules |
| Frontend test suite | **NOT VERIFIED** | No test files exist to run (`find src -iname "*.test.*" -o -iname "*.spec.*"` → empty) |
| `cargo test --workspace` | **NOT VERIFIED** | This sandbox only had `apt`-installable `rustc 1.75.0`; several transitive dependencies (`windows-link`, `idna_adapter`, `getrandom` 0.4) require a modern toolchain (edition 2024 / Rust ≥1.80+), and network policy blocks `rustup`/`static.rust-lang.org` needed to install one. This is an **environment limitation of this audit**, not a confirmed defect in the repository — treat Rust-side correctness claims in this report as **static-analysis-verified**, not execution-verified. |
| Rust unit test *inventory* (static count, not executed) | For reference | 29 `#[test]`/`#[tokio::test]` functions found: `kage-cdp/broker.rs` (3), `kage-context/{budget,pack,scrubber}.rs` (3 each), `kage-core/bus.rs` (3), `kage-core/policy.rs` (4), `kage-core/sanitizer.rs` (4), `kage-storage/{audit,workspace}.rs` (3 each). `kage-plugin` has none. |
| Tauri/Rust host build (`src-tauri`, full app) | **NOT VERIFIED** | Requires GUI/webview system libraries and platform target not available/attempted in this headless container, on top of the toolchain issue above |
| Packaging (installer/bundle) | **NOT VERIFIED** | Not attempted; no evidence either way |

---

## 10. Documentation Drift

| Claim | Source | Actual Reality | Status |
|---|---|---|---|
| "Chromium handles web standards... via CEF" | `README.md` | No `cef` dependency anywhere in the Rust workspace; the one CEF-related spike simulates a window, not Chromium | **Documented, not implemented** |
| "AI never gets direct browser authority... routes through the Rust Tool Bus" | `AGENTS.md` Prime Invariant | True in the code that exists (`bus.rs`), but moot — there is no AI-to-browser action to gate yet, since there's no LLM call and no registered tools | **Architecturally correct but currently untested/unexercised** |
| "Immutable audit logs" / tamper-evident `security_audit.db` | `AGENTS.md`, `Security_Model.md` | `AuditDb` hash-chaining is real and tested, but nothing in `src-tauri` ever constructs or writes to it; `verify_audit_chain` IPC hardcodes `true` | **Implemented as a library, not integrated — and the integration point that exists is misleading** |
| "Sandboxed Wasm plugin runtime (Wasmtime)" | `docs/07-platform/Plugin_API.md`, `Cargo.toml` description | Zero Wasm runtime dependency; two type definitions only | **Not started** |
| "Multi-provider LLM routing... capability-based dynamic provider discovery" | `docs/04-ai/AI_Architecture.md`, `aiProviderDiscovery.ts` docstring | Static hardcoded provider array; only genuine dynamic check is a local Ollama `fetch` | **Partially true, materially overstated** |
| 74 actionable requirements across 16 categories | `docs/01-product/Requirements.md` | A meaningful fraction map to UI-only or fully stubbed backends (see §5 matrix) | **Requirements catalogue is real; fulfillment is not** |
| Frontend builds cleanly with strict TS and 0 lint errors | (implicit in `AGENTS.md` coding standards) | Confirmed true by direct execution | **Accurate — one of the few claims verified as-is** |
| `.agents/skills/*` links in `AGENTS.md` | `AGENTS.md` §2 | Points to `file:///c:/Users/sneha/Videos/Kage/...`, a different machine's path | **Stale/incorrect, but files exist under the correct relative path** |

No internal contradictions were found *between* the 25 spec documents themselves — the drift is uniformly one-directional (docs ahead of code), not documents disagreeing with each other.

---

## 11. Current State

**Genuinely complete (evidence-backed):**
- Frontend build/lint pipeline (Vite + TS strict + oxlint).
- `kage-core`'s permission-tier policy engine and secret sanitizer, as standalone, tested units.
- `kage-storage`'s SQLite schema + SHA-256 hash-chained audit log, as a standalone, tested library.
- UI chrome breadth: tab strip, omnibox, icon sidebar, settings, downloads, extensions panel, DevTools-styled panels, AI sidebar — all exist as real, reasonably-built React components.
- Local Ollama model discovery via a real HTTP probe.

**Partially complete:**
- Context Engine (`kage-context`): correct prioritization/budgeting *algorithm*, with no live signal source.
- Tool Bus: correct dispatch pipeline, with zero registered production tools and a commented-out audit-append step.
- Liquid Glass design tokens: defined and consumed, not verified against every documented spec value.

**Missing (documented, not implemented):**
- CEF/Chromium embedding and real page rendering.
- Real navigation (`navigate_to`/`go_back`/`go_forward`/`reload_tab` are no-ops).
- Live CDP connection (client is a hardcoded stub).
- Any LLM call / real AI agent loop.
- Wasm plugin runtime.
- Testing Lab (recorder/replay) and any automated test suite for the frontend.
- Micro Inspect's actual DOM-location resolution (currently a fixed fake response).

**Broken:**
- No functionality that "exists but is defective" was found beyond the security-relevant IPC handlers in Finding 1/2 (§7), which are better characterized as unfinished/misleading stubs than as broken logic — the logic they *should* call works fine in isolation.

**Diverges from design:**
- `BrowserWebview` (simulated pages instead of a rendering engine), `AISidebar` (canned replies instead of a Tool-Bus-mediated agent loop), `DevTools` panels (synthetic/random data instead of CDP telemetry), `inspect_at_location`/`eval_js` (fixed/fake outputs regardless of input).

**Risky:**
- The two audit-related IPC handlers that report constant "success" (§7, Findings 1–2) are the one place where the gap between documentation and code could cause real harm later: if a future contributor builds a permission-confirmation UI on top of `verify_audit_chain` and trusts its `true`, they've built a false sense of security. Worth fixing before anyone treats those endpoints as load-bearing.

---

## 12. What Kage Actually Is Today

> Based on the current repository, Kage is currently a polished, well-organized Tauri + React desktop-app skeleton for a browser, with a convincing tab/omnibox/AI-sidebar/DevTools UI, backed by a small set of genuinely well-engineered and unit-tested Rust libraries for permissioning and tamper-evident logging — none of which are yet connected to a real web-rendering engine, a real CDP connection, a real LLM, or each other. If you opened the app today, you could click around a realistic-looking browser chrome, but typing a URL shows a hand-authored fake page, and asking the AI sidebar a question gets you one of five scripted sentences chosen by keyword matching, not a model response.

---

## 13. Remaining Work

### P0 — Blocking
| Task | Why | Dependency | Relevant files/docs |
|---|---|---|---|
| Choose and integrate a real rendering engine (CEF as documented, or a pragmatic alternative such as Tauri's native webview per tab) | Nothing else in the "browser" half of the product can be real until pages actually load | None — this is the root dependency for §5's MISSING items | `docs/02-architecture/CEF_Integration.md`, `spikes/01-cef-tauri-embedding` |
| Wire `kage-cdp::CdpClient::call()` to a real WebSocket connection against whatever engine is chosen | Context Engine, DevTools panels, Micro Inspect, and `eval_js` all depend on real CDP responses | Rendering engine decision above | `crates/kage-cdp/src/client.rs` |
| Wire `AuditDb` into `src-tauri` (construct on startup, append in `ToolBus::dispatch`, back the two audit IPC commands with real queries) | Currently the security-relevant guarantees the docs describe are not actually enforced end-to-end, and two IPC handlers actively lie | None — all pieces exist, this is pure wiring | `crates/kage-storage/src/audit.rs`, `crates/kage-core/src/bus.rs`, `src-tauri/src/ipc.rs` |

### P1 — Core
| Task | Why | Dependency | Relevant files/docs |
|---|---|---|---|
| Implement and register real `KageTool`s (DOM read, navigate, click, network read) through `ToolBus::register` | The Tool Bus governance pipeline is built but has nothing to govern | P0 rendering + CDP | `crates/kage-core/src/tool.rs`, `docs/03-core/Tool_System.md` |
| Replace `BrowserContext.tsx`'s synthetic DOM tree / random console logs with real CDP event subscriptions | DevTools panels currently show fake data | P0 CDP wiring | `src-ui/src/context/BrowserContext.tsx` |
| Add a real LLM call path (at minimum one provider) and remove the keyword-regex canned-reply logic in `AISidebar.tsx` | The AI sidebar is the headline feature and currently does not call a model | P1 registered tools (so the agent has something to call) | `src-ui/src/components/AISidebar/AISidebar.tsx`, `docs/04-ai/AI_Architecture.md` |
| Implement `inspect_at_location`/`inspect_node` against real `DOM.getNodeForLocation`/`getBoxModel`/`getComputedStyleForNode` | Micro Inspect currently returns identical fake data for every call | P0 CDP wiring | `src-tauri/src/ipc.rs`, `docs/03-core/Micro_Inspect.md` |

### P2 — Engineering
| Task | Why | Dependency | Relevant files/docs |
|---|---|---|---|
| Add a frontend test suite (component + IPC client) | Currently zero automated coverage on the UI/state layer | None | `src-ui/` |
| Add cross-crate integration tests (Tool Bus → Audit → Storage round trip) | Unit tests exist per-crate but nothing exercises the wiring once it's added | P0 audit wiring | `crates/*` |
| Give `kage-plugin` an actual Wasmtime dependency and minimal load/execute path, or explicitly re-scope it in the docs as future work | Docs and `Cargo.toml` description overstate current state | None | `crates/kage-plugin/`, `docs/07-platform/Plugin_API.md` |
| Fix the two stale `.agents/skills/*` file paths in `AGENTS.md` | Currently dead links to another machine's filesystem | None | `AGENTS.md` |

### P3 — Future
| Task | Why | Dependency | Relevant files/docs |
|---|---|---|---|
| Testing Lab (recorder + headless replay runner) | Documented but entirely unbuilt; depends on a real rendering/CDP stack existing first | P0/P1 complete | `docs/06-testing/Testing_Lab.md` |
| Multi-provider LLM routing with real credential validation per provider (not just Ollama) | Currently only one provider has a genuine connectivity check | P1 LLM call path | `src-ui/src/services/aiProviderDiscovery.ts` |
| Chromium-fork migration path | Explicitly a long-term item even in the docs | Full CEF integration first | `docs/02-architecture/Migration_to_Chromium_Fork.md` |

---

## 14. Recommended Development Sequence

```
1. Land a real rendering engine (CEF or webview-per-tab)
      → enables real navigation IPC handlers
      → enables a live CDP connection

2. Wire kage-cdp::CdpClient to that live connection
      → enables Context Engine to receive real ContextSignals
      → enables DevTools panels to show real DOM/console/network data
      → enables Micro Inspect to resolve real nodes
      → enables eval_js to actually execute

3. Wire AuditDb into ToolBus::dispatch and the two audit IPC handlers
      → makes the "immutable audit log" claim true end-to-end
      → unblocks safely registering real KageTools (P1)

4. Register real KageTools (dom.read, navigate, click, network.read) via ToolBus
      → gives the AI sidebar something real to call

5. Add an actual LLM call path in AISidebar, replacing the keyword-regex mock
      → the agent loop / Tier-based confirmation UX can only be honestly tested here

6. Backfill tests: frontend test suite + cross-crate integration tests
      → do this once the wiring in steps 1–5 is stable, not before (avoids testing throwaway stubs)

7. Testing Lab, Wasm plugin runtime, multi-provider routing, Chromium-fork migration
      → all explicitly downstream/future work per the docs themselves
```

---

## 15. Completion Snapshot

| Subsystem | State | Key Missing Piece |
|---|---|---|
| Architecture (docs) | Complete as a spec | N/A — this is the one fully "done" artifact, being documentation |
| Browser Shell | UI-only | Real tab/navigation backing (currently no-op IPC) |
| Rendering (CEF) | Not started | No CEF dependency exists anywhere in the repo |
| Navigation | Stubbed | Every navigation IPC command is a logged no-op |
| CDP / DevTools data | Stubbed / synthetic | `CdpClient::call()` returns a hardcoded stub value |
| Context Engine | Library-complete, unwired | No real `ContextSignals` producer |
| Tool Bus / Security policy | Implemented & tested | Zero production tools registered |
| Audit log | Implemented & tested | Never constructed or called from `src-tauri` |
| AI / LLM | Not started | No LLM API call exists; regex + canned text only |
| Plugin (Wasm) | Not started | No Wasm runtime dependency |
| Testing | Rust: unit-only / Frontend: none | No integration/E2E harness, no frontend tests |
| Frontend build/lint | Working | None — verified clean |

---

## 16. Final Ground-Truth Summary

**If I stopped reading the documentation completely and only looked at the repository today, what would I believe Kage currently is capable of doing?**

I would believe Kage is a desktop app that opens to a stylish, dark/peach-toned browser-style UI — tabs, an address bar, an icon sidebar, a DevTools-style panel set, and a chat sidebar. I would believe I can "open" a handful of specific sites (GitHub, Vercel, a generic docs layout) because those exact templates are hardcoded, but I would discover I cannot actually browse the web, because no URL results in real content — it's always one of a few pre-built fake pages or a generic placeholder. I would believe the AI sidebar can respond to messages, but on inspection I'd find it never calls any model — it matches a few keywords and prints one of five fixed sentences. I would believe there's a permission system and an audit trail because the code for both is real and tested — but I'd find neither is actually turned on: no tool is ever dispatched through it in production, and the two IPC endpoints meant to expose the audit log are hardcoded to say "everything's fine, log is empty." In short: a browser-shaped shell around some solid, isolated security-engineering building blocks, with the browser itself not yet built.
