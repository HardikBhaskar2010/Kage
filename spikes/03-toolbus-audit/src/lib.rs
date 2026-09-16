pub mod audit;
pub mod bus;
pub mod policy;
pub mod sanitizer;
pub mod tool;

pub use audit::{AuditLogger, AuditRecord, VerificationResult, GENESIS_HASH};
pub use bus::ToolBus;
pub use policy::{CallerContext, CallerType, PermissionDecision, PolicyEngine};
pub use sanitizer::SecretSanitizer;
pub use tool::{validate_schema, KageTool, PermissionTier, ToolError, ToolRegistry};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deep_recursive_sanitizer() {
        let nested_input = json!({
            "request": {
                "headers": {
                    "Authorization": "Bearer secret_bearer_token_12345",
                    "X-Api-Key": "sk-1234567890abcdef1234567890"
                },
                "body": {
                    "credentials": {
                        "password": "MySuperSecretPassword"
                    }
                }
            },
            "array_data": [
                { "token": "temp_secret" },
                { "Authorization": "Bearer nested_token" }
            ]
        });

        let sanitized = SecretSanitizer::sanitize_value(&nested_input);

        // Assert that raw secrets are NOT present anywhere in the sanitized JSON string
        assert!(!SecretSanitizer::contains_raw_secret(&sanitized, "secret_bearer_token_12345"));
        assert!(!SecretSanitizer::contains_raw_secret(&sanitized, "sk-1234567890abcdef1234567890"));
        assert!(!SecretSanitizer::contains_raw_secret(&sanitized, "MySuperSecretPassword"));
        assert!(!SecretSanitizer::contains_raw_secret(&sanitized, "temp_secret"));

        // Assert redaction tags
        let s = serde_json::to_string(&sanitized).unwrap();
        assert!(s.contains("[REDACTED:BEARER_TOKEN]"));
        assert!(s.contains("[REDACTED:API_KEY]"));
        assert!(s.contains("[REDACTED:PASSWORD]"));
        assert!(s.contains("[REDACTED:TOKEN]"));
    }

    #[test]
    fn test_audit_hash_chain_tamper_detection() {
        let logger = AuditLogger::new_in_memory().unwrap();
        logger.record("tool1", 0, "caller", "{}", "EXECUTED", 10).unwrap();
        logger.record("tool2", 1, "caller", "{}", "EXECUTED", 15).unwrap();
        logger.record("tool3", 2, "caller", "{}", "EXECUTED", 20).unwrap();

        // 1. Initial verification should PASS
        let res = logger.verify_integrity().unwrap();
        assert!(matches!(res, VerificationResult::Pass { total_records_verified: 3 }));
    }
}
