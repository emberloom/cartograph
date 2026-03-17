use anyhow::{Result, bail};
use std::collections::HashMap;

use super::FileCoverage;

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
pub fn parse_lcov(content: &str) -> Result<Vec<FileCoverage>> {
    let mut results = Vec::new();
    let mut current_file: Option<String> = None;
    let mut covered_lines: Vec<u32> = Vec::new();
    let mut uncovered_lines: Vec<u32> = Vec::new();
    let mut lines_hit: u32 = 0;
    let mut lines_found: u32 = 0;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(path) = line.strip_prefix("SF:") {
            current_file = Some(path.to_string());
            covered_lines.clear();
            uncovered_lines.clear();
            lines_hit = 0;
            lines_found = 0;
        } else if let Some(data) = line.strip_prefix("DA:") {
            let parts: Vec<&str> = data.splitn(2, ',').collect();
            if parts.len() == 2
                && let (Ok(line_num), Ok(count)) =
                    (parts[0].parse::<u32>(), parts[1].parse::<u32>())
            {
                if count > 0 {
                    covered_lines.push(line_num);
                } else {
                    uncovered_lines.push(line_num);
                }
            }
        } else if let Some(val) = line.strip_prefix("LH:") {
            lines_hit = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("LF:") {
            lines_found = val.parse().unwrap_or(0);
        } else if line == "end_of_record" {
            if let Some(ref path) = current_file {
                let pct = if lines_found > 0 {
                    (lines_hit as f64 / lines_found as f64) * 100.0
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
            (lines_covered as f64 / lines_total as f64) * 100.0
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
}
