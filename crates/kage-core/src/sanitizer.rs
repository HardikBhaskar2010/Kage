//! Recursive secret sanitizer (KAGE-SEC-002).
//!
//! The sanitizer walks an arbitrary [`serde_json::Value`] tree and redacts any
//! string that matches known secret patterns.  It is applied by the [`ToolBus`]
//! *before* any context is forwarded to the AI subsystem or written to the audit log.
//!
//! # Design invariant
//!
//! Audit records must contain **zero raw secrets**.  This module enforces that.

use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Pattern registry
// ---------------------------------------------------------------------------

/// Compiled regexes for known secret patterns.  Extend this list for new
/// credential formats; patterns are applied to every string leaf in the JSON tree.
fn secret_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let raw = [
            // Bearer / OAuth tokens
            r"(?i)bearer\s+[A-Za-z0-9\-._~+/]+=*",
            // Generic API key (key = ..., api_key=..., apikey=...)
            r"(?i)(?:api[_-]?key|api[_-]?secret|secret[_-]?key)\s*[=:]\s*\S+",
            // AWS access key IDs
            r"AKIA[0-9A-Z]{16}",
            // AWS secret access keys
            r"(?i)aws[_-]?secret[_-]?access[_-]?key\s*[=:]\s*\S+",
            // GitHub personal access tokens
            r"gh[pousr]_[A-Za-z0-9]{36,}",
            // JWTs (three base64-url segments)
            r"eyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+",
            // Hex-encoded 32-byte secrets (common for session keys)
            r"\b[0-9a-fA-F]{64}\b",
            // Cookie header values
            r"(?i)cookie\s*:\s*\S+",
            // Password field values
            r#"(?i)"?password"?\s*[=:]\s*"?\S+"?"#,
            // Credit card numbers (16 digits with dashes or spaces)
            r"\b(?:\d{4}[-\s]?){3}\d{4}\b",
        ];
        raw.iter()
            .map(|p| Regex::new(p).expect("static secret pattern must be valid"))
            .collect()
    })
}

// ---------------------------------------------------------------------------
// SecretSanitizer
// ---------------------------------------------------------------------------

/// Stateless recursive sanitizer for JSON values.
#[derive(Debug, Clone, Default)]
pub struct SecretSanitizer;

impl SecretSanitizer {
    pub fn new() -> Self {
        SecretSanitizer
    }

    /// Redact secret patterns within a single string slice.
    pub fn sanitize_string(&self, s: &str) -> String {
        let mut result = s.to_string();
        for pattern in secret_patterns() {
            result = pattern.replace_all(&result, "[REDACTED]").into_owned();
        }
        result
    }

    /// Recursively walk `value` and redact any string leaves that match a
    /// registered secret pattern.  Returns an owned, sanitized copy.
    pub fn sanitize(&self, value: Value) -> Value {
        self.walk(value)
    }

    fn walk(&self, value: Value) -> Value {
        match value {
            Value::String(s) => Value::String(self.redact_string(s)),
            Value::Array(arr) => Value::Array(arr.into_iter().map(|v| self.walk(v)).collect()),
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(k, v)| {
                        // Also redact keys that look like secret field names.
                        let sanitized_v = if self.is_secret_key(&k) {
                            Value::String("[REDACTED]".to_string())
                        } else {
                            self.walk(v)
                        };
                        (k, sanitized_v)
                    })
                    .collect(),
            ),
            // Booleans, numbers, null — pass through unchanged.
            other => other,
        }
    }

    /// Returns `true` if the JSON key itself indicates a secret field.
    fn is_secret_key(&self, key: &str) -> bool {
        let lower = key.to_lowercase();
        [
            "password", "passwd", "secret", "api_key", "apikey", "access_token",
            "refresh_token", "auth_token", "bearer", "private_key", "cookie",
            "session_id", "session_token", "csrf_token", "credit_card", "card_number",
            "cvv", "cvc", "ssn", "authorization", "auth_header",
        ]
        .iter()
        .any(|&pat| lower.contains(pat))
    }

    /// Redact secret patterns within a single string value.
    fn redact_string(&self, s: String) -> String {
        self.sanitize_string(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_bearer_token_in_string() {
        let sanitizer = SecretSanitizer::new();
        let input = json!("Authorization: Bearer eyABC.defGHI.xyz123");
        let output = sanitizer.sanitize(input);
        let s = output.as_str().unwrap();
        assert!(!s.contains("eyABC"), "Bearer token must be redacted: {s}");
        assert!(s.contains("[REDACTED]"), "Redaction marker must appear: {s}");
    }

    #[test]
    fn redacts_secret_key_by_field_name() {
        let sanitizer = SecretSanitizer::new();
        let input = json!({ "password": "hunter2", "username": "alice" });
        let output = sanitizer.sanitize(input);
        assert_eq!(output["password"], "[REDACTED]");
        assert_eq!(output["username"], "alice");
    }

    #[test]
    fn recursively_redacts_nested_values() {
        let sanitizer = SecretSanitizer::new();
        let input = json!({
            "headers": {
                "Authorization": "Bearer secret-token-value",
                "Content-Type": "application/json"
            }
        });
        let output = sanitizer.sanitize(input);
        let auth = output["headers"]["Authorization"].as_str().unwrap();
        assert!(auth.contains("[REDACTED]"), "Nested header must be redacted: {auth}");
        assert_eq!(output["headers"]["Content-Type"], "application/json");
    }

    #[test]
    fn passthrough_safe_values_unchanged() {
        let sanitizer = SecretSanitizer::new();
        let input = json!({ "url": "https://example.com", "status": 200 });
        let output = sanitizer.sanitize(input.clone());
        assert_eq!(output["url"], "https://example.com");
        assert_eq!(output["status"], 200);
    }
}
