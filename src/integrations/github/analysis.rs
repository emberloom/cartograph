use crate::query;
use crate::store::graph::GraphStore;

use super::{CoChangeWarning, FileAnalysis, PrAnalysisConfig, PrReport, RiskLevel};

/// Analyze a set of changed files against the Cartograph graph.
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
        assert_eq!(compute_file_risk(2, 2), RiskLevel::Medium);
        assert_eq!(compute_file_risk(5, 5), RiskLevel::High);
        assert_eq!(compute_file_risk(15, 10), RiskLevel::Critical);
    }
}
