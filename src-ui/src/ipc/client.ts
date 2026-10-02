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

// ─── Phase 6 Developer Intelligence Helpers (INV-01, INV-02) ───────────────────

/**
 * Governed JavaScript evaluation in the active tab V8 context via ToolBus.
 * Command: `eval_js` -> Tool: `devtools.runtime.evaluate`
 */
export async function evalJs(
  command: string,
  tabId?: string
): Promise<unknown> {
  return invoke<unknown>("eval_js", { command, tab_id: tabId });
}

/**
 * Retrieve the active tab DOM document tree.
 * Tool: `devtools.dom.get_document`
 */
export async function getDomDocument(
  tabId: string,
  depth = -1,
  pierce = true
): Promise<any> {
  const resp = await toolDispatch({
    tool_id: "devtools.dom.get_document",
    args: { tab_id: tabId, depth, pierce },
    request_id: `req_dom_doc_${Date.now()}`,
    reason: "DevTools Elements Inspection",
    session_id: `devtools_${tabId}`,
    workspace_id: "default_workspace",
    session_granted: true,
  });
  return resp.output;
}

/**
 * Query DOM element attributes by CDP nodeId.
 * Tool: `devtools.dom.get_attributes`
 */
export async function getDomAttributes(
  tabId: string,
  nodeId: number
): Promise<any> {
  const resp = await toolDispatch({
    tool_id: "devtools.dom.get_attributes",
    args: { tab_id: tabId, node_id: nodeId },
    request_id: `req_dom_attr_${Date.now()}`,
    reason: "DevTools Elements Attributes Inspection",
    session_id: `devtools_${tabId}`,
    workspace_id: "default_workspace",
    session_granted: true,
  });
  return resp.output;
}

/**
 * Query DOM element box model bounds by CDP nodeId.
 * Tool: `devtools.dom.get_bounds`
 */
export async function getDomBounds(
  tabId: string,
  nodeId: number
): Promise<any> {
  const resp = await toolDispatch({
    tool_id: "devtools.dom.get_bounds",
    args: { tab_id: tabId, node_id: nodeId },
    request_id: `req_dom_bounds_${Date.now()}`,
    reason: "DevTools Elements Box Model Inspection",
    session_id: `devtools_${tabId}`,
    workspace_id: "default_workspace",
    session_granted: true,
  });
  return resp.output;
}

/**
 * Query cookies for the active tab (redacted by SecretSanitizer INV-06).
 * Tool: `devtools.storage.get_cookies`
 */
export async function getCookies(
  tabId: string,
  urls?: string[]
): Promise<any> {
  const resp = await toolDispatch({
    tool_id: "devtools.storage.get_cookies",
    args: { tab_id: tabId, ...(urls ? { urls } : {}) },
    request_id: `req_cookies_${Date.now()}`,
    reason: "DevTools Storage Cookies Inspection",
    session_id: `devtools_${tabId}`,
    workspace_id: "default_workspace",
    session_granted: true,
  });
  return resp.output;
}

/**
 * Query localStorage key-value items for the active tab.
 * Tool: `devtools.storage.get_local_storage`
 */
export async function getLocalStorage(
  tabId: string,
  origin?: string
): Promise<any> {
  const resp = await toolDispatch({
    tool_id: "devtools.storage.get_local_storage",
    args: { tab_id: tabId, ...(origin ? { origin } : {}) },
    request_id: `req_storage_local_${Date.now()}`,
    reason: "DevTools Storage LocalStorage Inspection",
    session_id: `devtools_${tabId}`,
    workspace_id: "default_workspace",
    session_granted: true,
  });
  return resp.output;
}

/**
 * Query sessionStorage key-value items for the active tab.
 * Tool: `devtools.storage.get_session_storage`
 */
export async function getSessionStorage(
  tabId: string,
  origin?: string
): Promise<any> {
  const resp = await toolDispatch({
    tool_id: "devtools.storage.get_session_storage",
    args: { tab_id: tabId, ...(origin ? { origin } : {}) },
    request_id: `req_storage_session_${Date.now()}`,
    reason: "DevTools Storage SessionStorage Inspection",
    session_id: `devtools_${tabId}`,
    workspace_id: "default_workspace",
    session_granted: true,
  });
  return resp.output;
}

/**
 * Query network response body by CDP requestId.
 * Tool: `devtools.network.get_response_body`
 */
export async function getResponseBody(
  tabId: string,
  requestId: string
): Promise<any> {
  const resp = await toolDispatch({
    tool_id: "devtools.network.get_response_body",
    args: { tab_id: tabId, request_id: requestId },
    request_id: `req_net_body_${Date.now()}`,
    reason: "DevTools Network Payload Inspection",
    session_id: `devtools_${tabId}`,
    workspace_id: "default_workspace",
    session_granted: true,
  });
  return resp.output;
}

// ─── Phase 7 Profiles & Permission Types & Helpers (INV-06, INV-10) ──────────

export type ProfileKind = "personal" | "work" | "agent_sandbox" | "temporary";

export interface ProfileMetadata {
  id: string;
  name: string;
  kind: ProfileKind;
  color: string;
  icon: string;
  created_at: number;
  last_used: number;
  is_ephemeral: boolean;
}

export type PermissionType =
  | "geolocation"
  | "notifications"
  | "camera"
  | "microphone"
  | "clipboard_read"
  | "clipboard_write"
  | "downloads"
  | "popups";

export type PermissionDecision = "allow" | "deny" | "prompt";

export interface OriginPermissionRule {
  profile_id: string;
  origin: string;
  permission: PermissionType;
  decision: PermissionDecision;
  updated_at: number;
}

/**
 * List all registered profile metadata for UI presentation.
 * Command: `list_profiles`
 */
export async function listProfiles(): Promise<ProfileMetadata[]> {
  return invoke<ProfileMetadata[]>("list_profiles");
}

/**
 * Create a new custom user profile.
 * Command: `create_profile`
 */
export async function createProfile(payload: {
  id: string;
  name: string;
  kind: ProfileKind;
  color?: string;
  icon?: string;
}): Promise<ProfileMetadata> {
  return invoke<ProfileMetadata>("create_profile", { payload });
}

/**
 * Delete a profile and purge its storage directories.
 * Command: `delete_profile`
 */
export async function deleteProfile(profileId: string): Promise<void> {
  return invoke<void>("delete_profile", { profile_id: profileId });
}

/**
 * Retrieve metadata for the currently active tab's profile.
 * Command: `get_active_profile`
 */
export async function getActiveProfile(): Promise<ProfileMetadata> {
  return invoke<ProfileMetadata>("get_active_profile");
}

/**
 * Query origin-scoped permission decision.
 * Command: `query_permission`
 */
export async function queryPermission(
  profileId: string,
  origin: string,
  permissionType: PermissionType
): Promise<PermissionDecision> {
  return invoke<PermissionDecision>("query_permission", {
    profile_id: profileId,
    origin,
    permission_type: permissionType,
  });
}

/**
 * Set origin-scoped permission decision.
 * Command: `set_permission`
 */
export async function setPermission(
  profileId: string,
  origin: string,
  permissionType: PermissionType,
  decision: PermissionDecision
): Promise<void> {
  return invoke<void>("set_permission", {
    profile_id: profileId,
    origin,
    permission_type: permissionType,
    decision,
  });
}

/**
 * Escalate agent session to user profile with fail-closed two-stage audit commitment (INV-06).
 * Command: `escalate_session`
 */
export async function escalateSession(payload: {
  tab_id: string;
  target_profile_id: string;
  reason: string;
  approved: boolean;
}): Promise<boolean> {
  return invoke<boolean>("escalate_session", { payload });
}

