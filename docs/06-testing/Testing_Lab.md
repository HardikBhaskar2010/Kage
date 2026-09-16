# KAGE Testing Lab Specification

| Document Metadata | Details |
|---|---|
| **Document ID** | KAGE-TEST-001 |
| **Status** | Approved Core Specification |
| **Version** | v0.2.1 |
| **Last Updated** | 2026-09-16 |
| **Target Milestone** | v1.0.0 MVP |
| **Classification** | Automation, Recording & Test Engineering |

---

## 1. Executive Summary & Product Mission

End-to-end (E2E) web testing has historically been tedious and fragile. Developers spend hours writing boilerplate selector code in external tools, struggle to debug flaky tests, and frequently abandon automation suites.

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

The **KAGE Testing Lab** turns the browser itself into a native test development environment. Developers can record user flows with zero code, generate assertions visually, capture reconstructable state snapshots, replay flows deterministically with checkpoint-based step debugging, and export production-ready Playwright TypeScript tests instantly.

```
┌─────────────────────────────────────────────────────────────┐
│                      KAGE TESTING LAB                       │
│                                                             │
│  [ RECORD FLOW ]  ──►  Live multi-channel event observation │
│          │                                                  │
│          ▼                                                  │
│  [ INTENT CAPTURE ]──►  Multi-tier resilient selector engine │
│          │                                                  │
│          ▼                                                  │
│  [ ASSERTION BUILD]──►  Visual, DOM, & Network assertions    │
│          │                                                  │
│          ▼                                                  │
│  [ REPLAY & STEP ] ──►  Checkpoint-based replay & debug     │
│          │                                                  │
│          ▼                                                  │
│  [ EXPORT SUITE ]  ──►  Clean Playwright TypeScript spec     │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. Action Recorder & Observation Architecture

The Action Recorder observes user interactions passively without injecting invasive scripts into the web document. It separates **Observation** from **Replay Simulation**:

### 2.1 Multi-Channel Observation Streams

```
┌─────────────────────────────────────────────────────────────┐
│                   OBSERVATION CHANNELS                      │
├───────────────────────┬─────────────────────────────────────┤
│ User Input Stream     │ Native OS mouse & keyboard events   │
│                       │ observed via CEF Client Handlers    │
├───────────────────────┼─────────────────────────────────────┤
│ DOM Mutation Stream   │ CDP `DOM` & `Page.javascriptDialog` │
│                       │ tracking active target element state│
├───────────────────────┼─────────────────────────────────────┤
│ Navigation Stream     │ `Page.frameNavigated` &             │
│                       │ `Page.lifecycleEvent` (load, FCP)   │
├───────────────────────┼─────────────────────────────────────┤
│ Network Stream        │ `Network.requestWillBeSent` &       │
│                       │ `Network.responseReceived`          │
└───────────────────────┴─────────────────────────────────────┘
```

These raw observation channels are aggregated and normalized into typed `KageAction` structs:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KageAction {
    pub id: String,
    pub step_index: u32,
    pub action_type: ActionType,
    pub target: TargetSelectorHierarchy,
    pub payload: ActionPayload,
    pub timestamp_ms: u64,
    pub checkpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionType {
    Click,
    Type { text: String, is_masked: bool },
    SelectOption { value: String },
    Navigate { url: String },
    Scroll { delta_x: f64, delta_y: f64 },
    WaitFor { condition: WaitCondition },
    Assert { assertion: AssertionDefinition },
}
```

### 2.2 Resilient Multi-Tier Selector Strategy
KAGE synthesizes a resilient fallback selector hierarchy for every recorded element:

```json
{
  "primary_selector": "[data-testid='submit-order']",
  "fallback_selectors": [
    "button[aria-label='Submit Order']",
    "form#checkout-form button.btn-primary",
    "//button[contains(text(), 'Submit Order')]"
  ],
  "element_fingerprint": {
    "tag": "button",
    "role": "button",
    "text_content": "Submit Order",
    "bounding_box": { "width": 160, "height": 44 }
  }
}
```

---

## 3. Assertion Engine

Developers can insert assertions during recording with a single click in the Testing Lab panel:

```
┌─────────────────────────────────────────────────────────────┐
│ ✦ Add Assertion                                             │
├─────────────────────────────────────────────────────────────┤
│ Target Element: button#submit-order                         │
│                                                             │
│ Assertion Type:                                             │
│  ◉ Element is Visible                                       │
│  ○ Text Content Equals: ["Order Confirmed"]                 │
│  ○ Attribute "disabled" is: [ false ]                       │
│  ○ Visual Snapshot Match (Pixel Tolerance: 0.1%)            │
│  ○ Network Request Status: [POST /api/order -> 200 OK]      │
│                                                             │
│ [ Save Assertion ]  [ Cancel ]                              │
└─────────────────────────────────────────────────────────────┘
```

1. **DOM State Assertions:** Element visibility, text content equality, CSS attribute values, computed styles.
2. **Network Assertions:** Verifies that a specific API endpoint was called with expected query params and returned an HTTP 200/201.
3. **Visual Snapshot Assertions:** Captures element or viewport PNG snapshots and performs pixel-diff comparisons during replay.

---

## 4. Reconstructable Browser State Snapshots

> [!IMPORTANT]
> **Technical Scope Clarification:** A snapshot captures **reconstructable browser state**, not an exact replica of deep runtime memory.

### 4.1 Captured State Payload
- Complete DOM outerHTML and document URL.
- HTTP Cookies (partitioned by domain and path).
- Web Storage (`localStorage` and `sessionStorage`).
- Navigation history stack (forward/back entries).
- Active console errors, warnings, and unhandled rejections.
- Viewport PNG screenshot.

### 4.2 Known State Restoration Boundaries
Developers must understand that certain transient runtime internals cannot be reconstructed from a snapshot:
- **JavaScript Closures & Heap State:** Internal JS variables not mirrored in DOM or storage.
- **In-Flight WebSockets & Streaming HTTP:** Active persistent network sockets.
- **Service Worker Internal Cache:** Background worker thread state.
- **Hardware/WebGL Framebuffers:** Ephemeral GPU canvas memory.

Restoring a state snapshot instantiates a clean CEF tab, seeds its cookies and storage partitions, navigates to the target URL, and waits for DOM stabilization.

---

## 5. Checkpoint-Based Replay Engine & Debugger

Deterministic replay and step-through debugging require a **Checkpoint Architecture**:

```
[Start] ──► Checkpoint 0 (Initial State)
                 │ Action 1 (Navigate)
                 ▼
            Checkpoint 1 (Post-Navigate State)
                 │ Action 2 (Fill Form)
                 ▼
            Checkpoint 2 (Post-Input State)
                 │ Action 3 (Click Submit)
                 ▼
            Checkpoint 3 (Order State)
```

### 5.1 Step-Through Debugging Mechanics
- **Step Forward:** Replay next single action using CDP input synthesis (`Input.dispatchMouseEvent`, `Input.dispatchKeyEvent`).
- **Step Backward:** Rather than attempting impossible reverse-execution of DOM mutations, KAGE executes a **Restart-from-Checkpoint** operation: it restores `Checkpoint N-1` and validates that the browser reflects the preceding state.

### 5.2 Replay Execution Modes
1. **Interactive Replay:** Executes in the active, foreground CEF tab with human-paced delays (default: 300ms) and visual element highlighting.
2. **Background Replay:** Executes in an off-screen background tab at maximum machine speed for rapid regression passes.
3. **CI / Headless Execution:** Exported to Playwright for execution in headless CI/CD pipelines (GitHub Actions, GitLab CI).

---

## 6. Export Pipeline: Playwright & TypeScript

Recorded tests can be exported directly into idiomatic TypeScript Playwright test files:

```typescript
import { test, expect } from '@playwright/test';

test('User Checkout Flow', async ({ page }) => {
  // Generated by KAGE Testing Lab on 2026-09-16
  await page.goto('https://store.example.com');
  
  // Action 1: Add product to cart
  const addToCartBtn = page.locator("[data-testid='add-to-cart']");
  await expect(addToCartBtn).toBeVisible();
  await addToCartBtn.click();

  // Action 2: Enter email
  await page.locator('input#email').fill('user@example.com');

  // Action 3: Checkout
  await page.locator('button#checkout').click();

  // Assertion: Order confirmation
  await expect(page.locator('.order-status')).toHaveText('Thank you for your order');
});
```

The exporter generates clean, human-readable code that adheres to standard testing best practices without proprietary runtime dependencies.
