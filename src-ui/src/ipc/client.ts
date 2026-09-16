/**
 * Typed IPC client — matches `docs/02-architecture/IPC_Protocol.md`
 *
 * All Tauri command invocations go through this module. The React UI
 * never calls `@tauri-apps/api` directly from component code.
 */

import { invoke } from "@tauri-apps/api/core";

// ─── Shared types ────────────────────────────────────────────────────────────

export interface ToolDispatchPayload {
  tool_id: string;
  args: Record<string, unknown>;
  request_id: string;
  reason: string;
  session_id: string;
  workspace_id: string;
  session_granted: boolean;
}

export interface ToolResponse {
  request_id: string;
  output: unknown;
  elapsed_ms: number;
}

// ─── Commands ─────────────────────────────────────────────────────────────────

/**
 * Dispatch an AI tool call through the Rust ToolBus.
 * Command: `kage:tool:dispatch`
 */
export async function toolDispatch(
  payload: ToolDispatchPayload
): Promise<ToolResponse> {
  return invoke<ToolResponse>("tool_dispatch", { payload });
}

export interface CdpConnectionDescriptor {
  port: number;
  nonce: string;
  ws_url: string;
}

/**
 * Retrieve the full dynamic CDP connection descriptor from the Rust host.
 * Command: `kage:cdp:get_connection`
 *
 * Ephemeral port and session nonce are dynamically negotiated.
 * Never hardcode localhost ports.
 */
export async function getCdpConnection(): Promise<CdpConnectionDescriptor> {
  return invoke<CdpConnectionDescriptor>("get_cdp_connection");
}

/**
 * Retrieve the ephemeral CDP session nonce.
 * Command: `kage:cdp:get_nonce`
 *
 * IMPORTANT: This value must never be forwarded to web page content.
 */
export async function getCdpNonce(): Promise<string> {
  return invoke<string>("get_cdp_nonce");
}
