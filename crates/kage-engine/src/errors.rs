//! Unified error types for KAGE CEF Engine Foundation (Phase 2).

use thiserror::Error;

/// Core error type representing any failure in the KAGE engine subsystem.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Win32 platform failure: {0}")]
    Win32(String),

    #[error("CEF runtime initialization or call failure: {0}")]
    Cef(String),

    #[error("Surface composition failure: {0}")]
    Surface(String),

    #[error("Lifecycle or teardown state error: {0}")]
    Lifecycle(String),

    #[error("Controlled CEF shutdown timeout: active browsers failed to close within deadline: {0}")]
    ShutdownTimeout(String),

    #[error("Executor task dispatch failure: {0}")]
    Executor(#[from] CefExecutorError),

    #[error("Runtime packaging or asset validation error: {0}")]
    Packaging(String),

    #[error("DPI context or coordinate calculation error: {0}")]
    Dpi(String),
}

/// Errors originating specifically from cross-thread dispatch to CEF's UI thread.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum CefExecutorError {
    #[error("CefUiExecutor is not attached to an active CEF UI message loop")]
    NotAttached,

    #[error("Failed to send task to CEF UI thread: channel closed")]
    ChannelClosed,

    #[error("Task execution was cancelled or the result channel dropped")]
    ExecutionCancelled,

    #[error("CEF operation timed out before completion")]
    OperationTimeout,

    #[error("Task dispatch rejected: {0}")]
    Rejected(String),
}
