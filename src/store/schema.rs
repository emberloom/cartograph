use anyhow::Result;
use rusqlite::Connection;
use serde_json;
use std::fmt;
use std::str::FromStr;

// ─── EntityKind ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityKind {
    Module,
    File,
    Function,
    Struct,
    Trait,
    Impl,
    Class,
    Service,
    Person,
    Team,
    Test,
    Document,
    Deployment,
}

impl fmt::Display for EntityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FromStr for EntityKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "Module" => Ok(EntityKind::Module),
            "File" => Ok(EntityKind::File),
            "Function" => Ok(EntityKind::Function),
            "Struct" => Ok(EntityKind::Struct),
            "Trait" => Ok(EntityKind::Trait),
            "Impl" => Ok(EntityKind::Impl),
            "Class" => Ok(EntityKind::Class),
            "Service" => Ok(EntityKind::Service),
            "Person" => Ok(EntityKind::Person),
            "Team" => Ok(EntityKind::Team),
            "Test" => Ok(EntityKind::Test),
            "Document" => Ok(EntityKind::Document),
            "Deployment" => Ok(EntityKind::Deployment),
            other => Err(anyhow::anyhow!("Unknown EntityKind: {}", other)),
        }
    }
}

// ─── EdgeKind ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeKind {
    Imports,
    Calls,
    Inherits,
    Implements,
    Exposes,
    DependsOn,
    CoChangesWith,
    BrokeAfter,
    DeployedTo,
    RevertedBecause,
    OwnedBy,
    ReviewedBy,
    DocumentedIn,
    DecidedBecause,
    FailedWhen,
}

impl fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            EdgeKind::Imports => "imports",
            EdgeKind::Calls => "calls",
            EdgeKind::Inherits => "inherits",
            EdgeKind::Implements => "implements",
            EdgeKind::Exposes => "exposes",
            EdgeKind::DependsOn => "depends_on",
            EdgeKind::CoChangesWith => "co_changes_with",
            EdgeKind::BrokeAfter => "broke_after",
            EdgeKind::DeployedTo => "deployed_to",
            EdgeKind::RevertedBecause => "reverted_because",
            EdgeKind::OwnedBy => "owned_by",
            EdgeKind::ReviewedBy => "reviewed_by",
            EdgeKind::DocumentedIn => "documented_in",
            EdgeKind::DecidedBecause => "decided_because",
            EdgeKind::FailedWhen => "failed_when",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for EdgeKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "imports" => Ok(EdgeKind::Imports),
            "calls" => Ok(EdgeKind::Calls),
            "inherits" => Ok(EdgeKind::Inherits),
            "implements" => Ok(EdgeKind::Implements),
            "exposes" => Ok(EdgeKind::Exposes),
            "depends_on" => Ok(EdgeKind::DependsOn),
            "co_changes_with" => Ok(EdgeKind::CoChangesWith),
            "broke_after" => Ok(EdgeKind::BrokeAfter),
            "deployed_to" => Ok(EdgeKind::DeployedTo),
            "reverted_because" => Ok(EdgeKind::RevertedBecause),
            "owned_by" => Ok(EdgeKind::OwnedBy),
            "reviewed_by" => Ok(EdgeKind::ReviewedBy),
            "documented_in" => Ok(EdgeKind::DocumentedIn),
            "decided_because" => Ok(EdgeKind::DecidedBecause),
            "failed_when" => Ok(EdgeKind::FailedWhen),
            other => Err(anyhow::anyhow!("Unknown EdgeKind: {}", other)),
        }
    }
}

// ─── Entity ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: String,
    pub kind: EntityKind,
    pub name: String,
    pub path: Option<String>,
    pub language: Option<String>,
    pub metadata: serde_json::Value,
    pub last_indexed: String,
}

// ─── Edge ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Edge {
    pub from_id: String,
    pub to_id: String,
    pub kind: EdgeKind,
    pub confidence: f64,
    pub last_evidence: String,
    pub evidence_count: u32,
    pub decay_half_life: f64,
    pub evidence: Vec<String>,
}

// ─── init_db ─────────────────────────────────────────────────────────────────

pub fn init_db(conn: &Connection) -> Result<()> {
    // journal_mode returns a result row, so we use pragma_update or query it
    conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entities (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            name TEXT NOT NULL,
            path TEXT,
            language TEXT,
            metadata TEXT NOT NULL DEFAULT '{}',
            last_indexed TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_entities_kind ON entities(kind);
        CREATE INDEX IF NOT EXISTS idx_entities_path ON entities(path);
        CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(name);

        CREATE TABLE IF NOT EXISTS edges (
            from_id TEXT NOT NULL REFERENCES entities(id),
            to_id TEXT NOT NULL REFERENCES entities(id),
            kind TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 1.0,
            last_evidence TEXT NOT NULL DEFAULT (datetime('now')),
            evidence_count INTEGER NOT NULL DEFAULT 1,
            decay_half_life REAL NOT NULL DEFAULT 180.0,
            evidence TEXT NOT NULL DEFAULT '[]',
            PRIMARY KEY (from_id, to_id, kind)
        );

        CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
        CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id);
        CREATE INDEX IF NOT EXISTS idx_edges_kind ON edges(kind);

        CREATE TABLE IF NOT EXISTS index_metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_creates_tables() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_insert_and_retrieve_entity() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO entities (id, kind, name, path, language, metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, "File", "main.rs", "src/main.rs", "rust", "{}"],
        ).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM entities WHERE id = ?1", [&id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(name, "main.rs");
    }

    #[test]
    fn test_insert_and_retrieve_edge() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        let from_id = uuid::Uuid::new_v4().to_string();
        let to_id = uuid::Uuid::new_v4().to_string();

        for (id, name) in [(&from_id, "a.rs"), (&to_id, "b.rs")] {
            conn.execute(
                "INSERT INTO entities (id, kind, name, path, language, metadata) VALUES (?1, 'File', ?2, ?2, 'rust', '{}')",
                rusqlite::params![id, name],
            ).unwrap();
        }

        conn.execute(
            "INSERT INTO edges (from_id, to_id, kind, confidence, evidence_count, decay_half_life, evidence, last_evidence) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![from_id, to_id, "imports", 1.0_f64, 1_u32, 180.0_f64, "[]", chrono::Utc::now().to_rfc3339()],
        ).unwrap();

        let kind: String = conn
            .query_row(
                "SELECT kind FROM edges WHERE from_id = ?1",
                [&from_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kind, "imports");
    }
}
