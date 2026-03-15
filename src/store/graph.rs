use anyhow::Result;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use rusqlite::Connection;
use std::collections::{HashMap, VecDeque};

use crate::store::schema::{EdgeKind, Entity, EntityKind};

/// Structural edge kinds — these represent code-level dependencies.
/// Used to filter deps and blast radius (exclude co-change, ownership, etc.)
const STRUCTURAL_EDGES: &[EdgeKind] = &[
    EdgeKind::Imports,
    EdgeKind::Calls,
    EdgeKind::Inherits,
    EdgeKind::Implements,
    EdgeKind::Exposes,
    EdgeKind::DependsOn,
];

// ─── GraphStore ───────────────────────────────────────────────────────────────

pub struct GraphStore {
    conn: Connection,
    graph: petgraph::Graph<String, EdgeKind>,
    node_map: HashMap<String, NodeIndex>,
    entities: HashMap<String, Entity>,
}

impl GraphStore {
    pub fn new(conn: Connection) -> Result<Self> {
        let mut store = GraphStore {
            conn,
            graph: petgraph::Graph::new(),
            node_map: HashMap::new(),
            entities: HashMap::new(),
        };
        store.load_from_db()?;
        Ok(store)
    }

    /// Clear all entities and edges (for re-indexing).
    pub fn clear(&mut self) -> Result<()> {
        self.conn
            .execute_batch("DELETE FROM edges; DELETE FROM entities;")?;
        self.graph.clear();
        self.node_map.clear();
        self.entities.clear();
        Ok(())
    }

    fn load_from_db(&mut self) -> Result<()> {
        // Load all entities
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, name, path, language, metadata, last_indexed FROM entities",
        )?;
        let entity_rows: Vec<Entity> = stmt
            .query_map([], |row| {
                let kind_str: String = row.get(1)?;
                let metadata_str: String = row.get(5)?;
                Ok(Entity {
                    id: row.get(0)?,
                    kind: kind_str.parse::<EntityKind>().unwrap_or(EntityKind::File),
                    name: row.get(2)?,
                    path: row.get(3)?,
                    language: row.get(4)?,
                    metadata: serde_json::from_str(&metadata_str)
                        .unwrap_or(serde_json::Value::Object(Default::default())),
                    last_indexed: row.get(6)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        for entity in entity_rows {
            let node = self.graph.add_node(entity.id.clone());
            self.node_map.insert(entity.id.clone(), node);
            self.entities.insert(entity.id.clone(), entity);
        }

        // Load all edges
        let mut stmt = self
            .conn
            .prepare("SELECT from_id, to_id, kind FROM edges")?;
        let edge_rows: Vec<(String, String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .filter_map(|r| r.ok())
            .collect();

        for (from_id, to_id, kind_str) in edge_rows {
            if let (Some(&from_node), Some(&to_node)) =
                (self.node_map.get(&from_id), self.node_map.get(&to_id))
                && let Ok(kind) = kind_str.parse::<EdgeKind>()
            {
                self.graph.add_edge(from_node, to_node, kind);
            }
        }

        Ok(())
    }

    pub fn add_entity(
        &mut self,
        kind: EntityKind,
        name: &str,
        path: Option<&str>,
        language: Option<&str>,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        self.conn.execute(
            "INSERT INTO entities (id, kind, name, path, language, metadata, last_indexed) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                id,
                kind.to_string(),
                name,
                path,
                language,
                "{}",
                now,
            ],
        )?;

        let entity = Entity {
            id: id.clone(),
            kind,
            name: name.to_string(),
            path: path.map(|s| s.to_string()),
            language: language.map(|s| s.to_string()),
            metadata: serde_json::Value::Object(Default::default()),
            last_indexed: now,
        };

        let node = self.graph.add_node(id.clone());
        self.node_map.insert(id.clone(), node);
        self.entities.insert(id.clone(), entity);

        Ok(id)
    }

    pub fn add_edge(
        &mut self,
        from_id: &str,
        to_id: &str,
        kind: EdgeKind,
        confidence: f64,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        self.conn.execute(
            "INSERT OR REPLACE INTO edges (from_id, to_id, kind, confidence, last_evidence, evidence_count, decay_half_life, evidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![from_id, to_id, kind.to_string(), confidence, now, 1_i64, 180.0_f64, "[]"],
        )?;

        let from_node = self
            .node_map
            .get(from_id)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Entity not found: {}", from_id))?;
        let to_node = self
            .node_map
            .get(to_id)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Entity not found: {}", to_id))?;

        self.graph.add_edge(from_node, to_node, kind);

        Ok(())
    }

    /// Return structural dependencies only (imports, calls, inherits, etc.)
    pub fn dependencies(&self, entity_id: &str, direction: petgraph::Direction) -> Vec<Entity> {
        let Some(&node) = self.node_map.get(entity_id) else {
            return vec![];
        };

        self.graph
            .edges_directed(node, direction)
            .filter(|edge_ref| STRUCTURAL_EDGES.contains(edge_ref.weight()))
            .filter_map(|edge_ref| {
                let neighbor = match direction {
                    petgraph::Direction::Outgoing => edge_ref.target(),
                    petgraph::Direction::Incoming => edge_ref.source(),
                };
                let neighbor_id = self.graph[neighbor].clone();
                self.entities.get(&neighbor_id).cloned()
            })
            .collect()
    }

    /// Return ALL neighbors regardless of edge kind
    pub fn all_neighbors(&self, entity_id: &str, direction: petgraph::Direction) -> Vec<Entity> {
        let Some(&node) = self.node_map.get(entity_id) else {
            return vec![];
        };

        self.graph
            .edges_directed(node, direction)
            .filter_map(|edge_ref| {
                let neighbor = match direction {
                    petgraph::Direction::Outgoing => edge_ref.target(),
                    petgraph::Direction::Incoming => edge_ref.source(),
                };
                let neighbor_id = self.graph[neighbor].clone();
                self.entities.get(&neighbor_id).cloned()
            })
            .collect()
    }

    pub fn dependencies_by_id(
        &self,
        entity_id: &str,
        direction: petgraph::Direction,
    ) -> Vec<Entity> {
        self.dependencies(entity_id, direction)
    }

    /// BFS blast radius following only structural edges (imports, calls, etc.)
    pub fn blast_radius(&self, entity_id: &str, max_depth: usize) -> Vec<Entity> {
        let Some(&start_node) = self.node_map.get(entity_id) else {
            return vec![];
        };

        let mut visited: HashMap<NodeIndex, usize> = HashMap::new();
        let mut queue: VecDeque<(NodeIndex, usize)> = VecDeque::new();
        let mut result: Vec<Entity> = Vec::new();

        visited.insert(start_node, 0);
        queue.push_back((start_node, 0));

        while let Some((current, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            for edge_ref in self
                .graph
                .edges_directed(current, petgraph::Direction::Outgoing)
                .filter(|e| STRUCTURAL_EDGES.contains(e.weight()))
            {
                let neighbor = edge_ref.target();
                if let std::collections::hash_map::Entry::Vacant(e) = visited.entry(neighbor) {
                    let new_depth = depth + 1;
                    e.insert(new_depth);
                    let neighbor_id = self.graph[neighbor].clone();
                    if let Some(entity) = self.entities.get(&neighbor_id) {
                        result.push(entity.clone());
                    }
                    queue.push_back((neighbor, new_depth));
                }
            }
        }

        result
    }

    pub fn find_entity_by_path(&self, path: &str) -> Option<Entity> {
        self.entities
            .values()
            .find(|e| e.path.as_deref() == Some(path))
            .cloned()
    }

    pub fn entities(&self) -> &HashMap<String, Entity> {
        &self.entities
    }

    /// Return all entities in the graph.
    pub fn all_entities(&self) -> Vec<Entity> {
        self.graph
            .node_indices()
            .map(|i| self.graph[i].clone())
            .filter_map(|id| self.entities.get(&id).cloned())
            .collect()
    }

    /// Return all (entity, confidence) pairs reachable via outgoing edges of a given kind.
    pub fn edges_of_kind(&self, entity_id: &str, kind: &EdgeKind) -> Vec<(Entity, f64)> {
        let Some(&node) = self.node_map.get(entity_id) else {
            return vec![];
        };

        // Build a fast lookup from (from_node_idx, to_node_idx) -> confidence using the DB.
        // We use the in-memory graph to find neighbors, then fetch confidence from SQLite.
        let neighbors: Vec<(String, EdgeKind)> = self
            .graph
            .edges_directed(node, petgraph::Direction::Outgoing)
            .filter(|e| e.weight() == kind)
            .map(|e| {
                let neighbor_id = self.graph[e.target()].clone();
                (neighbor_id, e.weight().clone())
            })
            .collect();

        neighbors
            .into_iter()
            .filter_map(|(neighbor_id, _)| {
                let entity = self.entities.get(&neighbor_id)?.clone();
                // Fetch confidence from SQLite
                let confidence: f64 = self
                    .conn
                    .query_row(
                        "SELECT confidence FROM edges WHERE from_id = ?1 AND to_id = ?2 AND kind = ?3",
                        rusqlite::params![entity_id, neighbor_id, kind.to_string()],
                        |r| r.get(0),
                    )
                    .unwrap_or(1.0);
                Some((entity, confidence))
            })
            .collect()
    }

    /// Return the total number of edges (in + out) for an entity — used as a hotspot proxy.
    pub fn edge_degree(&self, entity_id: &str) -> usize {
        let Some(&node) = self.node_map.get(entity_id) else {
            return 0;
        };
        self.graph
            .edges_directed(node, petgraph::Direction::Outgoing)
            .count()
            + self
                .graph
                .edges_directed(node, petgraph::Direction::Incoming)
                .count()
    }

    /// BFS returning (entity, depth, edge_kind) tuples — structural edges only.
    pub fn blast_radius_with_depth(
        &self,
        entity_id: &str,
        max_depth: usize,
    ) -> Vec<(Entity, usize, EdgeKind)> {
        let Some(&start_node) = self.node_map.get(entity_id) else {
            return vec![];
        };

        let mut visited: HashMap<NodeIndex, usize> = HashMap::new();
        let mut queue: VecDeque<(NodeIndex, usize)> = VecDeque::new();
        let mut result: Vec<(Entity, usize, EdgeKind)> = Vec::new();

        visited.insert(start_node, 0);
        queue.push_back((start_node, 0));

        while let Some((current, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            // Follow both outgoing (what this depends on) and incoming
            // (who depends on this) edges so blast radius captures the full
            // impact: files that import this entity are also reachable.
            for direction in [petgraph::Direction::Outgoing, petgraph::Direction::Incoming] {
                for edge_ref in self
                    .graph
                    .edges_directed(current, direction)
                    .filter(|e| STRUCTURAL_EDGES.contains(e.weight()))
                {
                    let neighbor = match direction {
                        petgraph::Direction::Outgoing => edge_ref.target(),
                        petgraph::Direction::Incoming => edge_ref.source(),
                    };
                    if let std::collections::hash_map::Entry::Vacant(e) = visited.entry(neighbor) {
                        let new_depth = depth + 1;
                        e.insert(new_depth);
                        let neighbor_id = self.graph[neighbor].clone();
                        if let Some(entity) = self.entities.get(&neighbor_id) {
                            result.push((entity.clone(), new_depth, edge_ref.weight().clone()));
                        }
                        queue.push_back((neighbor, new_depth));
                    }
                }
            }
        }

        result
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::schema::{EdgeKind, EntityKind};

    fn test_store() -> GraphStore {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::store::schema::init_db(&conn).unwrap();
        GraphStore::new(conn).unwrap()
    }

    #[test]
    fn test_add_entity_and_edge() {
        let mut store = test_store();
        let a = store
            .add_entity(
                EntityKind::File,
                "main.rs",
                Some("src/main.rs"),
                Some("rust"),
            )
            .unwrap();
        let b = store
            .add_entity(EntityKind::File, "lib.rs", Some("src/lib.rs"), Some("rust"))
            .unwrap();
        store.add_edge(&a, &b, EdgeKind::Imports, 1.0).unwrap();

        let deps = store.dependencies(&a, petgraph::Direction::Outgoing);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "lib.rs");
    }

    #[test]
    fn test_blast_radius_bfs() {
        let mut store = test_store();
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

        let radius = store.blast_radius(&a, 2);
        assert_eq!(radius.len(), 2); // b and c
    }

    #[test]
    fn test_blast_radius_depth_limit() {
        let mut store = test_store();
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

        let radius = store.blast_radius(&a, 1);
        assert_eq!(radius.len(), 1); // only b
    }

    #[test]
    fn test_find_entity_by_path() {
        let mut store = test_store();
        store
            .add_entity(
                EntityKind::File,
                "main.rs",
                Some("src/main.rs"),
                Some("rust"),
            )
            .unwrap();

        let found = store.find_entity_by_path("src/main.rs");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "main.rs");
    }
}
