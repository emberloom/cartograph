pub mod engine;
pub mod report;
pub mod rules;

use serde::{Deserialize, Serialize};

/// Severity level for policy violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info => write!(f, "info"),
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

/// A single policy violation.
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub policy_id: String,
    pub severity: Severity,
    pub entity_path: String,
    pub message: String,
}

/// Result of evaluating all policies.
#[derive(Debug, Clone, Serialize)]
pub struct PolicyResult {
    pub violations: Vec<Violation>,
    pub policies_checked: usize,
    pub has_errors: bool,
}
