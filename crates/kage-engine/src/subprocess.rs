//! Subprocess binary discovery and packaging validator (CEF-02, CEF-12).
//!
//! Enforces that CEF helper processes (renderer, GPU, utility) use the dedicated
//! `kage-cef-subprocess.exe` binary rather than re-bootstrapping the Tauri host.

use std::path::{Path, PathBuf};

/// Manager responsible for locating and verifying the CEF subprocess executable.
pub struct SubprocessManager;

impl SubprocessManager {
    /// Locate `kage-cef-subprocess.exe` relative to the current running executable.
    ///
    /// In both dev (`target/debug/`) and release distributions, the subprocess helper
    /// resides alongside the main `kage-host.exe` binary.
    pub fn find_subprocess_path() -> Result<PathBuf, String> {
        let current_exe = std::env::current_exe()
            .map_err(|e| format!("Cannot determine host executable path: {e}"))?;

        let exe_dir = current_exe
            .parent()
            .ok_or_else(|| "Current executable has no parent directory".to_string())?;

        let candidate = exe_dir.join("kage-cef-subprocess.exe");
        if candidate.exists() {
            Ok(candidate)
        } else {
            // Also check current working directory / target folder in dev mode
            let dev_candidate = PathBuf::from("target/debug/kage-cef-subprocess.exe");
            if dev_candidate.exists() {
                Ok(std::fs::canonicalize(dev_candidate).unwrap_or(candidate))
            } else {
                Err(format!(
                    "kage-cef-subprocess.exe not found at expected location: {}",
                    candidate.display()
                ))
            }
        }
    }

    /// Validate the presence of all required CEF packaged runtime assets (CEF-12).
    pub fn validate_runtime_package(base_dir: &Path) -> Result<(), Vec<String>> {
        let required_files = [
            "libcef.dll",
            "kage-cef-subprocess.exe",
            "icudtl.dat",
            "snapshot_blob.bin",
            "v8_context_snapshot.bin",
        ];

        let mut missing = Vec::new();
        for file in required_files {
            let path = base_dir.join(file);
            if !path.exists() {
                missing.push(file.to_string());
            }
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_validation_detects_missing_files() {
        let temp_dir = std::env::temp_dir();
        let result = SubprocessManager::validate_runtime_package(&temp_dir);
        assert!(result.is_err());
        let missing = result.unwrap_err();
        assert!(missing.contains(&"libcef.dll".to_string()));
    }
}
