use serde_json::{Value, json};

use super::CiReport;

/// The SARIF 2.1.0 JSON schema URL.
pub const SARIF_SCHEMA_URL: &str = "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json";

/// Convert a CiReport to SARIF 2.1.0 format.
///
/// SARIF (Static Analysis Results Interchange Format) is an OASIS standard
/// supported by GitHub Code Scanning, VS Code, and other tools.
pub fn to_sarif(report: &CiReport) -> Value {
    let results: Vec<Value> = report
        .findings
        .iter()
        .map(|f| {
            let level = match f.severity.as_str() {
                "critical" | "high" => "error",
                "medium" => "warning",
                _ => "note",
            };

            let mut location = json!({
                "physicalLocation": {
                    "artifactLocation": {
                        "uri": &f.file
                    }
                }
            });

            if let Some(line) = f.line {
                location["physicalLocation"]["region"] = json!({ "startLine": line });
            }

            json!({
                "ruleId": &f.rule_id,
                "level": level,
                "message": {
                    "text": &f.message
                },
                "locations": [location]
            })
        })
        .collect();

    // Collect unique rule IDs for the rules array
    let mut seen_rules: std::collections::HashSet<String> = std::collections::HashSet::new();
    let rules: Vec<Value> = report
        .findings
        .iter()
        .filter(|f| seen_rules.insert(f.rule_id.clone()))
        .map(|f| {
            let desc = match f.rule_id.as_str() {
                "cartograph/high-blast-radius" => {
                    "File changes affect a large number of dependent files"
                }
                "cartograph/missing-co-change" => {
                    "A file that usually changes together with the modified file was not included"
                }
                "cartograph/hotspot-change" => "A highly-connected hotspot file was modified",
                _ => "Cartograph analysis finding",
            };

            json!({
                "id": &f.rule_id,
                "shortDescription": {
                    "text": desc
                }
            })
        })
        .collect();

    json!({
        "$schema": SARIF_SCHEMA_URL,
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "cartograph",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/emberloom/cartograph",
                    "rules": rules
                }
            },
            "results": results
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::cicd::{CiFinding, CiReport, CiSummary};

    #[test]
    fn test_sarif_output_structure() {
        let report = CiReport {
            findings: vec![CiFinding {
                file: "src/main.rs".to_string(),
                severity: "high".to_string(),
                message: "High blast radius: 15 files affected".to_string(),
                rule_id: "cartograph/high-blast-radius".to_string(),
                line: Some(10),
            }],
            summary: CiSummary {
                total_findings: 1,
                by_severity: [("high".to_string(), 1)].into_iter().collect(),
                max_risk: "high".to_string(),
            },
            exit_code: 1,
        };

        let sarif = to_sarif(&report);

        assert_eq!(sarif["version"], "2.1.0");
        assert_eq!(sarif["runs"][0]["tool"]["driver"]["name"], "cartograph");
        assert_eq!(sarif["runs"][0]["results"][0]["level"], "error");
        assert_eq!(
            sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]
                ["uri"],
            "src/main.rs"
        );
        assert_eq!(
            sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"]["startLine"],
            10
        );
    }

    #[test]
    fn test_sarif_schema_url() {
        let report = CiReport {
            findings: vec![],
            summary: CiSummary {
                total_findings: 0,
                by_severity: std::collections::HashMap::new(),
                max_risk: "low".to_string(),
            },
            exit_code: 0,
        };

        let sarif = to_sarif(&report);
        assert_eq!(
            sarif["$schema"].as_str().unwrap(),
            "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json"
        );
    }

    #[test]
    fn test_sarif_severity_mapping() {
        let make_finding = |severity: &str| CiFinding {
            file: "test.rs".to_string(),
            severity: severity.to_string(),
            message: "test".to_string(),
            rule_id: "test/rule".to_string(),
            line: None,
        };

        let report = CiReport {
            findings: vec![
                make_finding("critical"),
                make_finding("high"),
                make_finding("medium"),
                make_finding("low"),
            ],
            summary: CiSummary {
                total_findings: 4,
                by_severity: std::collections::HashMap::new(),
                max_risk: "critical".to_string(),
            },
            exit_code: 1,
        };

        let sarif = to_sarif(&report);
        let results = sarif["runs"][0]["results"].as_array().unwrap();

        assert_eq!(results[0]["level"], "error"); // critical
        assert_eq!(results[1]["level"], "error"); // high
        assert_eq!(results[2]["level"], "warning"); // medium
        assert_eq!(results[3]["level"], "note"); // low
    }

    #[test]
    fn test_sarif_empty_report() {
        let report = CiReport {
            findings: vec![],
            summary: CiSummary {
                total_findings: 0,
                by_severity: std::collections::HashMap::new(),
                max_risk: "low".to_string(),
            },
            exit_code: 0,
        };

        let sarif = to_sarif(&report);
        assert!(sarif["runs"][0]["results"].as_array().unwrap().is_empty());
        assert!(
            sarif["runs"][0]["tool"]["driver"]["rules"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn test_sarif_rule_deduplication() {
        // Two findings with the same rule_id should produce only one rule entry
        let report = CiReport {
            findings: vec![
                CiFinding {
                    file: "src/a.rs".to_string(),
                    severity: "high".to_string(),
                    message: "Blast radius for a.rs".to_string(),
                    rule_id: "cartograph/high-blast-radius".to_string(),
                    line: None,
                },
                CiFinding {
                    file: "src/b.rs".to_string(),
                    severity: "high".to_string(),
                    message: "Blast radius for b.rs".to_string(),
                    rule_id: "cartograph/high-blast-radius".to_string(),
                    line: None,
                },
                CiFinding {
                    file: "src/c.rs".to_string(),
                    severity: "medium".to_string(),
                    message: "Missing co-change".to_string(),
                    rule_id: "cartograph/missing-co-change".to_string(),
                    line: None,
                },
            ],
            summary: CiSummary {
                total_findings: 3,
                by_severity: [("high".to_string(), 2), ("medium".to_string(), 1)]
                    .into_iter()
                    .collect(),
                max_risk: "high".to_string(),
            },
            exit_code: 1,
        };

        let sarif = to_sarif(&report);
        let rules = sarif["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .unwrap();
        // Should have 2 unique rules, not 3
        assert_eq!(rules.len(), 2);
        let results = sarif["runs"][0]["results"].as_array().unwrap();
        // But all 3 results should be present
        assert_eq!(results.len(), 3);
    }
}
