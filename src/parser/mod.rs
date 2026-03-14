pub mod rust;
pub use rust::{parse_rust_source, ParseResult, ParsedEntity};

use anyhow::Result;
use std::path::Path;

use crate::store::graph::GraphStore;
use crate::store::schema::{EdgeKind, EntityKind};

/// Walk `repo_path` recursively, parse all `.rs` files (skipping `target/`),
/// and populate the graph store with File/Function/Struct/Trait entities and
/// inter-file dependency edges.
///
/// Safe to call multiple times — clears existing entities/edges before re-indexing.
pub fn index_repo(repo_path: &Path, store: &mut GraphStore) -> Result<()> {
    // Clear existing data for a clean re-index
    store.clear()?;

    // Collect all .rs files first (skip target/ directory)
    let mut rs_files: Vec<std::path::PathBuf> = Vec::new();
    collect_rs_files(repo_path, repo_path, &mut rs_files)?;

    // First pass: create a File entity for every .rs file and record path→id mapping
    let mut file_ids: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for abs_path in &rs_files {
        let rel_path = abs_path
            .strip_prefix(repo_path)
            .unwrap()
            .to_string_lossy()
            .to_string();
        let file_name = abs_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let id = store.add_entity(EntityKind::File, &file_name, Some(&rel_path), Some("rust"))?;
        file_ids.insert(rel_path, id);
    }

    // Second pass: parse each file, add child entities, add inter-file edges
    for abs_path in &rs_files {
        let rel_path = abs_path
            .strip_prefix(repo_path)
            .unwrap()
            .to_string_lossy()
            .to_string();
        let file_id = file_ids[&rel_path].clone();

        let source = std::fs::read_to_string(abs_path)?;
        let parse_result = parse_rust_source(&source, abs_path);

        // Add function/struct/trait/impl entities as children of this file.
        // Use a qualified path `<file_path>::<name>` so that find_entity_by_path
        // on the bare file path still resolves to the File entity, not a child.
        for entity in &parse_result.entities {
            let kind = match entity.kind.as_str() {
                "Function" => EntityKind::Function,
                "Struct" => EntityKind::Struct,
                "Trait" => EntityKind::Trait,
                "Impl" => EntityKind::Impl,
                _ => continue,
            };
            let qualified_path = format!("{}::{}", rel_path, entity.name);
            store.add_entity(kind, &entity.name, Some(&qualified_path), Some("rust"))?;
        }

        // Resolve `mod foo;` declarations to target file paths
        for mod_name in &parse_result.modules {
            let target_paths = resolve_mod_paths(&rel_path, mod_name);
            for target_rel in target_paths {
                if let Some(target_id) = file_ids.get(&target_rel) {
                    store.add_edge(&file_id, target_id, EdgeKind::Imports, 1.0)?;
                    break; // only the first match that exists
                }
            }
        }

        // Resolve `use crate::…` imports to target file paths
        for import in &parse_result.imports {
            // Only handle crate-internal paths starting with "crate::"
            if let Some(inner) = import.strip_prefix("crate::") {
                let module_path = inner.split("::").next().unwrap_or("");
                if module_path.is_empty() {
                    continue;
                }
                // Attempt to find the file that corresponds to this module
                let candidate = format!("src/{}.rs", module_path);
                if let Some(target_id) = file_ids.get(&candidate) {
                    store.add_edge(&file_id, target_id, EdgeKind::DependsOn, 1.0)?;
                }
            }
        }
    }

    Ok(())
}

/// Recursively collect all `.rs` files under `dir`, skipping `target/`.
fn collect_rs_files(
    _root: &Path,
    dir: &Path,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            // Skip target directory
            if path.file_name().map(|n| n == "target").unwrap_or(false) {
                continue;
            }
            collect_rs_files(_root, &path, out)?;
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(path);
        }
    }
    Ok(())
}

/// Given the relative path of the file containing `mod <name>;` and the module
/// name, return the candidate relative paths for the target file in priority
/// order.
///
/// Rules:
/// - `mod foo;` in `src/main.rs` or `src/lib.rs`  → `src/foo.rs`, `src/foo/mod.rs`
/// - `mod foo;` in `src/bar.rs`                    → `src/bar/foo.rs`, `src/bar/foo/mod.rs`
fn resolve_mod_paths(declaring_rel: &str, mod_name: &str) -> Vec<String> {
    let parent = Path::new(declaring_rel).parent().unwrap_or(Path::new(""));
    let file_stem = Path::new(declaring_rel)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    if file_stem == "main" || file_stem == "lib" || file_stem == "mod" {
        // Sibling: src/foo.rs or src/foo/mod.rs
        vec![
            parent.join(format!("{}.rs", mod_name)).to_string_lossy().to_string(),
            parent.join(mod_name).join("mod.rs").to_string_lossy().to_string(),
        ]
    } else {
        // Sub-module: src/bar/foo.rs or src/bar/foo/mod.rs
        vec![
            parent.join(&file_stem).join(format!("{}.rs", mod_name)).to_string_lossy().to_string(),
            parent.join(&file_stem).join(mod_name).join("mod.rs").to_string_lossy().to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::graph::GraphStore;
    use std::path::Path;

    #[test]
    fn test_index_sample_repo() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::store::schema::init_db(&conn).unwrap();
        let mut store = GraphStore::new(conn).unwrap();

        let repo_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample_repo");
        index_repo(&repo_path, &mut store).unwrap();

        // Should have file entities
        let main = store.find_entity_by_path("src/main.rs");
        assert!(main.is_some(), "main.rs should be indexed");

        let auth = store.find_entity_by_path("src/auth.rs");
        assert!(auth.is_some(), "auth.rs should be indexed");

        // main.rs should have edges to auth (mod declaration)
        let main_id = &main.unwrap().id;
        let deps = store.dependencies(main_id, petgraph::Direction::Outgoing);
        assert!(!deps.is_empty(), "main.rs should have dependencies");
    }
}