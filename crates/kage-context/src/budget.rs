//! Token budget enforcer for context packs (KAGE-CTX-001).
//!
//! The baseline budget is **4,000 tokens**.  A simple character-count approximation
//! is used (`1 token ≈ 4 chars`) to avoid a heavy tokenizer dependency at this stage.
//! The production implementation will plug in the model-specific tokenizer.

/// Default token budget per context pack assembly.
pub const DEFAULT_BUDGET: usize = 4_000;

/// Token approximation: 1 token ≈ 4 UTF-8 characters.
const CHARS_PER_TOKEN: usize = 4;

/// Tracks remaining token budget during context pack assembly.
pub struct TokenBudget {
    total: usize,
    used: usize,
}

impl TokenBudget {
    pub fn new(total: usize) -> Self {
        TokenBudget { total, used: 0 }
    }

    pub fn default_budget() -> Self {
        Self::new(DEFAULT_BUDGET)
    }

    /// Approximate token count for `text`.
    pub fn estimate_tokens(text: &str) -> usize {
        (text.len() + CHARS_PER_TOKEN - 1) / CHARS_PER_TOKEN
    }

    /// Returns `true` if `text` fits within the remaining budget and consumes it.
    pub fn try_consume(&mut self, text: &str) -> bool {
        let cost = Self::estimate_tokens(text);
        if self.used + cost <= self.total {
            self.used += cost;
            true
        } else {
            false
        }
    }

    /// Remaining token capacity.
    pub fn remaining(&self) -> usize {
        self.total.saturating_sub(self.used)
    }

    /// Proportion of budget consumed (0.0 – 1.0).
    pub fn utilization(&self) -> f64 {
        self.used as f64 / self.total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_rough() {
        assert_eq!(TokenBudget::estimate_tokens("hello"), 2); // 5 chars / 4 = 1.25 → 2
    }

    #[test]
    fn try_consume_within_budget() {
        let mut budget = TokenBudget::new(10);
        let text = "a".repeat(20); // 5 tokens
        assert!(budget.try_consume(&text));
        assert_eq!(budget.remaining(), 5);
    }

    #[test]
    fn try_consume_exceeds_budget() {
        let mut budget = TokenBudget::new(2);
        let text = "a".repeat(100); // 25 tokens — too big
        assert!(!budget.try_consume(&text));
        assert_eq!(budget.remaining(), 2); // unchanged
    }
}
