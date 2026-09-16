//! Host library root — exposes [`run`] and IPC modules for testability.

pub mod ipc;

use std::sync::Arc;
use tauri::Manager;
use kage_core::ToolBus;
use kage_cdp::CdpBroker;

/// Initialise and run the Tauri application.
pub fn run() {
    // Initialise structured logging from RUST_LOG env var.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kage=info".parse().unwrap()),
        )
        .init();

    tracing::info!("KAGE host starting — v{}", env!("CARGO_PKG_VERSION"));

    // Construct core singletons.
    let tool_bus = Arc::new(ToolBus::new());
    let cdp_broker = Arc::new(CdpBroker::new());

    tracing::info!("CDP broker ephemeral nonce generated");

    tauri::Builder::default()
        .manage(tool_bus)
        .manage(cdp_broker)
        .invoke_handler(tauri::generate_handler![
            ipc::tool_dispatch,
            ipc::get_cdp_nonce,
        ])
        .run(tauri::generate_context!())
        .expect("KAGE Tauri application failed to start");
}
