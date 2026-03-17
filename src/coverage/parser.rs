use anyhow::{Result, bail};
use std::collections::HashMap;
use tracing::warn;

use super::FileCoverage;

/// Maximum coverage file size (100 MB).
const MAX_COVERAGE_FILE_SIZE: usize = 100 * 1024 * 1024;

/// Validate lcov content before parsing.
fn validate_lcov(content: &str) -> Result<()> {
    if content.len() > MAX_COVERAGE_FILE_SIZE {
        bail!(
            "Coverage file too large ({} bytes, max {} bytes)",
            content.len(),
            MAX_COVERAGE_FILE_SIZE
        );
    }
    Ok(())
}

/// Validate that a source file path doesn't contain path traversal.
fn validate_source_path(path: &str) -> bool {
    !path.contains("..")
}

/// Clamp a percentage to [0, 100].
fn clamp_pct(pct: f64) -> f64 {
    pct.clamp(0.0, 100.0)
}

/// Parse lcov-format coverage data.
///
/// lcov format:
/// ```text
/// SF:<file path>
/// DA:<line>,<execution count>
/// LH:<lines hit>
/// LF:<lines found>
/// end_of_record
/// ```
///
/// Malformed lines are warned and skipped rather than causing a parse failure.
pub fn parse_lcov(content: &str) -> Result<Vec<FileCoverage>> {
    validate_lcov(content)?;

    let mut results = Vec::new();
    let mut current_file: Option<String> = None;
    let mut covered_lines: Vec<u32> = Vec::new();
    let mut uncovered_lines: Vec<u32> = Vec::new();
    let mut lines_hit: u32 = 0;
    let mut lines_found: u32 = 0;

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(path) = line.strip_prefix("SF:") {
            if !validate_source_path(path) {
                warn!(
                    "Skipping file with path traversal at line {}: {}",
                    line_num + 1,
                    path
                );
                current_file = None;
                continue;
            }
            current_file = Some(path.to_string());
            covered_lines.clear();
            uncovered_lines.clear();
            lines_hit = 0;
            lines_found = 0;
        } else if let Some(data) = line.strip_prefix("DA:") {
            if current_file.is_none() {
                warn!(
                    "DA: line without preceding SF: at line {}, skipping",
                    line_num + 1
                );
                continue;
            }
            let parts: Vec<&str> = data.splitn(2, ',').collect();
            if parts.len() == 2 {
                match (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    (Ok(line_no), Ok(count)) => {
                        if count > 0 {
                            covered_lines.push(line_no);
                        } else {
                            uncovered_lines.push(line_no);
                        }
                    }
                    _ => {
                        warn!(
                            "Malformed DA: line at line {}: {}, skipping",
                            line_num + 1,
                            line
                        );
                    }
                }
            } else {
                warn!(
                    "Malformed DA: line at line {}: {}, skipping",
                    line_num + 1,
                    line
                );
            }
        } else if let Some(val) = line.strip_prefix("LH:") {
            lines_hit = val.parse().unwrap_or_else(|_| {
                warn!(
                    "Malformed LH: value at line {}: {}, defaulting to 0",
                    line_num + 1,
                    val
                );
                0
            });
        } else if let Some(val) = line.strip_prefix("LF:") {
            lines_found = val.parse().unwrap_or_else(|_| {
                warn!(
                    "Malformed LF: value at line {}: {}, defaulting to 0",
                    line_num + 1,
                    val
                );
                0
            });
        } else if line == "end_of_record" {
            if let Some(ref path) = current_file {
                let pct = if lines_found > 0 {
                    clamp_pct((lines_hit as f64 / lines_found as f64) * 100.0)
                } else {
                    0.0
                };
                results.push(FileCoverage {
                    path: path.clone(),
                    lines_covered: lines_hit,
                    lines_total: lines_found,
                    line_coverage_pct: pct,
                    covered_lines: covered_lines.clone(),
                    uncovered_lines: uncovered_lines.clone(),
                });
            }
            current_file = None;
        }
        // Other lines (TN:, FN:, FNDA:, BRDA:, etc.) are silently ignored
    }

    Ok(results)
}

/// Parse simple JSON coverage format.
///
/// Expected format:
/// ```json
/// {
///   "src/main.rs": { "lines_covered": 50, "lines_total": 100 },
///   "src/lib.rs": { "lines_covered": 80, "lines_total": 80 }
/// }
/// ```
pub fn parse_json(content: &str) -> Result<Vec<FileCoverage>> {
    let data: HashMap<String, serde_json::Value> = serde_json::from_str(content)?;
    let mut results = Vec::new();

    for (path, value) in data {
        if !validate_source_path(&path) {
            warn!("Skipping file with path traversal: {}", path);
            continue;
        }

        let lines_covered = value
            .get("lines_covered")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("missing lines_covered for {}", path))?
            as u32;
        let lines_total = value
            .get("lines_total")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("missing lines_total for {}", path))?
            as u32;

        let pct = if lines_total > 0 {
            clamp_pct((lines_covered as f64 / lines_total as f64) * 100.0)
        } else {
            0.0
        };

        results.push(FileCoverage {
            path,
            lines_covered,
            lines_total,
            line_coverage_pct: pct,
            covered_lines: Vec::new(),
            uncovered_lines: Vec::new(),
        });
    }

    Ok(results)
}

/// Detect the coverage format from content.
pub fn detect_format(content: &str) -> Result<&'static str> {
    let trimmed = content.trim();
    if trimmed.starts_with('{') {
        Ok("json")
    } else if trimmed.contains("SF:") || trimmed.contains("end_of_record") {
        Ok("lcov")
    } else {
        bail!("Unable to detect coverage format. Use --format to specify.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lcov() {
        let lcov = "\
SF:src/main.rs
DA:1,1
DA:2,1
DA:3,0
DA:4,1
LH:3
LF:4
end_of_record
SF:src/lib.rs
DA:1,1
DA:2,0
LH:1
LF:2
end_of_record
";
        let result = parse_lcov(lcov).unwrap();
        assert_eq!(result.len(), 2);

        let main = &result[0];
        assert_eq!(main.path, "src/main.rs");
        assert_eq!(main.lines_covered, 3);
        assert_eq!(main.lines_total, 4);
        assert!((main.line_coverage_pct - 75.0).abs() < 0.1);
        assert_eq!(main.covered_lines, vec![1, 2, 4]);
        assert_eq!(main.uncovered_lines, vec![3]);

        let lib = &result[1];
        assert_eq!(lib.path, "src/lib.rs");
        assert_eq!(lib.lines_covered, 1);
        assert_eq!(lib.lines_total, 2);
    }

    #[test]
    fn test_parse_json() {
        let json = r#"{
            "src/main.rs": { "lines_covered": 50, "lines_total": 100 },
            "src/lib.rs": { "lines_covered": 80, "lines_total": 80 }
        }"#;
        let result = parse_json(json).unwrap();
        assert_eq!(result.len(), 2);

        let main = result.iter().find(|f| f.path == "src/main.rs").unwrap();
        assert_eq!(main.lines_covered, 50);
        assert_eq!(main.lines_total, 100);
        assert!((main.line_coverage_pct - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_lcov_empty() {
        let result = parse_lcov("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_detect_format_json() {
        assert_eq!(detect_format(r#"{ "src/main.rs": {} }"#).unwrap(), "json");
    }

    #[test]
    fn test_detect_format_lcov() {
        assert_eq!(
            detect_format("SF:src/main.rs\nend_of_record").unwrap(),
            "lcov"
        );
    }

    #[test]
    fn test_detect_format_unknown() {
        assert!(detect_format("random content").is_err());
    }

    #[test]
    fn test_parse_lcov_malformed_input() {
        // DA: without preceding SF: should be skipped
        let lcov = "\
DA:1,1
DA:2,0
end_of_record
";
        let result = parse_lcov(lcov).unwrap();
        assert!(result.is_empty(), "Records without SF: should be skipped");

        // Malformed DA: lines (non-numeric) should be skipped
        let lcov = "\
SF:src/main.rs
DA:abc,1
DA:1,xyz
DA:1,1
LH:1
LF:1
end_of_record
";
        let result = parse_lcov(lcov).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].covered_lines, vec![1]);

        // Missing comma in DA: line
        let lcov = "\
SF:src/main.rs
DA:1
DA:2,1
LH:1
LF:2
end_of_record
";
        let result = parse_lcov(lcov).unwrap();
        assert_eq!(result.len(), 1);
        // Only DA:2,1 should be parsed
        assert_eq!(result[0].covered_lines, vec![2]);
    }

    #[test]
    fn test_parse_lcov_zero_coverage() {
        let lcov = "\
SF:src/uncovered.rs
DA:1,0
DA:2,0
DA:3,0
DA:4,0
LH:0
LF:4
end_of_record
";
        let result = parse_lcov(lcov).unwrap();
        assert_eq!(result.len(), 1);
        let file = &result[0];
        assert_eq!(file.path, "src/uncovered.rs");
        assert_eq!(file.lines_covered, 0);
        assert_eq!(file.lines_total, 4);
        assert!((file.line_coverage_pct - 0.0).abs() < 0.01);
        assert!(file.covered_lines.is_empty());
        assert_eq!(file.uncovered_lines, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_parse_lcov_path_traversal_rejected() {
        let lcov = "\
SF:../../../etc/passwd
DA:1,1
LH:1
LF:1
end_of_record
SF:src/safe.rs
DA:1,1
LH:1
LF:1
end_of_record
";
        let result = parse_lcov(lcov).unwrap();
        // Only src/safe.rs should be included
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "src/safe.rs");
    }

    #[test]
    fn test_parse_json_path_traversal_rejected() {
        let json = r#"{
            "../../../etc/passwd": { "lines_covered": 1, "lines_total": 1 },
            "src/safe.rs": { "lines_covered": 50, "lines_total": 100 }
        }"#;
        let result = parse_json(json).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "src/safe.rs");
    }

    #[test]
    fn test_coverage_pct_clamped() {
        // lines_covered > lines_total should still clamp to 100%
        let json = r#"{
            "src/main.rs": { "lines_covered": 150, "lines_total": 100 }
        }"#;
        let result = parse_json(json).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].line_coverage_pct <= 100.0,
            "Coverage percentage should be clamped to 100"
        );
    }
}
