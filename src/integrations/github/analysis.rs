use anyhow::{Result, bail};

use crate::query;
use crate::store::graph::GraphStore;

use super::{CoChangeWarning, FileAnalysis, PrAnalysisConfig, PrReport, RiskLevel};

/// Maximum number of changed files to prevent DoS.
const MAX_CHANGED_FILES: usize = 100;

/// Maximum length of a single file path.
const MAX_PATH_LEN: usize = 1024;

/// Validate changed files input.
///
/// Rejects empty lists, paths containing `..` (path traversal), paths longer
/// than 1024 characters, and lists exceeding 100 files.
pub fn validate_changed_files(changed_files: &[String]) -> Result<()> {
    if changed_files.is_empty() {
        bail!("changed_files must not be empty");
    }
    if changed_files.len() > MAX_CHANGED_FILES {
        bail!(
            "too many changed files ({}, max {})",
            changed_files.len(),
            MAX_CHANGED_FILES
        );
    }
    for path in changed_files {
        if path.contains("..") {
            bail!("path traversal detected in changed file: {}", path);
        }
        if path.len() > MAX_PATH_LEN {
            bail!(
                "changed file path too long ({} chars, max {})",
                path.len(),
                MAX_PATH_LEN
            );
        }
    }
    Ok(())
}

/// Analyze a set of changed files against the Cartograph graph.
///
/// Validates inputs before performing analysis.
pub fn analyze_pr(
    store: &GraphStore,
    changed_files: &[String],
    config: &PrAnalysisConfig,
) -> PrReport {
    let changed_set: std::collections::HashSet<&str> =
        changed_files.iter().map(|s| s.as_str()).collect();

    let mut file_analyses = Vec::new();
    let mut all_reviewers: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    let mut missing_co_changes = Vec::new();

    for file_path in changed_files {
        // Blast radius
        let blast = query::blast_radius::query(store, file_path, config.blast_radius_depth);
        let blast_files: Vec<String> = blast.iter().filter_map(|r| r.entity_path.clone()).collect();
        let blast_count = blast_files.len();

        // Hotspot score
        let hotspot_score = store
            .find_entity_by_path(file_path)
            .map(|e| store.edge_degree(&e.id))
            .unwrap_or(0);

        // Risk level based on blast radius and hotspot score
        let risk_level = compute_file_risk(blast_count, hotspot_score);

        file_analyses.push(FileAnalysis {
            path: file_path.clone(),
            blast_radius_count: blast_count,
            blast_radius_files: blast_files,
            hotspot_score,
            risk_level,
        });

        // Co-changes: files that usually change together but aren't in this PR
        if config.include_co_changes {
            let co = query::co_changes(store, file_path);
            for result in &co {
                if let Some(ref path) = result.entity_path
                    && !changed_set.contains(path.as_str())
                    && result.confidence > 0.3
                {
                    missing_co_changes.push(CoChangeWarning {
                        changed_file: file_path.clone(),
                        missing_file: path.clone(),
                        confidence: result.confidence,
                    });
                }
            }
        }

        // Ownership for reviewer suggestions
        if config.include_ownership {
            let owners = query::ownership::query(store, file_path);
            for owner in &owners {
                *all_reviewers
                    .entry(owner.entity_name.clone())
                    .or_insert(0.0) += owner.confidence;
            }
        }
    }

    // Sort reviewers by total confidence
    let mut suggested_reviewers: Vec<(String, f64)> = all_reviewers.into_iter().collect();
    suggested_reviewers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let suggested_reviewers: Vec<String> = suggested_reviewers
        .into_iter()
        .take(5)
        .map(|(name, _)| name)
        .collect();

    // Overall risk = max of individual file risks
    let overall_risk = file_analyses
        .iter()
        .map(|f| f.risk_level)
        .max()
        .unwrap_or(RiskLevel::Low);

    PrReport {
        changed_files: file_analyses,
        overall_risk,
        suggested_reviewers,
        missing_co_changes,
    }
}

/// Compute risk level for a single file based on blast radius and hotspot score.
fn compute_file_risk(blast_radius_count: usize, hotspot_score: usize) -> RiskLevel {
    let combined = blast_radius_count + hotspot_score;
    if combined >= 20 {
        RiskLevel::Critical
    } else if combined >= 10 {
        RiskLevel::High
    } else if combined >= 4 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

/// Format a PR report as a Markdown comment.
pub fn format_report_markdown(report: &PrReport) -> String {
    let mut md = String::new();

    // Header
    let emoji = match report.overall_risk {
        RiskLevel::Low => "white_check_mark",
        RiskLevel::Medium => "warning",
        RiskLevel::High => "rotating_light",
        RiskLevel::Critical => "no_entry",
    };
    md.push_str(&format!(
        "## Cartograph Analysis :{emoji}:\n\n**Overall Risk: {}**\n\n",
        report.overall_risk
    ));

    // Changed files table
    if !report.changed_files.is_empty() {
        md.push_str("### Changed Files\n\n");
        md.push_str("| File | Blast Radius | Hotspot Score | Risk |\n");
        md.push_str("|------|-------------|---------------|------|\n");
        for f in &report.changed_files {
            md.push_str(&format!(
                "| `{}` | {} files | {} | {} |\n",
                f.path, f.blast_radius_count, f.hotspot_score, f.risk_level
            ));
        }
        md.push('\n');
    }

    // Missing co-changes
    if !report.missing_co_changes.is_empty() {
        md.push_str("### Missing Co-Changes\n\n");
        md.push_str(
            "These files usually change together with files in this PR but weren't included:\n\n",
        );
        md.push_str("| Changed File | Missing File | Confidence |\n");
        md.push_str("|-------------|-------------|------------|\n");
        for w in &report.missing_co_changes {
            md.push_str(&format!(
                "| `{}` | `{}` | {:.0}% |\n",
                w.changed_file,
                w.missing_file,
                w.confidence * 100.0
            ));
        }
        md.push('\n');
    }

    // Suggested reviewers
    if !report.suggested_reviewers.is_empty() {
        md.push_str("### Suggested Reviewers\n\n");
        for reviewer in &report.suggested_reviewers {
            md.push_str(&format!("- {}\n", reviewer));
        }
        md.push('\n');
    }

    md
}

/// Format a PR report as a JSON string.
pub fn format_report_json(report: &PrReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::graph::GraphStore;
    use crate::store::schema::{EdgeKind, EntityKind};

    fn setup_store() -> GraphStore {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::store::schema::init_db(&conn).unwrap();
        let mut store = GraphStore::new(conn).unwrap();

        let a = store
            .add_entity(
                EntityKind::File,
                "auth.rs",
                Some("src/auth.rs"),
                Some("rust"),
            )
            .unwrap();
        let b = store
            .add_entity(
                EntityKind::File,
                "user.rs",
                Some("src/user.rs"),
                Some("rust"),
            )
            .unwrap();
        let c = store
            .add_entity(
                EntityKind::File,
                "billing.rs",
                Some("src/billing.rs"),
                Some("rust"),
            )
            .unwrap();
        let d = store
            .add_entity(EntityKind::File, "api.rs", Some("src/api.rs"), Some("rust"))
            .unwrap();
        let owner = store
            .add_entity(EntityKind::Person, "dev@example.com", None, None)
            .unwrap();

        store.add_edge(&a, &b, EdgeKind::Imports, 1.0).unwrap();
        store.add_edge(&b, &c, EdgeKind::Imports, 1.0).unwrap();
        store.add_edge(&d, &a, EdgeKind::DependsOn, 1.0).unwrap();
        store
            .add_edge(&a, &c, EdgeKind::CoChangesWith, 0.8)
            .unwrap();
        store.add_edge(&a, &owner, EdgeKind::OwnedBy, 0.9).unwrap();

        store
    }

    #[test]
    fn test_analyze_pr_basic() {
        let store = setup_store();
        let config = PrAnalysisConfig::default();
        let report = analyze_pr(&store, &["src/auth.rs".to_string()], &config);

        assert!(!report.changed_files.is_empty());
        assert_eq!(report.changed_files[0].path, "src/auth.rs");
        assert!(report.changed_files[0].blast_radius_count > 0);
    }

    #[test]
    fn test_analyze_pr_missing_co_changes() {
        let store = setup_store();
        let config = PrAnalysisConfig::default();
        // Change auth.rs but not billing.rs (which co-changes with auth.rs)
        let report = analyze_pr(&store, &["src/auth.rs".to_string()], &config);

        assert!(
            report
                .missing_co_changes
                .iter()
                .any(|w| w.missing_file == "src/billing.rs"),
            "should warn about missing billing.rs co-change"
        );
    }

    #[test]
    fn test_analyze_pr_suggested_reviewers() {
        let store = setup_store();
        let config = PrAnalysisConfig::default();
        let report = analyze_pr(&store, &["src/auth.rs".to_string()], &config);

        assert!(
            report
                .suggested_reviewers
                .contains(&"dev@example.com".to_string()),
            "should suggest dev@example.com as reviewer"
        );
    }

    #[test]
    fn test_format_report_markdown() {
        let report = PrReport {
            changed_files: vec![FileAnalysis {
                path: "src/auth.rs".to_string(),
                blast_radius_count: 5,
                blast_radius_files: vec!["src/user.rs".to_string()],
                hotspot_score: 3,
                risk_level: RiskLevel::Medium,
            }],
            overall_risk: RiskLevel::Medium,
            suggested_reviewers: vec!["dev@example.com".to_string()],
            missing_co_changes: vec![],
        };

        let md = format_report_markdown(&report);
        assert!(md.contains("Cartograph Analysis"));
        assert!(md.contains("medium"));
        assert!(md.contains("src/auth.rs"));
        assert!(md.contains("dev@example.com"));
    }

    #[test]
    fn test_compute_file_risk_levels() {
        assert_eq!(compute_file_risk(0, 0), RiskLevel::Low);
        assert_eq!(compute_file_risk(1, 0), RiskLevel::Low);
        assert_eq!(compute_file_risk(0, 3), RiskLevel::Low);
        assert_eq!(compute_file_risk(3, 0), RiskLevel::Low);
        assert_eq!(compute_file_risk(2, 2), RiskLevel::Medium);
        assert_eq!(compute_file_risk(4, 0), RiskLevel::Medium);
        assert_eq!(compute_file_risk(0, 9), RiskLevel::Medium);
        assert_eq!(compute_file_risk(5, 5), RiskLevel::High);
        assert_eq!(compute_file_risk(10, 0), RiskLevel::High);
        assert_eq!(compute_file_risk(0, 19), RiskLevel::High);
        assert_eq!(compute_file_risk(15, 10), RiskLevel::Critical);
        assert_eq!(compute_file_risk(20, 0), RiskLevel::Critical);
        assert_eq!(compute_file_risk(10, 10), RiskLevel::Critical);
    }

    #[test]
    fn test_validate_empty_changed_files() {
        let result = validate_changed_files(&[]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must not be empty")
        );
    }

    #[test]
    fn test_validate_path_traversal_rejected() {
        let files = vec!["src/../../../etc/passwd".to_string()];
        let result = validate_changed_files(&files);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("path traversal detected")
        );
    }

    #[test]
    fn test_validate_path_too_long() {
        let long_path = "a".repeat(1025);
        let files = vec![long_path];
        let result = validate_changed_files(&files);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[test]
    fn test_validate_too_many_files() {
        let files: Vec<String> = (0..101).map(|i| format!("src/file_{}.rs", i)).collect();
        let result = validate_changed_files(&files);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("too many changed files")
        );
    }

    #[test]
    fn test_validate_valid_files() {
        let files = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "tests/integration.rs".to_string(),
        ];
        assert!(validate_changed_files(&files).is_ok());
    }

    #[test]
    fn test_format_report_json() {
        let report = PrReport {
            changed_files: vec![FileAnalysis {
                path: "src/auth.rs".to_string(),
                blast_radius_count: 5,
                blast_radius_files: vec!["src/user.rs".to_string()],
                hotspot_score: 3,
                risk_level: RiskLevel::Medium,
            }],
            overall_risk: RiskLevel::Medium,
            suggested_reviewers: vec!["dev@example.com".to_string()],
            missing_co_changes: vec![CoChangeWarning {
                changed_file: "src/auth.rs".to_string(),
                missing_file: "src/billing.rs".to_string(),
                confidence: 0.8,
            }],
        };

        let json_str = format_report_json(&report);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed["overall_risk"], "Medium");
        assert_eq!(parsed["changed_files"][0]["path"], "src/auth.rs");
        assert_eq!(parsed["changed_files"][0]["blast_radius_count"], 5);
        assert_eq!(parsed["changed_files"][0]["risk_level"], "Medium");
        assert_eq!(parsed["suggested_reviewers"][0], "dev@example.com");
        assert_eq!(
            parsed["missing_co_changes"][0]["missing_file"],
            "src/billing.rs"
        );
        assert_eq!(parsed["missing_co_changes"][0]["confidence"], 0.8);
    }

    #[test]
    fn test_format_report_json_empty_report() {
        let report = PrReport {
            changed_files: vec![],
            overall_risk: RiskLevel::Low,
            suggested_reviewers: vec![],
            missing_co_changes: vec![],
        };

        let json_str = format_report_json(&report);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed["overall_risk"], "Low");
        assert!(parsed["changed_files"].as_array().unwrap().is_empty());
    }
}
