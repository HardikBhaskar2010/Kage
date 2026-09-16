/**
 * BrowserAdapter — Core abstraction separating frontend UI chrome from browser runtime.
 *
 * In browser development mode (Vite): delegates to MockBrowserAdapter.
 * In native Tauri execution: delegates to NativeBrowserAdapter (Rust -> CEF -> CDP).
 */

export interface TabInfo {
  id: string;
  url: string;
  title: string;
  favicon?: string;
  isLoading?: boolean;
  canGoBack?: boolean;
  canGoForward?: boolean;
  isSecure?: boolean;
}

export interface ViewportBounds {
  x: number;
  y: number;
  width: number;
  height: number;
  scale_factor: number;
}

export interface BrowserAdapter {
  /** Create a new tab and return its metadata */
  createTab(url?: string, title?: string): Promise<TabInfo>;
  /** Close an existing tab by ID */
  closeTab(tabId: string): Promise<void>;
  /** Switch active tab */
  switchTab(tabId: string): Promise<void>;
  /** Navigate a tab to a new URL */
  navigate(tabId: string, url: string): Promise<void>;
  /** Go back in navigation history */
  goBack(tabId: string): Promise<void>;
  /** Go forward in navigation history */
  goForward(tabId: string): Promise<void>;
  /** Reload the active page */
  refresh(tabId: string): Promise<void>;
  /** Synchronize viewport container bounds to the native CEF window surface */
  syncViewportBounds(bounds: ViewportBounds): Promise<void>;
}

/** Check if executing inside Tauri desktop runtime */
export function isTauriEnvironment(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

import { MockBrowserAdapter } from "./MockBrowserAdapter";
import { NativeBrowserAdapter } from "./NativeBrowserAdapter";

let activeAdapter: BrowserAdapter | null = null;

export function getBrowserAdapter(): BrowserAdapter {
  if (!activeAdapter) {
    activeAdapter = isTauriEnvironment()
      ? new NativeBrowserAdapter()
      : new MockBrowserAdapter();
  }
  return activeAdapter;
}
