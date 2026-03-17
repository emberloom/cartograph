use anyhow::Result;
use rusqlite::Connection;

use crate::store::graph::GraphStore;

use super::{CoverageGap, CoverageReport, FileCoverage};

/// Find coverage gaps: hotspot files with low or no test coverage.
///
/// Returns files that have high connectivity (hotspot score) but low coverage,
/// sorted by risk (highest risk first = highest connectivity + lowest coverage).
pub fn find_coverage_gaps(
    store: &GraphStore,
    conn: &Connection,
    min_connections: usize,
    max_results: usize,
) -> Result<Vec<CoverageGap>> {
    let report = super::store::read_all_coverage(conn)?;
    let coverage_map: std::collections::HashMap<&str, &FileCoverage> =
        report.files.iter().map(|f| (f.path.as_str(), f)).collect();

    let mut gaps: Vec<CoverageGap> = store
        .all_entities()
        .into_iter()
        .filter(|e| matches!(e.kind, crate::store::schema::EntityKind::File))
        .filter_map(|e| {
            let path = e.path.as_deref()?;
            let hotspot_score = store.edge_degree(&e.id);

            if hotspot_score < min_connections {
                return None;
            }

            let coverage_pct = coverage_map
                .get(path)
                .map(|c| c.line_coverage_pct.clamp(0.0, 100.0))
                .unwrap_or(0.0);

            let risk = if coverage_pct == 0.0 {
                format!(
                    "No coverage data for hotspot with {} connections",
                    hotspot_score
                )
            } else if coverage_pct < 50.0 {
                format!(
                    "Low coverage ({:.1}%) on hotspot with {} connections",
                    coverage_pct, hotspot_score
                )
            } else {
                return None; // Covered hotspots aren't gaps
            };

            Some(CoverageGap {
                entity_path: path.to_string(),
                coverage_pct,
                hotspot_score,
                risk_description: risk,
            })
        })
        .collect();

    // Sort by risk: highest connectivity with lowest coverage first
    gaps.sort_by(|a, b| {
        let risk_a = (a.hotspot_score as f64) * (100.0 - a.coverage_pct);
        let risk_b = (b.hotspot_score as f64) * (100.0 - b.coverage_pct);
        risk_b
            .partial_cmp(&risk_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    gaps.truncate(max_results);

    Ok(gaps)
}

/// Format a coverage report as a human-readable string.
pub fn format_coverage_report(report: &CoverageReport) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "Overall Coverage: {:.1}% ({}/{} lines)\n\n",
        report.overall_pct, report.total_lines_covered, report.total_lines
    ));

    if !report.files.is_empty() {
        out.push_str(&format!(
            "{:<40} {:<10} {:<10} COVERAGE\n",
            "FILE", "COVERED", "TOTAL"
        ));
        out.push_str(&"-".repeat(70));
        out.push('\n');

        for file in &report.files {
            out.push_str(&format!(
                "{:<40} {:<10} {:<10} {:.1}%\n",
                file.path, file.lines_covered, file.lines_total, file.line_coverage_pct
            ));
        }
    }

    out
}

/// Format coverage gaps as a human-readable string.
pub fn format_coverage_gaps(gaps: &[CoverageGap]) -> String {
    if gaps.is_empty() {
        return "No coverage gaps found.\n".to_string();
    }

    let mut out = format!(
        "{:<40} {:<10} {:<12} RISK\n",
        "FILE", "COVERAGE", "CONNECTIONS"
    );
    out.push_str(&"-".repeat(80));
    out.push('\n');

    for gap in gaps {
        out.push_str(&format!(
            "{:<40} {:<10.1}% {:<12} {}\n",
            gap.entity_path, gap.coverage_pct, gap.hotspot_score, gap.risk_description
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::store::{init_coverage_table, write_coverage};
    use crate::store::graph::GraphStore;
    use crate::store::schema::{EdgeKind, EntityKind};

    /// Create a GraphStore with its coverage table initialized on the same connection.
    fn setup() -> GraphStore {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::store::schema::init_db(&conn).unwrap();
        init_coverage_table(&conn).unwrap();

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

        // Make b.rs a hotspot with many edges
        store.add_edge(&a, &b, EdgeKind::Imports, 1.0).unwrap();
        store.add_edge(&c, &b, EdgeKind::Imports, 1.0).unwrap();
        store.add_edge(&b, &c, EdgeKind::DependsOn, 1.0).unwrap();
        store.add_edge(&b, &a, EdgeKind::DependsOn, 1.0).unwrap();

        // Add coverage: a.rs has good coverage, b.rs has low, c.rs has none
        write_coverage(
            store.conn(),
            &[
                FileCoverage {
                    path: "src/a.rs".to_string(),
                    lines_covered: 90,
                    lines_total: 100,
                    line_coverage_pct: 90.0,
                    covered_lines: vec![],
                    uncovered_lines: vec![],
                },
                FileCoverage {
                    path: "src/b.rs".to_string(),
                    lines_covered: 10,
                    lines_total: 100,
                    line_coverage_pct: 10.0,
                    covered_lines: vec![],
                    uncovered_lines: vec![],
                },
            ],
        )
        .unwrap();

        store
    }

    #[test]
    fn test_find_coverage_gaps() {
        let store = setup();
        let gaps = find_coverage_gaps(&store, store.conn(), 2, 10).unwrap();

        // b.rs should be flagged: hotspot (4 edges) with low coverage (10%)
        assert!(
            gaps.iter().any(|g| g.entity_path == "src/b.rs"),
            "b.rs should be a coverage gap"
        );
    }

    #[test]
    fn test_find_coverage_gaps_no_data() {
        let store = setup();
        let gaps = find_coverage_gaps(&store, store.conn(), 2, 10).unwrap();

        // c.rs has 2 edges and no coverage data -> should appear
        assert!(
            gaps.iter().any(|g| g.entity_path == "src/c.rs"),
            "c.rs should be a coverage gap (no data)"
        );
    }

    #[test]
    fn test_find_coverage_gaps_min_connections_zero() {
        let store = setup();
        // With min_connections=0, all files with coverage < 50% (or no coverage) should appear
        let gaps = find_coverage_gaps(&store, store.conn(), 0, 100).unwrap();

        // b.rs (10% coverage) and c.rs (no coverage) should both appear
        // a.rs has 90% coverage, so it should NOT appear
        assert!(
            gaps.iter().any(|g| g.entity_path == "src/b.rs"),
            "b.rs should be a coverage gap with min_connections=0"
        );
        assert!(
            gaps.iter().any(|g| g.entity_path == "src/c.rs"),
            "c.rs should be a coverage gap with min_connections=0"
        );
        assert!(
            !gaps.iter().any(|g| g.entity_path == "src/a.rs"),
            "a.rs should NOT be a coverage gap (90% coverage)"
        );
    }

    #[test]
    fn test_find_coverage_gaps_max_results_limit() {
        let store = setup();
        // Request at most 1 result
        let gaps = find_coverage_gaps(&store, store.conn(), 0, 1).unwrap();
        assert!(
            gaps.len() <= 1,
            "Should return at most 1 result, got {}",
            gaps.len()
        );
    }

    #[test]
    fn test_format_coverage_report() {
        let report = CoverageReport {
            files: vec![FileCoverage {
                path: "src/main.rs".to_string(),
                lines_covered: 50,
                lines_total: 100,
                line_coverage_pct: 50.0,
                covered_lines: vec![],
                uncovered_lines: vec![],
            }],
            total_lines_covered: 50,
            total_lines: 100,
            overall_pct: 50.0,
        };

        let output = format_coverage_report(&report);
        assert!(output.contains("50.0%"));
        assert!(output.contains("src/main.rs"));
    }

    #[test]
    fn test_format_coverage_gaps_empty() {
        let output = format_coverage_gaps(&[]);
        assert!(output.contains("No coverage gaps"));
    }
}
