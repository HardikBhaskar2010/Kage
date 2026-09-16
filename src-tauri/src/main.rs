//! KAGE host process entry point.
//!
//! This binary bootstraps the Tauri runtime, initialises logging, creates the
//! central [`ToolBus`] and [`CdpBroker`], and registers all typed IPC handlers
//! before handing control to Tauri's event loop.
//!
//! # CEF lifecycle (stub — full integration in Chunk 7)
//!
//! In production, `main` must call `CefExecuteProcess` before any Tauri
//! initialisation so that CEF renderer and utility sub-processes can take over
//! their own process roles immediately and exit.  This is scaffolded here as a
//! comment block until the CEF binaries are linked.

// Tauri requires a `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`
// annotation to suppress the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kage_host_lib::run;

fn main() {
    run();
}
