pub mod analysis;
pub mod client;
pub mod webhook;

use serde::{Deserialize, Serialize};

/// Risk level for a PR or individual file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "low"),
            RiskLevel::Medium => write!(f, "medium"),
            RiskLevel::High => write!(f, "high"),
            RiskLevel::Critical => write!(f, "critical"),
        }
    }
}

/// Configuration for PR analysis.
#[derive(Debug, Clone)]
pub struct PrAnalysisConfig {
    pub blast_radius_depth: usize,
    pub include_ownership: bool,
    pub include_co_changes: bool,
}

impl Default for PrAnalysisConfig {
    fn default() -> Self {
        Self {
            blast_radius_depth: 2,
            include_ownership: true,
            include_co_changes: true,
        }
    }
}

/// Analysis of a single changed file.
#[derive(Debug, Clone, Serialize)]
pub struct FileAnalysis {
    pub path: String,
    pub blast_radius_count: usize,
    pub blast_radius_files: Vec<String>,
    pub hotspot_score: usize,
    pub risk_level: RiskLevel,
}

/// Warning about missing co-changes.
#[derive(Debug, Clone, Serialize)]
pub struct CoChangeWarning {
    pub changed_file: String,
    pub missing_file: String,
    pub confidence: f64,
}

/// Report for an entire PR.
#[derive(Debug, Clone, Serialize)]
pub struct PrReport {
    pub changed_files: Vec<FileAnalysis>,
    pub overall_risk: RiskLevel,
    pub suggested_reviewers: Vec<String>,
    pub missing_co_changes: Vec<CoChangeWarning>,
}
