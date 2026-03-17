use crate::store::graph::GraphStore;
use crate::store::schema::EntityKind;

use super::rules::{Policy, PolicyConfig, Rule};
use super::{PolicyResult, Severity, Violation};

/// Evaluate all policies against the graph store.
pub fn evaluate(store: &GraphStore, config: &PolicyConfig) -> PolicyResult {
    let mut violations = Vec::new();
    let mut has_errors = false;

    for policy in &config.policies {
        let policy_violations = evaluate_policy(store, policy);
        if policy.severity == Severity::Error && !policy_violations.is_empty() {
            has_errors = true;
        }
        violations.extend(policy_violations);
    }

    PolicyResult {
        policies_checked: config.policies.len(),
        violations,
        has_errors,
    }
}

/// Evaluate a single policy against the graph store.
fn evaluate_policy(store: &GraphStore, policy: &Policy) -> Vec<Violation> {
    match &policy.rule {
        Rule::NoDependency { from, to } => evaluate_no_dependency(store, policy, from, to),
        Rule::MaxConnections { pattern, threshold } => {
            evaluate_max_connections(store, policy, pattern, *threshold)
        }
        Rule::HasEdge { pattern, edge_kind } => {
            evaluate_has_edge(store, policy, pattern, edge_kind)
        }
        Rule::LayerBoundary { layers } => evaluate_layer_boundary(store, policy, layers),
    }
}

/// Check that no dependency exists from files matching `from` to files matching `to`.
fn evaluate_no_dependency(
    store: &GraphStore,
    policy: &Policy,
    from_pattern: &str,
    to_pattern: &str,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    let all_entities = store.all_entities();
    let from_entities: Vec<_> = all_entities
        .iter()
        .filter(|e| e.kind == EntityKind::File)
        .filter(|e| {
            e.path
                .as_deref()
                .is_some_and(|p| glob_match(from_pattern, p))
        })
        .collect();

    let to_paths: std::collections::HashSet<String> = all_entities
        .iter()
        .filter(|e| e.kind == EntityKind::File)
        .filter(|e| e.path.as_deref().is_some_and(|p| glob_match(to_pattern, p)))
        .filter_map(|e| e.path.clone())
        .collect();

    for from_entity in &from_entities {
        let deps = store.dependencies(&from_entity.id, petgraph::Direction::Outgoing);
        for dep in &deps {
            if let Some(ref dep_path) = dep.path
                && to_paths.contains(dep_path)
            {
                violations.push(Violation {
                    policy_id: policy.id.clone(),
                    severity: policy.severity,
                    entity_path: from_entity.path.clone().unwrap_or_default(),
                    message: format!(
                        "{}: {} depends on {} (policy: {})",
                        policy.description,
                        from_entity.path.as_deref().unwrap_or(&from_entity.name),
                        dep_path,
                        policy.id
                    ),
                });
            }
        }
    }

    violations
}

/// Check that files matching `pattern` don't exceed `threshold` connections.
fn evaluate_max_connections(
    store: &GraphStore,
    policy: &Policy,
    pattern: &str,
    threshold: usize,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    for entity in store.all_entities() {
        if entity.kind != EntityKind::File {
            continue;
        }
        let Some(ref path) = entity.path else {
            continue;
        };
        if !glob_match(pattern, path) {
            continue;
        }

        let degree = store.edge_degree(&entity.id);
        if degree > threshold {
            violations.push(Violation {
                policy_id: policy.id.clone(),
                severity: policy.severity,
                entity_path: path.clone(),
                message: format!(
                    "{}: {} has {} connections (threshold: {})",
                    policy.description, path, degree, threshold
                ),
            });
        }
    }

    violations
}

/// Check that files matching `pattern` have at least one edge of kind `edge_kind`.
fn evaluate_has_edge(
    store: &GraphStore,
    policy: &Policy,
    pattern: &str,
    edge_kind: &str,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    let target_kind = match edge_kind.parse::<crate::store::schema::EdgeKind>() {
        Ok(k) => k,
        Err(_) => return violations, // Unknown edge kind, skip
    };

    for entity in store.all_entities() {
        if entity.kind != EntityKind::File {
            continue;
        }
        let Some(ref path) = entity.path else {
            continue;
        };
        if !glob_match(pattern, path) {
            continue;
        }

        let edges = store.edges_of_kind(&entity.id, &target_kind);
        if edges.is_empty() {
            violations.push(Violation {
                policy_id: policy.id.clone(),
                severity: policy.severity,
                entity_path: path.clone(),
                message: format!(
                    "{}: {} has no '{}' edges",
                    policy.description, path, edge_kind
                ),
            });
        }
    }

    violations
}

/// Check layer boundary violations. Dependencies should only flow from
/// higher layers (earlier in the list) to lower layers (later in the list).
fn evaluate_layer_boundary(
    store: &GraphStore,
    policy: &Policy,
    layers: &[super::rules::LayerDef],
) -> Vec<Violation> {
    let mut violations = Vec::new();

    // Map each file to its layer index
    let all_entities = store.all_entities();
    let mut file_layer: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for entity in &all_entities {
        if entity.kind != EntityKind::File {
            continue;
        }
        let Some(ref path) = entity.path else {
            continue;
        };
        for (i, layer) in layers.iter().enumerate() {
            if glob_match(&layer.pattern, path) {
                file_layer.insert(entity.id.clone(), i);
                break;
            }
        }
    }

    // Check each entity's dependencies
    for entity in &all_entities {
        if entity.kind != EntityKind::File {
            continue;
        }
        let Some(&from_layer) = file_layer.get(&entity.id) else {
            continue;
        };

        let deps = store.dependencies(&entity.id, petgraph::Direction::Outgoing);
        for dep in &deps {
            if let Some(&to_layer) = file_layer.get(&dep.id) {
                // Violation if dependency flows upward (from lower layer to higher layer)
                if to_layer < from_layer {
                    violations.push(Violation {
                        policy_id: policy.id.clone(),
                        severity: policy.severity,
                        entity_path: entity.path.clone().unwrap_or_default(),
                        message: format!(
                            "{}: {} (layer: {}) depends on {} (layer: {}) — upward dependency",
                            policy.description,
                            entity.path.as_deref().unwrap_or(&entity.name),
                            layers[from_layer].name,
                            dep.path.as_deref().unwrap_or(&dep.name),
                            layers[to_layer].name,
                        ),
                    });
                }
            }
        }
    }

    violations
}

/// Simple glob matching supporting `*` (single segment) and `**` (any depth).
fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();
    glob_match_parts(&pattern_parts, &path_parts)
}

fn glob_match_parts(pattern: &[&str], path: &[&str]) -> bool {
    if pattern.is_empty() && path.is_empty() {
        return true;
    }
    if pattern.is_empty() {
        return false;
    }

    if pattern[0] == "**" {
        // ** matches zero or more segments
        // Try matching the rest of the pattern against every possible suffix of path
        for i in 0..=path.len() {
            if glob_match_parts(&pattern[1..], &path[i..]) {
                return true;
            }
        }
        return false;
    }

    if path.is_empty() {
        return false;
    }

    if segment_match(pattern[0], path[0]) {
        glob_match_parts(&pattern[1..], &path[1..])
    } else {
        false
    }
}

fn segment_match(pattern: &str, segment: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    // Handle patterns like "*.rs"
    if let Some(suffix) = pattern.strip_prefix('*') {
        return segment.ends_with(suffix);
    }

    // Handle patterns like "src*"
    if let Some(prefix) = pattern.strip_suffix('*') {
        return segment.starts_with(prefix);
    }

    pattern == segment
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Severity;
    use crate::policy::rules::{LayerDef, Policy, PolicyConfig, Rule};
    use crate::store::graph::GraphStore;
    use crate::store::schema::{EdgeKind, EntityKind};

    fn setup_store() -> GraphStore {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::store::schema::init_db(&conn).unwrap();
        let mut store = GraphStore::new(conn).unwrap();

        let server_mod = store
            .add_entity(
                EntityKind::File,
                "mod.rs",
                Some("src/server/mod.rs"),
                Some("rust"),
            )
            .unwrap();
        let parser_mod = store
            .add_entity(
                EntityKind::File,
                "mod.rs",
                Some("src/parser/mod.rs"),
                Some("rust"),
            )
            .unwrap();
        let query_mod = store
            .add_entity(
                EntityKind::File,
                "mod.rs",
                Some("src/query/mod.rs"),
                Some("rust"),
            )
            .unwrap();
        let store_mod = store
            .add_entity(
                EntityKind::File,
                "mod.rs",
                Some("src/store/mod.rs"),
                Some("rust"),
            )
            .unwrap();

        // server -> query (allowed: presentation -> domain)
        store
            .add_edge(&server_mod, &query_mod, EdgeKind::Imports, 1.0)
            .unwrap();
        // server -> parser (forbidden by no_dependency policy)
        store
            .add_edge(&server_mod, &parser_mod, EdgeKind::Imports, 1.0)
            .unwrap();
        // query -> store (allowed: domain -> infrastructure)
        store
            .add_edge(&query_mod, &store_mod, EdgeKind::Imports, 1.0)
            .unwrap();
        // store -> server (forbidden: upward dependency)
        store
            .add_edge(&store_mod, &server_mod, EdgeKind::Imports, 1.0)
            .unwrap();

        store
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("src/**", "src/main.rs"));
        assert!(glob_match("src/**", "src/parser/mod.rs"));
        assert!(glob_match("src/**/*.rs", "src/parser/mod.rs"));
        assert!(glob_match("src/*.rs", "src/main.rs"));
        assert!(!glob_match("src/*.rs", "src/parser/mod.rs"));
        assert!(glob_match("**", "anything"));
        assert!(glob_match("**/*.rs", "src/deep/nested/file.rs"));
    }

    #[test]
    fn test_no_dependency_policy() {
        let store = setup_store();
        let config = PolicyConfig {
            policies: vec![Policy {
                id: "no-server-parser".to_string(),
                description: "Server should not depend on parser".to_string(),
                rule: Rule::NoDependency {
                    from: "src/server/**".to_string(),
                    to: "src/parser/**".to_string(),
                },
                severity: Severity::Error,
            }],
        };

        let result = evaluate(&store, &config);
        assert!(
            !result.violations.is_empty(),
            "should detect server -> parser dependency"
        );
        assert!(result.has_errors);
    }

    #[test]
    fn test_max_connections_policy() {
        let store = setup_store();
        let config = PolicyConfig {
            policies: vec![Policy {
                id: "max-connections".to_string(),
                description: "Max connections".to_string(),
                rule: Rule::MaxConnections {
                    pattern: "src/**".to_string(),
                    threshold: 1,
                },
                severity: Severity::Warning,
            }],
        };

        let result = evaluate(&store, &config);
        // server/mod.rs has 3 edges (2 outgoing + 1 incoming) > threshold of 1
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.entity_path.contains("server")),
            "server/mod.rs should exceed max connections"
        );
    }

    #[test]
    fn test_has_edge_policy() {
        let store = setup_store();
        let config = PolicyConfig {
            policies: vec![Policy {
                id: "ownership".to_string(),
                description: "All files must have owners".to_string(),
                rule: Rule::HasEdge {
                    pattern: "src/**".to_string(),
                    edge_kind: "owned_by".to_string(),
                },
                severity: Severity::Info,
            }],
        };

        let result = evaluate(&store, &config);
        // No files have ownership edges
        assert!(
            !result.violations.is_empty(),
            "should detect missing ownership edges"
        );
    }

    #[test]
    fn test_layer_boundary_policy() {
        let store = setup_store();
        let config = PolicyConfig {
            policies: vec![Policy {
                id: "layers".to_string(),
                description: "Layer boundaries".to_string(),
                rule: Rule::LayerBoundary {
                    layers: vec![
                        LayerDef {
                            name: "presentation".to_string(),
                            pattern: "src/server/**".to_string(),
                        },
                        LayerDef {
                            name: "domain".to_string(),
                            pattern: "src/query/**".to_string(),
                        },
                        LayerDef {
                            name: "infrastructure".to_string(),
                            pattern: "src/store/**".to_string(),
                        },
                    ],
                },
                severity: Severity::Error,
            }],
        };

        let result = evaluate(&store, &config);
        // store -> server is an upward dependency (infrastructure -> presentation)
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.message.contains("upward dependency")),
            "should detect upward dependency from store to server"
        );
        assert!(result.has_errors);
    }

    #[test]
    fn test_evaluate_empty_config() {
        let store = setup_store();
        let config = PolicyConfig { policies: vec![] };
        let result = evaluate(&store, &config);
        assert!(result.violations.is_empty());
        assert!(!result.has_errors);
        assert_eq!(result.policies_checked, 0);
    }
}
