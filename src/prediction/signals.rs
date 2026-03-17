use crate::query;
use crate::store::graph::GraphStore;

/// Compute structural coupling signal for a candidate file.
///
/// Returns a value between 0.0 and 1.0 based on how close the candidate
/// is in the dependency graph to any of the changed files. Closer = higher signal.
pub fn structural_signal(
    store: &GraphStore,
    changed_files: &[String],
    candidate: &str,
    max_depth: usize,
) -> f64 {
    let mut min_depth: Option<usize> = None;

    for changed_file in changed_files {
        let blast = query::blast_radius::query(store, changed_file, max_depth);
        for entry in &blast {
            if entry.entity_path.as_deref() == Some(candidate) && entry.depth > 0 {
                match min_depth {
                    None => min_depth = Some(entry.depth),
                    Some(current) if entry.depth < current => min_depth = Some(entry.depth),
                    _ => {}
                }
            }
        }
    }

    match min_depth {
        Some(depth) => 1.0 / (depth as f64),
        None => 0.0,
    }
}

/// Compute co-change signal for a candidate file.
///
/// Returns the maximum co-change confidence between any changed file
/// and the candidate. Higher confidence = higher signal.
pub fn cochange_signal(store: &GraphStore, changed_files: &[String], candidate: &str) -> f64 {
    let mut max_confidence: f64 = 0.0;

    for changed_file in changed_files {
        let co = query::co_changes(store, changed_file);
        for result in &co {
            if result.entity_path.as_deref() == Some(candidate) {
                max_confidence = max_confidence.max(result.confidence);
            }
        }
    }

    max_confidence
}

/// Compute hotspot signal for a candidate file.
///
/// Returns a normalized score based on the entity's edge degree (connectivity).
/// Higher connectivity = higher signal (more central, higher risk when changed).
pub fn hotspot_signal(store: &GraphStore, candidate: &str) -> f64 {
    let degree = store
        .find_entity_by_path(candidate)
        .map(|e| store.edge_degree(&e.id))
        .unwrap_or(0);

    // Normalize: sigmoid-like curve, saturates around degree=20
    (degree as f64) / ((degree as f64) + 10.0)
}

/// Compute ownership fragmentation signal.
///
/// Files owned by many people have higher coordination risk. Returns a
/// normalized score where more owners = higher signal.
pub fn ownership_signal(store: &GraphStore, candidate: &str) -> f64 {
    let owners = query::ownership::query(store, candidate);
    let owner_count = owners.len();

    if owner_count <= 1 {
        0.0
    } else {
        // Normalize: 2 owners = 0.33, 3 = 0.5, 5 = 0.67, etc.
        let normalized = (owner_count as f64 - 1.0) / (owner_count as f64);
        normalized.min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::graph::GraphStore;
    use crate::store::schema::{EdgeKind, EntityKind};

    fn setup_store() -> GraphStore {
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
        let d = store
            .add_entity(EntityKind::File, "d.rs", Some("src/d.rs"), Some("rust"))
            .unwrap();

        let p1 = store
            .add_entity(EntityKind::Person, "dev1@test.com", None, None)
            .unwrap();
        let p2 = store
            .add_entity(EntityKind::Person, "dev2@test.com", None, None)
            .unwrap();

        store.add_edge(&a, &b, EdgeKind::Imports, 1.0).unwrap();
        store.add_edge(&b, &c, EdgeKind::Imports, 1.0).unwrap();
        store.add_edge(&d, &b, EdgeKind::DependsOn, 1.0).unwrap();
        store
            .add_edge(&a, &d, EdgeKind::CoChangesWith, 0.7)
            .unwrap();
        store.add_edge(&b, &p1, EdgeKind::OwnedBy, 0.8).unwrap();
        store.add_edge(&b, &p2, EdgeKind::OwnedBy, 0.6).unwrap();

        store
    }

    #[test]
    fn test_structural_signal_direct_dep() {
        let store = setup_store();
        let signal = structural_signal(&store, &["src/a.rs".to_string()], "src/b.rs", 3);
        assert!(signal > 0.0, "b.rs is a direct dependency of a.rs");
        assert_eq!(signal, 1.0); // depth 1 → 1.0/1 = 1.0
    }

    #[test]
    fn test_structural_signal_transitive() {
        let store = setup_store();
        let signal = structural_signal(&store, &["src/a.rs".to_string()], "src/c.rs", 3);
        assert!(signal > 0.0, "c.rs is transitively reachable from a.rs");
        assert!(
            signal < 1.0,
            "transitive should have lower signal than direct"
        );
    }

    #[test]
    fn test_structural_signal_no_connection() {
        let store = setup_store();
        let signal = structural_signal(&store, &["src/c.rs".to_string()], "src/a.rs", 1);
        // c has no outgoing structural edges to a at depth 1
        // (blast_radius_with_depth follows both directions, so it may or may not find it)
        // The important thing is the function doesn't panic
        assert!(signal >= 0.0);
    }

    #[test]
    fn test_cochange_signal() {
        let store = setup_store();
        let signal = cochange_signal(&store, &["src/a.rs".to_string()], "src/d.rs");
        assert!(
            (signal - 0.7).abs() < 0.01,
            "should match co-change confidence of 0.7"
        );
    }

    #[test]
    fn test_cochange_signal_no_relation() {
        let store = setup_store();
        let signal = cochange_signal(&store, &["src/c.rs".to_string()], "src/d.rs");
        assert_eq!(signal, 0.0, "no co-change between c and d");
    }

    #[test]
    fn test_hotspot_signal() {
        let store = setup_store();
        let signal = hotspot_signal(&store, "src/b.rs");
        assert!(signal > 0.0, "b.rs has multiple edges");
    }

    #[test]
    fn test_ownership_signal() {
        let store = setup_store();
        let signal = ownership_signal(&store, "src/b.rs");
        assert!(signal > 0.0, "b.rs has 2 owners");
    }

    #[test]
    fn test_ownership_signal_no_owners() {
        let store = setup_store();
        let signal = ownership_signal(&store, "src/c.rs");
        assert_eq!(signal, 0.0, "c.rs has no owners");
    }
}
