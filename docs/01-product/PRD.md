# KAGE — Product Requirements Document

**Codename:** KAGE (internal working title: DevBrowser)
**Owner:** Hardik Bhaskar (Luna Kitsune)
**Status:** Draft v0.2
**Last updated:** 2026-09-16

---

## 1. Summary

KAGE is a standalone developer-first browser, built on Chromium/Blink, with its own browser shell and UI. It fuses four things that today live in four different tools — the browser, DevTools, a testing/automation lab, and an AI agent — into one workspace with a shared context model.

The wedge feature is **Explain This**: select anything in the browser (an element, a request, a console error, a layout shift) and get a grounded explanation built from real browser state (DOM, computed styles, network, console, performance, a11y tree) — not a screenshot guess and not a copy-pasted snippet into a generic chatbot.

The long-term differentiator is that the AI is not a sidebar chat — it's an **agent with a permissioned tool interface** into the browser's own debugging primitives (the same primitives DevTools itself uses).

This document defines *what* KAGE is and *why* it should exist. [`Requirements.md`](Requirements.md) defines *what it must do*. [`Design.md`](../Design.md) defines *how it's built*.

---

## 2. Problem Statement

Developers currently juggle:

- **The browser** — for actually viewing/using the site
- **DevTools** — for inspecting DOM/network/console/performance, opened and closed dozens of times a day
- **A separate AI tool** (ChatGPT, Claude, Cursor) — pasted-in HTML/CSS/error text, stripped of the actual runtime context that would make the answer accurate
- **A separate automation/testing tool** (Playwright, Cypress) — written after the bug is already found, by hand, from scratch

Nothing here shares context. Every hop back and forth (browser → DevTools → clipboard → chat → back) throws away state: the actual computed styles, the actual stack trace, the actual network waterfall. The developer becomes the integration layer between four tools that don't talk to each other.

**The core insight:** an AI answer about a web page is only as good as the browser context it's grounded in. A screenshot or a pasted `<div>` is not context. The DOM tree, cascade, box model, accessibility tree, network graph, and console state *is* context — and the browser is the only thing that has all of it simultaneously.

---

## 3. Product Vision

> A browser that understands the page as deeply as DevTools does, reasons about it as well as an AI agent can, and lets a developer turn "I found the bug" into "it's fixed and there's a regression test" without leaving the tab.

KAGE is **Browser + DevTools + Testing Lab + Web Automation + AI Agent + Developer Workspace** — a standalone developer-first browser with its own browser shell and UI, built on Chromium/Blink so it inherits web compatibility, security patching, and rendering correctness for free, rather than trying to reinvent them.

KAGE is not an extension attached to another browser. KAGE *is* the browser. The tab strip, address bar, sidebar, command center, and every developer surface are owned by KAGE. Chromium owns rendering.

**What KAGE is not:**
- Not "a browser with a ChatGPT sidebar bolted on"
- Not a rendering-engine research project — Blink/V8 are used as-is
- Not a general consumer browser competing on mass-market features — its differentiation is developer workflow and browser instrumentation
- Not an extension product sitting on top of Chrome or Edge

---

## 4. Target Users

| Persona | Description | Primary jobs-to-be-done |
|---|---|---|
| **Frontend developer** | Builds and debugs web UIs daily | Debug layout/CSS bugs fast, understand why something renders wrong, check a11y/perf before shipping |
| **Full-stack / solo builder** | Ships product end-to-end (like Hardik himself) | Move fast across many small projects, needs the AI to *do* things, not just describe them |
| **QA / test engineer** | Verifies behavior across browsers/devices | Convert manual repro steps into automated, saved, replayable tests |
| **Reverse-engineer / UI researcher** | Studies other sites for structure, inspiration, or competitive analysis | Understand a site's stack/architecture without violating IP or scraping boundaries |
| **Extension/plugin author** (later) | Extends KAGE itself | Build new panels, commands, or AI tools on top of the platform |

Primary persona for MVP: **the solo/full-stack developer.** Everything else generalizes from this.

---

## 5. Goals

**G1.** Make "select something → get a grounded explanation" faster and more accurate than pasting into a separate AI chat.

**G2.** Let the AI *act* on the browser (inspect, modify temporarily, run JS, throttle network, capture state) through an explicit, permissioned tool interface — not free-range control.

**G3.** Collapse "I manually reproduced a bug" into "there's now a saved, replayable, assertable test" with minimal extra effort.

**G4.** Make KAGE a programmable developer workspace where browser surfaces, tools, commands, AI workflows, themes, and plugins are customizable — the user should feel they own it, the way they own a terminal or editor setup.

**G5.** Ship on top of Chromium/Blink so KAGE never has to solve web-compat or rendering-correctness itself.

### Non-Goals (v1)

- KAGE does not attempt to compete with consumer browsers on mass-market features (Chrome, Edge, Arc own this space). Its differentiation is developer workflow and browser instrumentation — KAGE is a developer browser that happens to browse the web normally, not a consumer browser with developer features bolted on.
- Not replacing dedicated E2E frameworks (Playwright/Cypress) for CI-grade test suites — KAGE's Testing Lab is for *capture → local repro → optional export*, not a CI runner
- Not building a novel rendering engine, JS engine, or network stack
- Not shipping a full autonomous "browse the web and complete tasks for me" agent (that's Comet/Atlas/Dia's bet) — KAGE's agent scope is deliberately narrower: *developer tooling*, not *general task automation*

---

## 6. Competitive Landscape

| Product | What it actually is | Where KAGE differs |
|---|---|---|
| **Chrome DevTools + Gemini** | Best-in-class native devtools, now with an AI panel for performance/console explanations | KAGE unifies the *whole* devtools surface under one agent, owns the browser shell, and adds Testing Lab + customization as first-class |
| **Perplexity Comet** | Search-first agentic browser; strong cross-tab context, general task automation | Not developer-tooling-focused; no DOM/network/perf-grounded "Explain This" |
| **ChatGPT Atlas** | Chromium-based, ChatGPT woven into every interaction, general-purpose agent | General browsing agent, not a debugging/testing environment |
| **Dia (The Browser Company, now Atlassian)** | Privacy-conscious agentic browser, local encryption, "Skills" | Consumer/productivity focus, not dev-tool-primitive-grounded |
| **Arc** | Design-forward, highly customizable browser | Customization philosophy KAGE should learn from, but no dev-tooling or AI-agent layer |

**Positioning:**
- KAGE ≠ AI browser
- KAGE ≠ DevTools replacement
- KAGE = developer browser + browser instrumentation + AI agent + testing workspace

**Key finding from research:** security researchers confirmed in mid-2026 that prompt injection — hostile instructions smuggled through ordinary web content — cannot be fully solved in *any* agentic browser; OpenAI itself has said this is "unlikely to ever be fully solved." This is directly relevant to KAGE's tool/permission architecture (see `Design.md` §7) — every leading agentic browser is judged today primarily on how it *contains* this risk. KAGE treats this as a first-class constraint from day one, not an afterthought.

---

## 7. Feature Pillars (high level — full breakdown in Requirements.md)

1. **Browser Shell** — KAGE-owned tab strip, address bar, navigation, sidebar, command center, profiles, bookmarks, downloads
2. **Browser Core** — standard Chromium-backed browser fundamentals: permissions, storage, history, certificates
3. **Micro Inspect** — hover-to-inspect, click-to-lock element analysis; one of KAGE's identity features
4. **Explain This** — the universal, context-grounded explanation feature
5. **Agentic Tool System** — permissioned AI actions over browser/devtools primitives
6. **Developer Tooling** — Elements, Console, Network, Performance, Storage, Accessibility, Security, Device Emulation
7. **Testing Lab** — record → replay → assert → save → report; native KAGE workspace
8. **Workspace System** — developer workspaces as first-class project containers
9. **AI Sidebar** — persistent, context-aware agent surface (UI for the Agent System, not the Agent System itself)
10. **Exploration/Inspiration Mode** — understand third-party sites within IP/scraping boundaries
11. **Command System** — palette + shortcuts + context menus + AI, all routed through one command bus
12. **Customization Platform** — design tokens, themes, layouts, workflows, saved agents, prompts
13. **Plugin System** — extensibility model (hybrid: MV3-compatible + native KAGE platform plugin API)

---

## 8. Success Metrics (v1)

Since this starts as a solo-dev / early-adopter project, metrics should be usage-quality signals, not growth signals:

| Metric | Target for MVP validation |
|---|---|
| Time from "notice a bug" to "get a correct explanation" | < 15 seconds via Explain This vs. baseline manual DevTools + separate chat workflow |
| % of Explain This answers that require no follow-up clarification | > 70% (context engine is doing its job) |
| Time from "reproduce bug manually" to "saved replayable test" | < 60 seconds |
| Daily active use by the builder himself | Used as the default browser for real project work, not just demoed |
| Plugin/command extensibility | At least 1 custom command/workflow authored and reused |

---

## 9. Constraints & Assumptions

- **Team size:** solo developer (+ AI pair-programming agents). This materially changes the buildable architecture — see Design.md §3 for why "fork Chromium from source" is explicitly rejected in favor of CEF.
- **Hardware:** primary dev machine is a Lenovo ThinkPad T14s Gen 4 (AMD, integrated graphics). Chromium full-source builds require ~100 GB+ disk and multi-hour build times even on strong hardware — this rules out a from-source fork as a realistic v1 path.
- **Aesthetic:** Liquid Glass + Anime Futuristic. Design character: spacious, minimal, premium, developer-focused, soft neon / controlled glow. Primary palette: `#F9DBBD` (Peach) → `#FFA5AB` (Pink Primary) → `#DA627D` (Rose Accent) → `#A53860` (Magenta Secondary) → `#450920` (Wine Background). Typography: Inter for all primary UI; JetBrains Mono for code and data surfaces only. Reference assets: `Design System.png`, `Mockup_Browsing Screen.png`, `Mockup_New Tab Screen.png` (all in `public/`).
- **Philosophy:** documentation-first. This PRD/Requirements/Design/Plan set exists *before* implementation, and should stay AI-agent-readable so future coding agents can build directly from it.

---

## 10. Open Questions

- How much of the Testing Lab should interoperate with Playwright's format (export-to-Playwright) vs. being a fully separate saved-test format
- Whether "Exploration Mode" ships in v1 at all, given its legal/IP surface area is the riskiest part of the product to get wrong — currently considered P2
- Local vs. cloud LLM for the agent — given the ThinkPad's hardware limits, v1 likely defaults to an API-based model (OpenRouter-style multi-model routing) with local-model support as a stretch goal
- CEF vs. alternative embedding strategy — CEF is the current leading direction; see Design.md §3 for the full analysis
