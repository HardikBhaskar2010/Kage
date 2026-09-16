# KAGE — Technical Design Document

**Status:** Approved v0.2.1 · **Master Index:** [README.md](README.md) · **Companion to:** [PRD.md](01-product/PRD.md), [Requirements.md](01-product/Requirements.md)

---

## 1. Design Principles

1. **Never rebuild what Chromium already does correctly.** Rendering, JS execution, sandboxing, site isolation, certificate handling — all inherited, not reimplemented.
2. **Everything is a tool call.** The AI, the command palette, and KAGE's own built-in panels all route through the same instrumentation and permission layer. No special-cased "trusted" internal path that bypasses the rules third-party plugins follow.
3. **Context is assembled, not dumped.** Every AI-facing feature goes through a Context Engine that decides *what's relevant*, never a raw serialize-everything approach.
4. **Untrusted by default.** Anything sourced from a web page (DOM text, console strings, network bodies) is data, never instruction, when it reaches a model. This is a security requirement, not a style choice (see §7).
5. **Solo-dev-buildable.** Every architectural choice is filtered through "can one person, using AI-assisted coding, actually build and maintain this" — not "what would a 200-person browser team build."
6. **Browser-first, developer-first.** KAGE is a complete browser, not an extension attached to another browser. Every browser surface — tabs, address bar, navigation, profiles — is designed around developer workflows.
7. **Progressive disclosure.** Advanced tooling is available instantly without forcing advanced UI onto every user. Simple browsing remains visually calm; developer power appears when invoked.
8. **Spatial clarity over information density.** KAGE prioritizes spacious layouts, strong visual hierarchy, translucent surfaces, and focused panels over dense DevTools-style information walls. Liquid Glass is a design constraint, not a decoration.

---

## 2. Product Architecture

KAGE is a browser. Chromium provides rendering; KAGE owns everything the user sees and touches.

```
                         KAGE BROWSER
                              │
              ┌───────────────┼────────────────┐
              │               │                │
         Browser Core     Developer Core    KAGE UI
              │               │                │
          Chromium/Blink     CDP/DevTools     Liquid Glass
          Tabs/Windows       DOM              Command Center
          Profiles           CSS              AI Sidebar
          Permissions        Network          Micro Inspect
          Storage            Runtime          Workspaces
                              │
                    ┌─────────┴─────────┐
                    │                   │
              Context Engine       Testing Lab
                    │                   │
                 AI Agent          Record/Replay
                    │              Assertions
                    │              Reports
                    └────────┬──────────┘
                             │
                        Command Bus
                             │
                         Plugin API
```

**Layer responsibilities:**

- **Browser Core** — wraps the Chromium/Blink rendering engine; manages tabs, windows, profiles, permissions, and storage. KAGE owns this shell entirely; Chromium owns the actual render process.
- **Developer Core** — the CDP/DevTools instrumentation layer; exposes DOM, CSS, Network, Runtime, Performance, Emulation, Accessibility domains as a typed internal API. The same layer powers Micro Inspect, Testing Lab, and AI tool calls.
- **KAGE UI** — every surface the user touches: browser chrome, tabs, address bar, sidebar, Command Center, AI Sidebar. Built on the Liquid Glass design system (see §13).
- **Context Engine** — assembles AI-facing context from CDP reads; never dumps raw data (see §6).
- **Testing Lab** — native KAGE workspace for recording, assertion, replay, and reporting (see §9).
- **Command Bus** — single dispatch point for every action: keyboard shortcut, command palette, context menu, AI-resolved command, plugin call.
- **Plugin API** — plugins register commands, tools, context providers, and UI panels through the same interface KAGE's built-ins use.

---

## 3. Chromium Integration Strategy

### 3.1 Embedding Decision

Four options were evaluated. One is rejected; one is the current leading direction.

#### Option A — Fork Chromium from source *(rejected for v1)*
Chromium is ~35 million lines of C++, requires `depot_tools`, multi-hour builds, 100 GB+ disk, and ongoing rebasing work to stay current with upstream security patches. This is what Brave, Edge, Opera, and Arc do — because they have dedicated build infrastructure and multi-person teams.

**Verdict:** rejected. It front-loads months of infrastructure work before a single product feature exists. Revisit only if KAGE needs to change Blink/V8 behavior itself — nothing in the current requirements calls for that.

#### Option B — Chromium Embedded Framework (CEF) *(current leading direction)*
CEF wraps Chromium as a library with a stable C++ API, letting you embed a full browser view inside a native application. You own the window chrome, tab strip, address bar, and every visible surface — Chromium handles all rendering, JS execution, and sandboxing underneath.

This is the right fit for KAGE's actual product: a browser the user *lives in*, not a companion hovering over another browser. The critical distinction is that CEF lets KAGE *be* the browser shell while delegating web compatibility entirely to Chromium.

**Verdict:** CEF is the current leading direction for v1. The implementation should be structured so the embedding layer (CEF ↔ browser shell interface) is isolated behind an abstraction boundary — keeping the door open to tighter Chromium API integration later without rewriting the whole product.

#### Option C — Stock browser + MV3 extension + CDP companion *(no longer fits product vision)*
This was the original v1 recommendation. It is a viable architecture for an *extension product* — a tool that enhances a browser the user already has. KAGE is not that. Running on top of Chrome/Edge means KAGE can never own the tab strip, address bar, window chrome, or browser identity. That is a fundamental product conflict.

**Verdict:** no longer the target architecture. The CDP instrumentation design, Command Bus, Context Engine, and Testing Lab from this model are *fully preserved* — only the outer container changes from "stock Chromium + overlay" to "CEF-based KAGE Browser Shell."

#### Option D — Raw Chromium APIs (Electron-style)
Lower-level than CEF; gives deeper access but removes CEF's stability guarantees. Appropriate if KAGE eventually needs Node.js-style renderer integration, but not a v1 requirement.

**Verdict:** future option. Not needed now.

### 3.2 CDP Layer

Regardless of embedding strategy, the **Chrome DevTools Protocol** is the instrumentation backbone — the same protocol DevTools, Playwright, Puppeteer, and Lighthouse are built on. Every Agentic Tool in Requirements.md §3 maps directly onto a CDP domain:

| Domain | What KAGE uses it for |
|---|---|
| `DOM` | Element inspection, Micro Inspect, Testing Lab selectors |
| `CSS` | Computed styles, cascade, box model |
| `Network` | Request/response capture, header inspection, performance |
| `Runtime` | JS evaluation, property inspection, error capture |
| `Performance` | Web Vitals, long tasks, rendering metrics |
| `Emulation` | Device/network/CPU throttling |
| `Accessibility` | A11y tree, violation detection |
| `Page` / `Input` | Navigation, Testing Lab replay |

### 3.3 Upgrade Strategy

CEF releases track Chromium with a short lag. Security patches require updating the CEF version pin — not rebuilding from source. This is a known, manageable maintenance burden suitable for a solo developer.

---

## 4. Browser Shell Architecture

KAGE owns the browser experience. Chromium owns web compatibility and rendering. That distinction is a first-class architectural principle.

### 4.1 Browser Chrome Surfaces

| Surface | Description |
|---|---|
| **Tab strip** | Full tab management: groups, pinning, hibernation, per-tab context indicators |
| **Address / search bar** | URL input, search, inline AI commands, context-aware completions |
| **Navigation controls** | Back, forward, reload — minimal, developer-aware (e.g., reload with cache-bust) |
| **Window controls** | Minimize, maximize, close; platform-native behavior |
| **Sidebar** | Persistent left or right rail; AI Sidebar, Bookmarks, Workspace switcher |
| **Browser menus** | Settings, Profiles, Extensions, Downloads — all KAGE-native surfaces |
| **Profiles** | Per-project browser identity: separate cookies, storage, extensions, DevTools state |
| **Downloads** | Inline download management with developer-relevant context (MIME, size, source) |
| **Bookmarks** | Developer-oriented: taggable, workspace-scoped, quick-add from Command Center |

### 4.2 KAGE Developer Surfaces

| Surface | Description |
|---|---|
| **Micro Inspect** | Hover-to-inspect, click-to-lock element analysis (see §5 for full architecture) |
| **AI Sidebar** | Persistent agent surface — context-aware developer panel, not a chatbot tab (see §8) |
| **Developer Panel** | Full DOM/CSS/Network/Performance/Console/Storage tool suite, native to KAGE |
| **Command Palette** | Universal keyboard-first command surface: browser actions, developer tools, AI, workspaces |
| **Workspace Switcher** | Navigate between active developer workspaces (see §11) |
| **Status / Diagnostic Indicators** | Persistent browser-chrome indicators: AI active, permission state, network errors, performance budget |

### 4.3 Principle

> KAGE owns the browser experience. Chromium owns web compatibility and rendering.

When something *looks* like a browser decision (tab title, favicon, address display, back-forward), KAGE decides how to present it. When something *is* a web-platform decision (how a CSS property renders, how a JS API behaves), Chromium handles it without interference.

---

## 5. Micro Inspect Architecture

Micro Inspect is not a feature. It is one of KAGE's defining interactions — the moment where "developer browser" becomes concrete for a user.

### 5.1 Pipeline

```
User activates Micro Inspect (keyboard shortcut / toolbar / context menu)
        ↓
Pointer tracking  (native window events — avoids page interference)
        ↓
Element hit testing  (CDP DOM.getNodeForLocation at cursor coordinates)
        ↓
Element highlighting  (CDP Overlay.highlightNode with box model geometry)
        ↓
DOM / CSS / CDP context collection
        │  (Micro Inspect is a primary trigger for the Context Engine pipeline — see §6)
        ↓
Inspection overlay  (KAGE UI glass panel, spatially anchored to element)
        ↓
Optional actions
        │
        ├── Copy HTML
        ├── Copy CSS (computed / cascade / shorthand)
        ├── Measure (distance to other elements, responsive breakpoint analysis)
        ├── Screenshot (element-scoped, full fidelity)
        └── Explain This  (fires full Context Engine → AI Agent pipeline)
```

### 5.2 Interaction Model

| Input | Behavior |
|---|---|
| **Hover** | Live preview overlay — box model, tag, class, computed dimensions. Non-blocking. |
| **Click** | Lock inspection to element — overlay persists, panel expands with full context |
| **Escape** | Exit Micro Inspect mode, return to normal browsing |
| **Shift** | Measurement mode — ruler, spacing between elements, responsive breakpoint indicators |
| **AI shortcut** | From locked element, fire contextual AI analysis ("Explain This", "Suggest fix", "Accessibility audit") |

### 5.3 Implementation Notes

- Highlighting is done via the `CDP Overlay` domain — not JS-injected CSS — so it works even on pages that block content script injection, and never interferes with the page's own layout.
- The inspection panel is a KAGE UI surface rendered *outside* the web content area (browser chrome layer), so it cannot be obscured by page content or z-index conflicts.
- Pointer tracking uses the embedding layer's native event stream, not DOM events — ensuring pixel-accurate hit testing even on GPU-composited layers and iframes.

---

## 6. The Context Engine

This is what makes "Explain This" different from pasting a `<div>` into ChatGPT.

### 6.1 Pipeline

```
Selection event (element / request / error / perf entry / a11y node)
        │
        ▼
Intent Classifier  ──►  what kind of question is this, roughly?
        │                (layout bug / network failure / a11y / perf / "explain generally")
        ▼
Context Selector   ──►  given the selection type + intent, which CDP domains
        │                actually need to be queried? (never "all of them")
        ▼
Context Collectors ──►  parallel CDP calls: DOM.getOuterHTML, CSS.getComputedStyleForNode,
        │                CSS.getMatchedStylesForNode, Accessibility.getPartialAXTree,
        │                Network.getResponseBody, Runtime.getProperties, etc.
        ▼
Context Bundle     ──►  a structured, size-bounded object — not raw dumps.
        │                Long lists (e.g. full stylesheet) get summarized/truncated
        │                with the ability for the model to request more via tool calls.
        ▼
Model + Tool Loop  ──►  model reasons over the bundle, can call further read-only
        │                tools if it needs more context (this is why Tool System
        │                and Context Engine share the same underlying CDP layer)
        ▼
Grounded Answer    ──►  cites which piece of context supports each claim
```

### 6.2 Why "selection → intent → selective collection" and not "dump everything"

Three reasons, in priority order:
1. **Cost / latency** — a full DOM + full stylesheet + full network log for a busy page is enormous; most questions need a small slice of it.
2. **Signal-to-noise** — dumping everything makes the model's job harder, not easier; targeted context produces better answers, not just cheaper ones.
3. **Security** — bounding what's collected also bounds what untrusted page content can smuggle into the model's context window (ties directly into §7).

### 6.3 Context Bundle shape (illustrative)

```json
{
  "selection": { "type": "dom_element", "node_id": 42 },
  "dom": { "outerHTML": "...", "ancestry": ["body", "main", ".card", "button.cta"] },
  "styles": {
    "computed": { "overflow": "hidden", "width": "142px" },
    "cascade": [
      { "selector": ".button", "source": "styles.css:88", "specificity": "0,1,0" }
    ],
    "boxModel": { "content": ["coords"], "padding": 8, "border": 0, "margin": 0 }
  },
  "accessibility": { "role": "button", "name": "Get Started", "violations": [] },
  "relatedConsole": [],
  "relatedNetwork": [],
  "framework_hint": "likely React (fiber props detected)",
  "source_map": null
}
```

Each field is populated only if the Context Selector decided it was relevant to the detected intent.

---

## 7. Agentic Tool & Permission Architecture

### 7.1 Trust Boundary

The security model for every agentic browser in 2026 is being judged on **prompt injection** — hostile instructions hidden in ordinary page content (a comment, an alt-text, a hidden div, an API response) that try to redirect the agent's behavior. Industry consensus is that this class of attack is not fully solvable — only *containable*. KAGE's design goal is containment, not false promises of elimination.

```
┌─────────────────────────────────────────────┐
│   TRUSTED                                    │
│   - User's typed instructions                │
│   - KAGE's own system prompts                │
│   - Tool call results the user explicitly    │
│     approved seeing                          │
├─────────────────────────────────────────────┤
│   UNTRUSTED — always                         │
│   - Page DOM text/attributes                 │
│   - Console output originating from the page │
│   - Network response bodies                  │
│   - Anything returned BY a tool call,        │
│     treated as data to reason about,         │
│     never as a new instruction to follow     │
└─────────────────────────────────────────────┘
```

Implementation rule: untrusted content is always wrapped and tagged distinctly in the prompt sent to the model — inside clearly delimited "page content" blocks with an explicit system instruction that content inside these blocks is data, never commands.

### 7.2 Permission Tiers

| Tier | Examples | Approval model |
|---|---|---|
| **Tier 0 — Read-only, low-risk** | `inspect_dom`, `get_computed_style`, `get_console_errors`, `get_network_requests` | Pre-approvable per-origin, persists across session |
| **Tier 1 — Read-only, sensitive** | `get_network_request_detail` (may include auth headers/cookies), `take_screenshot` | Pre-approvable, but flagged distinctly in the audit log |
| **Tier 2 — Mutating, reversible** | `modify_css`, `modify_dom` (session-scoped, reset on reload) | Approved once per session per origin; persistent "AI is modifying this page" indicator in browser chrome |
| **Tier 3 — Mutating, high-impact** | `run_javascript`, `navigate` | Explicit confirmation every time, showing exactly what will run/where it will go |
| **Tier 4 — Destructive** | `clear_storage`, anything irreversible | Explicit confirmation every time, no batch/auto-approval ever |

### 7.3 Browser-Level Permissions

Because KAGE *is* the browser, an additional permission surface exists above the standard CDP tier model:

| Permission | Description |
|---|---|
| **Page Access** | Can the AI read content from a given tab/origin? |
| **Browser Storage** | Access to cookies, localStorage, IndexedDB across the profile |
| **Network Data** | Access to request/response bodies, headers, timing data |
| **Screenshots** | Full-page or element-scoped capture |
| **Local Files** | File system access for workspace projects |
| **Workspace Data** | Notes, commands, Testing Lab history for a given workspace |
| **AI Providers** | Which models/providers KAGE is authorized to call |
| **Plugins** | Which native KAGE plugins have been granted elevated permissions |

### 7.4 Why This Can't Just Be "Ask Once and Remember Forever"

Because the whole point of the trust boundary in §7.1 is that a compromised Tier-0 read must never be able to *escalate* into approving a Tier-3/4 action on its own. Permission grants are scoped to **user-initiated actions**, never granted as a side effect of content the agent merely read.

### 7.5 Audit Log

Every tool call — tier, target, timestamp, and (for mutating calls) a diff of what changed — is logged and visible in a dedicated panel, exportable as JSON. This is both a debugging aid and a forensic trail if something ever goes wrong.

---

## 8. AI Architecture

### 8.1 The AI Sidebar Is a UI Surface, Not the Agent System

The AI Sidebar is what the user sees and interacts with. The Agent System is what actually does work. These are separate architectural concerns — and keeping them separate prevents KAGE from accidentally becoming "a browser with a ChatGPT sidebar bolted on."

```
AI Sidebar (UI)
│
├── Current Page Context  (live summary: URL, title, detected framework, open errors)
├── Selection Context     (what the user has highlighted or Micro-Inspected)
├── Conversation          (multi-turn, session-scoped)
├── Suggested Actions     (context-aware quick-start prompts)
├── Diagnostics           (active tool calls, permission state, model in use)
├── Tool Call Log         (collapsible — what the agent did, in plain language)
└── History               (past sessions for this workspace, searchable)
```

The Sidebar feeds user intent into the Context Engine → Agent pipeline. It does not contain agent logic itself.

### 8.2 Agent Pipeline

```
User input (typed or voice)
        ↓
Intent Classification
        ↓
Context Engine (§6)
        ↓
Model + Tool Loop
        │
        ├── Read tools      (Tier 0/1 — auto-approved within session grants)
        └── Mutating tools  (Tier 2/3/4 — require explicit user approval each time)
        ↓
Grounded response → AI Sidebar
```

### 8.3 Model Routing

- Model-agnostic; multi-provider routing (OpenRouter-style).
- Provider configured per workspace or globally in settings.
- KAGE does not hard-code a single AI provider — avoids both lock-in and obsolescence as the model landscape continues shifting.

---

## 9. Testing Lab Architecture

Testing Lab is a native KAGE workspace, not a bolted-on panel.

```
Testing Lab
│
├── Recorder              (passive session recording — always on, last N minutes in memory)
├── Step Editor           (visual editor for captured step sequences)
├── Assertions            (per-step DOM / CSS / network / perf / console checks)
├── Replay Engine         (re-dispatches steps via CDP Input domain; supports device/network/CPU profiles)
├── Environment Profiles  (device emulation, network throttling, locale)
├── Run History           (timestamped, per-workspace run results)
├── Reports               (pass/fail, annotated diffs, performance budget violations)
└── Export                (Playwright .ts / .js — step sequences map directly)
```

### 9.1 Recording

The CDP layer listens to `Input.dispatchMouseEvent`, `Input.dispatchKeyEvent`, and `Page.navigate`-level events during live sessions and serializes them into a step sequence (selector + action + optional value), not a video. Steps are deterministic and assertable, not pixel-dependent.

### 9.2 "Save This Exact State as a Test"

The companion always keeps the last N minutes of interaction passively buffered in memory. This means "I already reproduced it" becomes "here is the test" without requiring the developer to have hit Record in advance. This is the core UX promise of Testing Lab.

### 9.3 Assertions

Each step can have assertions backed by CDP reads:
- DOM state via `DOM.querySelector` + `CSS.getComputedStyleForNode`
- Console cleanliness via `Log` / `Runtime.exceptionThrown`
- Network status via `Network`
- Performance budget via `Performance`

### 9.4 Storage

Local, per-workspace, plain JSON step sequences — human-readable, diffable, committable to version control alongside the project under test.

---

## 10. Command System

All user-initiated actions funnel through a single Command Bus.

```
User input
(shortcut / palette / menu / AI / plugin)
        ↓
Command Bus
        │
        ├── Browser commands    (new tab, navigate, reload, profile switch)
        ├── Developer commands  (Micro Inspect, DevTools, screenshots, emulation)
        ├── AI commands         (Explain This, suggest fix, run analysis)
        ├── Workspace commands  (switch, create, configure)
        ├── Testing commands    (record, replay, assert, export)
        └── Plugin commands     (registered by installed native plugins)
```

The palette is always one keystroke away. Every command is addressable by text search, including plugin-registered commands.

---

## 11. Workspace Architecture

Workspaces are the primary organizational unit for developers using KAGE. A workspace is not a saved browser session — it is a structured project container.

```
Workspace
│
├── Tabs                (pinned + active tabs for this project)
├── Tab Groups          (semantic groupings: Frontend, Backend, API, Docs)
├── DevTools State      (persisted panel state, breakpoints, watched expressions)
├── AI Context          (project-level context: tech stack, known issues, goals)
├── Testing Lab         (project-scoped step sequences, run history, assertions)
├── Notes               (freeform developer notes, pinned to URLs or elements)
├── Commands            (workspace-specific shortcuts and saved commands)
├── Theme               (optional per-workspace visual variation)
└── Permissions         (AI and plugin permissions scoped to this workspace)
```

**Example:**

```
KAGE Project: MyApp
│
├── Frontend    (tab group: localhost:3000, component storybook, design system)
├── Backend     (tab group: API docs, localhost:8000, logs)
├── Database    (tab group: DB admin, schema reference)
├── Tests       (Testing Lab workspace, Playwright export)
└── Performance (Lighthouse, Web Vitals dashboard, performance profiles)
```

Workspaces make KAGE's tab model genuinely developer-useful rather than treating every tab as an isolated, context-free entity.

---

## 12. Plugin Architecture

### 12.1 The Hybrid Model

KAGE is a browser platform that supports existing web-extension technology while exposing a native developer-oriented plugin layer.

| Approach | Pros | Cons |
|---|---|---|
| Standard MV3 extensions only | Existing ecosystem; no new API to design | Can't expose DevTools-internal primitives, full CDP access, or cross-workspace orchestration |
| Fully custom KAGE plugin API | Full control; can expose exactly what developer plugins need | Zero existing ecosystem; entire API surface for KAGE to design and maintain |
| **Hybrid (current direction)** | MV3 extensions work for content-script-level tasks; native KAGE API for DevTools panel registration, AI tool registration, context providers, workspace integration | Two systems conceptually — kept manageable because they share the same permission/sandboxing model |

### 12.2 Native Plugin API Scope

The native layer is deliberately scoped to *only* what MV3 structurally cannot provide:
- New DevTools panels
- New AI tools (new entries in the agent's tool registry)
- New Context Engine providers
- New Command Bus registrations
- Workspace panel integrations

This keeps the native layer small rather than becoming a second, competing extension platform.

---

## 13. Design System Architecture

KAGE has a single design system. Every screen, panel, and surface is built from the same token set. No screen invents its own UI.

### 13.1 Design Tokens

```
Design Tokens
│
├── Colors      (primary palette + semantic aliases)
├── Typography  (typeface, scale, weight, line-height)
├── Spacing     (4px base grid)
├── Radius      (consistent corner rounding by surface type)
├── Shadows     (elevation model)
├── Blur        (glass blur values by surface depth)
├── Borders     (stroke weights, glass borders)
├── Motion      (duration, easing curves for all transitions)
└── Elevation   (z-axis layering model for overlapping surfaces)
```

### 13.2 Color Palette

| Token | Value | Usage |
|---|---|---|
| `--kage-sand` | `#F9DBBD` | Light surfaces, backgrounds, hover states |
| `--kage-blush` | `#FFA5AB` | Accent, active states, notifications |
| `--kage-rose` | `#DA627D` | Primary interactive elements, CTA |
| `--kage-crimson` | `#A53860` | Emphasis, selected states, header accents |
| `--kage-deep` | `#450920` | Deep background, modal overlays, sidebar base |

### 13.3 Typography

**Primary UI Font — Inter**

Used for all browser surfaces: tabs, address bar, settings, buttons, navigation, AI Sidebar, general text, and all product-facing copy. Inter creates the premium, spacious character KAGE needs.

**Monospace Font — JetBrains Mono**

Used exclusively on technical surfaces: DOM inspector, CSS panels, network data, console output, code editors, technical values, hex colors, and timing numbers.

This distinction prevents KAGE from looking like a terminal-themed app. Inter is for the browser. JetBrains Mono is for the data.

### 13.4 Visual Direction

- **Style:** Anime Futuristic · Liquid Glass · Spacious · Minimal · Premium developer tooling
- **Glass surfaces:** translucent panels with backdrop blur; used for sidebars, overlays, inspection panels, command palette — not for every surface.
- **Spacing:** generous. Developer tools should feel like they have room to breathe, not like they've been squeezed into a toolbar.
- **Motion:** purposeful — panels slide in, elements highlight, overlays fade. Nothing animates for decoration alone.

### 13.5 Component Library

```
Components
│
├── Browser Chrome
│   ├── Tab Strip
│   ├── Address Bar
│   ├── Navigation Controls
│   └── Window Controls
├── Buttons          (primary, secondary, ghost, icon)
├── Inputs           (text, search, command, URL)
├── Cards            (workspace cards, tab cards, result cards)
├── Glass Panels     (sidebar, inspection overlay, Command Palette)
├── Command Palette
├── Inspector        (element, network, performance)
├── AI Panels        (sidebar, inline suggestion, tool call card)
├── Tool Rail        (vertical icon rail for Developer Panel navigation)
└── Status Indicators (AI active, permission state, network errors, perf budget)
```

---

## 14. Performance Architecture

One of KAGE's flagship features is: *"Analyze this website."* That requires an actual pipeline, not an ad-hoc CDP query.

```
Page Load / Navigation
        ↓
Instrumentation Layer  (passive — always listening during browsing)
        ↓
Performance Collector
        │
        ├── Web Vitals   (LCP, CLS, INP, FID, TTFB — via PerformanceObserver + CDP)
        ├── Network      (request waterfall, blocking resources, cache behavior)
        ├── CPU          (main thread blocking, long tasks, scripting cost)
        ├── Rendering    (paint timing, layout shifts, compositor work)
        ├── Resources    (asset sizes, compression, third-party cost)
        └── Long Tasks   (tasks >50ms blocking the main thread)
        ↓
Performance Context Bundle  (structured, size-bounded — same pattern as Context Engine)
        ↓
AI Analysis  (model reasons over the bundle, calls further Performance tools if needed)
        ↓
Actionable Report  (ranked findings with explanations and suggested fixes)
```

This pipeline uses the CDP `Performance`, `Network`, and `Runtime` domains already in the Developer Core — no separate system required.

---

## 15. Security Model

KAGE adds new surface area (agent tools, plugin API, browser-level permissions) on top of Chromium's existing security guarantees. The design commitment is: **nothing KAGE adds should be able to do something a well-behaved, sandboxed extension couldn't already do.**

- **Renderer sandboxing and site isolation** are untouched. KAGE's developer layer talks to the browser process over CDP — the same interface DevTools uses — from outside the renderer sandbox.
- **Agent tool calls** that touch a specific origin are scoped to that origin's CDP session/target. No cross-origin reach without an explicit new permission grant.
- **Plugin sandboxing** follows the same isolated-world model Chromium extensions use. The native plugin layer's additional APIs are gated by the same Tier system as agent tools (§7.2).
- **CDP access** is a known sensitive surface. The embedded CDP server must bind to localhost-only and use per-session authentication tokens.
- **Browser-level permissions** (§7.3) add a layer above CDP tier permissions — explicitly governing what the AI can access across the browser profile, not just within a single tab session.

---

## 16. Data & Privacy

- All Testing Lab data, workspace configuration, AI conversation history, and performance profiles are stored locally — no cloud sync by default in v1.
- AI model calls go directly from KAGE to the configured provider. KAGE does not intermediate or log AI traffic on any KAGE-controlled server.
- Browser storage (cookies, localStorage, IndexedDB) remains in the standard Chromium profile store; KAGE adds no additional persistence layer.
- Network interception for Developer Core (request body capture, header inspection) is session-local and never persisted beyond what the Testing Lab explicitly saves.

---

## 17. Tech Stack

| Layer | Choice | Rationale |
|---|---|---|
| **Browser shell** | CEF (Chromium Embedded Framework) | Owns the full browser surface while delegating rendering to Chromium; right fit for a product that *is* the browser |
| **Shell app runtime** | Tauri | Rust backend suits a CDP client well; smaller footprint than Electron |
| **CDP client** | Typed CDP bindings over WebSocket (thin custom wrapper) | Don't reinvent the protocol layer |
| **AI subsystem** | Model-agnostic function-calling layer; OpenRouter-style multi-provider routing | Avoids provider lock-in |
| **Testing Lab storage** | Local JSON, per-workspace directory | Human-readable, diffable, no DB dependency for v1 |
| **UI framework** | React + TypeScript | Component model suits KAGE's panel-heavy interface |
| **Styling** | Tailwind CSS + Liquid Glass design system | Utility-first with design token constraints |
| **Primary UI font** | Inter | Premium, readable, spacious — for all browser and product surfaces |
| **Monospace font** | JetBrains Mono | For code, DOM, CSS panels, network data, console, technical values only |
| **Visual direction** | Anime Futuristic · Liquid Glass · Spacious · Minimal | Peach/pink/burgundy palette: `#F9DBBD` → `#FFA5AB` → `#DA627D` → `#A53860` → `#450920` |

---

## 18. Migration & Future Architecture

### v1 Target
CEF-based KAGE Browser Shell + Chromium rendering + Developer Core (CDP) + Context Engine + AI Sidebar + Micro Inspect + Testing Lab + Command Bus + Plugin API.

### v2+ Considerations
- **Tighter Chromium integration:** if CEF's stability guarantees become a limitation, migration to raw Chromium APIs is the next step. The abstraction boundary in §3.1 is designed to make this feasible without rewriting the product.
- **From-source Chromium fork:** only justified if KAGE needs to change Blink/V8 behavior itself. Nothing in the current requirements calls for this.
- **Mobile:** out of scope for v1.
- **Collaboration features:** shared workspaces, multi-user Testing Lab — post-v1, dependent on validating the single-user product first.

### What the Architecture Preserved

The center of gravity shifted from:

> Stock Chromium + KAGE extension + companion app

to:

> KAGE Browser Shell + Chromium (via CEF) + native Developer Platform

The Context Engine, Permission Tiers, Command Bus, Testing Lab, CDP instrumentation, and Plugin hybrid model survived this transition intact — because they were always about what KAGE *does*, not about which container it runs in.

---

*End of Design.md v0.2*
