//! KAGE Client Library (kage_client.dll).
//!
//! Implements the official CEF Windows bootstrap architecture (CEF-03b-D):
//! Exports `RunWinMain` and `RunConsoleMain` called by `bootstrap.exe` / `KAGE.exe`.
//! Receives `sandbox_info` and delegates execution:
//! - Auxiliary subprocesses (`--type=renderer`, `--type=gpu-process`, etc.):
//!   executed via `cef::execute_process(..., sandbox_info)` and exits with child code.
//! - Main browser host process:
//!   initializes KAGE control plane with `sandbox_info` and enters main application loop.

use std::path::PathBuf;
use cef::*;

wrap_render_process_handler! {
    struct SubprocessRenderHandler {
        role_dir: PathBuf,
    }

    impl RenderProcessHandler {
        fn on_browser_created(
            &self,
            browser: Option<&mut Browser>,
            _extra_info: Option<&mut DictionaryValue>,
        ) {
            let pid = std::process::id();
            let b_id = browser.map(|b| b.identifier()).unwrap_or(0);
            // Record bidirectional association: PID -> BrowserId and BrowserId -> PID
            let _ = std::fs::write(self.role_dir.join(format!("{}.browser", pid)), format!("{}", b_id));
            let _ = std::fs::write(self.role_dir.join(format!("browser_{}.pid", b_id)), format!("{}", pid));
        }
    }
}

wrap_app! {
    struct SubprocessApp {
        role_dir: PathBuf,
    }

    impl App {
        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(SubprocessRenderHandler::new(self.role_dir.clone()))
        }
    }
}

/// Standard Windows GUI entry point called by CEF `bootstrap.exe`.
#[no_mangle]
pub unsafe extern "C" fn RunWinMain(
    _h_instance: isize,
    _lp_cmd_line: *const u16,
    _n_cmd_show: i32,
    sandbox_info: *mut std::ffi::c_void,
    _version_info: *mut std::ffi::c_void,
) -> i32 {
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let args = cef::args::Args::new();

    let pid = std::process::id();
    let cmd = std::env::args().collect::<Vec<_>>().join(" ");
    let role_dir = std::env::var("KAGE_SUBPROCESS_ROLE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("kage_subprocess_roles"));
    let _ = std::fs::create_dir_all(&role_dir);
    let _ = std::fs::write(role_dir.join(format!("{}.txt", pid)), &cmd);

    // If secondary subprocess (renderer, GPU, utility), execute_process handles it
    let mut app = SubprocessApp::new(role_dir.clone());
    let exit_code = cef::execute_process(Some(args.as_main_args()), Some(&mut app), sandbox_info as *mut u8);
    if exit_code >= 0 {
        return exit_code;
    }

    // Main browser host process
    0
}

/// Console entry point called by CEF `bootstrapc.exe`.
#[no_mangle]
pub unsafe extern "C" fn RunConsoleMain(
    _argc: i32,
    _argv: *const *const u8,
    sandbox_info: *mut std::ffi::c_void,
    version_info: *mut std::ffi::c_void,
) -> i32 {
    unsafe { RunWinMain(0, std::ptr::null(), 0, sandbox_info, version_info) }
}
