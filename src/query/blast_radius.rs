use crate::store::graph::GraphStore;

pub struct ImpactEntry {
    pub entity_name: String,
    pub entity_path: Option<String>,
    pub depth: usize,
    pub edge_kind: String,
}

/// BFS from the entity at `entity_path`, up to `max_depth` hops.
/// Returns one entry per reachable entity, enriched with hop depth and the
/// edge kind that first discovered it.
pub fn query(store: &GraphStore, entity_path: &str, max_depth: usize) -> Vec<ImpactEntry> {
    let Some(entity) = store.find_entity_by_path(entity_path) else {
        return vec![];
    };

    store
        .blast_radius_with_depth(&entity.id, max_depth)
        .into_iter()
        .map(|(e, depth, edge_kind)| ImpactEntry {
            entity_name: e.name,
            entity_path: e.path,
            depth,
            edge_kind: edge_kind.to_string(),
        })
        .collect()
}
