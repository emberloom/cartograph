use super::{PolicyResult, Severity};

/// Format policy results as a human-readable report.
pub fn format_report(result: &PolicyResult) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "Policies checked: {}\nViolations found: {}\n\n",
        result.policies_checked,
        result.violations.len()
    ));

    if result.violations.is_empty() {
        out.push_str("All policies passed.\n");
        return out;
    }

    // Group by severity
    let errors: Vec<_> = result
        .violations
        .iter()
        .filter(|v| v.severity == Severity::Error)
        .collect();
    let warnings: Vec<_> = result
        .violations
        .iter()
        .filter(|v| v.severity == Severity::Warning)
        .collect();
    let infos: Vec<_> = result
        .violations
        .iter()
        .filter(|v| v.severity == Severity::Info)
        .collect();

    if !errors.is_empty() {
        out.push_str(&format!("Errors ({}):\n", errors.len()));
        for v in &errors {
            out.push_str(&format!(
                "  [{}] {}: {}\n",
                v.severity, v.entity_path, v.message
            ));
        }
        out.push('\n');
    }

    if !warnings.is_empty() {
        out.push_str(&format!("Warnings ({}):\n", warnings.len()));
        for v in &warnings {
            out.push_str(&format!(
                "  [{}] {}: {}\n",
                v.severity, v.entity_path, v.message
            ));
        }
        out.push('\n');
    }

    if !infos.is_empty() {
        out.push_str(&format!("Info ({}):\n", infos.len()));
        for v in &infos {
            out.push_str(&format!(
                "  [{}] {}: {}\n",
                v.severity, v.entity_path, v.message
            ));
        }
        out.push('\n');
    }

    if result.has_errors {
        out.push_str("FAILED: Policy violations with severity 'error' detected.\n");
    }

    out
}

/// Format policy results as JSON.
pub fn format_json(result: &PolicyResult) -> String {
    serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{PolicyResult, Severity, Violation};

    #[test]
    fn test_format_report_no_violations() {
        let result = PolicyResult {
            violations: vec![],
            policies_checked: 3,
            has_errors: false,
        };

        let output = format_report(&result);
        assert!(output.contains("All policies passed"));
        assert!(output.contains("Policies checked: 3"));
    }

    #[test]
    fn test_format_report_with_violations() {
        let result = PolicyResult {
            violations: vec![
                Violation {
                    policy_id: "test-policy".to_string(),
                    severity: Severity::Error,
                    entity_path: "src/main.rs".to_string(),
                    message: "Bad dependency".to_string(),
                },
                Violation {
                    policy_id: "test-warn".to_string(),
                    severity: Severity::Warning,
                    entity_path: "src/lib.rs".to_string(),
                    message: "Too many connections".to_string(),
                },
            ],
            policies_checked: 2,
            has_errors: true,
        };

        let output = format_report(&result);
        assert!(output.contains("Errors (1)"));
        assert!(output.contains("Warnings (1)"));
        assert!(output.contains("FAILED"));
        assert!(output.contains("Bad dependency"));
    }

    #[test]
    fn test_format_json() {
        let result = PolicyResult {
            violations: vec![Violation {
                policy_id: "test".to_string(),
                severity: Severity::Warning,
                entity_path: "src/main.rs".to_string(),
                message: "test message".to_string(),
            }],
            policies_checked: 1,
            has_errors: false,
        };

        let json = format_json(&result);
        assert!(json.contains("test message"));
        assert!(json.contains("\"policies_checked\": 1"));
    }
}
