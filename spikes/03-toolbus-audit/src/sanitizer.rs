use regex::Regex;
use serde_json::{Map, Value};
use std::sync::LazyLock;

static BEARER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9_\-\.~+/]+=*").unwrap()
});

static API_KEY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(sk-[A-Za-z0-9_\-]{20,}|gh[pousr]_[A-Za-z0-9]{36,})\b").unwrap()
});

static SENSITIVE_KEYS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(password|secret|token|api_key|apikey|credential|auth_token|bearer)$").unwrap()
});

pub struct SecretSanitizer;

impl SecretSanitizer {
    /// Recursively redacts secrets from any JSON Value (strings, nested objects, and arrays)
    pub fn sanitize_value(val: &Value) -> Value {
        match val {
            Value::String(s) => Value::String(Self::sanitize_string(s)),
            Value::Array(arr) => {
                let sanitized_arr: Vec<Value> = arr.iter().map(Self::sanitize_value).collect();
                Value::Array(sanitized_arr)
            }
            Value::Object(map) => {
                let mut sanitized_map = Map::new();
                for (k, v) in map {
                    if SENSITIVE_KEYS.is_match(k) {
                        // Key itself designates a credential/secret
                        sanitized_map.insert(k.clone(), Value::String(format!("[REDACTED:{}]", k.to_uppercase())));
                    } else {
                        sanitized_map.insert(k.clone(), Self::sanitize_value(v));
                    }
                }
                Value::Object(sanitized_map)
            }
            other => other.clone(),
        }
    }

    /// Redacts known secret tokens from a raw string
    pub fn sanitize_string(s: &str) -> String {
        let mut result = s.to_string();
        result = BEARER_REGEX.replace_all(&result, "Bearer [REDACTED:BEARER_TOKEN]").to_string();
        result = API_KEY_REGEX.replace_all(&result, "[REDACTED:API_KEY]").to_string();
        result
    }

    /// Verifies whether a given secret string exists inside a JSON Value (for audit safety assertions)
    pub fn contains_raw_secret(val: &Value, secret: &str) -> bool {
        let serialized = serde_json::to_string(val).unwrap_or_default();
        serialized.contains(secret)
    }
}
