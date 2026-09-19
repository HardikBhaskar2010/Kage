//! KAGE Dedicated CEF Subprocess Helper (Phase 2).
//!
//! Exclusively executes secondary process roles (renderer, GPU, utility).
//! Zero Tauri, SQLite, audit, or DevTools dependencies.
//!
//! Enforces:
//! - **CEF-02**: Dedicated Subprocess Launch.
//! - **CEF-03**: Subprocess Plumbing & MainArgs.
//! - Leaves extensible interface for future renderer-side `CefApp` callbacks.

use std::ptr;

fn main() {
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let args = cef::args::Args::new();
    let exit_code = cef::execute_process(Some(args.as_main_args()), None, ptr::null_mut());
    std::process::exit(exit_code);
}
