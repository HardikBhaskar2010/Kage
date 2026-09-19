// KAGE Host build script.
//
// Responsibilities:
// 1. Run Tauri's build machinery (`tauri_build::build()`).
// 2. Enforce CEF-03b: **Release builds must never disable the CEF sandbox.**
//    Any feature or environment variable that disables the sandbox in a release
//    binary is a hard build failure. The sandbox may only be disabled in debug
//    builds (e.g. for debugger attach scenarios).

fn main() {
    // --------------------------------------------------------------------------
    // Gate CEF-03b: Enforce sandbox in release builds
    // --------------------------------------------------------------------------
    // Declare the cfg to rustc so consumers don't see `unexpected_cfg` warnings.
    println!("cargo::rustc-check-cfg=cfg(kage_cef_sandbox_enabled)");

    // The KAGE_DISABLE_CEF_SANDBOX env var may be set by developers to
    // ease debugger attachment. It is strictly prohibited in release mode.
    let disable_sandbox = std::env::var("KAGE_DISABLE_CEF_SANDBOX")
        .map(|v| !v.is_empty() && v != "0" && v.to_lowercase() != "false")
        .unwrap_or(false);

    let is_release = !cfg!(debug_assertions)
        // `PROFILE` is set by Cargo: "debug" or "release"
        || std::env::var("PROFILE").as_deref() == Ok("release");

    if disable_sandbox && is_release {
        // Hard failure: this is a security contract, not a warning.
        panic!(
            "\n\
            ╔══════════════════════════════════════════════════════════════════╗\n\
            ║           CEF-03b SECURITY GATE: BUILD REJECTED                 ║\n\
            ║                                                                  ║\n\
            ║  KAGE_DISABLE_CEF_SANDBOX is set in a RELEASE build.            ║\n\
            ║  This violates Architecture Contract INV-03b and is FORBIDDEN.  ║\n\
            ║                                                                  ║\n\
            ║  The CEF sandbox is a mandatory isolation boundary in all        ║\n\
            ║  production and distribution builds.                             ║\n\
            ║                                                                  ║\n\
            ║  Resolution: unset KAGE_DISABLE_CEF_SANDBOX or build in debug   ║\n\
            ║  mode (`cargo build` without `--release`).                       ║\n\
            ╚══════════════════════════════════════════════════════════════════╝\n"
        );
    }

    if disable_sandbox {
        // Debug-only warning — acceptable for local developer use only.
        println!(
            "cargo:warning=CEF-03b: KAGE_DISABLE_CEF_SANDBOX is set — sandbox disabled \
             (debug build only, never ship)"
        );
    }

    // Emit a cfg flag so runtime code can assert the gate was respected.
    if !disable_sandbox {
        println!("cargo:rustc-cfg=kage_cef_sandbox_enabled");
    }

    // Re-run if the env var changes
    println!("cargo:rerun-if-env-changed=KAGE_DISABLE_CEF_SANDBOX");
    println!("cargo:rerun-if-env-changed=PROFILE");

    // --------------------------------------------------------------------------
    // Tauri build machinery
    // --------------------------------------------------------------------------
    tauri_build::build()
}
