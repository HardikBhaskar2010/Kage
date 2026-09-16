import type {
  TelemetryProvider,
  ConsoleEntry,
  NetworkEntry,
  DomInspectNode,
  PerfSample,
} from "./TelemetryProvider";

/**
 * MockTelemetryProvider — Generates simulated telemetry events for
 * the frontend prototype when running outside of Tauri/CEF.
 */
export class MockTelemetryProvider implements TelemetryProvider {
  private consoleListeners: Array<(entry: ConsoleEntry) => void> = [];
  private networkListeners: Array<(entry: NetworkEntry) => void> = [];
  private perfListeners: Array<(sample: PerfSample) => void> = [];

  onConsoleMessage(cb: (entry: ConsoleEntry) => void): () => void {
    this.consoleListeners.push(cb);
    return () => {
      this.consoleListeners = this.consoleListeners.filter((l) => l !== cb);
    };
  }

  onNetworkRequest(cb: (entry: NetworkEntry) => void): () => void {
    this.networkListeners.push(cb);
    return () => {
      this.networkListeners = this.networkListeners.filter((l) => l !== cb);
    };
  }

  onPerformanceMetric(cb: (sample: PerfSample) => void): () => void {
    this.perfListeners.push(cb);
    return () => {
      this.perfListeners = this.perfListeners.filter((l) => l !== cb);
    };
  }

  async inspectElement(selector: string): Promise<DomInspectNode | null> {
    return {
      id: `node-${Date.now()}`,
      tag: selector.startsWith("#") ? "DIV" : "BUTTON",
      classes: ["liquid-glass-btn", "kage-interactive"],
      attributes: { "aria-label": "Inspected Element", role: "button" },
      boxModel: {
        margin: [8, 8, 8, 8],
        border: [1, 1, 1, 1],
        padding: [6, 12, 6, 12],
        dimensions: { width: 180, height: 36 },
      },
      computedStyles: {
        display: "flex",
        background: "rgba(43, 14, 22, 0.75)",
        color: "#F9DBBD",
        backdropFilter: "blur(20px)",
        borderRadius: "8px",
      },
    };
  }

  async evalJs(command: string): Promise<unknown> {
    try {
      // Safe sandboxed eval for mock testing
      if (command.trim() === "document.title") return "KAGE — Developer Browser";
      if (command.trim() === "navigator.userAgent") return "Kage/1.0.0 (CEF 130.0; LiquidGlass)";
      // eslint-disable-next-line no-eval
      return Function(`"use strict"; return (${command})`)();
    } catch (err) {
      throw new Error(err instanceof Error ? err.message : String(err));
    }
  }

  /** Emit synthetic mock event */
  emitConsole(type: ConsoleEntry["type"], message: string): void {
    const entry: ConsoleEntry = {
      id: `log-${Date.now()}`,
      type,
      message,
      timestamp: new Date().toLocaleTimeString(),
    };
    this.consoleListeners.forEach((l) => l(entry));
  }

  emitNetwork(entry: Omit<NetworkEntry, "id" | "time">): void {
    const fullEntry: NetworkEntry = {
      ...entry,
      id: `req-${Date.now()}`,
      time: `${Math.floor(Math.random() * 80 + 20)}ms`,
    };
    this.networkListeners.forEach((l) => l(fullEntry));
  }
}
