use crate::store::graph::GraphStore;
use crate::store::schema::EdgeKind;

pub struct OwnerEntry {
    pub entity_name: String,
    pub entity_path: Option<String>,
    pub confidence: f64,
}

/// Return all Person/Team entities connected to `entity_path` via an OwnedBy edge.
pub fn query(store: &GraphStore, entity_path: &str) -> Vec<OwnerEntry> {
    let Some(entity) = store.find_entity_by_path(entity_path) else {
        return vec![];
    };

    store
        .edges_of_kind(&entity.id, &EdgeKind::OwnedBy)
        .into_iter()
        .map(|(owner, confidence)| OwnerEntry {
            entity_name: owner.name,
            entity_path: owner.path,
            confidence,
        })
        .collect()
}
