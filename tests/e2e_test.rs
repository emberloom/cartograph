use cartograph::store::{schema, graph::GraphStore};
use cartograph::{parser, historian, query};
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
    assert!(!deps.is_empty(), "main.rs should have outgoing dependencies");

    // Verify blast radius works
    let blast = query::blast_radius::query(&store, "src/main.rs", 3);
    assert!(!blast.is_empty(), "blast radius from main.rs should find connected entities");

    // Verify hotspots returns something
    let hot = query::hotspots::query(&store, 5);
    assert!(!hot.is_empty(), "should have hotspot entries");
}

#[test]
fn test_git_mining_on_self() {
    // Test git mining on the cartograph repo itself
    let repo_path = Path::new(env!("CARGO_MANIFEST_DIR"));

    let commits = historian::mine_commits(repo_path, Some(10)).unwrap();
    assert!(!commits.is_empty(), "should find commits in own repo");

    let cochanges = historian::analyze_cochanges(&commits);
    // With multiple commits, there should be some co-change pairs
    // (files that were committed together)
    assert!(!cochanges.is_empty(), "should find co-change pairs");
}
