# Rule: Architectural Boundaries & Invariant Enforcement

## Scope
Applies to all code, tools, plugins, tests, and configurations in KAGE.

## Core Mandates
1. **Prime Invariant:** Under no circumstances should AI, LLM prompts, DevTools panels, or plugins bypass the Rust `ToolBus`.
2. **Authority Hierarchy:**
   * React UI requests action ➔
   * Tauri IPC dispatches to Rust Host ➔
   * Rust `ToolBus` validates arguments, checks Permission Engine, checks Cancellation Token ➔
   * Execution Driver issues CDP command to CEF ➔
   * Result logged to `security_audit.db` and returned to caller.
3. **CEF Ownership:** CEF owns Blink rendering, V8 execution, network handling, and process sandboxing. Do not duplicate or wrap standard browser behaviors in custom JavaScript unless specified in `docs/02-architecture/CEF_Integration.md`.
4. **Desktop Native Services:** Tauri/Rust owns windowing, menus, system notifications, filesystem access, and local SQLite databases (`kage_data.db`, `security_audit.db`).
5. **No Direct IPC Bypass:** UI components must never speak directly to CEF or WebSocket debugging ports; all traffic routes through Tauri IPC channels defined in `docs/02-architecture/IPC_Protocol.md`.
