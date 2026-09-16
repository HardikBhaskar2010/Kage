import type { BrowserAdapter, TabInfo } from "./BrowserAdapter";

/**
 * MockBrowserAdapter — Handles tab management and simulated navigation
 * when running in pure frontend web development mode (http://localhost:5173/).
 */
export class MockBrowserAdapter implements BrowserAdapter {
  private tabs: TabInfo[] = [
    {
      id: "tab-1",
      url: "",
      title: "New Tab",
      favicon: "kage",
      isLoading: false,
      canGoBack: false,
      canGoForward: false,
      isSecure: true,
    },
  ];

  async createTab(url = "", title = "New Tab"): Promise<TabInfo> {
    const newTab: TabInfo = {
      id: `tab-${Date.now()}`,
      url,
      title: title || (url ? this.formatTitle(url) : "New Tab"),
      favicon: url ? "globe" : "kage",
      isLoading: Boolean(url),
      canGoBack: false,
      canGoForward: false,
      isSecure: url.startsWith("https://"),
    };
    this.tabs.push(newTab);
    return newTab;
  }

  async closeTab(tabId: string): Promise<void> {
    this.tabs = this.tabs.filter((t) => t.id !== tabId);
    if (this.tabs.length === 0) {
      await this.createTab();
    }
  }

  async switchTab(_tabId: string): Promise<void> {
    // Pure state tracking in mock mode
  }

  async navigate(tabId: string, url: string): Promise<void> {
    const tab = this.tabs.find((t) => t.id === tabId);
    if (tab) {
      tab.url = url;
      tab.title = url ? this.formatTitle(url) : "New Tab";
      tab.isSecure = url.startsWith("https://");
      tab.canGoBack = true;
    }
  }

  async goBack(tabId: string): Promise<void> {
    const tab = this.tabs.find((t) => t.id === tabId);
    if (tab) {
      tab.canGoBack = false;
      tab.canGoForward = true;
    }
  }

  async goForward(tabId: string): Promise<void> {
    const tab = this.tabs.find((t) => t.id === tabId);
    if (tab) {
      tab.canGoForward = false;
      tab.canGoBack = true;
    }
  }

  async refresh(_tabId: string): Promise<void> {
    // Simulated reload
  }

  private formatTitle(url: string): string {
    try {
      const parsed = new URL(url.startsWith("http") ? url : `https://${url}`);
      return parsed.hostname.replace("www.", "");
    } catch {
      return url;
    }
  }
}
