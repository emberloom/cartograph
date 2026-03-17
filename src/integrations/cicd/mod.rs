pub mod github_actions;
pub mod reporter;
pub mod sarif;

use serde::{Deserialize, Serialize};

use crate::integrations::github::RiskLevel;

/// Output format for CI reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Sarif,
    GithubActions,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sarif" => Ok(OutputFormat::Sarif),
            "github-actions" | "github_actions" | "gha" => Ok(OutputFormat::GithubActions),
            "json" => Ok(OutputFormat::Json),
            other => Err(anyhow::anyhow!("Unknown output format: {}", other)),
        }
    }
}

/// Threshold at which the CI pipeline should fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailThreshold {
    None,
    Low,
    Medium,
    High,
    Critical,
}

impl std::str::FromStr for FailThreshold {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(FailThreshold::None),
            "low" => Ok(FailThreshold::Low),
            "medium" => Ok(FailThreshold::Medium),
            "high" => Ok(FailThreshold::High),
            "critical" => Ok(FailThreshold::Critical),
            other => Err(anyhow::anyhow!("Unknown fail threshold: {}", other)),
        }
    }
}

impl FailThreshold {
    pub fn should_fail(&self, risk: &RiskLevel) -> bool {
        match self {
            FailThreshold::None => false,
            FailThreshold::Low => true,
            FailThreshold::Medium => matches!(
                risk,
                RiskLevel::Medium | RiskLevel::High | RiskLevel::Critical
            ),
            FailThreshold::High => matches!(risk, RiskLevel::High | RiskLevel::Critical),
            FailThreshold::Critical => matches!(risk, RiskLevel::Critical),
        }
    }
}

/// A single finding from CI analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiFinding {
    pub file: String,
    pub severity: String,
    pub message: String,
    pub rule_id: String,
    pub line: Option<u32>,
}

/// Summary of CI analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiSummary {
    pub total_findings: usize,
    pub by_severity: std::collections::HashMap<String, usize>,
    pub max_risk: String,
}

/// Complete CI report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiReport {
    pub findings: Vec<CiFinding>,
    pub summary: CiSummary,
    pub exit_code: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_parsing() {
        assert_eq!(
            "sarif".parse::<OutputFormat>().unwrap(),
            OutputFormat::Sarif
        );
        assert_eq!(
            "github-actions".parse::<OutputFormat>().unwrap(),
            OutputFormat::GithubActions
        );
        assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert_eq!(
            "gha".parse::<OutputFormat>().unwrap(),
            OutputFormat::GithubActions
        );
        assert!("invalid".parse::<OutputFormat>().is_err());
    }

    #[test]
    fn test_fail_threshold_parsing() {
        assert_eq!(
            "none".parse::<FailThreshold>().unwrap(),
            FailThreshold::None
        );
        assert_eq!(
            "high".parse::<FailThreshold>().unwrap(),
            FailThreshold::High
        );
    }

    #[test]
    fn test_fail_threshold_should_fail() {
        assert!(!FailThreshold::None.should_fail(&RiskLevel::Critical));
        assert!(FailThreshold::Low.should_fail(&RiskLevel::Low));
        assert!(!FailThreshold::High.should_fail(&RiskLevel::Medium));
        assert!(FailThreshold::High.should_fail(&RiskLevel::High));
        assert!(FailThreshold::Critical.should_fail(&RiskLevel::Critical));
        assert!(!FailThreshold::Critical.should_fail(&RiskLevel::High));
    }
}
