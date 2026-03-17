use super::CiReport;

/// Format a CiReport as GitHub Actions workflow commands.
///
/// Uses `::warning::` and `::error::` annotations that GitHub Actions
/// natively renders as inline annotations on the Files Changed tab.
pub fn format_annotations(report: &CiReport) -> String {
    let mut output = String::new();

    for finding in &report.findings {
        let level = match finding.severity.as_str() {
            "critical" | "high" => "error",
            "medium" => "warning",
            _ => "notice",
        };

        let location = if let Some(line) = finding.line {
            format!("file={},line={}", finding.file, line)
        } else {
            format!("file={}", finding.file)
        };

        output.push_str(&format!(
            "::{}  {}::{}  {}\n",
            level, location, finding.rule_id, finding.message
        ));
    }

    // Summary using step summary
    output.push_str(&format!(
        "\n## Cartograph CI Summary\n- Total findings: {}\n- Max risk: {}\n",
        report.summary.total_findings, report.summary.max_risk
    ));

    for (severity, count) in &report.summary.by_severity {
        output.push_str(&format!("- {}: {}\n", severity, count));
    }

    output
}

/// Format as GitHub Actions step output (for use in workflow conditionals).
pub fn format_step_output(report: &CiReport) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "::set-output name=risk_level::{}\n",
        report.summary.max_risk
    ));
    output.push_str(&format!(
        "::set-output name=finding_count::{}\n",
        report.summary.total_findings
    ));
    output.push_str(&format!(
        "::set-output name=exit_code::{}\n",
        report.exit_code
    ));

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::cicd::{CiFinding, CiReport, CiSummary};

    fn sample_report() -> CiReport {
        CiReport {
            findings: vec![
                CiFinding {
                    file: "src/auth.rs".to_string(),
                    severity: "high".to_string(),
                    message: "High blast radius: 15 files affected".to_string(),
                    rule_id: "cartograph/high-blast-radius".to_string(),
                    line: Some(1),
                },
                CiFinding {
                    file: "src/billing.rs".to_string(),
                    severity: "medium".to_string(),
                    message: "Missing co-change with user.rs".to_string(),
                    rule_id: "cartograph/missing-co-change".to_string(),
                    line: None,
                },
            ],
            summary: CiSummary {
                total_findings: 2,
                by_severity: [("high".to_string(), 1), ("medium".to_string(), 1)]
                    .into_iter()
                    .collect(),
                max_risk: "high".to_string(),
            },
            exit_code: 1,
        }
    }

    #[test]
    fn test_format_annotations() {
        let report = sample_report();
        let output = format_annotations(&report);

        assert!(output.contains("::error"));
        assert!(output.contains("::warning"));
        assert!(output.contains("src/auth.rs"));
        assert!(output.contains("src/billing.rs"));
        assert!(output.contains("Total findings: 2"));
    }

    #[test]
    fn test_format_step_output() {
        let report = sample_report();
        let output = format_step_output(&report);

        assert!(output.contains("::set-output name=risk_level::high"));
        assert!(output.contains("::set-output name=finding_count::2"));
        assert!(output.contains("::set-output name=exit_code::1"));
    }
}
