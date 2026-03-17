use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::Severity;

/// A complete policy configuration file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub policies: Vec<Policy>,
}

/// A single policy definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub id: String,
    pub description: String,
    pub rule: Rule,
    pub severity: Severity,
}

/// The different types of rules that can be evaluated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Rule {
    /// Assert no dependency edge exists from files matching `from` to files matching `to`.
    NoDependency { from: String, to: String },

    /// Flag files matching `pattern` that exceed `threshold` connections.
    MaxConnections { pattern: String, threshold: usize },

    /// Assert files matching `pattern` have at least one edge of kind `edge_kind`.
    HasEdge { pattern: String, edge_kind: String },

    /// Define ordered layers and enforce that dependencies only flow downward.
    LayerBoundary { layers: Vec<LayerDef> },
}

/// A layer in a layer boundary policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerDef {
    pub name: String,
    pub pattern: String,
}

/// Validation error for a policy configuration.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub policy_id: String,
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "policy '{}': {}", self.policy_id, self.message)
    }
}

impl PolicyConfig {
    /// Validate the policy configuration, returning a list of errors for invalid configs.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        for policy in &self.policies {
            if policy.id.is_empty() {
                errors.push(ValidationError {
                    policy_id: "(empty)".to_string(),
                    message: "policy id must not be empty".to_string(),
                });
            }

            match &policy.rule {
                Rule::NoDependency { from, to } => {
                    if from.is_empty() {
                        errors.push(ValidationError {
                            policy_id: policy.id.clone(),
                            message: "'from' pattern must not be empty".to_string(),
                        });
                    }
                    if to.is_empty() {
                        errors.push(ValidationError {
                            policy_id: policy.id.clone(),
                            message: "'to' pattern must not be empty".to_string(),
                        });
                    }
                }
                Rule::MaxConnections { pattern, threshold } => {
                    if pattern.is_empty() {
                        errors.push(ValidationError {
                            policy_id: policy.id.clone(),
                            message: "'pattern' must not be empty".to_string(),
                        });
                    }
                    if *threshold == 0 {
                        errors.push(ValidationError {
                            policy_id: policy.id.clone(),
                            message: "'threshold' must be greater than 0".to_string(),
                        });
                    }
                }
                Rule::HasEdge { pattern, edge_kind } => {
                    if pattern.is_empty() {
                        errors.push(ValidationError {
                            policy_id: policy.id.clone(),
                            message: "'pattern' must not be empty".to_string(),
                        });
                    }
                    if edge_kind.is_empty() {
                        errors.push(ValidationError {
                            policy_id: policy.id.clone(),
                            message: "'edge_kind' must not be empty".to_string(),
                        });
                    }
                }
                Rule::LayerBoundary { layers } => {
                    if layers.len() < 2 {
                        errors.push(ValidationError {
                            policy_id: policy.id.clone(),
                            message: "layer_boundary requires at least 2 layers".to_string(),
                        });
                    }
                    for layer in layers {
                        if layer.name.is_empty() {
                            errors.push(ValidationError {
                                policy_id: policy.id.clone(),
                                message: "layer name must not be empty".to_string(),
                            });
                        }
                        if layer.pattern.is_empty() {
                            errors.push(ValidationError {
                                policy_id: policy.id.clone(),
                                message: format!(
                                    "layer '{}' pattern must not be empty",
                                    layer.name
                                ),
                            });
                        }
                    }
                }
            }
        }

        errors
    }
}

/// Parse a policy configuration from YAML.
pub fn parse_policy_config(yaml: &str) -> Result<PolicyConfig> {
    let config: PolicyConfig = serde_yaml_ng::from_str(yaml)?;
    Ok(config)
}

/// Generate a starter policy configuration.
pub fn generate_starter_config() -> String {
    r#"# Cartograph Policy Configuration
# See: https://github.com/emberloom/cartograph/docs/specs/policy-engine.md

policies:
  - id: max-blast-radius
    description: "Flag files with more than 10 connections (high blast radius)"
    rule:
      type: max_connections
      pattern: "src/**"
      threshold: 10
    severity: warning

  - id: no-server-to-parser
    description: "Server module should not depend on parser internals"
    rule:
      type: no_dependency
      from: "src/server/**"
      to: "src/parser/**"
    severity: error

  - id: ownership-required
    description: "All source files should have an identified owner"
    rule:
      type: has_edge
      pattern: "src/**/*.rs"
      edge_kind: owned_by
    severity: info

  - id: layer-boundaries
    description: "Enforce architectural layering"
    rule:
      type: layer_boundary
      layers:
        - name: presentation
          pattern: "src/server/**"
        - name: domain
          pattern: "src/query/**"
        - name: infrastructure
          pattern: "src/store/**"
    severity: error
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_policy_config() {
        let yaml = r#"
policies:
  - id: test-rule
    description: "Test rule"
    rule:
      type: max_connections
      pattern: "src/**"
      threshold: 5
    severity: warning
  - id: no-dep
    description: "No dependency"
    rule:
      type: no_dependency
      from: "src/a/**"
      to: "src/b/**"
    severity: error
"#;
        let config = parse_policy_config(yaml).unwrap();
        assert_eq!(config.policies.len(), 2);
        assert_eq!(config.policies[0].id, "test-rule");
        assert_eq!(config.policies[1].severity, Severity::Error);
    }

    #[test]
    fn test_parse_has_edge_rule() {
        let yaml = r#"
policies:
  - id: ownership
    description: "Has owner"
    rule:
      type: has_edge
      pattern: "src/**"
      edge_kind: owned_by
    severity: info
"#;
        let config = parse_policy_config(yaml).unwrap();
        match &config.policies[0].rule {
            Rule::HasEdge { pattern, edge_kind } => {
                assert_eq!(pattern, "src/**");
                assert_eq!(edge_kind, "owned_by");
            }
            _ => panic!("expected HasEdge rule"),
        }
    }

    #[test]
    fn test_parse_layer_boundary_rule() {
        let yaml = r#"
policies:
  - id: layers
    description: "Layer boundaries"
    rule:
      type: layer_boundary
      layers:
        - name: presentation
          pattern: "src/server/**"
        - name: domain
          pattern: "src/query/**"
    severity: error
"#;
        let config = parse_policy_config(yaml).unwrap();
        match &config.policies[0].rule {
            Rule::LayerBoundary { layers } => {
                assert_eq!(layers.len(), 2);
                assert_eq!(layers[0].name, "presentation");
            }
            _ => panic!("expected LayerBoundary rule"),
        }
    }

    #[test]
    fn test_generate_starter_config() {
        let config = generate_starter_config();
        // Should be valid YAML
        let parsed = parse_policy_config(&config);
        assert!(parsed.is_ok(), "starter config should be valid YAML");
        let config = parsed.unwrap();
        assert!(
            config.policies.len() >= 3,
            "starter config should have at least 3 policies"
        );
        // Starter config should also pass validation
        let errors = config.validate();
        assert!(errors.is_empty(), "starter config should pass validation");
    }

    #[test]
    fn test_validate_empty_policy_id() {
        let config = PolicyConfig {
            policies: vec![Policy {
                id: "".to_string(),
                description: "test".to_string(),
                rule: Rule::MaxConnections {
                    pattern: "src/**".to_string(),
                    threshold: 5,
                },
                severity: Severity::Warning,
            }],
        };
        let errors = config.validate();
        assert!(!errors.is_empty(), "empty policy id should be rejected");
        assert!(errors[0].message.contains("id must not be empty"));
    }

    #[test]
    fn test_validate_empty_patterns() {
        let config = PolicyConfig {
            policies: vec![Policy {
                id: "test".to_string(),
                description: "test".to_string(),
                rule: Rule::NoDependency {
                    from: "".to_string(),
                    to: "".to_string(),
                },
                severity: Severity::Error,
            }],
        };
        let errors = config.validate();
        assert_eq!(errors.len(), 2, "should reject both empty from and to");
    }

    #[test]
    fn test_validate_threshold_zero() {
        let config = PolicyConfig {
            policies: vec![Policy {
                id: "test".to_string(),
                description: "test".to_string(),
                rule: Rule::MaxConnections {
                    pattern: "src/**".to_string(),
                    threshold: 0,
                },
                severity: Severity::Warning,
            }],
        };
        let errors = config.validate();
        assert!(
            !errors.is_empty(),
            "threshold=0 should be rejected for max_connections"
        );
        assert!(errors[0].message.contains("threshold"));
    }

    #[test]
    fn test_validate_valid_config() {
        let config = PolicyConfig {
            policies: vec![Policy {
                id: "valid".to_string(),
                description: "valid rule".to_string(),
                rule: Rule::MaxConnections {
                    pattern: "src/**".to_string(),
                    threshold: 5,
                },
                severity: Severity::Warning,
            }],
        };
        let errors = config.validate();
        assert!(errors.is_empty(), "valid config should have no errors");
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let bad_yaml = "this is not: valid: yaml: [[[";
        let result = parse_policy_config(bad_yaml);
        assert!(result.is_err(), "malformed YAML should fail to parse");
    }

    #[test]
    fn test_parse_wrong_structure_yaml() {
        let yaml = r#"
not_policies:
  - something: else
"#;
        let result = parse_policy_config(yaml);
        assert!(
            result.is_err(),
            "YAML missing 'policies' key should fail to parse"
        );
    }

    #[test]
    fn test_parse_empty_policies_list() {
        let yaml = r#"
policies: []
"#;
        let config = parse_policy_config(yaml).unwrap();
        assert_eq!(config.policies.len(), 0);
        let errors = config.validate();
        assert!(errors.is_empty(), "empty policies list should be valid");
    }
}
