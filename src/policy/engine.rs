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

/// Detect circular dependencies in the graph (cycles).
/// Returns violations for each entity that participates in a cycle.
pub fn detect_cycles(store: &GraphStore) -> Vec<Violation> {
    let all_entities = store.all_entities();
    let file_entities: Vec<_> = all_entities
        .iter()
        .filter(|e| e.kind == EntityKind::File)
        .collect();

    // Build adjacency list
    let mut adj: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for entity in &file_entities {
        let deps = store.dependencies(&entity.id, petgraph::Direction::Outgoing);
        let dep_ids: Vec<&str> = file_entities
            .iter()
            .filter(|e| deps.iter().any(|d| d.id == e.id))
            .map(|e| e.id.as_str())
            .collect();
        adj.insert(entity.id.as_str(), dep_ids);
    }

    // Tarjan-style cycle detection using DFS coloring
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    let mut color: std::collections::HashMap<&str, Color> = std::collections::HashMap::new();
    let mut in_cycle: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for entity in &file_entities {
        color.insert(entity.id.as_str(), Color::White);
    }

    fn dfs<'a>(
        node: &'a str,
        adj: &std::collections::HashMap<&'a str, Vec<&'a str>>,
        color: &mut std::collections::HashMap<&'a str, Color>,
        in_cycle: &mut std::collections::HashSet<&'a str>,
        stack: &mut Vec<&'a str>,
    ) {
        color.insert(node, Color::Gray);
        stack.push(node);

        if let Some(neighbors) = adj.get(node) {
            for &neighbor in neighbors {
                match color.get(neighbor) {
                    Some(Color::Gray) => {
                        // Found a cycle — mark all nodes in the cycle
                        let cycle_start = stack.iter().position(|&n| n == neighbor).unwrap();
                        for &cycle_node in &stack[cycle_start..] {
                            in_cycle.insert(cycle_node);
                        }
                    }
                    Some(Color::White) => {
                        dfs(neighbor, adj, color, in_cycle, stack);
                    }
                    _ => {}
                }
            }
        }

        stack.pop();
        color.insert(node, Color::Black);
    }

    for entity in &file_entities {
        if color.get(entity.id.as_str()) == Some(&Color::White) {
            let mut stack = Vec::new();
            dfs(
                entity.id.as_str(),
                &adj,
                &mut color,
                &mut in_cycle,
                &mut stack,
            );
        }
    }

    // Build entity id -> path map
    let id_to_path: std::collections::HashMap<&str, &str> = file_entities
        .iter()
        .filter_map(|e| e.path.as_deref().map(|p| (e.id.as_str(), p)))
        .collect();

    in_cycle
        .iter()
        .map(|&id| Violation {
            policy_id: "builtin:cycle-detection".to_string(),
            severity: Severity::Error,
            entity_path: id_to_path.get(id).unwrap_or(&id).to_string(),
            message: format!(
                "Circular dependency detected involving {}",
                id_to_path.get(id).unwrap_or(&id)
            ),
        })
        .collect()
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
        // Files not in any layer are silently skipped
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
    // Edge cases: empty pattern or empty path
    if pattern.is_empty() && path.is_empty() {
        return true;
    }
    if pattern.is_empty() {
        return false;
    }
    if path.is_empty() {
        // Only match if pattern is purely ** segments
        return pattern.split('/').all(|s| s == "**");
    }

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
        // Collapse consecutive ** segments
        let mut next = 1;
        while next < pattern.len() && pattern[next] == "**" {
            next += 1;
        }
        let remaining_pattern = &pattern[next..];

        // ** matches zero or more segments
        for i in 0..=path.len() {
            if glob_match_parts(remaining_pattern, &path[i..]) {
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
    fn test_glob_edge_cases_empty_strings() {
        // Both empty
        assert!(glob_match("", ""));
        // Empty pattern, non-empty path
        assert!(!glob_match("", "src/main.rs"));
        // Non-empty pattern, empty path
        assert!(!glob_match("src/**", ""));
        // ** with empty path
        assert!(glob_match("**", ""));
    }

    #[test]
    fn test_glob_edge_cases_consecutive_double_star() {
        // Consecutive ** should be collapsed and work correctly
        assert!(glob_match("**/**/*.rs", "src/deep/nested/file.rs"));
        assert!(glob_match("**/**", "src/main.rs"));
        assert!(glob_match("src/**/**/*.rs", "src/a/b/c.rs"));
        // Multiple ** at different levels
        assert!(glob_match("**/src/**/*.rs", "root/src/nested/file.rs"));
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
    fn test_layer_boundary_files_not_in_any_layer() {
        // Files that don't match any layer pattern should be silently skipped
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::store::schema::init_db(&conn).unwrap();
        let mut store = GraphStore::new(conn).unwrap();

        let server = store
            .add_entity(
                EntityKind::File,
                "mod.rs",
                Some("src/server/mod.rs"),
                Some("rust"),
            )
            .unwrap();
        let utils = store
            .add_entity(
                EntityKind::File,
                "utils.rs",
                Some("src/utils.rs"),
                Some("rust"),
            )
            .unwrap();

        // utils -> server: utils is not in any layer, should be skipped
        store
            .add_edge(&utils, &server, EdgeKind::Imports, 1.0)
            .unwrap();

        let config = PolicyConfig {
            policies: vec![Policy {
                id: "layers".to_string(),
                description: "Layer test".to_string(),
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
                    ],
                },
                severity: Severity::Error,
            }],
        };

        let result = evaluate(&store, &config);
        // utils.rs is not in any layer, so no violation should be reported for it
        assert!(
            result.violations.is_empty(),
            "files not in any layer should not produce violations, got: {:?}",
            result.violations
        );
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

    #[test]
    fn test_cycle_detection() {
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

        // Create cycle: a -> b -> c -> a
        store.add_edge(&a, &b, EdgeKind::Imports, 1.0).unwrap();
        store.add_edge(&b, &c, EdgeKind::Imports, 1.0).unwrap();
        store.add_edge(&c, &a, EdgeKind::Imports, 1.0).unwrap();

        let violations = detect_cycles(&store);
        assert!(
            !violations.is_empty(),
            "should detect cycle in a -> b -> c -> a"
        );
        // All three nodes should be in the cycle
        let paths: std::collections::HashSet<&str> =
            violations.iter().map(|v| v.entity_path.as_str()).collect();
        assert!(paths.contains("src/a.rs"), "a.rs should be in cycle");
        assert!(paths.contains("src/b.rs"), "b.rs should be in cycle");
        assert!(paths.contains("src/c.rs"), "c.rs should be in cycle");
    }

    #[test]
    fn test_no_cycles() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::store::schema::init_db(&conn).unwrap();
        let mut store = GraphStore::new(conn).unwrap();

        let a = store
            .add_entity(EntityKind::File, "a.rs", Some("src/a.rs"), Some("rust"))
            .unwrap();
        let b = store
            .add_entity(EntityKind::File, "b.rs", Some("src/b.rs"), Some("rust"))
            .unwrap();

        store.add_edge(&a, &b, EdgeKind::Imports, 1.0).unwrap();

        let violations = detect_cycles(&store);
        assert!(
            violations.is_empty(),
            "should not detect cycle in a -> b (no cycle)"
        );
    }
}
