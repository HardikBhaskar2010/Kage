import React, { useState, useCallback } from "react";
import "./App.css";

import { TabStrip } from "./components/TabStrip/TabStrip";
import type { Tab } from "./components/TabStrip/TabStrip";
import { Omnibox } from "./components/Omnibox/Omnibox";
import { IconSidebar } from "./components/IconSidebar/IconSidebar";
import type { SidebarItem } from "./components/IconSidebar/IconSidebar";
import { AISidebar } from "./components/AISidebar/AISidebar";
import { NewTab } from "./components/NewTab/NewTab";

// ─── Sidebar icons ─────────────────────────────────────────────────────────

const HomeIcon    = () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M2 6.5L8 2l6 4.5V14H11v-3.5H5V14H2V6.5z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/></svg>;
const InspectIcon = () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M2 2h12v12H2z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/><path d="M5 5h6M5 8h4M5 11h3" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/></svg>;
const DomIcon     = () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M3 3h10v2H3zM5 7h6v2H5zM7 11h2v2H7z" stroke="currentColor" strokeWidth="1.1" strokeLinejoin="round"/></svg>;
const NetworkIcon = () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="5.5" stroke="currentColor" strokeWidth="1.2"/><path d="M8 2.5v11M2.5 8h11M3.5 5A9 9 0 0 1 12.5 5M3.5 11a9 9 0 0 0 9 0" stroke="currentColor" strokeWidth="1.1" strokeLinecap="round"/></svg>;
const PerfIcon    = () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M2 12L5.5 7 8 9.5 11 5.5 14 12H2z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/></svg>;
const TestsIcon   = () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M6 2h4l2 5H4L6 2z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/><path d="M4 7l-2 7h12l-2-7" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/><path d="M6 11l1.5 1.5L11 9" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round"/></svg>;
const ConsoleIcon = () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M2 4h12v8H2z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/><path d="M5 7l2 1.5L5 10M9 10h2" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round"/></svg>;
const StorageIcon = () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect x="2" y="3" width="12" height="3" rx="1" stroke="currentColor" strokeWidth="1.2"/><rect x="2" y="7" width="12" height="3" rx="1" stroke="currentColor" strokeWidth="1.2"/><rect x="2" y="11" width="12" height="3" rx="1" stroke="currentColor" strokeWidth="1.2"/></svg>;
const SecurityIcon= () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M8 2L3 4v4c0 2.8 2.1 5.4 5 6 2.9-.6 5-3.2 5-6V4L8 2z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/><path d="M5.5 8l2 2L11 6" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round"/></svg>;
const AIIcon      = () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M8 2l1.5 4.5H14L10.5 9 12 13.5 8 11 4 13.5 5.5 9 2 6.5h4.5L8 2z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/></svg>;
const ExtIcon     = () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M6 2h4v2h2a1 1 0 0 1 1 1v2h-2V5.5H5V7H3V5a1 1 0 0 1 1-1h2V2zM3 9h2v4.5h6V9h2v5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9z" stroke="currentColor" strokeWidth="1.1" strokeLinejoin="round"/></svg>;
const WorkspaceIcon=()=> <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect x="2" y="2" width="5" height="5" rx="1" stroke="currentColor" strokeWidth="1.2"/><rect x="9" y="2" width="5" height="5" rx="1" stroke="currentColor" strokeWidth="1.2"/><rect x="2" y="9" width="5" height="5" rx="1" stroke="currentColor" strokeWidth="1.2"/><rect x="9" y="9" width="5" height="5" rx="1" stroke="currentColor" strokeWidth="1.2"/></svg>;
const SettingsIcon= () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="2.5" stroke="currentColor" strokeWidth="1.2"/><path d="M8 2v1.5M8 12.5V14M2 8h1.5M12.5 8H14M3.6 3.6l1 1M11.4 11.4l1 1M12.4 3.6l-1 1M4.6 11.4l-1 1" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/></svg>;

const SIDEBAR_ITEMS: SidebarItem[] = [
  { id: "home",      icon: <HomeIcon />,     label: "Home"      },
  { id: "inspect",   icon: <InspectIcon />,  label: "Inspect"   },
  { id: "dom",       icon: <DomIcon />,      label: "DOM"       },
  { id: "network",   icon: <NetworkIcon />,  label: "Network"   },
  { id: "perf",      icon: <PerfIcon />,     label: "Perf"      },
  { id: "tests",     icon: <TestsIcon />,    label: "Tests"     },
  { id: "console",   icon: <ConsoleIcon />,  label: "Console"   },
  { id: "storage",   icon: <StorageIcon />,  label: "Storage"   },
  { id: "security",  icon: <SecurityIcon />, label: "Security"  },
  { id: "ai",        icon: <AIIcon />,       label: "AI"        },
  { id: "ext",       icon: <ExtIcon />,      label: "Extensions"},
];

const SIDEBAR_BOTTOM: SidebarItem[] = [
  { id: "workspaces",icon: <WorkspaceIcon />,label: "Workspaces"},
  { id: "settings",  icon: <SettingsIcon />, label: "Settings"  },
];

let tabCounter = 3;

const INITIAL_TABS: Tab[] = [
  { id: "tab-1", title: "New Tab",            isActive: true  },
  { id: "tab-2", title: "Vercel – Build Faster", isActive: false },
  { id: "tab-3", title: "GitHub",             isActive: false },
];

export const App: React.FC = () => {
  const [tabs, setTabs]             = useState<Tab[]>(INITIAL_TABS);
  const [activeTab, setActiveTab]   = useState<string>("tab-1");
  const [url, setUrl]               = useState("");
  const [sidebarActive, setSidebarActive] = useState("home");
  const [aiOpen, setAiOpen]         = useState(false);

  // ─── Tab management ──────────────────────────────────────────────
  const selectTab = useCallback((id: string) => {
    setTabs((prev) => prev.map((t) => ({ ...t, isActive: t.id === id })));
    setActiveTab(id);
  }, []);

  const closeTab = useCallback((id: string) => {
    setTabs((prev) => {
      const next = prev.filter((t) => t.id !== id);
      if (next.length === 0) return [{ id: "tab-new", title: "New Tab", isActive: true }];
      if (id === activeTab) {
        const lastIdx = Math.max(0, prev.findIndex((t) => t.id === id) - 1);
        next[Math.min(lastIdx, next.length - 1)].isActive = true;
        setActiveTab(next[Math.min(lastIdx, next.length - 1)].id);
      }
      return next;
    });
  }, [activeTab]);

  const newTab = useCallback(() => {
    const tab = { id: `tab-${++tabCounter}`, title: "New Tab", isActive: true };
    setTabs((prev) => [...prev.map((t) => ({ ...t, isActive: false })), tab]);
    setActiveTab(tab.id);
    setUrl("");
  }, []);

  // ─── Navigation ───────────────────────────────────────────────────
  const navigate = useCallback((dest: string) => {
    setUrl(dest);
    setTabs((prev) =>
      prev.map((t) =>
        t.id === activeTab ? { ...t, title: dest.replace(/^https?:\/\//, "").split("/")[0] } : t
      )
    );
  }, [activeTab]);

  // ─── Sidebar toggle for AI ────────────────────────────────────────
  const handleSidebarSelect = (id: string) => {
    setSidebarActive(id);
    if (id === "ai") setAiOpen((prev) => !prev);
  };

  const isNewTab = url === "" && tabs.find((t) => t.id === activeTab)?.title === "New Tab";

  return (
    <div className="app" role="application" aria-label="KAGE Developer Browser">
      {/* ── Top bar: logo + tabs ──────────────────────────────────── */}
      <header className="app__titlebar glass-panel" role="banner">
        <div className="app__titlebar-logo" aria-label="KAGE">
          <img src="/Logo.png" alt="KAGE" height={20} className="app__logo-img" />
        </div>
        <TabStrip
          tabs={tabs}
          onTabSelect={selectTab}
          onTabClose={closeTab}
          onNewTab={newTab}
        />
        {/* Window controls placeholder (Tauri handles native on Windows) */}
        <div className="app__window-controls" aria-hidden="true">
          <span className="wc-btn wc-btn--min" />
          <span className="wc-btn wc-btn--max" />
          <span className="wc-btn wc-btn--close" />
        </div>
      </header>

      {/* ── Omnibox ──────────────────────────────────────────────── */}
      <Omnibox
        url={url}
        onUrlChange={setUrl}
        onNavigate={navigate}
        isSecure={url.startsWith("https")}
        onRefresh={() => navigate(url)}
      />

      {/* ── Main body: sidebar + content + AI ────────────────────── */}
      <div className="app__body">
        <IconSidebar
          items={SIDEBAR_ITEMS}
          bottomItems={SIDEBAR_BOTTOM}
          onSelect={handleSidebarSelect}
          activeId={sidebarActive}
        />

        {/* Browser viewport / content area */}
        <main className="app__content" role="main" aria-label="Browser viewport">
          {isNewTab ? (
            <NewTab onNavigate={navigate} />
          ) : (
            <div className="app__webview-placeholder kage-bg">
              <p className="app__webview-url">{url || "about:blank"}</p>
              <p className="app__webview-note">CEF WebView renders here (Chunk 7)</p>
            </div>
          )}
        </main>

        <AISidebar
          isOpen={aiOpen}
          onClose={() => setAiOpen(false)}
        />
      </div>
    </div>
  );
};

export default App;
