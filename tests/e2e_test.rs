use cartograph::store::{graph::GraphStore, schema};
use cartograph::{historian, parser, query};
use std::path::Path;

#[test]
fn test_full_pipeline_on_fixture() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    schema::init_db(&conn).unwrap();
    let mut store = GraphStore::new(conn).unwrap();

    let repo_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample_repo");

    // Layer 1: Parse structure
    parser::index_repo(&repo_path, &mut store).unwrap();

    // Verify files were indexed
    assert!(store.find_entity_by_path("src/main.rs").is_some());
    assert!(store.find_entity_by_path("src/auth.rs").is_some());
    assert!(store.find_entity_by_path("src/lib.rs").is_some());
    assert!(store.find_entity_by_path("src/billing.rs").is_some());

    // Verify dependency edges exist (main.rs → auth.rs via mod declaration)
    let main_entity = store.find_entity_by_path("src/main.rs").unwrap();
    let deps = store.dependencies(&main_entity.id, petgraph::Direction::Outgoing);
    assert!(
        !deps.is_empty(),
        "main.rs should have outgoing dependencies"
    );

    // Verify blast radius works
    let blast = query::blast_radius::query(&store, "src/main.rs", 3);
    assert!(
        !blast.is_empty(),
        "blast radius from main.rs should find connected entities"
    );

    // Verify hotspots returns something
    let hot = query::hotspots::query(&store, 5);
    assert!(!hot.is_empty(), "should have hotspot entries");
}

#[test]
fn test_git_mining_on_self() {
    // Test git mining on the cartograph repo itself
    let repo_path = Path::new(env!("CARGO_MANIFEST_DIR"));

    let commits = historian::mine_commits(repo_path, None).unwrap();
    assert!(!commits.is_empty(), "should find commits in own repo");

    let cochanges = historian::analyze_cochanges(&commits);
    // With multiple commits, there should be some co-change pairs
    // (files that were committed together)
    assert!(!cochanges.is_empty(), "should find co-change pairs");
}

#[test]
fn test_typescript_pipeline() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    schema::init_db(&conn).unwrap();
    let mut store = GraphStore::new(conn).unwrap();

    let repo_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample_ts_repo");
    parser::index_repo(&repo_path, &mut store).unwrap();

    // 5 File entities: main.ts, utils.ts, types.ts, index.ts, external.ts
    let entities = store.all_entities();
    let file_entities: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.kind, cartograph::store::schema::EntityKind::File))
        .collect();
    assert_eq!(
        file_entities.len(),
        5,
        "expected 5 File entities, got {}. Entities: {:?}",
        file_entities.len(),
        file_entities.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    // main.ts -> utils.ts and main.ts -> types.ts
    let main = store
        .find_entity_by_path("src/main.ts")
        .expect("main.ts not found");
    let main_deps = store.dependencies(&main.id, petgraph::Direction::Outgoing);
    let main_dep_paths: Vec<_> = main_deps.iter().filter_map(|e| e.path.as_deref()).collect();
    assert!(
        main_dep_paths.contains(&"src/utils.ts"),
        "main.ts should import utils.ts, got: {:?}",
        main_dep_paths
    );
    assert!(
        main_dep_paths.contains(&"src/types.ts"),
        "main.ts should import types.ts, got: {:?}",
        main_dep_paths
    );

    // index.ts -> main.ts (barrel export * from "./main.js")
    let index = store
        .find_entity_by_path("src/index.ts")
        .expect("index.ts not found");
    let index_deps = store.dependencies(&index.id, petgraph::Direction::Outgoing);
    let index_dep_paths: Vec<_> = index_deps
        .iter()
        .filter_map(|e| e.path.as_deref())
        .collect();
    assert!(
        index_dep_paths.contains(&"src/main.ts"),
        "index.ts should import main.ts (barrel), got: {:?}",
        index_dep_paths
    );

    // external.ts -> no outgoing edges (all non-relative specifiers dropped)
    let external = store
        .find_entity_by_path("src/external.ts")
        .expect("external.ts not found");
    let external_deps = store.dependencies(&external.id, petgraph::Direction::Outgoing);
    assert!(
        external_deps.is_empty(),
        "external.ts should have 0 outgoing edges, got: {:?}",
        external_deps
    );

    // blast radius from types.ts must include utils.ts and main.ts
    let blast = query::blast_radius::query(&store, "src/types.ts", 3);
    let blast_paths: Vec<_> = blast
        .iter()
        .filter_map(|r| r.entity_path.as_deref())
        .collect();
    assert!(
        blast_paths.contains(&"src/utils.ts"),
        "blast radius from types.ts should include utils.ts, got: {:?}",
        blast_paths
    );
    assert!(
        blast_paths.contains(&"src/main.ts"),
        "blast radius from types.ts should include main.ts, got: {:?}",
        blast_paths
    );
}

#[test]
fn test_mixed_language_pipeline() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    schema::init_db(&conn).unwrap();
    let mut store = GraphStore::new(conn).unwrap();

    let repo_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample_mixed_repo");
    parser::index_repo(&repo_path, &mut store).unwrap();

    // Exactly 2 File entities: lib.rs and index.ts
    let entities = store.all_entities();
    let file_entities: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.kind, cartograph::store::schema::EntityKind::File))
        .collect();
    assert_eq!(
        file_entities.len(),
        2,
        "expected 2 File entities, got {:?}",
        file_entities.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    // Zero edges -- neither file imports anything
    let lib = store
        .find_entity_by_path("src/lib.rs")
        .expect("lib.rs not found");
    let ts = store
        .find_entity_by_path("src/index.ts")
        .expect("index.ts not found");
    assert!(
        store
            .dependencies(&lib.id, petgraph::Direction::Outgoing)
            .is_empty()
    );
    assert!(
        store
            .dependencies(&ts.id, petgraph::Direction::Outgoing)
            .is_empty()
    );

    // Language metadata
    assert_eq!(lib.language.as_deref(), Some("rust"));
    assert_eq!(ts.language.as_deref(), Some("typescript"));
}
