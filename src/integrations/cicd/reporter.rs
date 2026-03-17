use crate::integrations::github::RiskLevel;
use crate::query;
use crate::store::graph::GraphStore;

use super::{CiFinding, CiReport, CiSummary, FailThreshold};

/// Generate a CI report for the given changed files.
pub fn generate_report(
    store: &GraphStore,
    changed_files: &[String],
    fail_threshold: FailThreshold,
) -> CiReport {
    let mut findings = Vec::new();
    let mut max_risk = RiskLevel::Low;

    for file_path in changed_files {
        // Check blast radius
        let blast = query::blast_radius::query(store, file_path, 2);
        let blast_count = blast.len();

        if blast_count >= 10 {
            let risk = if blast_count >= 20 {
                RiskLevel::Critical
            } else {
                RiskLevel::High
            };
            if risk > max_risk {
                max_risk = risk;
            }
            findings.push(CiFinding {
                file: file_path.clone(),
                severity: format!("{}", risk),
                message: format!(
                    "High blast radius: {} files affected by changes to {}",
                    blast_count, file_path
                ),
                rule_id: "cartograph/high-blast-radius".to_string(),
                line: None,
            });
        }

        // Check for missing co-changes
        let co = query::co_changes(store, file_path);
        let changed_set: std::collections::HashSet<&str> =
            changed_files.iter().map(|s| s.as_str()).collect();

        for result in &co {
            if let Some(ref path) = result.entity_path
                && !changed_set.contains(path.as_str())
                && result.confidence > 0.5
            {
                let risk = RiskLevel::Medium;
                if risk > max_risk {
                    max_risk = risk;
                }
                findings.push(CiFinding {
                    file: file_path.clone(),
                    severity: format!("{}", risk),
                    message: format!(
                        "Missing co-change: {} usually changes with {} (confidence: {:.0}%)",
                        path,
                        file_path,
                        result.confidence * 100.0
                    ),
                    rule_id: "cartograph/missing-co-change".to_string(),
                    line: None,
                });
            }
        }

        // Check hotspot status
        let hotspot_score = store
            .find_entity_by_path(file_path)
            .map(|e| store.edge_degree(&e.id))
            .unwrap_or(0);

        if hotspot_score >= 8 {
            let risk = RiskLevel::Medium;
            if risk > max_risk {
                max_risk = risk;
            }
            findings.push(CiFinding {
                file: file_path.clone(),
                severity: format!("{}", risk),
                message: format!(
                    "Hotspot alert: {} has {} connections (high change surface area)",
                    file_path, hotspot_score
                ),
                rule_id: "cartograph/hotspot-change".to_string(),
                line: None,
            });
        }
    }

    // Compute summary
    let mut by_severity: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for f in &findings {
        *by_severity.entry(f.severity.clone()).or_insert(0) += 1;
    }

    let exit_code = if fail_threshold.should_fail(&max_risk) {
        1
    } else {
        0
    };

    CiReport {
        summary: CiSummary {
            total_findings: findings.len(),
            by_severity,
            max_risk: format!("{}", max_risk),
        },
        findings,
        exit_code,
    }
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

        // Create a chain of files to generate blast radius
        let files: Vec<String> = (0..15)
            .map(|i| {
                store
                    .add_entity(
                        EntityKind::File,
                        &format!("f{}.rs", i),
                        Some(&format!("src/f{}.rs", i)),
                        Some("rust"),
                    )
                    .unwrap()
            })
            .collect();

        // Star: f0 -> f1, f0 -> f2, ..., f0 -> f14 (all direct deps)
        for i in 1..15 {
            store
                .add_edge(&files[0], &files[i], EdgeKind::Imports, 1.0)
                .unwrap();
        }

        // Add co-change
        store
            .add_edge(&files[0], &files[5], EdgeKind::CoChangesWith, 0.8)
            .unwrap();

        store
    }

    #[test]
    fn test_generate_report_high_blast_radius() {
        let store = setup_store();
        let report = generate_report(&store, &["src/f0.rs".to_string()], FailThreshold::None);

        // f0 has blast radius of 14 files, should trigger high-blast-radius finding
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "cartograph/high-blast-radius"),
            "should detect high blast radius"
        );
    }

    #[test]
    fn test_generate_report_missing_co_change() {
        let store = setup_store();
        let report = generate_report(&store, &["src/f0.rs".to_string()], FailThreshold::None);

        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "cartograph/missing-co-change"),
            "should detect missing co-change with f5"
        );
    }

    #[test]
    fn test_generate_report_exit_code() {
        let store = setup_store();

        let report_no_fail =
            generate_report(&store, &["src/f0.rs".to_string()], FailThreshold::None);
        assert_eq!(report_no_fail.exit_code, 0);

        let report_fail =
            generate_report(&store, &["src/f0.rs".to_string()], FailThreshold::Medium);
        // Should fail because there are medium+ findings
        assert_eq!(report_fail.exit_code, 1);
    }
}
