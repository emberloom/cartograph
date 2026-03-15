pub mod rust;
pub use rust::{ParseResult, ParsedEntity, parse_rust_source};
pub mod typescript;
pub use typescript::parse_typescript_source;

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

    // Collect all source files first (skip target/, node_modules/, etc.)
    let mut all_files: Vec<std::path::PathBuf> = Vec::new();
    collect_source_files(repo_path, repo_path, &mut all_files)?;

    // Pass 1: register File entities
    let mut file_ids: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut rs_count: usize = 0;
    let mut ts_count: usize = 0;
    for abs_path in &all_files {
        let Some(rel) = abs_path.strip_prefix(repo_path).ok() else {
            tracing::warn!("skipping path outside repo: {}", abs_path.display());
            continue;
        };
        let rel_path = rel.to_string_lossy().to_string();
        let file_name = abs_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let lang = match abs_path.extension().map(|e| e.to_string_lossy().to_string()).as_deref() {
            Some("rs") => { rs_count += 1; "rust" }
            _ => { ts_count += 1; "typescript" }
        };
        let id = store.add_entity(EntityKind::File, &file_name, Some(&rel_path), Some(lang))?;
        file_ids.insert(rel_path, id);
    }

    // Pass 2: parse each file and wire edges
    for abs_path in &all_files {
        let Some(rel) = abs_path.strip_prefix(repo_path).ok() else {
            continue;
        };
        let rel_path = rel.to_string_lossy().to_string();

        let file_id = match file_ids.get(&rel_path) {
            Some(id) => id.clone(),
            None => continue,
        };

        let ext = abs_path.extension().map(|e| e.to_string_lossy().to_string());
        match ext.as_deref() {
            Some("rs") => {
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
                    // Reject module names containing path traversal components
                    if mod_name.contains("..") || mod_name.contains('/') || mod_name.contains('\\') {
                        continue;
                    }
                    let target_paths = resolve_mod_paths(&rel_path, mod_name);
                    for target_rel in target_paths {
                        // Ensure resolved path doesn't escape repo (no ".." components)
                        if target_rel.contains("..") {
                            continue;
                        }
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
            Some("ts") | Some("tsx") => {
                let source = std::fs::read_to_string(abs_path)?;
                let parse_result = parse_typescript_source(&source, abs_path);
                for specifier in &parse_result.imports {
                    if let Some(target_id) = resolve_ts_import(&rel_path, specifier, &file_ids) {
                        store.add_edge(&file_id, &target_id, EdgeKind::Imports, 1.0)?;
                    }
                }
            }
            _ => {}
        }
    }

    let _ = (rs_count, ts_count);
    Ok(())
}

/// Recursively collect all `.rs`, `.ts`, and `.tsx` source files under `dir`.
///
/// Skips directories: `target/`, `node_modules/`, `dist/`, `.next/`, `build/`
/// Skips `.d.ts` files (TypeScript declaration files — no runtime code).
fn collect_source_files(_root: &Path, dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
    let skip_dirs = ["target", "node_modules", "dist", ".next", "build"];
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name() {
                if skip_dirs.contains(&name.to_string_lossy().as_ref()) {
                    continue;
                }
            }
            collect_source_files(_root, &path, out)?;
        } else {
            let ext = path.extension().map(|e| e.to_string_lossy().to_string());
            match ext.as_deref() {
                Some("rs") => out.push(path),
                Some("ts") | Some("tsx") => {
                    if !path.to_string_lossy().ends_with(".d.ts") {
                        out.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Resolve a TypeScript ESM import specifier to a `file_ids` entity id.
///
/// Returns `Some(entity_id)` if a target file is found, `None` otherwise.
///
/// - Non-relative specifiers (no `./` or `../`) → `None`
/// - Strips extension from stem, tries: `<stem>.ts`, `<stem>/index.ts`, `<stem>.tsx`, `<stem>/index.tsx`
/// - Rejects resolved paths containing `..` (path traversal guard)
fn resolve_ts_import(
    declaring_rel: &str,
    specifier: &str,
    file_ids: &std::collections::HashMap<String, String>,
) -> Option<String> {
    if !specifier.starts_with("./") && !specifier.starts_with("../") {
        return None;
    }

    let declaring_dir = Path::new(declaring_rel).parent().unwrap_or(Path::new(""));

    let spec_path = Path::new(specifier);
    let stem = spec_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let spec_dir = spec_path.parent().unwrap_or(Path::new(".")).to_string_lossy().to_string();
    let spec_dir = if spec_dir == "." { String::new() } else { spec_dir + "/" };

    let candidates: Vec<String> = vec![
        format!("{}{}.ts", spec_dir, stem),
        format!("{}{}/index.ts", spec_dir, stem),
        format!("{}{}.tsx", spec_dir, stem),
        format!("{}{}/index.tsx", spec_dir, stem),
    ];

    for candidate in &candidates {
        let joined = declaring_dir.join(candidate);
        let joined_str = joined.to_string_lossy().to_string();
        if joined_str.contains("..") {
            continue;
        }
        if let Some(id) = file_ids.get(&joined_str) {
            return Some(id.clone());
        }
    }
    None
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
            parent
                .join(format!("{}.rs", mod_name))
                .to_string_lossy()
                .to_string(),
            parent
                .join(mod_name)
                .join("mod.rs")
                .to_string_lossy()
                .to_string(),
        ]
    } else {
        // Sub-module: src/bar/foo.rs or src/bar/foo/mod.rs
        vec![
            parent
                .join(&file_stem)
                .join(format!("{}.rs", mod_name))
                .to_string_lossy()
                .to_string(),
            parent
                .join(&file_stem)
                .join(mod_name)
                .join("mod.rs")
                .to_string_lossy()
                .to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::graph::GraphStore;
    use std::path::Path;

    #[test]
    fn test_collect_source_files_finds_ts_and_rs() {
        let repo_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample_mixed_repo");
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        collect_source_files(&repo_path, &repo_path, &mut files).unwrap();

        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(names.contains(&"lib.rs".to_string()), "should find lib.rs");
        assert!(names.contains(&"index.ts".to_string()), "should find index.ts");
        assert_eq!(files.len(), 2, "should find exactly 2 files, got: {:?}", names);
    }

    #[test]
    fn test_collect_source_files_skips_dts() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample_ts_repo");
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        collect_source_files(&repo, &repo, &mut files).unwrap();
        for f in &files {
            assert!(
                !f.to_string_lossy().ends_with(".d.ts"),
                "should not collect .d.ts files: {:?}", f
            );
        }
        assert_eq!(files.len(), 5, "sample_ts_repo/src has 5 .ts files, got: {:?}", files);
    }

    #[test]
    fn test_ts_resolution_js_extension_rewrite() {
        let mut file_ids = std::collections::HashMap::new();
        file_ids.insert("src/utils.ts".to_string(), "id-utils".to_string());
        let result = resolve_ts_import("src/main.ts", "./utils.js", &file_ids);
        assert_eq!(result, Some("id-utils".to_string()),
            "expected id-utils from .js→.ts rewrite");
    }

    #[test]
    fn test_ts_resolution_no_extension() {
        let mut file_ids = std::collections::HashMap::new();
        file_ids.insert("src/utils.ts".to_string(), "id-utils".to_string());
        let result = resolve_ts_import("src/main.ts", "./utils", &file_ids);
        assert_eq!(result, Some("id-utils".to_string()),
            "expected id-utils for bare specifier");
    }

    #[test]
    fn test_ts_resolution_index_fallback() {
        let mut file_ids = std::collections::HashMap::new();
        file_ids.insert("src/utils/index.ts".to_string(), "id-utils-idx".to_string());
        let result = resolve_ts_import("src/main.ts", "./utils", &file_ids);
        assert_eq!(result, Some("id-utils-idx".to_string()),
            "expected index.ts fallback");
    }

    #[test]
    fn test_ts_resolution_non_relative_skipped() {
        let file_ids = std::collections::HashMap::new();
        let result = resolve_ts_import("src/main.ts", "vitest", &file_ids);
        assert_eq!(result, None, "non-relative specifiers must return None");
    }

    #[test]
    fn test_ts_resolution_path_traversal_rejected() {
        let mut file_ids = std::collections::HashMap::new();
        file_ids.insert("../outside.ts".to_string(), "danger".to_string());
        let result = resolve_ts_import("src/main.ts", "../../outside", &file_ids);
        assert_eq!(result, None, "path traversal must be rejected");
    }

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
