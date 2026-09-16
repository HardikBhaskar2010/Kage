import React, { useState, useEffect } from "react";
import "./App.css";

import {
  Home,
  Sparkles,
  ScanEye,
  Layers,
  Network,
  Activity,
  Terminal,
  Database,
  ShieldCheck,
  Blocks,
  Settings,
  Minus,
  Square,
  X,
} from "lucide-react";

import { BrowserProvider, useBrowser } from "./context/BrowserContext";
import type { ActiveTool } from "./context/BrowserContext";
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

// ─── Sidebar Items with Crisp Lucide Icons ────────────────────────────
const SIDEBAR_ITEMS: SidebarItem[] = [
  { id: "home",        icon: <Home size={18} strokeWidth={1.8} />,        label: "Home" },
  { id: "ai",          icon: <Sparkles size={18} strokeWidth={1.8} />,    label: "AI" },
  { id: "inspect",     icon: <ScanEye size={18} strokeWidth={1.8} />,     label: "Inspect" },
  { id: "dom",         icon: <Layers size={18} strokeWidth={1.8} />,      label: "DOM" },
  { id: "network",     icon: <Network size={18} strokeWidth={1.8} />,     label: "Network" },
  { id: "performance", icon: <Activity size={18} strokeWidth={1.8} />,    label: "Performance" },
  { id: "console",     icon: <Terminal size={18} strokeWidth={1.8} />,    label: "Console" },
  { id: "storage",     icon: <Database size={18} strokeWidth={1.8} />,    label: "Storage" },
  { id: "security",    icon: <ShieldCheck size={18} strokeWidth={1.8} />, label: "Security" },
  { id: "extensions",  icon: <Blocks size={18} strokeWidth={1.8} />,      label: "Extensions" },
];

const SIDEBAR_BOTTOM: SidebarItem[] = [
  { id: "settings",    icon: <Settings size={18} strokeWidth={1.8} />,    label: "Settings" },
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

  // Sync input bar with active tab url
  useEffect(() => {
    setInputUrl(activeTab?.url || "");
  }, [activeTab?.url]);

  const handleSidebarSelect = (id: string) => {
    if (id === "ai") {
      setAiOpen(!aiOpen);
      setActiveTool(aiOpen ? "home" : "ai");
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
          <span className="app__titlebar-kage">K A G E</span>
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
          onClose={() => {
            setAiOpen(false);
            if (activeTool === "ai") setActiveTool("home");
          }}
        />

        {/* Main Viewport Stage */}
        <main className="app__content" role="main" aria-label="Browser viewport">
          <div className="app__viewport-stage">
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
