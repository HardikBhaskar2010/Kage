# KAGE IPC Protocol Specification

| Document Metadata | Details |
|---|---|
| **Document ID** | KAGE-ARCH-005 |
| **Status** | Approved Protocol Specification |
| **Version** | v0.2.0 |
| **Last Updated** | 2026-09-16 |
| **Target Milestone** | v1.0.0 MVP |
| **Classification** | Messaging Architecture / Protocol Engineering |

---

## 1. Executive Summary & Communication Landscape

KAGE operates across four discrete computational tiers with distinct runtime environments, threading models, and privilege levels. An organized, high-throughput Inter-Process Communication (IPC) protocol is required to connect these tiers without creating latency bottlenecks or security vulnerabilities.

```
┌────────────────────────────────────────────────────────┐
│               KAGE UI (React / TypeScript)             │
│               Privilege: Medium (UI Shell)             │
└───────────────────────────▲────────────────────────────┘
                            │
               Tauri IPC    │ Channel Streams / Invoke
               (JSON)       │
                            ▼
┌────────────────────────────────────────────────────────┐
│               TAURI HOST (Rust Core)                   │
│               Privilege: High (OS / Supervisor)        │
└─────────────┬──────────────────────────▲───────────────┘
              │                          │
   C-FFI /    │ Native Win32 API         │ CDP WebSocket
   PostTask   │                          │ (JSON-RPC 2.0)
              ▼                          ▼
┌─────────────────────────┐  ┌───────────────────────────┐
│  CEF BROWSER PROCESS    │  │  CHROMIUM DEVTOOLS (CDP)  │
│  (Window / Net / Cache) │  │  (Blink / V8 / Layout)    │
└─────────────┬───────────┘  └───────────────────────────┘
              │
CefProcessMsg │ Shared Memory / Pipes
              ▼
┌─────────────────────────┐
│  CEF RENDERER PROCESSES │
│  Privilege: Sandboxed   │
└─────────────────────────┘
```

---

## 2. Multi-Tier Communication Matrix

| Tier Boundary | Channel Medium | Serialization Format | Typical Latency | Concurrency Model |
|---|---|---|---|---|
| **React UI ↔ Rust Host** | Tauri IPC (`invoke` / `Channel`) | JSON / UTF-8 | `< 0.8 ms` | Async non-blocking, multi-threaded worker pool |
| **Rust Host ↔ CEF Core** | Direct C-FFI + `CefPostTask` | Native structs / CefRefPtr | `< 0.05 ms` | Cross-thread messaging (`TID_UI`, `TID_IO`) |
| **Rust Host ↔ CDP Server** | Loopback WebSocket (`127.0.0.1`) | JSON-RPC 2.0 | `< 1.2 ms` | Full-duplex asynchronous framing (`tokio-tungstenite`) |
| **CEF Browser ↔ Renderer** | Chromium Mojo IPC / `CefProcessMessage` | Binary / Pickled Arrays | `< 0.3 ms` | Sandboxed OS IPC pipes |

---

## 3. Standard Message Envelopes

All structured messages exchanged across KAGE tiers adhere to unified envelope schemas.

### 3.1 Command Request Envelope (UI ➔ Host)
Used for asynchronous request-response operations (e.g., creating a tab, running a tool, querying workspaces):

```json
{
  "id": "cmd_01J8Y4B8X7A9C2E3F4G5H6J7K8",
  "domain": "tool_bus",
  "action": "execute_tool",
  "auth_token": "nonce_9a8b7c6d5e4f3a2b",
  "timestamp": 1789574383517,
  "payload": {
    "tool_name": "inspect_dom",
    "tab_id": "tab_dev_default_01",
    "parameters": {
      "selector": "#main-navigation",
      "include_box_model": true
    }
  }
}
```

- **`id`**: Unique monotonic ULID/UUID identifying the request for correlation.
- **`domain`**: Functional subsystem routing (`shell`, `tab`, `tool_bus`, `context`, `storage`, `ai`).
- **`action`**: Verb identifying the requested operation.
- **`auth_token`**: Per-session ephemeral nonce verifying caller authenticity.
- **`timestamp`**: Millisecond UNIX timestamp for latency tracking and timeouts.
- **`payload`**: Strongly-typed arguments validated against the domain JSON Schema.

### 3.2 Command Response Envelope (Host ➔ UI)
The synchronous or asynchronous reply to a Command Request:

```json
{
  "id": "cmd_01J8Y4B8X7A9C2E3F4G5H6J7K8",
  "success": true,
  "data": {
    "node_id": 412,
    "tag_name": "NAV",
    "box_model": {
      "width": 1200,
      "height": 64,
      "margin": [0, 0, 16, 0],
      "padding": [8, 16, 8, 16]
    }
  },
  "error": null,
  "execution_time_ms": 3.42
}
```

### 3.3 Event Envelope (Broadcast / Push)
Unsolicited notifications emitted by the Host or CEF engine to subscribed UI components (e.g., URL change, network request observed, renderer crash):

```json
{
  "event": "kage:network:request_completed",
  "source": "cdp:tab_dev_default_01",
  "timestamp": 1789574383620,
  "payload": {
    "request_id": "req_88412",
    "url": "https://api.example.com/v1/user",
    "method": "GET",
    "status": 200,
    "mime_type": "application/json",
    "duration_ms": 142.8,
    "size_bytes": 4096
  }
}
```

### 3.4 Stream Chunk Envelope (Streaming Data)
High-frequency token streams (AI generation) or continuous binary chunks:

```json
{
  "stream_id": "strm_01J8Y4G8H9J0K1L2M3N4P5Q6R7",
  "sequence": 14,
  "is_final": false,
  "type": "token",
  "data": " rendering pipeline"
}
```

---

## 4. Error Taxonomy & Error Codes

When an operation fails at any tier, KAGE returns a standardized, actionable error object:

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "E_PERMISSION_DENIED",
    "message": "Tool 'modify_dom' requires Tier 2 authorization. User rejected confirmation prompt.",
    "domain": "tool_bus",
    "retryable": false,
    "details": {
      "required_tier": 2,
      "action": "modify_dom",
      "target_node": "#checkout-button"
    }
  },
  "execution_time_ms": 12.1
}
```

### Standardized Error Code Table

| Error Code | Category | Description | Recovery Strategy |
|---|---|---|---|
| `E_NOT_FOUND` | Resolution | Tab, element, workspace, or tool not found. | Verify identifier or DOM state. |
| `E_PERMISSION_DENIED` | Security | Operation denied by user or active security tier. | Prompt user or escalate tier. |
| `E_VALIDATION_FAILED` | Input | Schema validation failed on command payload. | Check argument types against schema. |
| `E_TIMEOUT` | Lifecycle | CDP or host execution exceeded time limit. | Retry with backoff or report freeze. |
| `E_CEF_ERROR` | Engine | Native CEF crash, load failure, or IPC failure. | Check renderer status; reload tab. |
| `E_CDP_ERROR` | DevTools | DevTools protocol command returned protocol error. | Inspect target node validity. |
| `E_AI_PROVIDER_ERROR` | AI Subsystem | LLM rate limit, invalid key, or upstream outage. | Rotate provider or notify user. |
| `E_INTERNAL` | Fatal | Unhandled panic or invariant violation in Rust core. | Write minidump; report crash. |

---

## 5. Security & Origin Verification

To prevent cross-site scripting (XSS) in a web page from issuing commands to the host OS, KAGE enforces strict origin verification:

1. **Isolation of Tauri IPC:** The `window.__TAURI__` object is injected exclusively into the React UI Shell context (`kage://localhost` or Tauri's secure asset bundle). Web pages rendered inside CEF run in an entirely isolated V8 context and have zero access to Tauri IPC APIs.
2. **Session Nonce Validation:** Every command issued from the React UI must include a session nonce generated by the Rust core at startup. Invocations lacking a valid nonce are immediately dropped.
3. **CDP Protection:** The CDP WebSocket server listens exclusively on loopback (`127.0.0.1`). When connecting, the Rust client passes an internal authorization header `X-Kage-Internal-Auth` containing a SHA-256 HMAC token.
4. **Command Allowlist:** Tauri commands are registered strictly through static Rust command bindings. Dynamic execution (`eval`) over IPC is strictly forbidden.

---

## 6. High-Performance Binary Data Transfer

Transferring large payloads (full-page screenshots, CPU profiler dumps, HAR network recordings) via standard JSON stringification introduces CPU bottlenecks and memory bloat.

### 6.1 Binary Transfer Strategy
For payloads exceeding 64 KB:
1. **Direct Memory Mapping / Raw Buffers:** Image and profile data is extracted as raw byte vectors (`Vec<u8>`) in Rust.
2. **Shared Memory or Binary Channels:** Tauri provides binary payload transfers that bypass JSON serialization:
   ```rust
   // Rust Host Command
   #[tauri::command]
   pub async fn capture_viewport_png(
       state: tauri::State<'_, KageState>,
       tab_id: String
   ) -> Result<tauri::ipc::Response, KageError> {
       let bytes: Vec<u8> = state.cef_manager.capture_screenshot(&tab_id).await?;
       Ok(tauri::ipc::Response::new(bytes))
   }
   ```
3. **Frontend Zero-Copy Consumption:**
   ```typescript
   import { invoke } from '@tauri-apps/api/core';
   
   // Receives ArrayBuffer directly without JSON deserialization overhead
   const arrayBuffer: ArrayBuffer = await invoke('capture_viewport_png', { tabId });
   const blob = new Blob([arrayBuffer], { type: 'image/png' });
   const imageUrl = URL.createObjectURL(blob);
   ```

---

## 7. Channel Concurrency & Backpressure

With high-frequency CDP event streams (such as scrolling DOM changes or streaming 100 network requests/second on a media-heavy website), the frontend could experience UI stutter if the message queue overflows.

### 7.1 Backpressure Mechanism
1. **Sliding Window Buffering:** The Rust Context Engine buffers incoming events in a bounded ring buffer (default: 500 events).
2. **Event Debouncing & Coalescing:** High-frequency events (e.g., `DOM.childNodeCountUpdated`, `Overlay.inspectNodeRequested`) are throttled to a maximum frequency of 60 Hz before being emitted to the React UI.
3. **Channel Flow Control:** Tauri Channels implement a bounded transmission buffer. If the frontend consumer falls behind, the host throttles non-critical telemetry while preserving critical state events.

---

## 8. Protocol Versioning & Evolution

To guarantee long-term stability and plugin compatibility:
- The IPC protocol follows **Semantic Versioning (SemVer 2.0.0)**.
- The current protocol milestone is **v1.0.0-rc1**.
- Backward compatibility rule: Minor version upgrades (e.g., `v1.1.0`) may add optional fields to command payloads or introduce new events, but must never remove or mutate existing required fields.
- Protocol negotiation: On startup, the UI and Host exchange a `handshake` command verifying protocol compatibility:
  ```json
  {
    "client_version": "1.0.0",
    "protocol_version": "1.0",
    "supported_capabilities": ["streaming", "binary_transfer", "cdp_direct"]
  }
  ```
