// build.rs for kage-integration-tests
//
// Propagates the CEF-03b sandbox gate cfg flag so
// `contract_gate_cef_03b_sandbox_enabled_in_normal_builds` can verify it at test time.
//
// Mirrors the gate logic in `src-tauri/build.rs`: if KAGE_DISABLE_CEF_SANDBOX is set
// when running integration tests, the cfg flag is absent and the contract test fails.

fn main() {
    // Declare the cfg to rustc so it doesn't emit `unexpected_cfg` warnings.
    println!("cargo::rustc-check-cfg=cfg(kage_cef_sandbox_enabled)");

    let disable_sandbox = std::env::var("KAGE_DISABLE_CEF_SANDBOX")
        .map(|v| !v.is_empty() && v != "0" && v.to_lowercase() != "false")
        .unwrap_or(false);

    let is_release = std::env::var("PROFILE").as_deref() == Ok("release");

    if disable_sandbox && is_release {
        panic!(
            "CEF-03b: KAGE_DISABLE_CEF_SANDBOX must not be set when running \
             integration tests in release mode"
        );
    }

    if !disable_sandbox {
        println!("cargo:rustc-cfg=kage_cef_sandbox_enabled");
    }

    println!("cargo:rerun-if-env-changed=KAGE_DISABLE_CEF_SANDBOX");
    println!("cargo:rerun-if-env-changed=PROFILE");
}
