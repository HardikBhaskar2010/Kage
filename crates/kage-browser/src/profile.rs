//! Profile architecture, storage partitioning, and ephemeral sandbox lifecycle.
//!
//! Enforces:
//! - Complete partition of cookie jars, local storage, cache, and session data.
//! - Web agents default strictly to `AgentSandbox` to prevent ambient credential leakage.
//! - Measurable ephemeral wipe on sandbox close: physical directory purge and verified disk absence.
//! - Two-stage fail-closed audit commitment for session escalation (INV-06).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::errors::BrowserError;
use crate::tab::{ProfileId, TabId};

/// Categorization of profile privacy, lifetime, and storage guarantees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

impl std::fmt::Display for ProfileKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileKind::Personal => write!(f, "personal"),
            ProfileKind::Work => write!(f, "work"),
            ProfileKind::AgentSandbox => write!(f, "agent_sandbox"),
            ProfileKind::Temporary => write!(f, "temporary"),
        }
    }
}

/// Rich metadata defining profile identity and display chrome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileMetadata {
    pub id: ProfileId,
    pub name: String,
    pub kind: ProfileKind,
    pub color: String,
    pub icon: String,
    pub created_at: u64,
    pub last_used: u64,
    pub is_ephemeral: bool,
}

impl ProfileMetadata {
    pub fn new(id: ProfileId, name: impl Into<String>, kind: ProfileKind) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let is_ephemeral = kind == ProfileKind::Temporary || kind == ProfileKind::AgentSandbox;
        let (color, icon) = match kind {
            ProfileKind::Personal => ("#3B82F6".to_string(), "user".to_string()),
            ProfileKind::Work => ("#8B5CF6".to_string(), "briefcase".to_string()),
            ProfileKind::AgentSandbox => ("#EC4899".to_string(), "bot".to_string()),
            ProfileKind::Temporary => ("#10B981".to_string(), "clock".to_string()),
        };
        Self {
            id,
            name: name.into(),
            kind,
            color,
            icon,
            created_at: now,
            last_used: now,
            is_ephemeral,
        }
    }
}

/// Metadata and storage directory mapping for an active profile.
#[derive(Debug, Clone)]
pub struct Profile {
    pub id: ProfileId,
    pub kind: ProfileKind,
    pub metadata: ProfileMetadata,
    pub root_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub cookie_store_dir: PathBuf,
}

impl Profile {
    pub fn new(id: ProfileId, kind: ProfileKind, base_dir: PathBuf) -> Self {
        let name = match kind {
            ProfileKind::Personal => "Personal",
            ProfileKind::Work => "Work",
            ProfileKind::AgentSandbox => "Agent Sandbox",
            ProfileKind::Temporary => "Temporary Session",
        };
        let metadata = ProfileMetadata::new(id.clone(), name, kind.clone());
        Self::with_metadata(metadata, base_dir)
    }

    pub fn with_metadata(metadata: ProfileMetadata, base_dir: PathBuf) -> Self {
        let root_dir = base_dir.join(&metadata.id.0);
        let cache_dir = root_dir.join("cache");
        let cookie_store_dir = root_dir.join("cookies");

        Self {
            id: metadata.id.clone(),
            kind: metadata.kind.clone(),
            metadata,
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
        if (self.kind == ProfileKind::Temporary || self.metadata.is_ephemeral) && self.root_dir.exists() {
            let _ = std::fs::remove_dir_all(&self.root_dir);
        }
    }

    /// Measurable Ephemeral Wipe: purge profile filesystem directories and verify absence on disk.
    pub fn wipe_disk(&self) -> Result<(), std::io::Error> {
        if self.root_dir.exists() {
            std::fs::remove_dir_all(&self.root_dir)?;
        }
        if self.root_dir.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Ephemeral wipe failed: {:?} still exists on disk", self.root_dir),
            ));
        }
        Ok(())
    }
}

/// Thread-safe registry and manager for browser profiles.
pub struct ProfileManager {
    base_dir: PathBuf,
    temp_dir: PathBuf,
    manifest_path: PathBuf,
    profiles: RwLock<HashMap<ProfileId, Arc<Profile>>>,
    metadata_cache: RwLock<HashMap<ProfileId, ProfileMetadata>>,
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

        Self::with_custom_dirs(base_dir, temp_dir)
    }

    /// Explicit base directory override (for testing).
    pub fn with_custom_dirs(base_dir: PathBuf, temp_dir: PathBuf) -> Self {
        let manifest_path = base_dir.join("profiles.json");
        let mut initial_meta = HashMap::new();

        // Seed canonical default profiles
        let defaults = vec![
            ProfileMetadata::new(ProfileId::personal(), "Personal", ProfileKind::Personal),
            ProfileMetadata::new(ProfileId::work(), "Work", ProfileKind::Work),
            ProfileMetadata::new(ProfileId::agent_sandbox(), "Agent Sandbox", ProfileKind::AgentSandbox),
        ];

        for def in defaults {
            initial_meta.insert(def.id.clone(), def);
        }

        // Load existing manifest if present on disk
        if manifest_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                if let Ok(loaded) = serde_json::from_str::<Vec<ProfileMetadata>>(&content) {
                    for meta in loaded {
                        initial_meta.insert(meta.id.clone(), meta);
                    }
                }
            }
        }

        let mgr = Self {
            base_dir,
            temp_dir,
            manifest_path,
            profiles: RwLock::new(HashMap::new()),
            metadata_cache: RwLock::new(initial_meta),
        };

        // Persist initial manifest if directory exists
        let _ = mgr.save_manifest_internal();
        mgr
    }

    pub fn base_dir(&self) -> &PathBuf {
        &self.base_dir
    }

    pub fn temp_dir(&self) -> &PathBuf {
        &self.temp_dir
    }

    /// List all registered profile metadata for UI presentation.
    pub async fn list_metadata(&self) -> Vec<ProfileMetadata> {
        let cache = self.metadata_cache.read().await;
        let mut list: Vec<ProfileMetadata> = cache.values().cloned().collect();
        // Sort stably: Personal first, then Work, then AgentSandbox, then created_at
        list.sort_by(|a, b| {
            let rank = |k: &ProfileKind| match k {
                ProfileKind::Personal => 0,
                ProfileKind::Work => 1,
                ProfileKind::AgentSandbox => 2,
                ProfileKind::Temporary => 3,
            };
            rank(&a.kind).cmp(&rank(&b.kind)).then(a.created_at.cmp(&b.created_at))
        });
        list
    }

    /// List active registered profile IDs.
    pub async fn list_profiles(&self) -> Vec<ProfileId> {
        let cache = self.metadata_cache.read().await;
        cache.keys().cloned().collect()
    }

    /// Retrieve metadata for a specific ProfileId.
    pub async fn get_metadata(&self, id: &ProfileId) -> Option<ProfileMetadata> {
        let cache = self.metadata_cache.read().await;
        cache.get(id).cloned()
    }

    /// Resolve or instantiate an active `Profile` by `ProfileId`.
    pub async fn get_or_create(&self, id: &ProfileId) -> Result<Arc<Profile>, BrowserError> {
        let mut map = self.profiles.write().await;
        if let Some(profile) = map.get(id) {
            return Ok(Arc::clone(profile));
        }

        let metadata = {
            let cache = self.metadata_cache.read().await;
            cache.get(id).cloned().unwrap_or_else(|| {
                let kind = if id.0 == "personal" {
                    ProfileKind::Personal
                } else if id.0 == "work" {
                    ProfileKind::Work
                } else if id.0 == "agent_sandbox" {
                    ProfileKind::AgentSandbox
                } else {
                    ProfileKind::Temporary
                };
                ProfileMetadata::new(id.clone(), &id.0, kind)
            })
        };

        let base = if metadata.is_ephemeral || metadata.kind == ProfileKind::Temporary {
            self.temp_dir.clone()
        } else {
            self.base_dir.clone()
        };

        let profile = Arc::new(Profile::with_metadata(metadata.clone(), base));
        profile.ensure_directories().map_err(BrowserError::Io)?;

        // Ensure in metadata cache
        {
            let mut cache = self.metadata_cache.write().await;
            if !cache.contains_key(id) {
                cache.insert(id.clone(), metadata);
                self.save_manifest_locked(&cache);
            }
        }

        map.insert(id.clone(), Arc::clone(&profile));
        Ok(profile)
    }

    /// Create a new custom user profile.
    pub async fn create_profile(&self, metadata: ProfileMetadata) -> Result<Arc<Profile>, BrowserError> {
        let id = metadata.id.clone();
        let base = if metadata.is_ephemeral || metadata.kind == ProfileKind::Temporary {
            self.temp_dir.clone()
        } else {
            self.base_dir.clone()
        };

        let profile = Arc::new(Profile::with_metadata(metadata.clone(), base));
        profile.ensure_directories().map_err(BrowserError::Io)?;

        {
            let mut cache = self.metadata_cache.write().await;
            cache.insert(id.clone(), metadata);
            self.save_manifest_locked(&cache);
        }

        {
            let mut map = self.profiles.write().await;
            map.insert(id, Arc::clone(&profile));
        }

        Ok(profile)
    }

    /// Update profile metadata (name, color, icon).
    pub async fn update_profile(&self, metadata: ProfileMetadata) -> Result<(), BrowserError> {
        let id = metadata.id.clone();
        let mut cache = self.metadata_cache.write().await;
        if let Some(existing) = cache.get_mut(&id) {
            existing.name = metadata.name;
            existing.color = metadata.color;
            existing.icon = metadata.icon;
            existing.last_used = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            self.save_manifest_locked(&cache);
            Ok(())
        } else {
            Err(BrowserError::ProfileError(format!("Profile '{id}' not found")))
        }
    }

    /// Delete a profile and purge its storage directories.
    /// Invariant: Default Personal profile cannot be deleted.
    pub async fn delete_profile(&self, id: &ProfileId) -> Result<(), BrowserError> {
        if id == &ProfileId::personal() {
            return Err(BrowserError::ProfileError(
                "Cannot delete the default Personal profile".to_string(),
            ));
        }

        let profile = {
            let mut map = self.profiles.write().await;
            map.remove(id)
        };

        if let Some(profile) = profile {
            let _ = profile.wipe_disk();
        } else {
            let p_dir = self.base_dir.join(&id.0);
            if p_dir.exists() {
                let _ = std::fs::remove_dir_all(&p_dir);
            }
        }

        {
            let mut cache = self.metadata_cache.write().await;
            cache.remove(id);
            self.save_manifest_locked(&cache);
        }

        Ok(())
    }

    /// Allocate a fresh, isolated temporary profile with a unique UUID.
    pub async fn create_temporary(&self) -> Result<Arc<Profile>, BrowserError> {
        let id = ProfileId(format!("temp_{}", Uuid::new_v4().simple()));
        self.get_or_create(&id).await
    }

    /// Measurable Ephemeral Wipe: purge the profile's disk footprint and verify disk absence.
    pub async fn wipe_ephemeral_profile(&self, id: &ProfileId) -> Result<(), BrowserError> {
        let profile = {
            let mut map = self.profiles.write().await;
            map.remove(id)
        };

        if let Some(profile) = profile {
            profile.wipe_disk().map_err(BrowserError::Io)?;
        } else {
            let temp_path = self.temp_dir.join(&id.0);
            if temp_path.exists() {
                std::fs::remove_dir_all(&temp_path).map_err(BrowserError::Io)?;
            }
            let base_path = self.base_dir.join(&id.0);
            if base_path.exists() {
                std::fs::remove_dir_all(&base_path).map_err(BrowserError::Io)?;
            }
        }

        // If temporary, remove from metadata cache
        {
            let mut cache = self.metadata_cache.write().await;
            if let Some(meta) = cache.get(id) {
                if meta.kind == ProfileKind::Temporary {
                    cache.remove(id);
                    self.save_manifest_locked(&cache);
                }
            }
        }

        Ok(())
    }

    fn save_manifest_internal(&self) -> Result<(), std::io::Error> {
        if let Ok(cache) = self.metadata_cache.try_read() {
            let _ = std::fs::create_dir_all(&self.base_dir);
            let vec: Vec<ProfileMetadata> = cache.values().cloned().collect();
            if let Ok(json) = serde_json::to_string_pretty(&vec) {
                let _ = std::fs::write(&self.manifest_path, json);
            }
        }
        Ok(())
    }

    fn save_manifest_locked(&self, cache: &HashMap<ProfileId, ProfileMetadata>) {
        let _ = std::fs::create_dir_all(&self.base_dir);
        let vec: Vec<ProfileMetadata> = cache.values().cloned().collect();
        if let Ok(json) = serde_json::to_string_pretty(&vec) {
            let _ = std::fs::write(&self.manifest_path, json);
        }
    }
}

// ---------------------------------------------------------------------------
// Credential Broker Boundary & Session Escalation Helper (Phase 7.4, INV-06)
// ---------------------------------------------------------------------------

/// Record a two-stage fail-closed audit intent for agent session escalation.
pub async fn record_session_escalation_intent(
    audit_sink: &Arc<dyn kage_core::audit::AuditSink>,
    tab_id: TabId,
    source_profile: &ProfileId,
    target_profile: &ProfileId,
    reason: &str,
    request_id: &str,
) -> Result<u64, BrowserError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string());

    let args_val = serde_json::json!({
        "tab_id": tab_id.to_string(),
        "source_profile": source_profile.to_string(),
        "target_profile": target_profile.to_string(),
        "reason": reason,
    });

    let args_digest = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(serde_json::to_string(&args_val).unwrap_or_default().as_bytes());
        format!("{:x}", hasher.finalize())
    };

    let record = kage_core::audit::CanonicalAuditRecord {
        sequence: None,
        timestamp: now,
        request_id: request_id.to_string(),
        parent_request_id: None,
        caller: "agent_planner".to_string(),
        actor: kage_core::audit::ActorType::Agent,
        tool_id: "profile.escalate_session".to_string(),
        capability: "credential_broker:shared_session".to_string(),
        profile_id: Some(target_profile.to_string()),
        tab_id: Some(tab_id.to_string()),
        target_id: None,
        session_id: None,
        origin: None,
        tier: 3,
        policy_decision: "require_confirmation".to_string(),
        confirmation_id: None,
        args_digest,
        result_digest: None,
        status: kage_core::audit::AuditStatus::Started,
        duration_ms: 0,
        error_code: None,
        host_instance_id: None,
        prev_hash: None,
    };

    audit_sink
        .append(record)
        .await
        .map_err(|e| BrowserError::ProfileError(format!("Fail-closed audit intent commitment failed: {e}")))
}

/// Commit or deny session escalation intent with immutable audit commitment.
pub async fn record_session_escalation_outcome(
    audit_sink: &Arc<dyn kage_core::audit::AuditSink>,
    tab_id: TabId,
    target_profile: &ProfileId,
    request_id: &str,
    approved: bool,
    duration_ms: u64,
) -> Result<u64, BrowserError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string());

    let result_val = serde_json::json!({
        "approved": approved,
        "escalated_profile": target_profile.to_string(),
    });

    let result_digest = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(serde_json::to_string(&result_val).unwrap_or_default().as_bytes());
        format!("{:x}", hasher.finalize())
    };

    let status = if approved {
        kage_core::audit::AuditStatus::Success
    } else {
        kage_core::audit::AuditStatus::Denied
    };

    let record = kage_core::audit::CanonicalAuditRecord {
        sequence: None,
        timestamp: now,
        request_id: request_id.to_string(),
        parent_request_id: None,
        caller: "agent_planner".to_string(),
        actor: kage_core::audit::ActorType::Agent,
        tool_id: "profile.escalate_session".to_string(),
        capability: "credential_broker:shared_session".to_string(),
        profile_id: Some(target_profile.to_string()),
        tab_id: Some(tab_id.to_string()),
        target_id: None,
        session_id: None,
        origin: None,
        tier: 3,
        policy_decision: if approved { "allow".to_string() } else { "deny".to_string() },
        confirmation_id: Some(format!("conf_{request_id}")),
        args_digest: String::new(),
        result_digest: Some(result_digest),
        status,
        duration_ms,
        error_code: if approved { None } else { Some("USER_REJECTED_ESCALATION".to_string()) },
        host_instance_id: None,
        prev_hash: None,
    };

    audit_sink
        .append(record)
        .await
        .map_err(|e| BrowserError::ProfileError(format!("Audit outcome commitment failed: {e}")))
}
