//! Profile architecture and storage partitioning.
//!
//! Profiles maintain isolated cookie jars, local storage, cache, and session data.
//! Web agents default strictly to `AgentSandbox` to prevent ambient credential leakage.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::errors::BrowserError;
use crate::tab::ProfileId;

/// Categorization of profile privacy, lifetime, and storage guarantees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileKind {
    /// Personal user profile with persistent storage.
    Personal,
    /// Work / Organization profile with persistent storage.
    Work,
    /// Automated Agent sandbox profile. Always starts clean with zero credentials.
    AgentSandbox,
    /// Ephemeral in-memory or transient profile. Discarded immediately upon close.
    Temporary,
}

/// Metadata and storage directory mapping for an active profile.
#[derive(Debug, Clone)]
pub struct Profile {
    pub id: ProfileId,
    pub kind: ProfileKind,
    pub root_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub cookie_store_dir: PathBuf,
}

impl Profile {
    pub fn new(id: ProfileId, kind: ProfileKind, base_dir: PathBuf) -> Self {
        let root_dir = base_dir.join(&id.0);
        let cache_dir = root_dir.join("cache");
        let cookie_store_dir = root_dir.join("cookies");

        Self {
            id,
            kind,
            root_dir,
            cache_dir,
            cookie_store_dir,
        }
    }

    /// Ensure physical directory structure exists on disk.
    pub fn ensure_directories(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.root_dir)?;
        std::fs::create_dir_all(&self.cache_dir)?;
        std::fs::create_dir_all(&self.cookie_store_dir)?;
        Ok(())
    }

    /// Clean up ephemeral storage if this is a temporary profile.
    pub fn cleanup_if_temporary(&self) {
        if self.kind == ProfileKind::Temporary && self.root_dir.exists() {
            let _ = std::fs::remove_dir_all(&self.root_dir);
        }
    }
}

/// Thread-safe registry and manager for browser profiles.
pub struct ProfileManager {
    base_dir: PathBuf,
    temp_dir: PathBuf,
    profiles: RwLock<HashMap<ProfileId, Arc<Profile>>>,
}

impl ProfileManager {
    /// Create a new `ProfileManager` anchored at standard application data locations.
    pub fn new() -> Self {
        let local_app_data = std::env::var("LOCALAPPDATA")
            .unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Local".to_string());
        let base_dir = PathBuf::from(local_app_data).join("KAGE").join("profiles");

        let temp_env = std::env::var("TEMP")
            .unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Local\\Temp".to_string());
        let temp_dir = PathBuf::from(temp_env).join("KAGE").join("temp_profiles");

        Self {
            base_dir,
            temp_dir,
            profiles: RwLock::new(HashMap::new()),
        }
    }

    /// Explicit base directory override (for testing).
    pub fn with_custom_dirs(base_dir: PathBuf, temp_dir: PathBuf) -> Self {
        Self {
            base_dir,
            temp_dir,
            profiles: RwLock::new(HashMap::new()),
        }
    }

    /// Resolve or create a profile by `ProfileId`.
    pub async fn get_or_create(&self, id: &ProfileId) -> Result<Arc<Profile>, BrowserError> {
        let mut map = self.profiles.write().await;
        if let Some(profile) = map.get(id) {
            return Ok(Arc::clone(profile));
        }

        let (kind, base) = if id.0 == "personal" {
            (ProfileKind::Personal, self.base_dir.clone())
        } else if id.0 == "work" {
            (ProfileKind::Work, self.base_dir.clone())
        } else if id.0 == "agent_sandbox" {
            (ProfileKind::AgentSandbox, self.base_dir.clone())
        } else {
            (ProfileKind::Temporary, self.temp_dir.clone())
        };

        let profile = Arc::new(Profile::new(id.clone(), kind, base));
        profile.ensure_directories().map_err(BrowserError::Io)?;
        map.insert(id.clone(), Arc::clone(&profile));
        Ok(profile)
    }

    /// Allocate a fresh, isolated temporary profile with a unique UUID.
    pub async fn create_temporary(&self) -> Result<Arc<Profile>, BrowserError> {
        let id = ProfileId(format!("temp_{}", Uuid::new_v4().simple()));
        self.get_or_create(&id).await
    }

    /// List all currently active registered profiles.
    pub async fn list_profiles(&self) -> Vec<ProfileId> {
        self.profiles.read().await.keys().cloned().collect()
    }
}
