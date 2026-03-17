use anyhow::Result;
use rusqlite::Connection;

use super::{CoverageReport, FileCoverage};

/// Initialize the coverage table in the database.
pub fn init_coverage_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS coverage (
            file_path TEXT PRIMARY KEY,
            lines_covered INTEGER NOT NULL DEFAULT 0,
            lines_total INTEGER NOT NULL DEFAULT 0,
            covered_lines TEXT NOT NULL DEFAULT '[]',
            uncovered_lines TEXT NOT NULL DEFAULT '[]',
            imported_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;
    Ok(())
}

/// Write coverage data to the database, replacing existing entries.
pub fn write_coverage(conn: &Connection, files: &[FileCoverage]) -> Result<usize> {
    init_coverage_table(conn)?;

    let mut count = 0;
    for file in files {
        let covered_json = serde_json::to_string(&file.covered_lines)?;
        let uncovered_json = serde_json::to_string(&file.uncovered_lines)?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT OR REPLACE INTO coverage (file_path, lines_covered, lines_total, covered_lines, uncovered_lines, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                file.path,
                file.lines_covered,
                file.lines_total,
                covered_json,
                uncovered_json,
                now,
            ],
        )?;
        count += 1;
    }

    Ok(count)
}

/// Read coverage for a specific file.
pub fn read_coverage(conn: &Connection, file_path: &str) -> Result<Option<FileCoverage>> {
    init_coverage_table(conn)?;

    let mut stmt = conn.prepare(
        "SELECT file_path, lines_covered, lines_total, covered_lines, uncovered_lines FROM coverage WHERE file_path = ?1"
    )?;

    let result = stmt.query_row(rusqlite::params![file_path], |row| {
        let path: String = row.get(0)?;
        let lines_covered: u32 = row.get(1)?;
        let lines_total: u32 = row.get(2)?;
        let covered_json: String = row.get(3)?;
        let uncovered_json: String = row.get(4)?;
        Ok((
            path,
            lines_covered,
            lines_total,
            covered_json,
            uncovered_json,
        ))
    });

    match result {
        Ok((path, lines_covered, lines_total, covered_json, uncovered_json)) => {
            let covered_lines: Vec<u32> = serde_json::from_str(&covered_json).unwrap_or_default();
            let uncovered_lines: Vec<u32> =
                serde_json::from_str(&uncovered_json).unwrap_or_default();
            let pct = if lines_total > 0 {
                (lines_covered as f64 / lines_total as f64) * 100.0
            } else {
                0.0
            };
            Ok(Some(FileCoverage {
                path,
                lines_covered,
                lines_total,
                line_coverage_pct: pct,
                covered_lines,
                uncovered_lines,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Read all coverage data and compute an aggregated report.
pub fn read_all_coverage(conn: &Connection) -> Result<CoverageReport> {
    init_coverage_table(conn)?;

    let mut stmt = conn.prepare(
        "SELECT file_path, lines_covered, lines_total, covered_lines, uncovered_lines FROM coverage ORDER BY file_path"
    )?;

    let files: Vec<FileCoverage> = stmt
        .query_map([], |row| {
            let path: String = row.get(0)?;
            let lines_covered: u32 = row.get(1)?;
            let lines_total: u32 = row.get(2)?;
            let covered_json: String = row.get(3)?;
            let uncovered_json: String = row.get(4)?;
            Ok((
                path,
                lines_covered,
                lines_total,
                covered_json,
                uncovered_json,
            ))
        })?
        .filter_map(|r| r.ok())
        .map(
            |(path, lines_covered, lines_total, covered_json, uncovered_json)| {
                let covered_lines: Vec<u32> =
                    serde_json::from_str(&covered_json).unwrap_or_default();
                let uncovered_lines: Vec<u32> =
                    serde_json::from_str(&uncovered_json).unwrap_or_default();
                let pct = if lines_total > 0 {
                    (lines_covered as f64 / lines_total as f64) * 100.0
                } else {
                    0.0
                };
                FileCoverage {
                    path,
                    lines_covered,
                    lines_total,
                    line_coverage_pct: pct,
                    covered_lines,
                    uncovered_lines,
                }
            },
        )
        .collect();

    let total_covered: u32 = files.iter().map(|f| f.lines_covered).sum();
    let total_lines: u32 = files.iter().map(|f| f.lines_total).sum();
    let overall_pct = if total_lines > 0 {
        (total_covered as f64 / total_lines as f64) * 100.0
    } else {
        0.0
    };

    Ok(CoverageReport {
        files,
        total_lines_covered: total_covered,
        total_lines,
        overall_pct,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_coverage_table(&conn).unwrap();
        conn
    }

    #[test]
    fn test_write_and_read_coverage() {
        let conn = test_conn();
        let files = vec![FileCoverage {
            path: "src/main.rs".to_string(),
            lines_covered: 50,
            lines_total: 100,
            line_coverage_pct: 50.0,
            covered_lines: vec![1, 2, 3],
            uncovered_lines: vec![4, 5],
        }];

        let count = write_coverage(&conn, &files).unwrap();
        assert_eq!(count, 1);

        let result = read_coverage(&conn, "src/main.rs").unwrap();
        assert!(result.is_some());
        let cov = result.unwrap();
        assert_eq!(cov.lines_covered, 50);
        assert_eq!(cov.lines_total, 100);
        assert_eq!(cov.covered_lines, vec![1, 2, 3]);
    }

    #[test]
    fn test_read_coverage_not_found() {
        let conn = test_conn();
        let result = read_coverage(&conn, "nonexistent.rs").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_read_all_coverage() {
        let conn = test_conn();
        let files = vec![
            FileCoverage {
                path: "src/a.rs".to_string(),
                lines_covered: 80,
                lines_total: 100,
                line_coverage_pct: 80.0,
                covered_lines: vec![],
                uncovered_lines: vec![],
            },
            FileCoverage {
                path: "src/b.rs".to_string(),
                lines_covered: 20,
                lines_total: 50,
                line_coverage_pct: 40.0,
                covered_lines: vec![],
                uncovered_lines: vec![],
            },
        ];

        write_coverage(&conn, &files).unwrap();
        let report = read_all_coverage(&conn).unwrap();

        assert_eq!(report.files.len(), 2);
        assert_eq!(report.total_lines_covered, 100);
        assert_eq!(report.total_lines, 150);
        assert!((report.overall_pct - 66.67).abs() < 0.1);
    }

    #[test]
    fn test_write_coverage_upsert() {
        let conn = test_conn();
        let files1 = vec![FileCoverage {
            path: "src/main.rs".to_string(),
            lines_covered: 50,
            lines_total: 100,
            line_coverage_pct: 50.0,
            covered_lines: vec![],
            uncovered_lines: vec![],
        }];
        write_coverage(&conn, &files1).unwrap();

        // Update with new data
        let files2 = vec![FileCoverage {
            path: "src/main.rs".to_string(),
            lines_covered: 80,
            lines_total: 100,
            line_coverage_pct: 80.0,
            covered_lines: vec![],
            uncovered_lines: vec![],
        }];
        write_coverage(&conn, &files2).unwrap();

        let cov = read_coverage(&conn, "src/main.rs").unwrap().unwrap();
        assert_eq!(cov.lines_covered, 80);
    }
}
