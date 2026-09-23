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
struct SecretPattern {
    regex: Regex,
    replacement: &'static str,
}

/// Compiled regexes for known secret patterns.  Extend this list for new
/// credential formats; patterns are applied to every string leaf in the JSON tree.
fn secret_patterns() -> &'static [SecretPattern] {
    static PATTERNS: OnceLock<Vec<SecretPattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            // Bearer / OAuth tokens: preserve "Bearer " label
            SecretPattern {
                regex: Regex::new(r"(?i)(bearer\s+)[A-Za-z0-9\-._~+/]+=*").unwrap(),
                replacement: "${1}[REDACTED]",
            },
            // Generic API key
            SecretPattern {
                regex: Regex::new(r#"((?i)(?:api[_-]?key|api[_-]?secret|secret[_-]?key)\s*[=:]\s*)[^\s'",;]+"#).unwrap(),
                replacement: "${1}[REDACTED]",
            },
            // AWS access key IDs
            SecretPattern {
                regex: Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
                replacement: "[REDACTED]",
            },
            // AWS secret access keys
            SecretPattern {
                regex: Regex::new(r#"((?i)aws[_-]?secret[_-]?access[_-]?key\s*[=:]\s*)[^\s'",;]+"#).unwrap(),
                replacement: "${1}[REDACTED]",
            },
            // GitHub personal access tokens
            SecretPattern {
                regex: Regex::new(r"gh[pousr]_[A-Za-z0-9]{36,}").unwrap(),
                replacement: "[REDACTED]",
            },
            // JWTs (three base64-url segments)
            SecretPattern {
                regex: Regex::new(r"eyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+").unwrap(),
                replacement: "[REDACTED]",
            },
            // Hex-encoded 32-byte secrets (common for session keys)
            SecretPattern {
                regex: Regex::new(r"\b[0-9a-fA-F]{64}\b").unwrap(),
                replacement: "[REDACTED]",
            },
            // Cookie header values
            SecretPattern {
                regex: Regex::new(r#"((?i)cookie\s*:\s*)[^\s'",;]+"#).unwrap(),
                replacement: "${1}[REDACTED]",
            },
            // Password field values: preserve "password=" or "\"password\":"
            SecretPattern {
                regex: Regex::new(r#"((?i)"?password"?\s*[=:]\s*"?)[^\s'",;)]+"#).unwrap(),
                replacement: "${1}[REDACTED]",
            },
            // Credit card numbers (16 digits with dashes or spaces)
            SecretPattern {
                regex: Regex::new(r"\b(?:\d{4}[-\s]?){3}\d{4}\b").unwrap(),
                replacement: "[REDACTED]",
            },
        ]
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
            result = pattern.regex.replace_all(&result, pattern.replacement).into_owned();
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
            "token", "password", "passwd", "secret", "api_key", "apikey", "access_token",
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

    #[test]
    fn redacts_sensitive_log_without_eating_quotes() {
        let sanitizer = SecretSanitizer::new();
        let input = "console.error('Request failed with Bearer secret_live_token_7721 and password=hunter2');";
        let output = sanitizer.sanitize_string(input);
        assert_eq!(
            output,
            "console.error('Request failed with Bearer [REDACTED] and password=[REDACTED]');"
        );
    }
}
