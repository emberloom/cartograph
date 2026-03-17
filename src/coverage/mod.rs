pub mod overlay;
pub mod parser;
pub mod store;

use serde::{Deserialize, Serialize};

/// Coverage data for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCoverage {
    pub path: String,
    pub lines_covered: u32,
    pub lines_total: u32,
    pub line_coverage_pct: f64,
    pub covered_lines: Vec<u32>,
    pub uncovered_lines: Vec<u32>,
}

/// Aggregated coverage report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    pub files: Vec<FileCoverage>,
    pub total_lines_covered: u32,
    pub total_lines: u32,
    pub overall_pct: f64,
}

/// A coverage gap — a hotspot file with low coverage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageGap {
    pub entity_path: String,
    pub coverage_pct: f64,
    pub hotspot_score: usize,
    pub risk_description: String,
}
