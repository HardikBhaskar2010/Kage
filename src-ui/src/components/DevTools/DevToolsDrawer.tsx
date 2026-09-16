import React, { useState, useEffect } from "react";
import "./DevToolsDrawer.css";
import { useBrowser } from "../../context/BrowserContext";
import { ConsolePanel } from "./ConsolePanel";
import { NetworkPanel } from "./NetworkPanel";
import { ElementsPanel } from "./ElementsPanel";
import { PerformancePanel } from "./PerformancePanel";
import { StoragePanel } from "./StoragePanel";
import { SecurityPanel } from "./SecurityPanel";
import {
  Terminal,
  Activity,
  Layers,
  Gauge,
  Database,
  ShieldCheck,
  X,
  Maximize2,
  Minimize2,
} from "lucide-react";

export type DevToolsTab = "console" | "network" | "dom" | "performance" | "storage" | "security";

export const DevToolsDrawer: React.FC = () => {
  const { devToolsOpen, setDevToolsOpen, activeTool, setActiveTool } = useBrowser();
  const [activeTab, setActiveTab] = useState<DevToolsTab>("console");
  const [isExpanded, setIsExpanded] = useState(false);

  // Sync with sidebar active tool
  useEffect(() => {
    if (
      activeTool === "console" ||
      activeTool === "network" ||
      activeTool === "dom" ||
      activeTool === "performance" ||
      activeTool === "storage" ||
      activeTool === "security"
    ) {
      setActiveTab(activeTool as DevToolsTab);
      setDevToolsOpen(true);
    }
  }, [activeTool, setDevToolsOpen]);

  return (
    <section
      className={`devtools-drawer ${devToolsOpen ? "devtools-drawer--open" : "devtools-drawer--closed"} ${isExpanded ? "devtools-drawer--expanded" : ""}`}
      aria-label="KAGE Developer Tools Suite"
      aria-hidden={!devToolsOpen}
    >
      {/* ── Tabs & Window Bar ──────────────────────────────────── */}
      <div className="devtools-header">
        <div className="devtools-tabs" role="tablist">
          <button
            className={`devtools-tab ${activeTab === "console" ? "devtools-tab--active" : ""}`}
            onClick={() => { setActiveTab("console"); setActiveTool("console"); }}
            role="tab"
            aria-selected={activeTab === "console"}
          >
            <span className="devtools-tab__icon devtools-tab__icon--console">
              <Terminal size={12} strokeWidth={2} />
            </span>
            <span>Console</span>
            <span className="devtools-tab__badge devtools-tab__badge--info">1</span>
          </button>
          <button
            className={`devtools-tab ${activeTab === "network" ? "devtools-tab--active" : ""}`}
            onClick={() => { setActiveTab("network"); setActiveTool("network"); }}
            role="tab"
            aria-selected={activeTab === "network"}
          >
            <span className="devtools-tab__icon devtools-tab__icon--network">
              <Activity size={12} strokeWidth={2} />
            </span>
            <span>Network</span>
            <span className="devtools-tab__badge devtools-tab__badge--live">LIVE</span>
          </button>
          <button
            className={`devtools-tab ${activeTab === "dom" ? "devtools-tab--active" : ""}`}
            onClick={() => { setActiveTab("dom"); setActiveTool("dom"); }}
            role="tab"
            aria-selected={activeTab === "dom"}
          >
            <span className="devtools-tab__icon devtools-tab__icon--dom">
              <Layers size={12} strokeWidth={2} />
            </span>
            <span>Elements</span>
          </button>
          <button
            className={`devtools-tab ${activeTab === "performance" ? "devtools-tab--active" : ""}`}
            onClick={() => { setActiveTab("performance"); setActiveTool("performance"); }}
            role="tab"
            aria-selected={activeTab === "performance"}
          >
            <span className="devtools-tab__icon devtools-tab__icon--perf">
              <Gauge size={12} strokeWidth={2} />
            </span>
            <span>Performance</span>
            <span className="devtools-tab__badge devtools-tab__badge--fps">120 FPS</span>
          </button>
          <button
            className={`devtools-tab ${activeTab === "storage" ? "devtools-tab--active" : ""}`}
            onClick={() => { setActiveTab("storage"); setActiveTool("storage"); }}
            role="tab"
            aria-selected={activeTab === "storage"}
          >
            <span className="devtools-tab__icon devtools-tab__icon--storage">
              <Database size={12} strokeWidth={2} />
            </span>
            <span>Storage</span>
          </button>
          <button
            className={`devtools-tab ${activeTab === "security" ? "devtools-tab--active" : ""}`}
            onClick={() => { setActiveTab("security"); setActiveTool("security"); }}
            role="tab"
            aria-selected={activeTab === "security"}
          >
            <span className="devtools-tab__icon devtools-tab__icon--sec">
              <ShieldCheck size={12} strokeWidth={2} />
            </span>
            <span>Security</span>
            <span className="devtools-tab__badge devtools-tab__badge--sec">TLS 1.3</span>
          </button>
        </div>

        <div className="devtools-controls">
          <button
            className="devtools-ctrl-btn"
            onClick={() => setIsExpanded(!isExpanded)}
            title={isExpanded ? "Restore height (Ctrl+Shift+D)" : "Maximize height (Ctrl+Shift+D)"}
            aria-label={isExpanded ? "Restore size" : "Expand size"}
          >
            {isExpanded ? <Minimize2 size={13} strokeWidth={2} /> : <Maximize2 size={13} strokeWidth={2} />}
          </button>
          <button
            className="devtools-ctrl-btn devtools-ctrl-btn--close"
            onClick={() => {
              setDevToolsOpen(false);
              setActiveTool("home");
            }}
            title="Close DevTools Drawer"
            aria-label="Close DevTools Drawer"
          >
            <X size={14} strokeWidth={2} />
          </button>
        </div>
      </div>

      {/* ── Active Panel View ──────────────────────────────────── */}
      <div className="devtools-content">
        {activeTab === "console" && <ConsolePanel />}
        {activeTab === "network" && <NetworkPanel />}
        {activeTab === "dom" && <ElementsPanel />}
        {activeTab === "performance" && <PerformancePanel />}
        {activeTab === "storage" && <StoragePanel />}
        {activeTab === "security" && <SecurityPanel />}
      </div>
    </section>
  );
};
