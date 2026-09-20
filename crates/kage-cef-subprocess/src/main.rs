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

    // Diagnostics: write subprocess role and PID for E2E process-type identification
    let pid = std::process::id();
    let cmd = std::env::args().collect::<Vec<_>>().join(" ");
    let role_dir = std::env::var("KAGE_SUBPROCESS_ROLE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("kage_subprocess_roles"));
    let _ = std::fs::create_dir_all(&role_dir);
    let _ = std::fs::write(role_dir.join(format!("{}.txt", pid)), &cmd);

    let exit_code = cef::execute_process(Some(args.as_main_args()), None, ptr::null_mut());
    std::process::exit(exit_code);
}
