import React, { createContext, useContext, useState, useCallback, useEffect } from "react";
import { getBrowserAdapter, isTauriEnvironment } from "../adapters/BrowserAdapter";
import { getTelemetryProvider } from "../adapters/TelemetryProvider";
import {
  getDomDocument,
  listProfiles,
  createProfile as ipcCreateProfile,
  deleteProfile as ipcDeleteProfile,
  getActiveProfile,
} from "../ipc/client";
import type { ProfileMetadata, ProfileKind } from "../ipc/client";
import { listen } from "@tauri-apps/api/event";

export interface TabState {
  id: string;
  title: string;
  url: string;
  favicon?: string;
  canGoBack: boolean;
  canGoForward: boolean;
  history: string[];
  historyIndex: number;
  isLoading: boolean;
  isSecure: boolean;
  profile_id?: string;
}

export interface Bookmark {
  id: string;
  title: string;
  url: string;
  createdAt: Date;
}

export interface DownloadItem {
  id: string;
  filename: string;
  size: string;
  progress: number; // 0 to 100
  speed: string;
  status: "downloading" | "completed" | "paused";
}

export interface ConsoleLog {
  id: string;
  level: "error" | "warn" | "info" | "log";
  message: string;
  source: string;
  timestamp: string;
  data?: any;
}

export interface NetworkRequest {
  id: string;
  url: string;
  method: "GET" | "POST" | "PUT" | "DELETE";
  status: number;
  statusText: string;
  type: "fetch" | "xhr" | "script" | "stylesheet" | "image" | "document";
  size: string;
  time: number; // in ms
  headers: Record<string, string>;
  responseSnippet?: string;
}

export interface DOMNode {
  id: string;
  tag: string;
  className?: string;
  attributes: Record<string, string>;
  text?: string;
  children?: DOMNode[];
  styles?: Record<string, string>;
  boxModel?: {
    margin: number;
    border: number;
    padding: number;
    width: number;
    height: number;
  };
}

export type ActiveTool =
  | "none"
  | "home"
  | "ai"
  | "inspect"
  | "dom"
  | "network"
  | "performance"
  | "console"
  | "storage"
  | "security"
  | "extensions"
  | "settings";

interface BrowserContextType {
  // Tabs & Navigation
  tabs: TabState[];
  activeTabId: string;
  activeTab: TabState | undefined;
  createTab: (url?: string, title?: string, profileId?: string) => void;
  closeTab: (id: string) => void;
  switchTab: (id: string) => void;
  navigate: (url: string) => void;
  goBack: () => void;
  goForward: () => void;
  refresh: () => void;

  // Profiles & Permission State (Phase 7 - INV-06, INV-10)
  profiles: ProfileMetadata[];
  activeProfileId: string;
  activeProfile: ProfileMetadata | undefined;
  profileSwitcherOpen: boolean;
  setProfileSwitcherOpen: (open: boolean) => void;
  escalationModal: {
    isOpen: boolean;
    tabId: string;
    targetProfileId: string;
    reason: string;
  } | null;
  setEscalationModal: (modal: {
    isOpen: boolean;
    tabId: string;
    targetProfileId: string;
    reason: string;
  } | null) => void;
  createProfile: (name: string, kind: ProfileKind, color?: string, icon?: string) => Promise<ProfileMetadata>;
  deleteProfile: (profileId: string) => Promise<void>;
  switchProfile: (profileId: string) => void;
  openTabInProfile: (profileId: string, url?: string) => void;
  requestSessionEscalation: (tabId: string, targetProfileId: string, reason: string) => Promise<boolean>;

  // Active Tools & Drawers
  activeTool: ActiveTool;
  setActiveTool: (tool: ActiveTool) => void;
  aiOpen: boolean;
  setAiOpen: (open: boolean) => void;
  devToolsOpen: boolean;
  setDevToolsOpen: (open: boolean) => void;
  inspectMode: boolean;
  setInspectMode: (active: boolean) => void;

  // Bookmarks
  bookmarks: Bookmark[];
  isBookmarked: (url: string) => boolean;
  toggleBookmark: (url: string, title?: string) => void;

  // Downloads
  downloads: DownloadItem[];
  downloadsOpen: boolean;
  setDownloadsOpen: (open: boolean) => void;
  startMockDownload: (filename: string, size: string) => void;

  // Telemetry (Console, Network, DOM for DevTools)
  consoleLogs: ConsoleLog[];
  addConsoleLog: (level: ConsoleLog["level"], message: string, source?: string) => void;
  clearConsoleLogs: () => void;

  networkRequests: NetworkRequest[];
  clearNetworkRequests: () => void;

  activeDomTree: DOMNode;
  isLoadingDom: boolean;
  selectedDomNodeId: string | null;
  setSelectedDomNodeId: (id: string | null) => void;
  refreshDomTree: () => Promise<void>;

  // Settings state
  accentTheme: "peach" | "violet" | "frost";
  setAccentTheme: (theme: "peach" | "violet" | "frost") => void;
  glassOpacity: number;
  setGlassOpacity: (val: number) => void;
  adBlockerEnabled: boolean;
  setAdBlockerEnabled: (val: boolean) => void;
  aiModel: string;
  setAiModel: (model: string) => void;
}

const DEFAULT_PROFILES: ProfileMetadata[] = [
  {
    id: "personal",
    name: "Personal",
    kind: "personal",
    color: "#3B82F6",
    icon: "user",
    created_at: Date.now(),
    last_used: Date.now(),
    is_ephemeral: false,
  },
  {
    id: "work",
    name: "Work Workspace",
    kind: "work",
    color: "#8B5CF6",
    icon: "briefcase",
    created_at: Date.now(),
    last_used: Date.now(),
    is_ephemeral: false,
  },
  {
    id: "agent_sandbox",
    name: "Agent Clean Sandbox",
    kind: "agent_sandbox",
    color: "#EC4899",
    icon: "shield-alert",
    created_at: Date.now(),
    last_used: Date.now(),
    is_ephemeral: true,
  },
];

const INITIAL_TABS: TabState[] = [
  {
    id: "tab-1",
    title: "New Tab",
    url: "",
    canGoBack: false,
    canGoForward: false,
    history: [""],
    historyIndex: 0,
    isLoading: false,
    isSecure: true,
    profile_id: "personal",
  },
];

const INITIAL_LOGS: ConsoleLog[] = [
  {
    id: "log-1",
    level: "info",
    message: "[KAGE Host] Chromium Embedded Framework v126 initialized with OSR compositor.",
    source: "host::cef_pump:88",
    timestamp: "02:01:02.140",
  },
  {
    id: "log-2",
    level: "log",
    message: "[ToolBus] Registered 12 native capability drivers (Tier 1-3).",
    source: "kage_core::bus:45",
    timestamp: "02:01:02.144",
  },
  {
    id: "log-3",
    level: "warn",
    message: "[Security] Third-party cross-origin cookie partitioned by default.",
    source: "kage_security::cookies:112",
    timestamp: "02:01:02.210",
  },
  {
    id: "log-4",
    level: "log",
    message: "[ContextEngine] DOM observer active with 4,000 token budget envelope.",
    source: "kage_context::pack:78",
    timestamp: "02:01:02.304",
  },
  {
    id: "log-5",
    level: "error",
    message: "Failed to load resource: the server responded with a status of 404 (Not Found) - favicon.ico",
    source: "https://example.com/favicon.ico",
    timestamp: "02:01:03.012",
  },
];

const INITIAL_REQUESTS: NetworkRequest[] = [
  {
    id: "net-1",
    url: "https://api.github.com/user/repos",
    method: "GET",
    status: 200,
    statusText: "OK",
    type: "fetch",
    size: "14.2 KB",
    time: 142,
    headers: {
      "content-type": "application/json; charset=utf-8",
      "x-ratelimit-remaining": "4980",
      "server": "GitHub.com",
    },
    responseSnippet: '{\n  "total_count": 18,\n  "repositories": ["kage-browser", "quantum-sim", "liquid-glass-ui"]\n}',
  },
  {
    id: "net-2",
    url: "https://cdn.tailwindcss.com/3.4.1",
    method: "GET",
    status: 304,
    statusText: "Not Modified",
    type: "script",
    size: "82.4 KB",
    time: 48,
    headers: {
      "cache-control": "public, max-age=31536000, immutable",
      "content-type": "application/javascript",
    },
  },
  {
    id: "net-3",
    url: "https://fonts.googleapis.com/css2?family=Inter:wght@400;600;700",
    method: "GET",
    status: 200,
    statusText: "OK",
    type: "stylesheet",
    size: "4.8 KB",
    time: 65,
    headers: { "content-type": "text/css; charset=utf-8" },
  },
  {
    id: "net-4",
    url: "https://analytics.internal/beacon",
    method: "POST",
    status: 404,
    statusText: "Not Found",
    type: "fetch",
    size: "0 B",
    time: 310,
    headers: { "content-type": "text/plain" },
    responseSnippet: "404 Not Found",
  },
];

export function transformCdpNode(cdpNode: any): DOMNode | null {
  if (!cdpNode) return null;

  if (cdpNode.nodeType === 9 && Array.isArray(cdpNode.children)) {
    const htmlChild = cdpNode.children.find((c: any) => c.nodeType === 1);
    if (htmlChild) return transformCdpNode(htmlChild);
  }

  const rawTag = cdpNode.localName || cdpNode.nodeName || "";
  const tag = rawTag.toLowerCase().replace(/^#/, "");

  const attributes: Record<string, string> = {};
  let className: string | undefined = undefined;

  if (Array.isArray(cdpNode.attributes)) {
    for (let i = 0; i < cdpNode.attributes.length; i += 2) {
      const k = cdpNode.attributes[i];
      const v = cdpNode.attributes[i + 1] ?? "";
      if (k === "class") {
        className = v;
      } else {
        attributes[k] = v;
      }
    }
  }

  let text: string | undefined = cdpNode.nodeValue || undefined;
  const children: DOMNode[] = [];

  if (Array.isArray(cdpNode.children)) {
    for (const child of cdpNode.children) {
      if (child.nodeType === 3) {
        const val = (child.nodeValue || "").trim();
        if (val) {
          text = text ? `${text} ${val}` : val;
        }
      } else if (child.nodeType === 1) {
        const trans = transformCdpNode(child);
        if (trans) children.push(trans);
      }
    }
  }

  return {
    id: String(cdpNode.nodeId || Math.random().toString(36).slice(2)),
    tag: tag || "div",
    className,
    attributes,
    text,
    children: children.length > 0 ? children : undefined,
    styles: {},
    boxModel: {
      margin: 0,
      border: 0,
      padding: 0,
      width: 0,
      height: 0,
    },
  };
}

const SAMPLE_DOM_TREE: DOMNode = {
  id: "node-root",
  tag: "html",
  className: "dark-mode",
  attributes: { lang: "en" },
  styles: { background: "#0a0307", color: "#f9dbbd", fontFamily: "Inter, sans-serif" },
  boxModel: { margin: 0, border: 0, padding: 0, width: 1440, height: 900 },
  children: [
    {
      id: "node-head",
      tag: "head",
      attributes: {},
      children: [
        { id: "node-title", tag: "title", attributes: {}, text: "KAGE Browser for Builders" },
        { id: "node-meta-1", tag: "meta", attributes: { charset: "utf-8" } },
      ],
    },
    {
      id: "node-body",
      tag: "body",
      className: "kage-app-canvas",
      attributes: { role: "main" },
      styles: { display: "flex", flexDirection: "column", minHeight: "100vh" },
      boxModel: { margin: 0, border: 0, padding: 0, width: 1440, height: 900 },
      children: [
        {
          id: "node-header",
          tag: "header",
          className: "hero-banner",
          attributes: { "aria-label": "Hero Navigation" },
          styles: { padding: "32px", display: "flex", alignItems: "center", justifyContent: "space-between" },
          boxModel: { margin: 0, border: 1, padding: 32, width: 1440, height: 80 },
          children: [
            { id: "node-logo", tag: "div", className: "brand-logo", attributes: {}, text: "KAGE 影" },
            { id: "node-nav", tag: "nav", className: "header-nav", attributes: {}, text: "Docs · Showcase · GitHub" },
          ],
        },
        {
          id: "node-main",
          tag: "main",
          className: "content-container",
          attributes: {},
          styles: { maxWidth: "1200px", margin: "0 auto", padding: "48px 24px" },
          boxModel: { margin: 24, border: 0, padding: 48, width: 1200, height: 600 },
          children: [
            { id: "node-h1", tag: "h1", className: "page-title", attributes: {}, text: "The Developer-First Autonomous Browser" },
            { id: "node-p1", tag: "p", className: "lead-text", attributes: {}, text: "Bridging the gap between manual inspection and agentic browser execution." },
            { id: "node-btn-cta", tag: "button", className: "btn-primary-peach", attributes: { type: "button" }, text: "Start Building with Kage" },
          ],
        },
      ],
    },
  ],
};

const BrowserContext = createContext<BrowserContextType | undefined>(undefined);

let tabIdCounter = 2;

export const BrowserProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [tabs, setTabs] = useState<TabState[]>(INITIAL_TABS);
  const [activeTabId, setActiveTabId] = useState<string>("tab-1");
  const [activeTool, setActiveToolRaw] = useState<ActiveTool>("home");
  const [aiOpen, setAiOpenRaw] = useState(false);

  // Profiles state (Phase 7 - INV-06, INV-10)
  const [profiles, setProfiles] = useState<ProfileMetadata[]>(DEFAULT_PROFILES);
  const [activeProfileId, setActiveProfileId] = useState<string>("personal");
  const [profileSwitcherOpen, setProfileSwitcherOpen] = useState(false);
  const [escalationModal, setEscalationModal] = useState<{
    isOpen: boolean;
    tabId: string;
    targetProfileId: string;
    reason: string;
  } | null>(null);

  // Load registered profiles on startup
  useEffect(() => {
    if (!isTauriEnvironment()) return;
    listProfiles()
      .then((loaded) => {
        if (loaded && loaded.length > 0) {
          setProfiles(loaded);
        }
      })
      .catch((err) => console.warn("Failed to load profiles:", err));

    getActiveProfile()
      .then((meta) => {
        if (meta && meta.id) {
          setActiveProfileId(meta.id);
        }
      })
      .catch((err) => console.warn("Failed to load active profile:", err));
  }, []);

  const createProfile = useCallback(
    async (name: string, kind: ProfileKind, color?: string, icon?: string) => {
      const id = `${kind}_${Date.now()}`;
      if (isTauriEnvironment()) {
        const created = await ipcCreateProfile({ id, name, kind, color, icon });
        setProfiles((prev) => [...prev.filter((p) => p.id !== created.id), created]);
        return created;
      } else {
        const fallback: ProfileMetadata = {
          id,
          name,
          kind,
          color: color || "#3B82F6",
          icon: icon || "user",
          created_at: Date.now(),
          last_used: Date.now(),
          is_ephemeral: kind === "agent_sandbox" || kind === "temporary",
        };
        setProfiles((prev) => [...prev, fallback]);
        return fallback;
      }
    },
    []
  );

  const deleteProfile = useCallback(async (profileId: string) => {
    if (profileId === "personal") {
      throw new Error("Cannot delete default personal profile");
    }
    if (isTauriEnvironment()) {
      await ipcDeleteProfile(profileId);
    }
    setProfiles((prev) => prev.filter((p) => p.id !== profileId));
  }, []);

  const switchProfile = useCallback((profileId: string) => {
    setActiveProfileId(profileId);
  }, []);

  // Synchronized state architecture (Apple Design & Emil Kowalski Design Engineering)
  // Single Source of Truth: Drawer visibility and rail tab state are 100% mutually consistent.
  const setActiveTool = useCallback((tool: ActiveTool) => {
    setActiveToolRaw(tool);
    setAiOpenRaw(tool === "ai");
  }, []);

  const setAiOpen = useCallback((open: boolean) => {
    setAiOpenRaw(open);
    setActiveToolRaw((prev) => {
      if (open) return "ai";
      return prev === "ai" ? "home" : prev;
    });
  }, []);
  const [devToolsOpen, setDevToolsOpen] = useState(false);
  const [inspectMode, setInspectMode] = useState(false);

  // Real user bookmarks (persisted in localStorage)
  const [bookmarks, setBookmarks] = useState<Bookmark[]>(() => {
    try {
      const saved = localStorage.getItem("kage_bookmarks");
      return saved ? JSON.parse(saved) : [];
    } catch {
      return [];
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem("kage_bookmarks", JSON.stringify(bookmarks));
    } catch {}
  }, [bookmarks]);

  // Real user downloads
  const [downloads, setDownloads] = useState<DownloadItem[]>([]);
  const [downloadsOpen, setDownloadsOpen] = useState(false);

  // Telemetry
  const [consoleLogs, setConsoleLogs] = useState<ConsoleLog[]>(INITIAL_LOGS);
  const [networkRequests, setNetworkRequests] = useState<NetworkRequest[]>(INITIAL_REQUESTS);
  const [activeDomTree, setActiveDomTree] = useState<DOMNode>(SAMPLE_DOM_TREE);
  const [isLoadingDom, setIsLoadingDom] = useState<boolean>(false);
  const [selectedDomNodeId, setSelectedDomNodeId] = useState<string | null>("node-h1");

  const refreshDomTree = useCallback(async () => {
    if (!isTauriEnvironment() || !activeTabId) return;
    setIsLoadingDom(true);
    try {
      const doc: any = await getDomDocument(activeTabId);
      if (doc && doc.root) {
        const transformed = transformCdpNode(doc.root);
        if (transformed) {
          setActiveDomTree(transformed);
        }
      }
    } catch (err) {
      console.warn("Failed to fetch live DOM tree:", err);
    } finally {
      setIsLoadingDom(false);
    }
  }, [activeTabId]);

  // Settings
  const [accentTheme, setAccentTheme] = useState<"peach" | "violet" | "frost">("peach");
  const [glassOpacity, setGlassOpacity] = useState<number>(0.85);
  const [adBlockerEnabled, setAdBlockerEnabled] = useState<boolean>(true);
  const [aiModel, setAiModel] = useState<string>("GPT-4o");

  const activeTab = tabs.find((t) => t.id === activeTabId);
  const activeProfile = profiles.find((p) => p.id === (activeTab?.profile_id || activeProfileId)) || profiles[0];

  // ─── Native Chromium Control Plane Events ──────────────────────────
  useEffect(() => {
    if (!isTauriEnvironment()) return;

    let unlistenFn: (() => void) | null = null;
    listen<any>("kage:browser:event", (event) => {
      const payload = event.payload;
      if (!payload || !payload.kind) return;
      const { tab_id, kind } = payload;

      if (kind.NavigationStarted) {
        const { requested_url } = kind.NavigationStarted;
        setTabs((prev) =>
          prev.map((t) =>
            !tab_id || t.id === tab_id
              ? { ...t, url: requested_url, isLoading: true }
              : t
          )
        );
      } else if (kind.NavigationCommitted) {
        const { url } = kind.NavigationCommitted;
        setTabs((prev) =>
          prev.map((t) =>
            !tab_id || t.id === tab_id
              ? { ...t, url }
              : t
          )
        );
      } else if (kind.NavigationCompleted) {
        const { url, http_status } = kind.NavigationCompleted;
        setTabs((prev) =>
          prev.map((t) =>
            !tab_id || t.id === tab_id
              ? {
                  ...t,
                  url,
                  isLoading: false,
                  title: t.title === "New Tab" || !t.title ? (url ? url.replace(/^https?:\/\//, "").split("/")[0] : "New Tab") : t.title,
                }
              : t
          )
        );
        setConsoleLogs((prev) => [
          ...prev,
          {
            id: `log-${Date.now()}`,
            level: "info",
            message: `Navigated to ${url} (HTTP ${http_status})`,
            source: "network::http_loader",
            timestamp: new Date().toLocaleTimeString(),
          },
        ]);
      } else if (kind.NavigationFailed) {
        const { url, error_code, reason } = kind.NavigationFailed;
        setTabs((prev) =>
          prev.map((t) =>
            !tab_id || t.id === tab_id
              ? { ...t, isLoading: false }
              : t
          )
        );
        setConsoleLogs((prev) => [
          ...prev,
          {
            id: `log-${Date.now()}`,
            level: "error",
            message: `Navigation to ${url} failed: ${reason} (code ${error_code})`,
            source: "network::http_loader",
            timestamp: new Date().toLocaleTimeString(),
          },
        ]);
      } else if (kind.NavigationCancelled) {
        setTabs((prev) =>
          prev.map((t) =>
            !tab_id || t.id === tab_id
              ? { ...t, isLoading: false }
              : t
          )
        );
      } else if (kind.TabAddressChanged) {
        const { url } = kind.TabAddressChanged;
        setTabs((prev) =>
          prev.map((t) =>
            !tab_id || t.id === tab_id
              ? { ...t, url }
              : t
          )
        );
      } else if (kind.TabTitleChanged) {
        const { title } = kind.TabTitleChanged;
        setTabs((prev) =>
          prev.map((t) =>
            !tab_id || t.id === tab_id
              ? { ...t, title }
              : t
          )
        );
      } else if (kind.ActiveTabSwitched) {
        const { current } = kind.ActiveTabSwitched;
        setActiveTabId(current);
      } else if (kind.TabClosed) {
        if (tab_id) {
          setTabs((prev) => prev.filter((t) => t.id !== tab_id));
        }
      } else if (kind.ConsoleMessage) {
        const { level, message } = kind.ConsoleMessage;
        setConsoleLogs((prev) => [
          ...prev,
          {
            id: `log-${Date.now()}-${Math.random()}`,
            level: level === "error" ? "error" : level === "warn" ? "warn" : "info",
            message,
            source: "chromium::console",
            timestamp: new Date().toLocaleTimeString(),
          },
        ]);
      } else if (kind.StateSnapshot) {
        const { snapshot } = kind.StateSnapshot;
        if (snapshot && Array.isArray(snapshot.tabs) && snapshot.tabs.length > 0) {
          setTabs((prev) => {
            return snapshot.tabs.map((snapTab: any) => {
              const existing = prev.find((p) => p.id === snapTab.id);
              return {
                id: snapTab.id,
                title: snapTab.title || existing?.title || "New Tab",
                url: snapTab.url || existing?.url || "",
                canGoBack: snapTab.can_go_back ?? existing?.canGoBack ?? false,
                canGoForward: snapTab.can_go_forward ?? existing?.canGoForward ?? false,
                history: existing?.history || [snapTab.url || ""],
                historyIndex: existing?.historyIndex || 0,
                isLoading: snapTab.navigation_state?.Loading !== undefined,
                isSecure: snapTab.url ? snapTab.url.startsWith("https") : true,
                profile_id: snapTab.profile_id || existing?.profile_id || "personal",
              };
            });
          });
          if (snapshot.active_tab) {
            setActiveTabId(snapshot.active_tab);
          }
        }
      }
    }).then((unlisten) => {
      unlistenFn = unlisten;
    });

    return () => {
      if (unlistenFn) unlistenFn();
    };
  }, []);

  // ─── Telemetry Subscriptions (CDP / Mock) ─────────────────────────
  useEffect(() => {
    const telemetry = getTelemetryProvider();
    const unsubConsole = telemetry.onConsoleMessage((entry) => {
      setConsoleLogs((prev) => [
        ...prev,
        {
          id: entry.id,
          level: entry.type === "log" ? "log" : entry.type === "warn" ? "warn" : entry.type === "error" ? "error" : "info",
          message: entry.message,
          source: "cdp::runtime",
          timestamp: entry.timestamp,
        },
      ]);
    });

    const unsubNetwork = telemetry.onNetworkRequest((entry) => {
      setNetworkRequests((prev) => [
        ...prev,
        {
          id: entry.id,
          url: entry.url,
          method: (entry.method as any) || "GET",
          status: entry.status,
          statusText: entry.status === 200 ? "OK" : `${entry.status}`,
          type: (entry.type as any) || "fetch",
          size: entry.size,
          time: parseInt(entry.time) || 45,
          headers: {},
        },
      ]);
    });

    return () => {
      unsubConsole();
      unsubNetwork();
    };
  }, []);

  // ─── Tab Actions (Delegated to BrowserAdapter) ─────────────────────
  const createTab = useCallback(
    (url = "", title = "New Tab", profileId?: string) => {
      const targetProfile = profileId || activeProfileId || "personal";
      getBrowserAdapter().createTab(url, title, targetProfile).then((tabInfo) => {
        const newId = tabInfo.id || `tab-${++tabIdCounter}`;
        const newTab: TabState = {
          id: newId,
          title: tabInfo.title || (url ? url.replace(/^https?:\/\//, "").split("/")[0] : "New Tab"),
          url: tabInfo.url || url,
          canGoBack: tabInfo.canGoBack ?? false,
          canGoForward: tabInfo.canGoForward ?? false,
          history: [url],
          historyIndex: 0,
          isLoading: !!url,
          isSecure: tabInfo.isSecure ?? (url.startsWith("https") || url === ""),
          profile_id: tabInfo.profile_id || targetProfile,
        };
        setTabs((prev) => [...prev, newTab]);
        setActiveTabId(newId);
      });
    },
    [activeProfileId]
  );

  const openTabInProfile = useCallback(
    (profileId: string, url = "") => {
      createTab(url, "New Tab", profileId);
    },
    [createTab]
  );

  const requestSessionEscalation = useCallback(
    async (tabId: string, targetProfileId: string, reason: string): Promise<boolean> => {
      setEscalationModal({
        isOpen: true,
        tabId,
        targetProfileId,
        reason,
      });
      return false;
    },
    []
  );

  const closeTab = useCallback((id: string) => {
    getBrowserAdapter().closeTab(id).then(() => {
      setTabs((prev) => {
        const next = prev.filter((t) => t.id !== id);
        if (next.length === 0) {
          return [
            {
              id: `tab-${++tabIdCounter}`,
              title: "New Tab",
              url: "",
              canGoBack: false,
              canGoForward: false,
              history: [""],
              historyIndex: 0,
              isLoading: false,
              isSecure: true,
              profile_id: activeProfileId || "personal",
            },
          ];
        }
        return next;
      });
      if (id === activeTabId) {
        setTabs((prev) => {
          const last = prev[prev.length - 1];
          setActiveTabId(last.id);
          return prev;
        });
      }
    });
  }, [activeTabId, activeProfileId]);

  const switchTab = useCallback((id: string) => {
    getBrowserAdapter().switchTab(id).then(() => {
      setActiveTabId(id);
    });
  }, []);

  // ─── Navigation ───────────────────────────────────────────────────
  const navigate = useCallback((targetUrl: string) => {
    let cleanUrl = targetUrl.trim();
    if (!cleanUrl) {
      // Return to New Tab
      setTabs((prev) =>
        prev.map((t) =>
          t.id === activeTabId
            ? { ...t, url: "", title: "New Tab", isLoading: false }
            : t
        )
      );
      return;
    }

    if (!/^https?:\/\//i.test(cleanUrl) && !cleanUrl.startsWith("kage://")) {
      if (/^[\w-]+(\.[\w-]+)+/.test(cleanUrl)) {
        cleanUrl = `https://${cleanUrl}`;
      } else {
        cleanUrl = `https://www.google.com/search?q=${encodeURIComponent(cleanUrl)}`;
      }
    }

    const host = cleanUrl.replace(/^https?:\/\//, "").split("/")[0];

    // Optimistically update tab UI state
    setTabs((prev) =>
      prev.map((t) =>
        t.id === activeTabId
          ? {
              ...t,
              url: cleanUrl,
              title: host || cleanUrl,
              isLoading: true,
              canGoBack: true,
              history: [...t.history.slice(0, t.historyIndex + 1), cleanUrl],
              historyIndex: t.historyIndex + 1,
            }
          : t
      )
    );

    // Dispatch real navigation to host / CEF adapter
    getBrowserAdapter().navigate(activeTabId, cleanUrl).catch((err) => {
      console.error("Navigation failed:", err);
      setTabs((prev) =>
        prev.map((t) => (t.id === activeTabId ? { ...t, isLoading: false } : t))
      );
    });

    if (!isTauriEnvironment()) {
      setTimeout(() => {
        setTabs((prev) =>
          prev.map((t) =>
            t.id === activeTabId ? { ...t, isLoading: false } : t
          )
        );
        setConsoleLogs((prev) => [
          ...prev,
          {
            id: `log-${Date.now()}`,
            level: "info",
            message: `Navigated to ${cleanUrl} (HTTP 200 OK)`,
            source: "network::http_loader",
            timestamp: new Date().toLocaleTimeString(),
          },
        ]);
      }, 450);
    }
  }, [activeTabId]);

  const goBack = useCallback(() => {
    if (!isTauriEnvironment()) {
      setTabs((prev) =>
        prev.map((t) => {
          if (t.id !== activeTabId || t.historyIndex <= 0) return t;
          const nextIdx = t.historyIndex - 1;
          const prevUrl = t.history[nextIdx];
          return {
            ...t,
            historyIndex: nextIdx,
            url: prevUrl,
            title: prevUrl ? prevUrl.replace(/^https?:\/\//, "").split("/")[0] : "New Tab",
            canGoBack: nextIdx > 0,
            canGoForward: true,
          };
        })
      );
    }
    getBrowserAdapter().goBack(activeTabId).catch((err) => {
      console.error("goBack error:", err);
    });
  }, [activeTabId]);

  const goForward = useCallback(() => {
    if (!isTauriEnvironment()) {
      setTabs((prev) =>
        prev.map((t) => {
          if (t.id !== activeTabId || t.historyIndex >= t.history.length - 1) return t;
          const nextIdx = t.historyIndex + 1;
          const nextUrl = t.history[nextIdx];
          return {
            ...t,
            historyIndex: nextIdx,
            url: nextUrl,
            title: nextUrl ? nextUrl.replace(/^https?:\/\//, "").split("/")[0] : "New Tab",
            canGoBack: true,
            canGoForward: nextIdx < t.history.length - 1,
          };
        })
      );
    }
    getBrowserAdapter().goForward(activeTabId).catch((err) => {
      console.error("goForward error:", err);
    });
  }, [activeTabId]);

  const refresh = useCallback(() => {
    if (!activeTab?.url) return;
    setTabs((prev) =>
      prev.map((t) => (t.id === activeTabId ? { ...t, isLoading: true } : t))
    );
    getBrowserAdapter().refresh(activeTabId).catch((err) => {
      console.error("refresh error:", err);
      setTabs((prev) =>
        prev.map((t) => (t.id === activeTabId ? { ...t, isLoading: false } : t))
      );
    });
    if (!isTauriEnvironment()) {
      setTimeout(() => {
        setTabs((prev) =>
          prev.map((t) => (t.id === activeTabId ? { ...t, isLoading: false } : t))
        );
      }, 500);
    }
  }, [activeTab, activeTabId]);

  // ─── Bookmarks ────────────────────────────────────────────────────
  const isBookmarked = useCallback(
    (url: string) => bookmarks.some((b) => b.url === url && url !== ""),
    [bookmarks]
  );

  const toggleBookmark = useCallback((url: string, title = "") => {
    if (!url) return;
    setBookmarks((prev) => {
      const exists = prev.some((b) => b.url === url);
      if (exists) {
        return prev.filter((b) => b.url !== url);
      }
      return [
        ...prev,
        {
          id: `bm-${Date.now()}`,
          title: title || url.replace(/^https?:\/\//, "").split("/")[0],
          url,
          createdAt: new Date(),
        },
      ];
    });
  }, []);

  // ─── Downloads Simulation ─────────────────────────────────────────
  const startMockDownload = useCallback((filename: string, size: string) => {
    const newId = `dl-${Date.now()}`;
    const item: DownloadItem = {
      id: newId,
      filename,
      size,
      progress: 0,
      speed: "3.8 MB/s",
      status: "downloading",
    };
    setDownloads((prev) => [item, ...prev]);
    setDownloadsOpen(true);

    let curr = 0;
    const interval = setInterval(() => {
      curr += 20;
      if (curr >= 100) {
        clearInterval(interval);
        setDownloads((prev) =>
          prev.map((d) =>
            d.id === newId ? { ...d, progress: 100, speed: "Completed", status: "completed" } : d
          )
        );
      } else {
        setDownloads((prev) =>
          prev.map((d) =>
            d.id === newId ? { ...d, progress: curr } : d
          )
        );
      }
    }, 400);
  }, []);

  // ─── Telemetry helpers ───────────────────────────────────────────
  const addConsoleLog = useCallback(
    (level: ConsoleLog["level"], message: string, source = "console::interactive") => {
      setConsoleLogs((prev) => [
        ...prev,
        {
          id: `log-${Date.now()}-${Math.random()}`,
          level,
          message,
          source,
          timestamp: new Date().toLocaleTimeString(),
        },
      ]);
    },
    []
  );

  const clearConsoleLogs = useCallback(() => setConsoleLogs([]), []);
  const clearNetworkRequests = useCallback(() => setNetworkRequests([]), []);

  return (
    <BrowserContext.Provider
      value={{
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

        // Profiles & Permission State (Phase 7 - INV-06, INV-10)
        profiles,
        activeProfileId,
        activeProfile,
        profileSwitcherOpen,
        setProfileSwitcherOpen,
        escalationModal,
        setEscalationModal,
        createProfile,
        deleteProfile,
        switchProfile,
        openTabInProfile,
        requestSessionEscalation,

        activeTool,
        setActiveTool,
        aiOpen,
        setAiOpen,
        devToolsOpen,
        setDevToolsOpen,
        inspectMode,
        setInspectMode,

        bookmarks,
        isBookmarked,
        toggleBookmark,

        downloads,
        downloadsOpen,
        setDownloadsOpen,
        startMockDownload,

        consoleLogs,
        addConsoleLog,
        clearConsoleLogs,
        networkRequests,
        clearNetworkRequests,

        activeDomTree,
        isLoadingDom,
        selectedDomNodeId,
        setSelectedDomNodeId,
        refreshDomTree,

        accentTheme,
        setAccentTheme,
        glassOpacity,
        setGlassOpacity,
        adBlockerEnabled,
        setAdBlockerEnabled,
        aiModel,
        setAiModel,
      }}
    >
      {children}
    </BrowserContext.Provider>
  );
};

export const useBrowser = () => {
  const ctx = useContext(BrowserContext);
  if (!ctx) {
    throw new Error("useBrowser must be used within a BrowserProvider");
  }
  return ctx;
};
