use crate::integrations::github::RiskLevel;
use crate::store::graph::GraphStore;
use crate::store::schema::EntityKind;

use super::signals;
use super::{PredictionConfig, RiskScore, SignalContribution};

/// Predict regression risk for all entities in the graph based on a set of changed files.
///
/// Returns a sorted list of risk scores (highest risk first), excluding the
/// changed files themselves.
pub fn predict_regressions(
    store: &GraphStore,
    changed_files: &[String],
    config: &PredictionConfig,
) -> Vec<RiskScore> {
    let changed_set: std::collections::HashSet<&str> =
        changed_files.iter().map(|s| s.as_str()).collect();

    // Get all file entities as candidates
    let candidates: Vec<String> = store
        .all_entities()
        .into_iter()
        .filter(|e| e.kind == EntityKind::File)
        .filter_map(|e| e.path)
        .filter(|p| !changed_set.contains(p.as_str()))
        .collect();

    let mut scores: Vec<RiskScore> = candidates
        .iter()
        .filter_map(|candidate| {
            let structural = signals::structural_signal(store, changed_files, candidate, 3);
            let cochange = signals::cochange_signal(store, changed_files, candidate);
            let hotspot = signals::hotspot_signal(store, candidate);
            let ownership = signals::ownership_signal(store, candidate);

            let w = &config.weights;
            let weighted_structural = structural * w.structural;
            let weighted_cochange = cochange * w.cochange;
            let weighted_hotspot = hotspot * w.hotspot;
            let weighted_ownership = ownership * w.ownership;

            let total =
                weighted_structural + weighted_cochange + weighted_hotspot + weighted_ownership;

            if total < config.min_score_threshold {
                return None;
            }

            let risk_level = score_to_risk_level(total);

            Some(RiskScore {
                entity_path: candidate.clone(),
                score: total,
                signals: vec![
                    SignalContribution {
                        signal_name: "structural".to_string(),
                        raw_value: structural,
                        weighted_value: weighted_structural,
                    },
                    SignalContribution {
                        signal_name: "cochange".to_string(),
                        raw_value: cochange,
                        weighted_value: weighted_cochange,
                    },
                    SignalContribution {
                        signal_name: "hotspot".to_string(),
                        raw_value: hotspot,
                        weighted_value: weighted_hotspot,
                    },
                    SignalContribution {
                        signal_name: "ownership".to_string(),
                        raw_value: ownership,
                        weighted_value: weighted_ownership,
                    },
                ],
                risk_level,
            })
        })
        .collect();

    // Sort by score descending
    scores.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Limit results
    scores.truncate(config.max_results);

    scores
}

/// Map a numeric score to a risk level.
fn score_to_risk_level(score: f64) -> RiskLevel {
    if score >= 0.7 {
        RiskLevel::Critical
    } else if score >= 0.5 {
        RiskLevel::High
    } else if score >= 0.25 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

/// Format predictions as a human-readable table.
pub fn format_predictions(predictions: &[RiskScore]) -> String {
    if predictions.is_empty() {
        return "No regression risk predicted for the given changes.\n".to_string();
    }

    let mut out = format!("{:<40} {:<10} {:<10} TOP SIGNAL\n", "FILE", "SCORE", "RISK");
    out.push_str(&"-".repeat(75));
    out.push('\n');

    for pred in predictions {
        let top_signal = pred
            .signals
            .iter()
            .max_by(|a, b| {
                a.weighted_value
                    .partial_cmp(&b.weighted_value)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|s| s.signal_name.as_str())
            .unwrap_or("none");

        out.push_str(&format!(
            "{:<40} {:<10.3} {:<10} {}\n",
            pred.entity_path, pred.score, pred.risk_level, top_signal
        ));
    }

    out
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
            .add_entity(EntityKind::File, "a.rs", Some("src/a.rs"), Some("rust"))
            .unwrap();
        let b = store
            .add_entity(EntityKind::File, "b.rs", Some("src/b.rs"), Some("rust"))
            .unwrap();
        let c = store
            .add_entity(EntityKind::File, "c.rs", Some("src/c.rs"), Some("rust"))
            .unwrap();
        let d = store
            .add_entity(EntityKind::File, "d.rs", Some("src/d.rs"), Some("rust"))
            .unwrap();

        store.add_edge(&a, &b, EdgeKind::Imports, 1.0).unwrap();
        store.add_edge(&b, &c, EdgeKind::Imports, 1.0).unwrap();
        store
            .add_edge(&a, &d, EdgeKind::CoChangesWith, 0.8)
            .unwrap();

        store
    }

    #[test]
    fn test_predict_regressions() {
        let store = setup_store();
        let config = PredictionConfig::default();
        let predictions = predict_regressions(&store, &["src/a.rs".to_string()], &config);

        // b.rs should be high risk (direct structural dependency)
        // d.rs should have risk (co-change signal)
        assert!(!predictions.is_empty(), "should predict some regressions");

        // b.rs should be found
        assert!(
            predictions.iter().any(|p| p.entity_path == "src/b.rs"),
            "b.rs should be in predictions (structural dependency)"
        );
    }

    #[test]
    fn test_predict_excludes_changed_files() {
        let store = setup_store();
        let config = PredictionConfig::default();
        let predictions = predict_regressions(&store, &["src/a.rs".to_string()], &config);

        assert!(
            !predictions.iter().any(|p| p.entity_path == "src/a.rs"),
            "changed file should not appear in predictions"
        );
    }

    #[test]
    fn test_predict_sorted_by_score() {
        let store = setup_store();
        let config = PredictionConfig::default();
        let predictions = predict_regressions(&store, &["src/a.rs".to_string()], &config);

        for window in predictions.windows(2) {
            assert!(
                window[0].score >= window[1].score,
                "predictions should be sorted by score descending"
            );
        }
    }

    #[test]
    fn test_score_to_risk_level() {
        assert_eq!(score_to_risk_level(0.1), RiskLevel::Low);
        assert_eq!(score_to_risk_level(0.3), RiskLevel::Medium);
        assert_eq!(score_to_risk_level(0.6), RiskLevel::High);
        assert_eq!(score_to_risk_level(0.8), RiskLevel::Critical);
    }

    #[test]
    fn test_format_predictions() {
        let store = setup_store();
        let config = PredictionConfig::default();
        let predictions = predict_regressions(&store, &["src/a.rs".to_string()], &config);
        let output = format_predictions(&predictions);

        assert!(output.contains("FILE"));
        assert!(output.contains("SCORE"));
        assert!(output.contains("RISK"));
    }

    #[test]
    fn test_format_predictions_empty() {
        let output = format_predictions(&[]);
        assert!(output.contains("No regression risk"));
    }
}
