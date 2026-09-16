import { isTauriEnvironment } from "./BrowserAdapter";

export interface ConsoleEntry {
  id: string;
  type: "log" | "warn" | "error" | "info";
  message: string;
  timestamp: string;
}

export interface NetworkEntry {
  id: string;
  url: string;
  method: string;
  status: number;
  type: string;
  size: string;
  time: string;
}

export interface DomBoxModel {
  margin: [number, number, number, number];
  border: [number, number, number, number];
  padding: [number, number, number, number];
  dimensions: { width: number; height: number };
}

export interface DomInspectNode {
  id: string;
  tag: string;
  classes: string[];
  attributes: Record<string, string>;
  boxModel: DomBoxModel;
  computedStyles: Record<string, string>;
}

export interface PerfSample {
  fps: number;
  memoryMb: number;
  cpuPercent: number;
}

export interface TelemetryProvider {
  /** Subscribe to console output stream */
  onConsoleMessage(cb: (entry: ConsoleEntry) => void): () => void;
  /** Subscribe to network request waterfall stream */
  onNetworkRequest(cb: (entry: NetworkEntry) => void): () => void;
  /** Query live DOM tree or inspect specific node */
  inspectElement(selector: string): Promise<DomInspectNode | null>;
  /** Subscribe to real-time FPS and memory metrics */
  onPerformanceMetric(cb: (sample: PerfSample) => void): () => void;
  /** Evaluate a command in page context */
  evalJs(command: string): Promise<unknown>;
}

import { MockTelemetryProvider } from "./MockTelemetryProvider";
import { CdpTelemetryProvider } from "./CdpTelemetryProvider";

let activeTelemetry: TelemetryProvider | null = null;

export function getTelemetryProvider(): TelemetryProvider {
  if (!activeTelemetry) {
    activeTelemetry = isTauriEnvironment()
      ? new CdpTelemetryProvider()
      : new MockTelemetryProvider();
  }
  return activeTelemetry;
}
