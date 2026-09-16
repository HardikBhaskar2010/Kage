# KAGE UI Design System Specification

| Document Metadata | Details |
|---|---|
| **Document ID** | KAGE-UI-001 |
| **Status** | Approved Core Specification |
| **Version** | v0.2.1 |
| **Last Updated** | 2026-09-16 |
| **Target Milestone** | v1.0.0 MVP |
| **Classification** | Design System, Token Architecture & UI Guidelines |

---

## 1. Visual Philosophy & Aesthetic Identity

KAGE rejects the tired visual cliches of early developer tools—dense, cluttered, dark cyberpunk HUDs dominated by neon purple and harsh cyan borders.

Instead, KAGE embodies a distinct, state-of-the-art aesthetic:
**Liquid Glass + Spacious + Anime Futuristic**.

```
┌────────────────────────────────────────────────────────────────────────┐
│                        CORE AESTHETIC PILLARS                          │
├───────────────────┬───────────────────┬────────────────────────────────┤
│  LIQUID GLASS     │  SPACIOUS ELEGANCE│  ANIME FUTURISTIC              │
├───────────────────┼───────────────────┼────────────────────────────────┤
│ Multi-layer blur, │ Generous padding, │ Elegant typography, precise    │
│ delicate specular │ breathable visual │ micro-interactions, warm coral │
│ border gradients, │ hierarchy, zero   │ & rose accents, high-end       │
│ ambient glow.     │ clutter or fatigue│ mechanical craft.              │
└───────────────────┴───────────────────┴────────────────────────────────┘
```

---

## 2. Token Architecture

The design system follows a 3-tier token hierarchy:
1. **Primitive Tokens:** Raw color hexes, base font sizes, raw elevation values.
2. **Semantic Tokens:** Contextual meanings (`surface-primary`, `text-muted`, `accent-glow`).
3. **Component Tokens:** Component-specific bindings (`tab-active-bg`, `omnibox-border`).

---

## 3. Color Palette & Theme Modes

KAGE is anchored by a curated warm palette transitioning from soft peach sand to deep wine:

```
┌────────────────────────────────────────────────────────────────────────┐
│                          FOUNDATIONAL PALETTE                          │
├────────────┬─────────────┬───────────┬─────────────────────────────────┤
│ TOKEN NAME │ VALUE       │ SWATCH    │ INTENDED ROLE                   │
├────────────┼─────────────┼───────────┼────────────────────────────────┤
│ Sand/Peach │ `#F9DBBD`   │ ░░░░░░░░░ │ Highlights, light text, accents │
│ Blush/Pink │ `#FFA5AB`   │ ▒▒▒▒▒▒▒▒▒ │ Hover states, secondary borders │
│ Rose       │ `#DA627D`   │ ▓▓▓▓▓▓▓▓▓ │ Primary interactive accents     │
│ Crimson    │ `#A53860`   │ █▓█▓█▓█▓█ │ Active badges, focus rings      │
│ Deep Wine  │ `#450920`   │ █████████ │ Core background, dark surfaces  │
└────────────┴─────────────┴───────────┴─────────────────────────────────┘
```

### 3.1 Multi-Theme Semantic Token Strategy
KAGE's components are styled entirely through semantic CSS variables so themes switch seamlessly:

```css
:root {
  /* Default Theme: Liquid Glass Dark Wine */
  --kage-bg-canvas: #1A040D;
  --kage-surface-glass: rgba(69, 9, 32, 0.65);
  --kage-surface-floating: rgba(69, 9, 32, 0.85);
  --kage-border-subtle: rgba(255, 165, 171, 0.15);
  --kage-border-highlight: rgba(218, 98, 125, 0.40);
  --kage-text-primary: #FFFFFF;
  --kage-text-secondary: #F9DBBD;
  --kage-text-muted: rgba(255, 165, 171, 0.60);
  --kage-accent: #DA627D;
}

[data-theme="light"] {
  /* Light Theme: Frosted Porcelain & Rose Quartz */
  --kage-bg-canvas: #FFF8F6;
  --kage-surface-glass: rgba(255, 245, 242, 0.75);
  --kage-surface-floating: rgba(255, 255, 255, 0.90);
  --kage-border-subtle: rgba(165, 56, 96, 0.12);
  --kage-border-highlight: rgba(218, 98, 125, 0.50);
  --kage-text-primary: #2B0614;
  --kage-text-secondary: #6B1A38;
  --kage-text-muted: #9E3A5E;
  --kage-accent: #A53860;
}

[data-theme="high-contrast"] {
  /* High Contrast: Maximum readability, solid borders, zero blur */
  --kage-bg-canvas: #000000;
  --kage-surface-glass: #111111;
  --kage-surface-floating: #1A1A1A;
  --kage-border-subtle: #FFFFFF;
  --kage-border-highlight: #FFD700;
  --kage-text-primary: #FFFFFF;
  --kage-text-secondary: #FFFFFF;
  --kage-text-muted: #CCCCCC;
  --kage-accent: #FFD700;
}

[data-reduced-transparency="true"] {
  /* Reduced Transparency: Disables backdrop filters for low-spec GPUs */
  --kage-surface-glass: #2D0716;
  --kage-surface-floating: #38091B;
}
```

---

## 4. Typography Hierarchy

KAGE enforces a strict two-family typography system:

| Role | Font Family | Weights | Usage Scope |
|---|---|---|---|
| **UI & Chrome** | **Inter** | 400 (Regular), 500 (Medium), 600 (Semi-Bold) | Tab titles, omnibox, menus, chat text, buttons, modals. |
| **Code & Telemetry**| **JetBrains Mono** | 400 (Regular), 500 (Medium) | CSS selectors, DOM tags, stack traces, JSON values, network timings. |

### Typography Scale
- **Display 1:** 24px / 32px (Inter Semi-Bold) — Welcome screens, empty workspace hero
- **Heading 1:** 16px / 24px (Inter Semi-Bold) — Panel titles, modal headers
- **Heading 2:** 14px / 20px (Inter Medium) — Section dividers, drawer headers
- **Body 1:** 13px / 18px (Inter Regular) — Omnibox text, AI chat messages
- **Body 2 (Caption):** 11px / 16px (Inter Regular) — Status indicators, timestamps
- **Code Regular:** 12px / 18px (JetBrains Mono) — Inspect metrics, CSS declarations
- **Code Small:** 10px / 14px (JetBrains Mono) — Micro Inspect overlay badges

---

## 5. Glass Materials & Elevation System

```css
/* Level 1: Subtle In-Page Glass (Tool Rail, Status Bar) */
.glass-surface-subtle {
  background: var(--kage-surface-glass);
  backdrop-filter: blur(12px) saturate(160%);
  border: 1px solid var(--kage-border-subtle);
}

/* Level 2: Interactive Glass (Tab Bar, AI Drawer) */
.glass-surface-medium {
  background: var(--kage-surface-glass);
  backdrop-filter: blur(20px) saturate(180%);
  border: 1px solid var(--kage-border-subtle);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
}

/* Level 3: Floating Elevation (Micro Inspect Card, Command Palette) */
.glass-surface-floating {
  background: var(--kage-surface-floating);
  backdrop-filter: blur(32px) saturate(200%);
  border: 1px solid var(--kage-border-highlight);
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.60), 0 0 1px rgba(255, 255, 255, 0.25);
}
```

---

## 6. Spacing & Border Radius Scales

### 6.1 Spacing (4px Base Grid)
- `space-1`: 4px | `space-2`: 8px | `space-3`: 12px | `space-4`: 16px | `space-6`: 24px | `space-8`: 32px

### 6.2 Border Radius
- `radius-sm`: 4px (tags, code chips)
- `radius-md`: 8px (buttons, input fields)
- `radius-lg`: 12px (cards, tool drawers)
- `radius-xl`: 16px (floating modals, Command Center)
- `radius-full`: 9999px (pills, status badges)

---

## 7. Core Component Specifications

```
┌─────────────────────────────────────────────────────────────┐
│ ✦ TAB STRIP                                                 │
│  [ Active Tab: KAGE Dev  ✕ ]  [ + New Tab ]                 │
├─────────────────────────────────────────────────────────────┤
│ ✦ OMNIBOX                                                   │
│  [ 🔒 https://store.example.com/checkout ]         [ ⚡ AI ] │
├──────────────┬───────────────────────────────┬──────────────┤
│ ✦ TOOL RAIL  │ ✦ CEF VIEWPORT                │ ✦ AI DRAWER  │
│  [Inspect]   │                               │ [Chat]       │
│  [Console]   │   (Web Page Content)          │ [Context]    │
│  [Network]   │                               │              │
│  [Tests]     │                               │              │
└──────────────┴───────────────────────────────┴──────────────┘
```

- **Tab Strip:** Height 36px. Active tab styled in `--kage-accent` with soft glow.
- **Omnibox:** Height 38px. Semi-transparent input with focus glow in `--kage-border-highlight`.
- **Command Center (`Ctrl+K`):** Width 640px. Level 3 Floating Glass surface with grouped category results.

---

## 8. Micro-Animations & Motion Design

Animations in KAGE are crisp, mechanical, and physics-grounded:
- **Duration Benchmark Targets:** Fast interactions `120 ms` (hover, press); panel transitions `220 ms` (drawer, modal).
- **Easing Curve:** Cubic bezier `cubic-bezier(0.16, 1, 0.3, 1)` (fluid spring ease-out).
- **Reduced Motion:** When `prefers-reduced-motion: reduce` is detected, opacity transitions replace coordinate transforms.

---

## 9. Accessibility (a11y) Verification Standards

> [!NOTE]
> **Contrast Validation Requirement:** Contrast cannot be assumed purely from static token values because glass surfaces composite dynamically over unpredictable webpage content or dark themes.

- **Target Standard:** **WCAG 2.1 AA Compliance** across all interactive controls.
- **Surface Testing Protocol:** Contrast ratios must be verified for each semantic text token against representative light and worst-case dark composited glass backgrounds before release.
- **Focus Rings:** Visible, high-contrast 2px ring in `--kage-accent` with 2px offset on keyboard `Tab` navigation.
- **ARIA Semantics:** Complete semantic structure across shell (`role="tablist"`, `role="tab"`, `role="dialog"`, `aria-live`).
