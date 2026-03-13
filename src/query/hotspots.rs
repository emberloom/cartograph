use crate::store::graph::GraphStore;
use crate::store::schema::EntityKind;

pub struct HotspotEntry {
    pub entity_name: String,
    pub entity_path: Option<String>,
    pub edge_count: usize,
}

/// Return the top `limit` entities sorted by total edge degree (incoming + outgoing).
/// Edge count is used as a v0.1 proxy for "hotness" — highly-connected nodes are
/// more central and tend to accumulate change pressure.
/// Only entities that have a path (i.e. code artifacts) are included.
pub fn query(store: &GraphStore, limit: usize) -> Vec<HotspotEntry> {
    let mut scored: Vec<HotspotEntry> = store
        .entities()
        .values()
        .filter(|e| {
            e.path.is_some()
                && matches!(
                    e.kind,
                    EntityKind::File
                        | EntityKind::Module
                        | EntityKind::Function
                        | EntityKind::Struct
                        | EntityKind::Trait
                        | EntityKind::Impl
                        | EntityKind::Class
                        | EntityKind::Service
                )
        })
        .map(|e| HotspotEntry {
            entity_name: e.name.clone(),
            entity_path: e.path.clone(),
            edge_count: store.edge_degree(&e.id),
        })
        .collect();

    scored.sort_by(|a, b| b.edge_count.cmp(&a.edge_count));
    scored.truncate(limit);
    scored
}
