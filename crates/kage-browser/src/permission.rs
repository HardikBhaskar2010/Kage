//! Centralized Permission Engine (KAGE-SEC-004, Phase 7.3).
//!
//! Origin-scoped permission matrix partitioned by (ProfileId, Origin, PermissionType).
//! Enforces strict least-privilege defaults for autonomous agent sandboxes while
//! providing granular user-controlled capability grants for Personal/Work profiles.

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::errors::BrowserError;
use crate::tab::ProfileId;

/// Supported permission capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionType {
    Geolocation,
    Notifications,
    Camera,
    Microphone,
    ClipboardRead,
    ClipboardWrite,
    Downloads,
    Popups,
}

impl std::fmt::Display for PermissionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionType::Geolocation => write!(f, "geolocation"),
            PermissionType::Notifications => write!(f, "notifications"),
            PermissionType::Camera => write!(f, "camera"),
            PermissionType::Microphone => write!(f, "microphone"),
            PermissionType::ClipboardRead => write!(f, "clipboard_read"),
            PermissionType::ClipboardWrite => write!(f, "clipboard_write"),
            PermissionType::Downloads => write!(f, "downloads"),
            PermissionType::Popups => write!(f, "popups"),
        }
    }
}

impl FromStr for PermissionType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "geolocation" | "geo" => Ok(PermissionType::Geolocation),
            "notifications" | "notify" => Ok(PermissionType::Notifications),
            "camera" | "cam" | "video" => Ok(PermissionType::Camera),
            "microphone" | "mic" | "audio" => Ok(PermissionType::Microphone),
            "clipboard_read" | "clipboardread" => Ok(PermissionType::ClipboardRead),
            "clipboard_write" | "clipboardwrite" => Ok(PermissionType::ClipboardWrite),
            "downloads" | "download" => Ok(PermissionType::Downloads),
            "popups" | "popup" => Ok(PermissionType::Popups),
            _ => Err(format!("Unknown permission type: '{s}'")),
        }
    }
}

/// Authorization decision emitted for a permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    Allow,
    Deny,
    Prompt,
}

impl std::fmt::Display for PermissionDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionDecision::Allow => write!(f, "allow"),
            PermissionDecision::Deny => write!(f, "deny"),
            PermissionDecision::Prompt => write!(f, "prompt"),
        }
    }
}

impl FromStr for PermissionDecision {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "allow" => Ok(PermissionDecision::Allow),
            "deny" => Ok(PermissionDecision::Deny),
            "prompt" => Ok(PermissionDecision::Prompt),
            _ => Err(format!("Unknown permission decision: '{s}'")),
        }
    }
}

/// Persisted origin-scoped permission rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OriginPermissionRule {
    pub profile_id: ProfileId,
    pub origin: String,
    pub permission: PermissionType,
    pub decision: PermissionDecision,
    pub updated_at: u64,
}

/// Thread-safe centralized permission manager.
pub struct PermissionManager {
    base_dir: PathBuf,
    rules: RwLock<HashMap<(ProfileId, String, PermissionType), OriginPermissionRule>>,
}

impl PermissionManager {
    /// Create a new `PermissionManager` anchored at the specified base directory.
    pub fn new(base_dir: PathBuf) -> Self {
        let manifest_path = base_dir.join("permissions.json");
        let mut initial_rules = HashMap::new();

        if manifest_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                if let Ok(loaded) = serde_json::from_str::<Vec<OriginPermissionRule>>(&content) {
                    for rule in loaded {
                        let key = (rule.profile_id.clone(), rule.origin.clone(), rule.permission);
                        initial_rules.insert(key, rule);
                    }
                }
            }
        }

        Self {
            base_dir,
            rules: RwLock::new(initial_rules),
        }
    }

    /// Normalize an origin string for deterministic lookup (trim trailing slash, lowercase).
    pub fn normalize_origin(origin: &str) -> String {
        let trimmed = origin.trim();
        let without_slash = trimmed.strip_suffix('/').unwrap_or(trimmed);
        without_slash.to_lowercase()
    }

    /// Query the permission decision for a given (profile_id, origin, permission_type).
    /// If no explicit rule is configured, defaults are resolved based on profile security tier.
    pub async fn query(
        &self,
        profile_id: &ProfileId,
        origin: &str,
        permission: PermissionType,
    ) -> PermissionDecision {
        let norm_origin = Self::normalize_origin(origin);
        let key = (profile_id.clone(), norm_origin, permission);

        let map = self.rules.read().await;
        if let Some(rule) = map.get(&key) {
            return rule.decision;
        }

        // Default least-privilege resolution
        Self::default_decision(profile_id, permission)
    }

    /// Default least-privilege decision for a profile when no origin override exists.
    pub fn default_decision(profile_id: &ProfileId, permission: PermissionType) -> PermissionDecision {
        let is_sandbox = profile_id == &ProfileId::agent_sandbox()
            || profile_id.0.starts_with("temp_")
            || profile_id.0.contains("sandbox");

        if is_sandbox {
            // Web agents and ephemeral sandboxes cannot access hardware sensors or ambient authority
            match permission {
                PermissionType::Camera
                | PermissionType::Microphone
                | PermissionType::Geolocation
                | PermissionType::Notifications
                | PermissionType::Popups => PermissionDecision::Deny,
                PermissionType::ClipboardRead => PermissionDecision::Prompt,
                PermissionType::ClipboardWrite => PermissionDecision::Allow,
                PermissionType::Downloads => PermissionDecision::Prompt,
            }
        } else {
            // Personal & Work user profiles default to safe interactive prompting
            match permission {
                PermissionType::Camera
                | PermissionType::Microphone
                | PermissionType::Geolocation
                | PermissionType::Notifications
                | PermissionType::ClipboardRead
                | PermissionType::Popups => PermissionDecision::Prompt,
                PermissionType::ClipboardWrite => PermissionDecision::Allow,
                PermissionType::Downloads => PermissionDecision::Allow,
            }
        }
    }

    /// Configure or update a permission rule. Persists the rule manifest to disk.
    pub async fn set(
        &self,
        profile_id: ProfileId,
        origin: &str,
        permission: PermissionType,
        decision: PermissionDecision,
    ) -> Result<(), BrowserError> {
        let norm_origin = Self::normalize_origin(origin);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let rule = OriginPermissionRule {
            profile_id: profile_id.clone(),
            origin: norm_origin.clone(),
            permission,
            decision,
            updated_at: now,
        };

        let key = (profile_id, norm_origin, permission);
        {
            let mut map = self.rules.write().await;
            map.insert(key, rule);
            self.save_manifest_locked(&map)?;
        }

        Ok(())
    }

    /// Delete a specific permission rule.
    pub async fn delete(
        &self,
        profile_id: &ProfileId,
        origin: &str,
        permission: PermissionType,
    ) -> Result<bool, BrowserError> {
        let norm_origin = Self::normalize_origin(origin);
        let key = (profile_id.clone(), norm_origin, permission);

        let mut map = self.rules.write().await;
        let removed = map.remove(&key).is_some();
        if removed {
            self.save_manifest_locked(&map)?;
        }
        Ok(removed)
    }

    /// List all configured rules for a specific profile.
    pub async fn list_permissions(&self, profile_id: &ProfileId) -> Vec<OriginPermissionRule> {
        let map = self.rules.read().await;
        map.values()
            .filter(|r| &r.profile_id == profile_id)
            .cloned()
            .collect()
    }

    /// List all permission rules across all profiles.
    pub async fn list_all(&self) -> Vec<OriginPermissionRule> {
        let map = self.rules.read().await;
        map.values().cloned().collect()
    }

    /// Reset all permissions for a specific profile back to defaults.
    pub async fn reset_profile_permissions(&self, profile_id: &ProfileId) -> Result<(), BrowserError> {
        let mut map = self.rules.write().await;
        map.retain(|(p, _, _), _| p != profile_id);
        self.save_manifest_locked(&map)?;
        Ok(())
    }

    fn save_manifest_locked(
        &self,
        map: &HashMap<(ProfileId, String, PermissionType), OriginPermissionRule>,
    ) -> Result<(), BrowserError> {
        let rules_vec: Vec<OriginPermissionRule> = map.values().cloned().collect();
        let json = serde_json::to_string_pretty(&rules_vec)
            .map_err(|e| BrowserError::ProfileError(format!("Failed to serialize permissions: {e}")))?;

        let _ = std::fs::create_dir_all(&self.base_dir);
        let manifest_path = self.base_dir.join("permissions.json");
        std::fs::write(&manifest_path, json).map_err(BrowserError::Io)?;
        Ok(())
    }
}
