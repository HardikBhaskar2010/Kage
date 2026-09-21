//! Error definitions for KAGE browser control plane.

use crate::tab::{RendererTerminationStatus, TabId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("tab with id '{0}' not found")]
    TabNotFound(TabId),

    #[error("surface error: {0}")]
    SurfaceError(String),

    #[error("profile error: {0}")]
    ProfileError(String),

    #[error("engine error: {0}")]
    EngineError(#[from] kage_engine::EngineError),

    #[error("navigation failed: {0}")]
    NavigationFailed(String),

    #[error("action cancelled: {0}")]
    ActionCancelled(String),

    #[error("renderer process terminated for tab '{tab_id}': {status:?}")]
    RendererTerminated {
        tab_id: TabId,
        status: RendererTerminationStatus,
    },

    #[error("invalid state: {0}")]
    InvalidState(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
