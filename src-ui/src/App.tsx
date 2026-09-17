import React, { useState, useEffect, useRef } from "react";
import "./App.css";

import {
  Minus,
  Square,
  X,
} from "lucide-react";
import {
  KageIconHome,
  KageIconAI,
  KageIconInspect,
  KageIconDOM,
  KageIconNetwork,
  KageIconPerformance,
  KageIconConsole,
  KageIconStorage,
  KageIconSecurity,
  KageIconExtensions,
  KageIconSettings,
} from "./components/ui";

import { BrowserProvider, useBrowser } from "./context/BrowserContext";
import type { ActiveTool } from "./context/BrowserContext";
import { getBrowserAdapter } from "./adapters/BrowserAdapter";
import { TabStrip } from "./components/TabStrip/TabStrip";
import { Omnibox } from "./components/Omnibox/Omnibox";
import { IconSidebar } from "./components/IconSidebar/IconSidebar";
import type { SidebarItem } from "./components/IconSidebar/IconSidebar";
import { AISidebar } from "./components/AISidebar/AISidebar";
import { NewTab } from "./components/NewTab/NewTab";
import { BrowserWebview } from "./components/BrowserWebview/BrowserWebview";
import { DevToolsDrawer } from "./components/DevTools/DevToolsDrawer";
import { MicroInspectOverlay } from "./components/MicroInspect/MicroInspectOverlay";
import { DownloadsPopover } from "./components/Downloads/DownloadsPopover";
import { ExtensionsPage } from "./components/Extensions/ExtensionsPage";
import { SettingsPage } from "./components/Settings/SettingsPage";

// ─── Sidebar Items with Canonical KAGE Vector Icons (Linear + Filled) ────
const SIDEBAR_ITEMS: SidebarItem[] = [
  { id: "home",        icon: <KageIconHome size={18} strokeWidth={1.8} />,        label: "Home" },
  { id: "ai",          icon: <KageIconAI size={18} strokeWidth={1.8} />,          label: "AI" },
  { id: "inspect",     icon: <KageIconInspect size={18} strokeWidth={1.8} />,     label: "Inspect" },
  { id: "dom",         icon: <KageIconDOM size={18} strokeWidth={1.8} />,         label: "DOM" },
  { id: "network",     icon: <KageIconNetwork size={18} strokeWidth={1.8} />,     label: "Network" },
  { id: "performance", icon: <KageIconPerformance size={18} strokeWidth={1.8} />, label: "Performance" },
  { id: "console",     icon: <KageIconConsole size={18} strokeWidth={1.8} />,     label: "Console" },
  { id: "storage",     icon: <KageIconStorage size={18} strokeWidth={1.8} />,     label: "Storage" },
  { id: "security",    icon: <KageIconSecurity size={18} strokeWidth={1.8} />,    label: "Security" },
  { id: "extensions",  icon: <KageIconExtensions size={18} strokeWidth={1.8} />,  label: "Extensions" },
];

const SIDEBAR_BOTTOM: SidebarItem[] = [
  { id: "settings",    icon: <KageIconSettings size={18} strokeWidth={1.8} />,    label: "Settings" },
];

const AppInner: React.FC = () => {
  const {
    tabs,
    activeTabId,
    activeTab,
    createTab,
    closeTab,
    switchTab,
    navigate,
    goBack,
    goForward,
    refresh,

    activeTool,
    setActiveTool,
    aiOpen,
    setAiOpen,
    setDevToolsOpen,
    setInspectMode,

    isBookmarked,
    toggleBookmark,

    downloadsOpen,
    setDownloadsOpen,
  } = useBrowser();

  const [inputUrl, setInputUrl] = useState(activeTab?.url || "");
  const viewportStageRef = useRef<HTMLDivElement>(null);

  // Synchronize viewport stage coordinates to native CEF child window (KAGE-ARCH-002)
  // Debounced and throttled to prevent IPC stalls and GPU thread lockups during smooth transitions
  useEffect(() => {
    const el = viewportStageRef.current;
    if (!el) return;

    let lastSent = { x: -1, y: -1, width: -1, height: -1, scaleFactor: -1 };
    let rafId: number | null = null;
    let settleTimer: number | null = null;

    const performSync = () => {
      const rect = el.getBoundingClientRect();
      const scaleFactor = window.devicePixelRatio || 1;
      const nextX = Math.round(rect.left * scaleFactor);
      const nextY = Math.round(rect.top * scaleFactor);
      const nextW = Math.round(rect.width * scaleFactor);
      const nextH = Math.round(rect.height * scaleFactor);

      // Skip IPC entirely if bounds have not changed
      if (
        nextX === lastSent.x &&
        nextY === lastSent.y &&
        nextW === lastSent.width &&
        nextH === lastSent.height &&
        scaleFactor === lastSent.scaleFactor
      ) {
        return;
      }

      lastSent = { x: nextX, y: nextY, width: nextW, height: nextH, scaleFactor };
      getBrowserAdapter().syncViewportBounds({
        x: nextX,
        y: nextY,
        width: nextW,
        height: nextH,
        scale_factor: scaleFactor,
      }).catch(() => {});
    };

    const scheduleSync = () => {
      // Settle timer fires once animation completes (280ms)
      if (settleTimer !== null) {
        window.clearTimeout(settleTimer);
      }
      settleTimer = window.setTimeout(() => {
        performSync();
      }, 280);

      // Throttle IPC with requestAnimationFrame during active movement
      if (rafId === null) {
        rafId = window.requestAnimationFrame(() => {
          rafId = null;
          performSync();
        });
      }
    };

    const observer = new ResizeObserver(() => {
      scheduleSync();
    });

    observer.observe(el);
    window.addEventListener("resize", scheduleSync);
    performSync();

    return () => {
      if (rafId !== null) window.cancelAnimationFrame(rafId);
      if (settleTimer !== null) window.clearTimeout(settleTimer);
      observer.disconnect();
      window.removeEventListener("resize", scheduleSync);
    };
  }, []);

  // Sync input bar with active tab url
  useEffect(() => {
    setInputUrl(activeTab?.url || "");
  }, [activeTab?.url]);

  const handleSidebarSelect = (id: string) => {
    if (id === "ai") {
      setAiOpen(!aiOpen);
      return;
    }

    if (id === "home") {
      navigate("");
      setActiveTool("home");
      setDevToolsOpen(false);
      return;
    }

    if (id === "inspect") {
      setInspectMode(true);
      setActiveTool("inspect");
      setDevToolsOpen(false);
      return;
    }

    if (
      id === "dom" ||
      id === "network" ||
      id === "performance" ||
      id === "console" ||
      id === "storage" ||
      id === "security"
    ) {
      setActiveTool(id as ActiveTool);
      setDevToolsOpen(true);
      return;
    }

    if (id === "extensions" || id === "settings") {
      setActiveTool(id as ActiveTool);
      setDevToolsOpen(false);
      return;
    }
  };

  const isNewTab = !activeTab?.url && activeTool !== "settings" && activeTool !== "extensions";

  return (
    <div className="app" role="application" aria-label="KAGE Developer Browser">
      {/* ── Top Bar: Logo + Tabs + Window Controls ───────────────── */}
      <header className="app__titlebar glass-panel" role="banner">
        <div className="app__titlebar-logo" aria-label="KAGE" onClick={() => navigate("")}>
          <img src="/kage-logo.png" alt="KAGE" className="app__titlebar-logo-img" />
        </div>
        <TabStrip
          tabs={tabs.map((t) => ({
            id: t.id,
            title: t.title,
            isActive: t.id === activeTabId,
            favicon: t.favicon,
            isLoading: t.isLoading,
          }))}
          onTabSelect={switchTab}
          onTabClose={closeTab}
          onNewTab={() => createTab()}
        />
        {/* Minimal Windows Window Controls */}
        <div className="app__window-controls" aria-label="Window controls">
          <button type="button" className="wc-btn wc-btn--minimize" aria-label="Minimize">
            <Minus size={11} strokeWidth={2} />
          </button>
          <button type="button" className="wc-btn wc-btn--maximize" aria-label="Maximize">
            <Square size={10} strokeWidth={1.8} />
          </button>
          <button type="button" className="wc-btn wc-btn--close" aria-label="Close">
            <X size={12} strokeWidth={2} />
          </button>
        </div>
      </header>

      {/* ── Omnibox Row with Modern Nav & Action Cluster ─────────── */}
      <Omnibox
        url={inputUrl}
        onUrlChange={setInputUrl}
        onNavigate={navigate}
        isSecure={activeTab?.isSecure}
        onRefresh={refresh}
        onBack={goBack}
        onForward={goForward}
        canGoBack={activeTab?.canGoBack}
        canGoForward={activeTab?.canGoForward}
        onToggleAi={() => setAiOpen(!aiOpen)}
        onFavourite={() => toggleBookmark(inputUrl, activeTab?.title)}
        isStarred={isBookmarked(inputUrl)}
        onToggleDownloads={() => setDownloadsOpen(!downloadsOpen)}
        onToggleExtensions={() => setActiveTool(activeTool === "extensions" ? "home" : "extensions")}
      />

      {/* ── Main Body: Slender Icon Dock + KAGE AI Panel + Viewport ── */}
      <div className="app__body">
        <IconSidebar
          items={SIDEBAR_ITEMS}
          bottomItems={SIDEBAR_BOTTOM}
          onSelect={handleSidebarSelect}
          activeId={activeTool}
        />

        {/* KAGE AI Panel (Docked beside IconSidebar) */}
        <AISidebar
          isOpen={aiOpen}
          onClose={() => setAiOpen(false)}
        />

        {/* Main Viewport Stage */}
        <main
          className={`app__content ${aiOpen ? "app__content--ai-open" : ""}`}
          role="main"
          aria-label="Browser viewport"
        >
          <div className="app__viewport-stage" ref={viewportStageRef}>
            {activeTool === "settings" ? (
              <SettingsPage />
            ) : activeTool === "extensions" ? (
              <ExtensionsPage />
            ) : isNewTab ? (
              <NewTab
                onNavigate={navigate}
                onToggleAi={() => setAiOpen(!aiOpen)}
              />
            ) : (
              <BrowserWebview url={activeTab?.url || ""} />
            )}
          </div>

          {/* Collapsible Bottom DevTools Drawer (Flow element, shrinks viewport naturally) */}
          <DevToolsDrawer />
        </main>
      </div>

      {/* Floating Blueprint Micro-Inspect Overlay */}
      <MicroInspectOverlay />

      {/* Floating Downloads Popover */}
      <DownloadsPopover />
    </div>
  );
};

export const App: React.FC = () => {
  return (
    <BrowserProvider>
      <AppInner />
    </BrowserProvider>
  );
};

export default App;
