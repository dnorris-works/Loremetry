use serde::{Deserialize, Serialize};

/// Access tier for a user account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    Free,
    Pro,
}

/// Result of an entitlement check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccessDecision {
    pub allowed: bool,
    pub reason: &'static str,
}

impl AccessDecision {
    pub fn allow() -> Self {
        Self {
            allowed: true,
            reason: "allowed",
        }
    }

    pub fn deny(reason: &'static str) -> Self {
        Self {
            allowed: false,
            reason,
        }
    }
}
