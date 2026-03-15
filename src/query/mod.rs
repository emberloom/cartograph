pub mod blast_radius;
pub mod hotspots;
pub mod ownership;

use crate::store::graph::GraphStore;
use crate::store::schema::EdgeKind;

pub struct CoChangeResult {
    pub entity_name: String,
    pub entity_path: Option<String>,
    pub confidence: f64,
}

/// Return all entities that co-change with the entity at `entity_path`.
pub fn co_changes(store: &GraphStore, entity_path: &str) -> Vec<CoChangeResult> {
    let Some(entity) = store.find_entity_by_path(entity_path) else {
        return vec![];
    };

    store
        .edges_of_kind(&entity.id, &EdgeKind::CoChangesWith)
        .into_iter()
        .map(|(e, confidence)| CoChangeResult {
            entity_name: e.name,
            entity_path: e.path,
            confidence,
        })
        .collect()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

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

        store.add_edge(&a, &b, EdgeKind::Imports, 1.0).unwrap();
        store.add_edge(&b, &c, EdgeKind::Imports, 1.0).unwrap();
        store
            .add_edge(&a, &c, EdgeKind::CoChangesWith, 0.8)
            .unwrap();

        // Add a person for ownership
        let person = store
            .add_entity(EntityKind::Person, "dev@test.com", None, None)
            .unwrap();
        store.add_edge(&a, &person, EdgeKind::OwnedBy, 0.9).unwrap();

        store
    }

    #[test]
    fn test_blast_radius_query() {
        let store = setup_store();
        let results = blast_radius::query(&store, "src/a.rs", 3);
        assert!(!results.is_empty());
        assert!(
            results
                .iter()
                .any(|r| r.entity_path.as_deref() == Some("src/b.rs"))
        );
    }

    #[test]
    fn test_hotspots_query() {
        let store = setup_store();
        let results = hotspots::query(&store, 10);
        // Should return files (entities with paths)
        assert!(!results.is_empty());
    }

    #[test]
    fn test_co_changes_query() {
        let store = setup_store();
        let results = co_changes(&store, "src/a.rs");
        // a.rs has a CoChangesWith edge to c.rs
        assert!(
            results
                .iter()
                .any(|r| r.entity_path.as_deref() == Some("src/c.rs"))
        );
    }

    #[test]
    fn test_who_owns_query() {
        let store = setup_store();
        let results = ownership::query(&store, "src/a.rs");
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.entity_name == "dev@test.com"));
    }
}
