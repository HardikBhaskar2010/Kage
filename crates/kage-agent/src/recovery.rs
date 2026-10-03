//! Idempotency-aware failure recovery and cycle detection engine.
//!
//! # Architecture Invariant
//! Non-idempotent browser mutations (e.g. form submissions, payments, irreversible state changes)
//! **MUST NOT** be automatically retried blindly upon failure or ambiguous state.
//! Only idempotent actions may be retried within a strict retry budget and backoff schedule.
//!
//! # Registry-Driven Recovery (Correction #3)
//! Recovery decisions are strictly derived from authoritative [`ToolMetadata`] in the
//! [`CapabilityRegistry`]. Tool names are never hardcoded.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use kage_core::registry::{IdempotencyClassification, RetryPolicy, ToolMetadata};
use crate::verifier::VerificationOutcome;

/// Action idempotency category governing recovery behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdempotencyCategory {
    /// Safe to execute multiple times without side effects (read-only queries).
    ReadOnly,
    /// Safe to execute multiple times with identical net outcome (navigate, scroll).
    Idempotent,
    /// Must NOT be blindly retried; mutations accumulate or commit irreversible actions.
    NonIdempotent,
}

impl From<IdempotencyClassification> for IdempotencyCategory {
    fn from(c: IdempotencyClassification) -> Self {
        match c {
            IdempotencyClassification::ReadOnly => Self::ReadOnly,
            IdempotencyClassification::Idempotent => Self::Idempotent,
            IdempotencyClassification::NonIdempotent => Self::NonIdempotent,
        }
    }
}

/// Recovery decision emitted when a step fails verification or encounters an error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", content = "payload", rename_all = "snake_case")]
pub enum RecoveryDecision {
    /// Intended browser effect has already been achieved in current observation; avoid redundant repeated action.
    AlreadyResolved {
        reason: String,
    },
    /// Action is idempotent and within retry budget; safe to retry after backoff.
    Retry {
        attempt: usize,
        max_attempts: usize,
        backoff_ms: u64,
    },
    /// Action requires re-observing state before retrying.
    ReobserveStateAndEvaluate {
        tool_id: String,
        backoff_ms: u64,
    },
    /// Action is non-idempotent; pause execution and request human guidance.
    PauseForGuidance {
        reason: String,
        options: Vec<String>,
    },
    /// Repetitive loop detected (same tool + args + state attempted 3 times); halt immediately.
    HaltCycleDetected {
        tool_id: String,
        cycle_count: usize,
    },
    /// Retry budget exhausted for idempotent action; halt execution.
    HaltExhausted {
        attempts: usize,
        last_error: String,
    },
}

/// State-aware cycle key combining tool ID, normalized arguments, and page state fingerprint.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleKey {
    pub tool_id: String,
    pub normalized_args: String,
    pub state_fingerprint: String,
}

impl CycleKey {
    pub fn new(tool_id: impl Into<String>, normalized_args: impl Into<String>, state_fingerprint: impl Into<String>) -> Self {
        Self {
            tool_id: tool_id.into(),
            normalized_args: normalized_args.into(),
            state_fingerprint: state_fingerprint.into(),
        }
    }

    pub fn to_signature(&self) -> String {
        format!("{}:{}:{}", self.tool_id, self.normalized_args, self.state_fingerprint)
    }
}

/// Cycle detector tracking repetitive action signatures.
#[derive(Debug, Clone, Default)]
pub struct CycleDetector {
    recent_signatures: Vec<String>,
    max_consecutive_repeats: usize,
}

impl CycleDetector {
    pub fn new(max_consecutive_repeats: usize) -> Self {
        Self {
            recent_signatures: Vec::new(),
            max_consecutive_repeats,
        }
    }

    /// Record a state-aware action signature and check for infinite cycles.
    pub fn record_and_check(&mut self, signature: &str) -> bool {
        self.recent_signatures.push(signature.to_string());
        let count = self.recent_signatures.len();
        if count < self.max_consecutive_repeats {
            return false;
        }

        // Check if the last N signatures are identical
        let last_n = &self.recent_signatures[count - self.max_consecutive_repeats..];
        last_n.iter().all(|s| s == signature)
    }

    /// Reset history upon goal progress or state transition.
    pub fn reset(&mut self) {
        self.recent_signatures.clear();
    }
}

/// Idempotency-aware recovery manager driving retry and intervention decisions.
#[derive(Debug, Clone)]
pub struct RecoveryManager {
    max_retries: usize,
    base_backoff_ms: u64,
    retry_counts: HashMap<String, usize>,
    cycle_detector: CycleDetector,
}

impl Default for RecoveryManager {
    fn default() -> Self {
        Self::new(3, 100)
    }
}

impl RecoveryManager {
    pub fn new(max_retries: usize, base_backoff_ms: u64) -> Self {
        Self {
            max_retries,
            base_backoff_ms,
            retry_counts: HashMap::new(),
            cycle_detector: CycleDetector::new(3),
        }
    }

    /// Evaluate failure recovery strategy given tool metadata, state fingerprint, verification outcome,
    /// and whether the intended effect has already succeeded in current browser state (Correction #3).
    ///
    /// Consumes authoritative [`ToolMetadata`]: never infers safety from tool names.
    pub fn evaluate_with_state(
        &mut self,
        tool_meta: &ToolMetadata,
        arguments_digest: &str,
        state_fingerprint: &str,
        outcome: &VerificationOutcome,
        intended_effect_achieved: bool,
    ) -> RecoveryDecision {
        // 0. Check if current observation already satisfies postcondition (delayed browser effect)
        if intended_effect_achieved {
            return RecoveryDecision::AlreadyResolved {
                reason: format!(
                    "Intended effect of '{}' already confirmed in current observation; preventing redundant retry",
                    tool_meta.tool_id
                ),
            };
        }

        self.evaluate_with_metadata(tool_meta, arguments_digest, state_fingerprint, outcome)
    }

    /// Evaluate failure recovery strategy given tool metadata, state fingerprint, and verification outcome.
    ///
    /// Consumes authoritative [`ToolMetadata`]: never infers safety from tool names.
    pub fn evaluate_with_metadata(
        &mut self,
        tool_meta: &ToolMetadata,
        arguments_digest: &str,
        state_fingerprint: &str,
        outcome: &VerificationOutcome,
    ) -> RecoveryDecision {
        let cycle_key = CycleKey::new(&tool_meta.tool_id, arguments_digest, state_fingerprint);
        let signature = cycle_key.to_signature();

        // 1. Cycle detection (Invariant: 3 identical repeats in same state -> E_CYCLE_DETECTED)
        if self.cycle_detector.record_and_check(&signature) {
            return RecoveryDecision::HaltCycleDetected {
                tool_id: tool_meta.tool_id.clone(),
                cycle_count: 3,
            };
        }

        // 2. Non-idempotent action handling: NEVER blind retry
        if tool_meta.idempotency == IdempotencyClassification::NonIdempotent || tool_meta.retry_policy == RetryPolicy::Never {
            let reason = match outcome {
                VerificationOutcome::Fail { reason, .. } => reason.clone(),
                VerificationOutcome::Unknown { uncertainty_cause, .. } => {
                    format!("Ambiguous state: {}", uncertainty_cause)
                }
                VerificationOutcome::Pass { .. } => "Unexpected recovery call for passing outcome".to_string(),
            };
            return RecoveryDecision::PauseForGuidance {
                reason: format!("Non-idempotent tool '{}' failed: {}", tool_meta.tool_id, reason),
                options: vec![
                    "Take Control (Manual)".to_string(),
                    "Retry Once".to_string(),
                    "Skip Step".to_string(),
                    "Abort Task".to_string(),
                ],
            };
        }

        let action_key = format!("{}:{}", tool_meta.tool_id, arguments_digest);

        // 3. VerifyBeforeRetry policy handling
        if tool_meta.retry_policy == RetryPolicy::VerifyBeforeRetry {
            let current_attempts = self.retry_counts.entry(action_key.clone()).or_insert(0);
            *current_attempts += 1;
            if *current_attempts > self.max_retries {
                let last_error = match outcome {
                    VerificationOutcome::Fail { reason, .. } => reason.clone(),
                    VerificationOutcome::Unknown { uncertainty_cause, .. } => uncertainty_cause.clone(),
                    _ => "Unknown failure".to_string(),
                };
                return RecoveryDecision::HaltExhausted {
                    attempts: *current_attempts,
                    last_error,
                };
            }
            let backoff_multiplier = 1 << (*current_attempts - 1);
            let backoff_ms = self.base_backoff_ms * backoff_multiplier;
            return RecoveryDecision::ReobserveStateAndEvaluate {
                tool_id: tool_meta.tool_id.clone(),
                backoff_ms,
            };
        }

        // 4. Idempotent action handling: automated retry with exponential backoff
        let current_attempts = self.retry_counts.entry(action_key).or_insert(0);
        *current_attempts += 1;

        if *current_attempts > self.max_retries {
            let last_error = match outcome {
                VerificationOutcome::Fail { reason, .. } => reason.clone(),
                VerificationOutcome::Unknown { uncertainty_cause, .. } => uncertainty_cause.clone(),
                _ => "Unknown failure".to_string(),
            };
            return RecoveryDecision::HaltExhausted {
                attempts: *current_attempts,
                last_error,
            };
        }

        // Exponential backoff: base * 2^(attempt - 1)
        let backoff_multiplier = 1 << (*current_attempts - 1);
        let backoff_ms = self.base_backoff_ms * backoff_multiplier;

        RecoveryDecision::Retry {
            attempt: *current_attempts,
            max_attempts: self.max_retries,
            backoff_ms,
        }
    }

    /// Backwards compatible evaluation method accepting `IdempotencyCategory`.
    pub fn evaluate(
        &mut self,
        tool_id: &str,
        arguments_digest: &str,
        idempotency: IdempotencyCategory,
        outcome: &VerificationOutcome,
    ) -> RecoveryDecision {
        let signature = format!("{}:{}:default_state", tool_id, arguments_digest);

        // 1. Cycle detection (Invariant: 3 identical repeats -> E_CYCLE_DETECTED)
        if self.cycle_detector.record_and_check(&signature) {
            return RecoveryDecision::HaltCycleDetected {
                tool_id: tool_id.to_string(),
                cycle_count: 3,
            };
        }

        // 2. Non-idempotent action handling: NEVER blind retry
        if idempotency == IdempotencyCategory::NonIdempotent {
            let reason = match outcome {
                VerificationOutcome::Fail { reason, .. } => reason.clone(),
                VerificationOutcome::Unknown { uncertainty_cause, .. } => {
                    format!("Ambiguous state: {}", uncertainty_cause)
                }
                VerificationOutcome::Pass { .. } => "Unexpected recovery call for passing outcome".to_string(),
            };
            return RecoveryDecision::PauseForGuidance {
                reason: format!("Non-idempotent tool '{}' failed: {}", tool_id, reason),
                options: vec![
                    "Take Control (Manual)".to_string(),
                    "Retry Once".to_string(),
                    "Skip Step".to_string(),
                    "Abort Task".to_string(),
                ],
            };
        }

        // 3. Idempotent action handling: automated retry with exponential backoff
        let current_attempts = self.retry_counts.entry(signature.clone()).or_insert(0);
        *current_attempts += 1;

        if *current_attempts > self.max_retries {
            let last_error = match outcome {
                VerificationOutcome::Fail { reason, .. } => reason.clone(),
                VerificationOutcome::Unknown { uncertainty_cause, .. } => uncertainty_cause.clone(),
                _ => "Unknown failure".to_string(),
            };
            return RecoveryDecision::HaltExhausted {
                attempts: *current_attempts,
                last_error,
            };
        }

        let backoff_multiplier = 1 << (*current_attempts - 1);
        let backoff_ms = self.base_backoff_ms * backoff_multiplier;

        RecoveryDecision::Retry {
            attempt: *current_attempts,
            max_attempts: self.max_retries,
            backoff_ms,
        }
    }

    /// Reset retry counters and cycle detection upon meaningful plan progression.
    pub fn reset_progress(&mut self) {
        self.retry_counts.clear();
        self.cycle_detector.reset();
    }
}
