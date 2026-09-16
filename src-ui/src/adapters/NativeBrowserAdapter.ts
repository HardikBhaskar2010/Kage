import { invoke } from "@tauri-apps/api/core";
import type { BrowserAdapter, TabInfo } from "./BrowserAdapter";

/**
 * NativeBrowserAdapter — Dispatches tab lifecycle and navigation requests
 * to the Tauri Rust host process, which drives the native CEF browser windows.
 */
export class NativeBrowserAdapter implements BrowserAdapter {
  async createTab(url = "", title = "New Tab"): Promise<TabInfo> {
    return invoke<TabInfo>("create_tab", { url, title });
  }

  async closeTab(tabId: string): Promise<void> {
    return invoke<void>("close_tab", { tabId });
  }

  async switchTab(tabId: string): Promise<void> {
    return invoke<void>("switch_tab", { tabId });
  }

  async navigate(tabId: string, url: string): Promise<void> {
    return invoke<void>("navigate_to", { tabId, url });
  }

  async goBack(tabId: string): Promise<void> {
    return invoke<void>("go_back", { tabId });
  }

  async goForward(tabId: string): Promise<void> {
    return invoke<void>("go_forward", { tabId });
  }

  async refresh(tabId: string): Promise<void> {
    return invoke<void>("reload_tab", { tabId });
  }
}
