# KAGE — Requirements Specification

**Status:** Approved v0.2.1 · **Companion to:** [PRD.md](PRD.md), [Design.md](../Design.md)

Numbering: `FR-<module>.<n>` for functional requirements, `NFR-<n>` for non-functional. Each FR has a priority: **P0** (MVP-blocking), **P1** (v1 target), **P2** (later).

---

## 1. Browser Shell (FR-SHELL)

KAGE owns its browser UI. These are not novel features — they are the product surface KAGE must own because it is a browser, not an extension.

| ID | Requirement | Priority |
|---|---|---|
| FR-SHELL.1 | Tab strip with full tab management: open, close, pin, drag-to-reorder, group, and tab preview on hover | P0 |
| FR-SHELL.2 | Address / navigation bar: URL display, direct navigation, inline search, inline AI command entry | P0 |
| FR-SHELL.3 | Navigation controls: back, forward, reload (standard + cache-bust reload for developers) | P0 |
| FR-SHELL.4 | Browser toolbar: configurable toolbar with access to KAGE surfaces, extension icons, profile switcher | P0 |
| FR-SHELL.5 | Command Center: universal keyboard-first command surface (fuzzy search, always one keystroke away) | P0 |
| FR-SHELL.6 | KAGE sidebar: persistent or collapsible left rail; surfaces AI Sidebar, Workspaces, Bookmarks, tool launcher | P0 |
| FR-SHELL.7 | Window controls: minimize, maximize, close — platform-native behavior | P0 |
| FR-SHELL.8 | Bookmarks surface: add, organize, tag; workspace-scoped bookmarks as first-class behavior | P1 |
| FR-SHELL.9 | Downloads surface: download queue, progress, MIME type and source visibility | P0 |
| FR-SHELL.10 | Browser menu: settings, profiles, extensions, help — all KAGE-native, not delegates to Chromium's built-in UI | P0 |
| FR-SHELL.11 | Workspace switcher: navigate between active developer workspaces directly from the browser chrome | P1 |
| FR-SHELL.12 | Developer tool launcher: one-click access to Micro Inspect, Developer Panel, Testing Lab from the toolbar | P0 |
| FR-SHELL.13 | Status / diagnostic indicators: persistent indicators in browser chrome for AI state, permission grants, performance budget alerts, network errors | P1 |

---

## 2. Browser Core (FR-CORE)

Baseline Chromium-backed browser functionality. Nothing here is novel — it's the floor KAGE must clear before anything else matters.

| ID | Requirement | Priority |
|---|---|---|
| FR-CORE.1 | Tabs, tab groups, multiple windows | P0 |
| FR-CORE.2 | Profiles (multiple identities) + private/incognito mode | P0 |
| FR-CORE.3 | History, bookmarks, downloads with standard UI | P0 |
| FR-CORE.4 | Find-in-page | P0 |
| FR-CORE.5 | Site permissions & settings (camera, mic, clipboard, notifications, location) surfaced per-origin | P0 |
| FR-CORE.6 | Cookie, cache, localStorage/sessionStorage/IndexedDB management UI | P0 |
| FR-CORE.7 | Service worker registration visibility and control (unregister, update, bypass) | P1 |
| FR-CORE.8 | Password/autofill integration (can defer to OS-level or Chromium's built-in store initially) | P1 |
| FR-CORE.9 | Popup/redirect handling, fullscreen, file handling, printing | P0 |
| FR-CORE.10 | URL bar with search + direct navigation + command palette entry point | P0 |
| FR-CORE.11 | Crash recovery (tab-level, not whole-browser) | P1 |
| FR-CORE.12 | Certificate handling, including developer/self-signed certs for local dev (`localhost`, custom domains) | P0 — this matters more for KAGE than a normal browser; developers constantly hit local HTTPS |
| FR-CORE.13 | Proxy configuration (needed for network interception/replay tooling later) | P1 |
| FR-CORE.14 | Extension support baseline (see Plugin section) | P1 |
| FR-CORE.15 | Accessibility of the browser chrome itself (keyboard nav, screen reader support) | P1 |

---

## 3. Micro Inspect (FR-MICRO)

Micro Inspect is one of KAGE's defining interactions — the moment where "developer browser" becomes concrete. It is not a DevTools sub-feature; it is a first-class KAGE surface.

| ID | Requirement | Priority |
|---|---|---|
| FR-MICRO.1 | Activate Micro Inspect from toolbar button, keyboard shortcut, command palette, or right-click context menu | P0 |
| FR-MICRO.2 | Hovering an element highlights its boundaries using a visible box-model overlay | P0 |
| FR-MICRO.3 | Hover state displays a lightweight tooltip/preview: element tag + class, dimensions (width × height), and primary computed style values (font, color, background) | P0 |
| FR-MICRO.4 | Clicking locks the selected element — overlay persists, full inspection panel opens | P0 |
| FR-MICRO.5 | Locked inspection panel exposes: typography, box model, layout mode (flex/grid/block), colors, positioning, DOM identity (tag, classes, ID, attributes), and full computed styles | P0 |
| FR-MICRO.6 | Measurement mode (Shift key): display distances between selected element and other hovered elements; show responsive breakpoint indicators | P1 |
| FR-MICRO.7 | Locked element can be sent directly to Explain This with one action | P0 |
| FR-MICRO.8 | From the locked panel, user can copy: outer HTML, computed CSS, CSS selector, and specific computed property values | P0 |
| FR-MICRO.9 | Escape key exits Micro Inspect mode and returns to normal browsing | P0 |
| FR-MICRO.10 | Micro Inspect overlay is rendered in the browser chrome layer — cannot be obscured by page content, z-index stacking, or page CSS | P0 |
| FR-MICRO.11 | Element highlighting uses CDP Overlay domain (not JS-injected CSS) — works on pages that block content script injection | P0 |

---

## 4. Explain This (FR-EXPLAIN)

The signature feature. Must work from **any** selectable surface, not just DOM elements.

| ID | Requirement | Priority |
|---|---|---|
| FR-EXPLAIN.1 | User can trigger "Explain This" on: a selected DOM element, a console error/warning, a network request, a performance entry, an accessibility violation, a selected screen region | P0 (element + console + network first; perf/a11y can follow) |
| FR-EXPLAIN.2 | Triggering surfaces: right-click context menu, command palette, keyboard shortcut, AI chat input referencing "this", direct action from Micro Inspect panel | P0 |
| FR-EXPLAIN.3 | System assembles a **Context Bundle** specific to the selection type before calling the model (see Design.md §6 Context Engine) — never a raw full-page dump | P0 |
| FR-EXPLAIN.4 | For a DOM element: bundle includes DOM subtree, computed styles, cascade/specificity chain, box model, ancestry, related accessibility tree node, and (if identifiable) source file/framework hint | P0 |
| FR-EXPLAIN.5 | For a network request: bundle includes request/response headers, payload, timing breakdown, initiator stack, related cookies | P0 |
| FR-EXPLAIN.6 | For a console error: bundle includes the error, stack trace, source-mapped location, surrounding console history, and DOM/network state at time of error if reconstructable | P0 |
| FR-EXPLAIN.7 | Answers must cite *which piece of context* they used so the developer can verify, not just trust | P0 |
| FR-EXPLAIN.8 | Support natural-language questions layered on top of a selection ("why is this overflowing," "make this responsive," "generate a minimal repro") | P0 for debugging-question set, P1 for generative asks |
| FR-EXPLAIN.9 | Cross-browser difference questions ("why does this break in browser X") — best-effort, clearly flagged as inference | P2 |
| FR-EXPLAIN.10 | Explain This must degrade gracefully offline / without API access — falls back to a static info panel (still shows computed styles etc., just no generated explanation) | P1 |

---

## 5. Agentic Tool System (FR-AGENT)

The AI must operate through **explicit, named, permissioned tools** — never raw/unrestricted access to the browser process.

| ID | Requirement | Priority |
|---|---|---|
| FR-AGENT.1 | Define a fixed tool registry (initial set below) with typed inputs/outputs, mirroring CDP domains where possible | P0 |
| FR-AGENT.2 | Every tool call requires an explicit permission grant scoped to: (a) the current tab/origin, (b) a category of action (read-only / mutating / destructive), (c) a session or one-time basis | P0 |
| FR-AGENT.3 | Read-only tools can be pre-approved at a coarser grain than mutating tools | P0 |
| FR-AGENT.4 | Destructive or broad-scope actions always require a fresh, visible confirmation — no silent auto-approval regardless of prior grants | P0 |
| FR-AGENT.5 | Full audit log of every tool call the agent makes, visible to the user, exportable | P0 |
| FR-AGENT.6 | Tool execution is sandboxed per Chromium's existing process/site-isolation boundaries | P0 |
| FR-AGENT.7 | Prompt-injection mitigation: content pulled from a page (DOM text, console output, network bodies) fed into a tool-calling model must be handled as **untrusted data**, never as instructions | P0 — hard security requirement |
| FR-AGENT.8 | User can define reusable, saved multi-step agent workflows ("record interaction → generate test → save") | P1 |
| FR-AGENT.9 | Agent tool calls run on the same instrumentation layer that the Developer Panel itself uses — "what the agent can see" and "what the developer can see" never diverge | P0 |

**Initial tool registry (v1 scope, P0 unless noted):**

```
inspect_dom(selector | node_id)
get_computed_style(node_id)
get_box_model(node_id)
get_accessibility_node(node_id)
get_console_errors(since?)
get_network_requests(filter?)
get_network_request_detail(request_id)
take_screenshot(node_id | viewport)
run_javascript(expression)              -- mutating, always confirmed
modify_css(node_id, property, value)    -- mutating, session-scoped, reversible
modify_dom(node_id, patch)              -- mutating, session-scoped, reversible
simulate_device(preset)
throttle_network(profile)
throttle_cpu(rate)
navigate(url)                           -- mutating, always confirmed
open_devtools_panel(panel)
clear_storage(scope)                    -- destructive, always confirmed
create_test(steps)                      -- P1
replay_request(request_id)              -- P1
compare_pages(url_a, url_b)             -- P2
```

---

## 6. Developer Tooling (FR-DEVTOOLS)

Each sub-area maps to a CDP domain. KAGE builds on top of the Chrome DevTools Protocol rather than reimplementing instrumentation.

| Area | Requirements | Priority | CDP domain(s) |
|---|---|---|---|
| **Elements/DOM** | Live DOM tree, computed styles, box model overlay, pseudo-elements, element states (`:hover`/`:focus` forcing), mutation observation, DOM diffing across two states | P0 | `DOM`, `CSS`, `Overlay` |
| **Console** | Logs/errors/warnings with stack traces, JS execution (REPL), source-mapped references, filtering, persistent logs across navigation, inline AI explain-on-error | P0 | `Runtime`, `Log`, `Debugger` |
| **Network** | Request/response inspector, headers/cookies/payloads, timing waterfall, initiator chain, WS/SSE/fetch/XHR distinct views, request replay, response override, request blocking, HAR export/import | P0 (inspector), P1 (replay/override/blocking) | `Network`, `Fetch` |
| **Performance** | Timeline, CPU profiling, memory profiling, layout/reflow + paint analysis, Web Vitals (LCP/CLS/INP), long-task flagging, resource timing, budgets with pass/fail thresholds | P1 | `Performance`, `Tracing`, `Memory` |
| **Storage** | Cookies, local/session storage, IndexedDB, Cache Storage, service worker state, quota usage — all viewable and editable | P0 | `Storage`, `IndexedDB`, `ServiceWorker` |
| **Accessibility** | Full a11y tree, contrast checking, keyboard-nav simulation, ARIA validation, automated audit (axe-core-equivalent) | P1 | `Accessibility` |
| **Security** | Certificate details, CSP violations surfaced inline, CORS failure explanations, mixed-content warnings, security headers audit | P1 | `Security`, `Network` |
| **Responsive/Device** | Viewport presets, device emulation, touch input simulation, orientation, DPR, geolocation/timezone/locale overrides, UA override where legitimate | P0 (viewport/device), P1 (geo/timezone/locale) | `Emulation` |

---

## 7. Testing Lab (FR-TEST)

Testing Lab is a native KAGE workspace, not a bolted-on panel.

| ID | Requirement | Priority |
|---|---|---|
| FR-TEST.1 | Record a real interaction session (clicks, input, navigation) as a structured step sequence, not just a video | P0 |
| FR-TEST.2 | Replay a recorded sequence deterministically against the same or a different environment (viewport/network/CPU profile) | P0 |
| FR-TEST.3 | Attach assertions to a recorded step: DOM state, console cleanliness, network status, performance budget, accessibility check | P1 |
| FR-TEST.4 | One-click: "save this exact browser state as a reproducible test" from an ad-hoc debugging session, not just from a pre-planned recording | P0 — this is the core UX promise |
| FR-TEST.5 | Passive session buffer: last N minutes of interaction always kept in memory, available to retroactively construct a test | P0 |
| FR-TEST.6 | Test run history + pass/fail reports across runs, per workspace | P1 |
| FR-TEST.7 | Environment profiles: run the same saved test across N device/viewport/network configurations | P2 |
| FR-TEST.8 | Export saved tests to a portable format; Playwright-compatible export is the preferred target | P2 |

---

## 8. Workspace Architecture (FR-WORKSPACE)

Workspaces are the primary organizational unit for developers using KAGE. A workspace is a structured project container, not just a saved browser session.

| ID | Requirement | Priority |
|---|---|---|
| FR-WORKSPACE.1 | Create, name, and switch between workspaces | P1 |
| FR-WORKSPACE.2 | Each workspace persists: its own tab set, tab groups, and pinned tabs | P1 |
| FR-WORKSPACE.3 | Each workspace persists DevTools state: panel layout, watched expressions, breakpoints | P1 |
| FR-WORKSPACE.4 | Each workspace stores an AI context profile: project name, tech stack hints, known issue notes — provided to the agent as background | P1 |
| FR-WORKSPACE.5 | Each workspace is scoped to its own Testing Lab session history and step sequences | P1 |
| FR-WORKSPACE.6 | Each workspace supports developer notes: freeform text notes, optionally pinned to specific URLs or elements | P1 |
| FR-WORKSPACE.7 | Each workspace can have its own saved commands and custom shortcuts | P2 |
| FR-WORKSPACE.8 | AI and plugin permissions are scoped per workspace — grants from one workspace do not carry over | P1 |
| FR-WORKSPACE.9 | Workspace presets (e.g., "frontend debugging," "performance audit," "a11y review") that pre-arrange panels and pre-approve relevant tool scopes | P2 |
| FR-WORKSPACE.10 | Developer profiles — separate customization sets per project/context (theme, sidebar layout, AI config) | P2 |

---

## 9. AI Sidebar (FR-AI-UI)

The AI Sidebar is a UI surface for the Agent System, not the Agent System itself. These requirements govern the presentation layer; the agent architecture is covered in FR-AGENT and FR-AI.

| ID | Requirement | Priority |
|---|---|---|
| FR-AI-UI.1 | Persistent, collapsible AI sidebar anchored in the browser chrome (not rendered inside the web content area) | P0 |
| FR-AI-UI.2 | Current-page context indicator: live summary of what the AI currently knows about the active page (URL, detected framework, open errors, performance state) | P0 |
| FR-AI-UI.3 | Current-selection context indicator: when the user has highlighted text, locked a Micro Inspect element, or selected a network request, the sidebar shows what context is available for that selection | P0 |
| FR-AI-UI.4 | Tool activity visibility: while the agent is executing tool calls, the sidebar shows which tools are running and what they are doing, in plain language | P0 |
| FR-AI-UI.5 | Suggested actions: context-aware quick-start prompts relevant to the current page state (e.g., "3 performance bottlenecks detected — analyze?" shown from the mockup) | P1 |
| FR-AI-UI.6 | Multi-turn conversation history, session-scoped, scrollable | P0 |
| FR-AI-UI.7 | Model/provider selector visible in the sidebar; user can switch provider without leaving context | P1 |
| FR-AI-UI.8 | Permission prompts surface inline in the sidebar — the user grants/denies elevated tool permissions without navigating to a separate settings screen | P0 |
| FR-AI-UI.9 | Tool-call audit visibility: collapsible log of every action the agent took in the current session, in plain language with a timestamp | P0 |
| FR-AI-UI.10 | AI must never appear to have context that it has not actually collected — the sidebar must not display context indicators for data that has not been assembled | P0 — hard UX integrity rule |
| FR-AI-UI.11 | AI sidebar panels visible from mockup: Overview, Performance, Security, Accessibility quick-summary cards as contextual entry points | P1 |

---

## 10. Exploration / Inspiration Mode (FR-EXPLORE)

High legal/ethical surface area — requirements are written narrowly and defensively on purpose. This feature is **P2** overall; it is not central to KAGE's identity in v1. Micro Inspect + Explain This + Performance + Network + Testing are.

| ID | Requirement | Priority |
|---|---|---|
| FR-EXPLORE.1 | Inspect visual structure, component hierarchy, and CSS of any page the browser can already legitimately render | P2 |
| FR-EXPLORE.2 | Best-effort framework/library identification from public signals (bundle names, DOM fingerprints, known class-name patterns) | P2 |
| FR-EXPLORE.3 | "Save inspiration reference" — store a note + non-asset structural summary for later review | P2 |
| FR-EXPLORE.4 | Generating a "starter implementation" from an inspected page must produce **original code inspired by the observed pattern**, never a reproduction of the page's actual copyrighted assets, copy, or verbatim source | P0 — hard constraint if feature ships at all |
| FR-EXPLORE.5 | Must not assist in bypassing authentication, paywalls, robots.txt/ToS-based scraping restrictions, or extracting data a user isn't already authorized to see through normal browsing | P0 — hard constraint |
| FR-EXPLORE.6 | Clear, persistent in-UI distinction between "observing what's already rendered to you" and "scraping/automated extraction" | P2 |

---

## 11. Command System (FR-CMD)

| ID | Requirement | Priority |
|---|---|---|
| FR-CMD.1 | Single command bus: every capability (browser action, devtools action, AI action, plugin action) is registered as a command with a name, description, and handler | P0 |
| FR-CMD.2 | Command palette (fuzzy search, keyboard-driven) as the universal entry point | P0 |
| FR-CMD.3 | Keyboard shortcuts, bindable/rebindable per command | P0 |
| FR-CMD.4 | Context menu integration — commands relevant to the current selection surface automatically | P0 |
| FR-CMD.5 | Natural-language commands routed to the agent when no direct command match exists | P1 |
| FR-CMD.6 | Plugins/scripts can register new commands into the same bus | P1 |

---

## 12. Design System (FR-DESIGN)

KAGE has a single design system. No screen or panel invents its own UI. This prevents visual inconsistency as the product grows.

| ID | Requirement | Priority |
|---|---|---|
| FR-DESIGN.1 | Design token system: all visual values (colors, spacing, radius, blur, shadow, motion, elevation) defined as named tokens, not hardcoded values | P0 |
| FR-DESIGN.2 | Color token set: primary palette tokens (`--kage-sand`, `--kage-blush`, `--kage-rose`, `--kage-crimson`, `--kage-deep`) + semantic aliases (background, surface, border, text-primary, text-secondary, accent, error, success) | P0 |
| FR-DESIGN.3 | Typography token set: Inter as primary UI font; JetBrains Mono for code/data surfaces only; type scale with defined sizes and weights | P0 |
| FR-DESIGN.4 | Spacing scale: 4px base grid; all margins, padding, and gaps use multiples of this scale | P0 |
| FR-DESIGN.5 | Border-radius scale: defined values by surface type (e.g., small for inputs, medium for cards, large for panels) | P0 |
| FR-DESIGN.6 | Glass/blur tokens: defined backdrop-blur values by surface depth (e.g., sidebar glass, overlay glass, tooltip glass) | P0 |
| FR-DESIGN.7 | Elevation/shadow tokens: z-axis layering model for overlapping surfaces; shadow values match elevation level | P0 |
| FR-DESIGN.8 | Motion tokens: defined duration and easing curves for all transitions; no arbitrary animation timings | P1 |
| FR-DESIGN.9 | Component states: every interactive component (buttons, inputs, tabs, cards) has defined default, hover, pressed, focused, disabled, and error states | P0 |
| FR-DESIGN.10 | Light/dark theme architecture: token system supports both themes; dark is the default | P1 |
| FR-DESIGN.11 | Theme presets: ship with at least the canonical KAGE dark theme; user-selectable theme presets as P2 | P2 |

---

## 13. Customization (FR-CUSTOM)

| ID | Requirement | Priority |
|---|---|---|
| FR-CUSTOM.1 | Theming system built on FR-DESIGN token system — user can override tokens to change colors, fonts, and panel appearance | P1 |
| FR-CUSTOM.2 | Panel/layout customization (rearrange, dock/undock, hide/show devtools panels and AI sidebar) | P1 |
| FR-CUSTOM.3 | User-defined commands/workflows/scripts, storable and shareable | P1 |
| FR-CUSTOM.4 | Custom AI agent configuration: system prompts, which tools are pre-approved, which model/provider is used | P1 |

---

## 14. Plugin System (FR-PLUGIN)

KAGE is a browser platform that supports existing web-extension technology while exposing a native developer-oriented plugin layer. See Design.md §12 for the architecture decision. Requirements assume the **hybrid model**.

| ID | Requirement | Priority |
|---|---|---|
| FR-PLUGIN.1 | Support standard Chrome/MV3 extensions unmodified, for ecosystem compatibility | P1 |
| FR-PLUGIN.2 | Native KAGE platform plugin API for capabilities MV3 doesn't expose: new devtools panels, new AI tools, new context providers for Explain This, new command registrations, workspace panel integrations | P1 |
| FR-PLUGIN.3 | Plugin permission model mirrors the agent tool permission model (FR-AGENT.2–4) — explicit, scoped, auditable | P0 |
| FR-PLUGIN.4 | Plugin sandboxing consistent with Chromium's existing renderer/extension isolation — no plugin gets ambient access beyond its declared scope | P0 |
| FR-PLUGIN.5 | Local/private plugin support without requiring marketplace publication | P0 |
| FR-PLUGIN.6 | Versioning/compatibility declarations so plugins can target a specific KAGE API version | P2 |
| FR-PLUGIN.7 | Marketplace/distribution — deferred; not required for MVP or solo use | P2 |

---

## 15. AI Architecture Requirements (FR-AI)

| ID | Requirement | Priority |
|---|---|---|
| FR-AI.1 | Context Engine collects only task-relevant browser state per request — never the entire browser state | P0 |
| FR-AI.2 | Context Selection step determines *which* sources (DOM/CSS/JS/console/network/perf/a11y/screenshot) are relevant to the specific user intent before building the bundle | P0 |
| FR-AI.3 | Tool System exposes the registry from §5 through a standard function-calling interface, model-agnostic | P0 |
| FR-AI.4 | Multi-model routing support (OpenRouter-style) so the user can choose/swap providers | P1 |
| FR-AI.5 | Local model support as a stretch goal, gated on hardware capability detection | P2 |
| FR-AI.6 | All page-sourced content entering the model context is explicitly tagged as untrusted/data, never concatenated in a way that could be mistaken for system instructions | P0 |

---

## 16. Non-Functional Requirements

| ID | Requirement |
|---|---|
| NFR-1 | **Performance:** KAGE's devtools/AI layer must not measurably degrade normal page load/render performance when idle (target: within 5% of stock Chromium on standard benchmarks) |
| NFR-2 | **Security:** All new attack surface (agent tools, plugin API, browser-level permissions) must be threat-modeled against Chromium's existing sandbox/site-isolation guarantees — nothing KAGE adds should weaken them |
| NFR-3 | **Privacy:** Context sent to any AI provider is scoped to what's explicitly needed for the request; no background/ambient telemetry of browsing content to third parties |
| NFR-4 | **Reliability:** A renderer/tab crash must not take down the AI sidebar, other tabs, or in-progress test recordings |
| NFR-5 | **Accessibility:** Browser chrome itself (not just the a11y auditing feature) meets WCAG 2.1 AA for its own UI |
| NFR-6 | **Maintainability:** Any layer built on top of Chromium must track upstream Chromium/CDP releases without requiring a source-level fork-and-patch workflow (see Design.md §3) |
| NFR-7 | **Extensibility:** Core features (Explain This, Testing Lab, Command System) must themselves be built on the same plugin/tool APIs exposed to third parties — dogfooding the platform, not special-casing built-ins |
