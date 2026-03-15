# TypeScript Parser Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add TypeScript/TSX import parsing to Cartograph so that TypeScript monorepos (target: openclaw, ~6,064 files) can be fully indexed and queried.

**Architecture:** Add `src/parser/typescript.rs` that mirrors the existing `src/parser/rust.rs` interface, returning a `ParseResult` with raw import specifiers. Extend `src/parser/mod.rs` to walk `.ts`/`.tsx` files alongside `.rs`, register them as `File` entities, dispatch parsing by extension, and resolve relative ESM import specifiers (with `.js`→`.ts` rewrite) to `EdgeKind::Imports` graph edges.

**Tech Stack:** Rust stable, `tree-sitter = "0.25"`, `tree-sitter-typescript = "0.23.2"` (already in `Cargo.toml`), `rusqlite`, `petgraph`

---

## Chunk 1: Fixtures

**Files:**
- Create: `fixtures/sample_ts_repo/src/main.ts`
- Create: `fixtures/sample_ts_repo/src/utils.ts`
- Create: `fixtures/sample_ts_repo/src/types.ts`
- Create: `fixtures/sample_ts_repo/src/index.ts`
- Create: `fixtures/sample_ts_repo/src/external.ts`
- Create: `fixtures/sample_mixed_repo/src/lib.rs`
- Create: `fixtures/sample_mixed_repo/src/index.ts`

### Task 1: Create the TypeScript fixture repo

These files are used by the unit and integration tests. Create them exactly as specified — the import paths use `.js` extensions (ESM convention) even though the source files are `.ts`.

- [ ] **Step 1: Create `fixtures/sample_ts_repo/src/main.ts`**

```typescript
import { Foo } from "./utils.js";
import { Bar } from "./types.js";

export function main(): void {
    console.log(new Foo(), new Bar());
}
```

- [ ] **Step 2: Create `fixtures/sample_ts_repo/src/utils.ts`**

```typescript
import type { Bar } from "./types.js";

export class Foo {
    bar?: Bar;
}
```

- [ ] **Step 3: Create `fixtures/sample_ts_repo/src/types.ts`**

```typescript
export interface Bar {
    id: number;
}
```

- [ ] **Step 4: Create `fixtures/sample_ts_repo/src/index.ts`**

```typescript
export * from "./main.js";
```

- [ ] **Step 5: Create `fixtures/sample_ts_repo/src/external.ts`**

```typescript
import { describe } from "vitest";
import * as fs from "node:fs";
import { spawn } from "node:child_process";

export function run(): void {
    describe("test", () => {});
    fs.readFileSync("x");
    spawn("ls", []);
}
```

- [ ] **Step 6: Create the mixed-language fixture**

```bash
mkdir -p /tmp/cartograph-fix/fixtures/sample_mixed_repo/src
```

`fixtures/sample_mixed_repo/src/lib.rs`:
```rust
// no imports
pub fn hello() -> &'static str {
    "hello"
}
```

`fixtures/sample_mixed_repo/src/index.ts`:
```typescript
// no imports
export const greeting = "hello";
```

- [ ] **Step 7: Verify fixture layout**

```bash
find /tmp/cartograph-fix/fixtures -type f | sort
```

Expected output includes:
```
fixtures/sample_mixed_repo/src/index.ts
fixtures/sample_mixed_repo/src/lib.rs
fixtures/sample_repo/src/auth.rs
fixtures/sample_repo/src/billing.rs
fixtures/sample_repo/src/lib.rs
fixtures/sample_repo/src/main.rs
fixtures/sample_ts_repo/src/external.ts
fixtures/sample_ts_repo/src/index.ts
fixtures/sample_ts_repo/src/main.ts
fixtures/sample_ts_repo/src/types.ts
fixtures/sample_ts_repo/src/utils.ts
```

- [ ] **Step 8: Commit fixtures**

```bash
cd /tmp/cartograph-fix
git add fixtures/
git commit -m "test: add TypeScript and mixed-language fixtures"
```

---

## Chunk 2: TypeScript parser unit

**Files:**
- Create: `src/parser/typescript.rs`
- Modify: `src/parser/mod.rs` (add `pub mod typescript;` and re-export)

### Task 2: Write `parse_typescript_source` with failing tests first

The function uses `tree-sitter-typescript` to walk the AST, extracting the `"source"` field from every `import_statement` and `export_statement` node. It returns a `ParseResult` with all found specifiers in `imports` (unfiltered — non-relative ones are dropped later in `mod.rs`).

**Key tree-sitter API notes:**
- Create a `tree_sitter::Parser`, call `parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())` (or `LANGUAGE_TSX` for `.tsx`)
- `parser.parse(source, None)` → `Option<Tree>`; return empty `ParseResult` on `None`
- Walk the tree with a `TreeCursor`: call `cursor.goto_first_child()` / `cursor.goto_next_sibling()` / `cursor.goto_parent()` to traverse
- `node.kind()` returns the node type string (e.g., `"import_statement"`)
- `node.child_by_field_name("source")` returns `Option<Node>` — the `string` AST node containing the specifier
- `&source[node.start_byte()..node.end_byte()]` extracts the raw text including quotes
- Strip quotes with `.trim_matches('"').trim_matches('\'')`

- [ ] **Step 1: Add module declaration to `src/parser/mod.rs`**

At the top of `src/parser/mod.rs`, add after `pub mod rust;`:
```rust
pub mod typescript;
pub use typescript::parse_typescript_source;
```

- [ ] **Step 2: Create `src/parser/typescript.rs` with the test module only**

```rust
use std::path::Path;

use super::ParseResult;

pub fn parse_typescript_source(source: &str, path: &Path) -> ParseResult {
    // TODO: implement
    let _ = (source, path);
    ParseResult {
        entities: Vec::new(),
        imports: Vec::new(),
        modules: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ts_path() -> &'static Path {
        Path::new("src/foo.ts")
    }

    fn tsx_path() -> &'static Path {
        Path::new("src/foo.tsx")
    }

    #[test]
    fn test_parse_named_import() {
        let src = r#"import { foo } from "./bar.js";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(result.imports.contains(&"./bar.js".to_string()),
            "expected ./bar.js in imports, got: {:?}", result.imports);
    }

    #[test]
    fn test_parse_default_import() {
        let src = r#"import foo from "./bar.js";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(result.imports.contains(&"./bar.js".to_string()),
            "expected ./bar.js in imports, got: {:?}", result.imports);
    }

    #[test]
    fn test_parse_type_import() {
        let src = r#"import type { T } from "./types.js";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(result.imports.contains(&"./types.js".to_string()),
            "expected ./types.js in imports, got: {:?}", result.imports);
    }

    #[test]
    fn test_parse_export_from() {
        let src = r#"export { foo } from "./utils.js";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(result.imports.contains(&"./utils.js".to_string()),
            "expected ./utils.js in imports, got: {:?}", result.imports);
    }

    #[test]
    fn test_parse_export_star() {
        let src = r#"export * from "./core.js";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(result.imports.contains(&"./core.js".to_string()),
            "expected ./core.js in imports, got: {:?}", result.imports);
    }

    #[test]
    fn test_parse_export_no_source() {
        // export const x = 1 has no "source" field — imports must be empty
        let src = r#"export const x = 1;"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(result.imports.is_empty(),
            "expected empty imports, got: {:?}", result.imports);
    }

    #[test]
    fn test_emits_package_import_specifier() {
        // Parser emits non-relative specifiers; mod.rs drops them at resolution time
        let src = r#"import { x } from "vitest";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(result.imports.contains(&"vitest".to_string()),
            "expected vitest in raw imports, got: {:?}", result.imports);
    }

    #[test]
    fn test_emits_node_protocol_specifier() {
        let src = r#"import fs from "node:fs";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(result.imports.contains(&"node:fs".to_string()),
            "expected node:fs in raw imports, got: {:?}", result.imports);
    }

    #[test]
    fn test_parse_tsx() {
        // .tsx extension selects LANGUAGE_TSX grammar; JSX syntax must not crash the parser
        let src = r#"import React from "./react.js";
export function App(): JSX.Element { return <div />; }"#;
        let result = parse_typescript_source(src, tsx_path());
        assert!(result.imports.contains(&"./react.js".to_string()),
            "expected ./react.js in tsx imports, got: {:?}", result.imports);
    }

    #[test]
    fn test_modules_always_empty() {
        let src = r#"import { foo } from "./bar.js";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(result.modules.is_empty(), "modules must always be empty for TypeScript");
    }

    #[test]
    fn test_entities_always_empty() {
        let src = r#"export class Foo {}"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(result.entities.is_empty(), "entities must be empty in v1");
    }
}
```

- [ ] **Step 3: Run failing tests to confirm they fail for the right reason**

```bash
cd /tmp/cartograph-fix && cargo test -p cartograph typescript:: -- --nocapture 2>&1 | head -40
```

Expected: tests compile but fail — `test_parse_named_import` etc. fail because `result.imports` is empty (stub returns empty vec). If you see a *compile error* instead, check that `use super::{ParseResult, ParsedEntity};` resolves — `ParsedEntity` is in `src/parser/rust.rs` and re-exported from `mod.rs`; the `super::` path from `typescript.rs` reaches `mod.rs`'s exports.

- [ ] **Step 4: Implement `parse_typescript_source`**

Replace the stub body in `src/parser/typescript.rs`:

```rust
use std::path::Path;

use super::ParseResult;

pub fn parse_typescript_source(source: &str, path: &Path) -> ParseResult {
    // Select grammar by extension: .tsx uses LANGUAGE_TSX, everything else uses LANGUAGE_TYPESCRIPT
    let is_tsx = path.extension().map(|e| e == "tsx").unwrap_or(false);

    let mut parser = tree_sitter::Parser::new();
    let language = if is_tsx {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    } else {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    };

    if parser.set_language(&language).is_err() {
        tracing::error!("failed to load TypeScript grammar");
        return ParseResult {
            entities: Vec::new(),
            imports: Vec::new(),
            modules: Vec::new(),
        };
    }

    let Some(tree) = parser.parse(source, None) else {
        tracing::warn!(
            "tree-sitter failed to parse TypeScript source ({} bytes)",
            source.len()
        );
        return ParseResult {
            entities: Vec::new(),
            imports: Vec::new(),
            modules: Vec::new(),
        };
    };

    let mut result = ParseResult {
        entities: Vec::new(),
        imports: Vec::new(),
        modules: Vec::new(),
    };

    // Walk every node in the tree looking for import_statement and export_statement
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        let kind = node.kind();

        if kind == "import_statement" || kind == "export_statement" {
            if let Some(source_node) = node.child_by_field_name("source") {
                let raw = &source[source_node.start_byte()..source_node.end_byte()];
                let specifier = raw.trim_matches('"').trim_matches('\'');
                if !specifier.is_empty() {
                    result.imports.push(specifier.to_string());
                }
            }
        }

        // Descend into children; if no children, move to next sibling; if no sibling, go up
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return result;
            }
        }
    }
}
```

- [ ] **Step 5: Run all TypeScript parser tests**

```bash
cd /tmp/cartograph-fix && cargo test -p cartograph typescript:: -- --nocapture 2>&1
```

Expected: all 11 tests in `src/parser/typescript.rs` pass. If `test_parse_tsx` fails with a parse error, verify that `LANGUAGE_TSX` is being selected for `.tsx` paths (check the `is_tsx` branch).

- [ ] **Step 6: Run the full test suite to check nothing regressed**

```bash
cd /tmp/cartograph-fix && cargo test --all 2>&1 | tail -20
```

Expected: all pre-existing tests still pass. The only new failures allowed are integration tests added in later tasks.

- [ ] **Step 7: Commit**

```bash
cd /tmp/cartograph-fix
git add src/parser/typescript.rs src/parser/mod.rs
git commit -m "feat: add parse_typescript_source with tree-sitter-typescript"
```

---

## Chunk 3: `mod.rs` — file collection and entity registration

**Files:**
- Modify: `src/parser/mod.rs`

### Task 3: Replace `collect_rs_files` with `collect_source_files`

The existing `collect_rs_files` helper only walks `.rs` files and skips `target/`. Replace it with `collect_source_files` that handles all supported extensions and all required skip directories.

- [ ] **Step 1: Add a failing test for `collect_source_files`**

In the `#[cfg(test)]` block at the bottom of `src/parser/mod.rs`, add:

```rust
#[test]
fn test_collect_source_files_finds_ts_and_rs() {
    // Uses the mixed fixture: one .rs and one .ts file
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
    // .d.ts files must be excluded; we test this by verifying the fixture's .ts
    // file is found but a hypothetical .d.ts would not be (tested via extension check)
    // Since we don't have a .d.ts in the fixture, verify the helper's filter logic
    // by confirming that a path ending in ".d.ts" is not a valid extension match.
    // The real guard: extension == "ts" && !path ends with ".d.ts"
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
```

- [ ] **Step 2: Run to confirm the tests fail (function not yet renamed)**

```bash
cd /tmp/cartograph-fix && cargo test -p cartograph test_collect_source_files -- --nocapture 2>&1
```

Expected: compile error — `collect_source_files` doesn't exist yet.

- [ ] **Step 3: Replace `collect_rs_files` with `collect_source_files` in `mod.rs`**

Remove the entire `collect_rs_files` function and replace with:

```rust
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
                    // Exclude .d.ts declaration files
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
```

Also update the call site in `index_repo` (replace `collect_rs_files(repo_path, repo_path, &mut rs_files)?;`):

```rust
let mut all_files: Vec<std::path::PathBuf> = Vec::new();
collect_source_files(repo_path, repo_path, &mut all_files)?;
```

Rename the local variable from `rs_files` to `all_files` throughout `index_repo`. At this point `index_repo` still only handles `.rs` files in Pass 1 and Pass 2 — that gets fixed in Task 4.

- [ ] **Step 4: Run collection tests**

```bash
cd /tmp/cartograph-fix && cargo test -p cartograph test_collect_source_files -- --nocapture 2>&1
```

Expected: both `test_collect_source_files_finds_ts_and_rs` and `test_collect_source_files_skips_dts` pass.

- [ ] **Step 5: Run full test suite**

```bash
cd /tmp/cartograph-fix && cargo test --all 2>&1 | tail -20
```

Expected: all existing tests still pass. The `test_index_sample_repo` test calls `index_repo` which now uses `collect_source_files`; since `fixtures/sample_repo` has only `.rs` files, nothing changes functionally.

- [ ] **Step 6: Commit**

```bash
cd /tmp/cartograph-fix
git add src/parser/mod.rs
git commit -m "refactor: replace collect_rs_files with collect_source_files (multi-language)"
```

---

## Chunk 4: `mod.rs` — Pass 1 & Pass 2 for TypeScript + resolution helper

**Files:**
- Modify: `src/parser/mod.rs`

### Task 4: Update `index_repo` to register TypeScript files and wire import edges

Pass 1 assigns language metadata per file extension. Pass 2 dispatches to `parse_typescript_source` for `.ts`/`.tsx` files and resolves relative specifiers through a new private `resolve_ts_import` helper.

- [ ] **Step 1: Add the resolution unit test (failing first)**

In the `#[cfg(test)]` block in `mod.rs`, add:

```rust
#[test]
fn test_ts_resolution_js_extension_rewrite() {
    // Simulates: declaring file "src/main.ts", specifier "./utils.js"
    // file_ids has "src/utils.ts" → "id-utils"
    // Expected: resolve_ts_import finds "src/utils.ts" via .js→.ts rewrite
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
```

- [ ] **Step 2: Run to confirm tests fail (function doesn't exist)**

```bash
cd /tmp/cartograph-fix && cargo test -p cartograph test_ts_resolution -- --nocapture 2>&1 | head -20
```

Expected: compile error — `resolve_ts_import` not found.

- [ ] **Step 3: Add the `resolve_ts_import` private helper**

Add this function to `src/parser/mod.rs` (alongside `resolve_mod_paths`):

```rust
/// Resolve a TypeScript ESM import specifier to a `file_ids` key.
///
/// Returns `Some(entity_id)` if a target file is found, `None` otherwise.
///
/// Rules:
/// - Non-relative specifiers (no `./` or `../` prefix) → `None` (npm packages, node: protocol)
/// - Strips any extension from the specifier stem and tries four candidates in priority order:
///   `<stem>.ts`, `<stem>/index.ts`, `<stem>.tsx`, `<stem>/index.tsx`
/// - A direct `.ts` or `.tsx` extension in the specifier is also tried as-is first
/// - Rejects any resolved path that contains `..` (path traversal guard)
fn resolve_ts_import(
    declaring_rel: &str,
    specifier: &str,
    file_ids: &std::collections::HashMap<String, String>,
) -> Option<String> {
    // Skip non-relative specifiers
    if !specifier.starts_with("./") && !specifier.starts_with("../") {
        return None;
    }

    let declaring_dir = Path::new(declaring_rel).parent().unwrap_or(Path::new(""));

    // Strip extension to get the stem, then build candidate list
    let spec_path = Path::new(specifier);
    let stem = spec_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let spec_dir = spec_path
        .parent()
        .unwrap_or(Path::new("."))
        .to_string_lossy()
        .to_string();
    let spec_dir = if spec_dir == "." { String::new() } else { spec_dir + "/" };

    let candidates: Vec<String> = vec![
        format!("{}{}.ts", spec_dir, stem),
        format!("{}{}/index.ts", spec_dir, stem),
        format!("{}{}.tsx", spec_dir, stem),
        format!("{}{}/index.tsx", spec_dir, stem),
    ];

    for candidate in &candidates {
        let joined = declaring_dir.join(candidate);
        // Normalize by converting to string and checking for path traversal
        let joined_str = joined.to_string_lossy().to_string();
        if joined_str.contains("..") {
            continue; // path traversal guard
        }
        if let Some(id) = file_ids.get(&joined_str) {
            return Some(id.clone());
        }
    }
    None
}
```

- [ ] **Step 4: Run resolution tests**

```bash
cd /tmp/cartograph-fix && cargo test -p cartograph test_ts_resolution -- --nocapture 2>&1
```

Expected: all 5 resolution tests pass.

- [ ] **Step 5: Update `index_repo` Pass 1 to assign language by extension**

In the Pass 1 loop in `index_repo`, replace the hardcoded `Some("rust")` language with extension-based detection:

```rust
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
```

Store `rs_count` and `ts_count` as local variables — they'll be used in Task 5 for the CLI output change.

- [ ] **Step 6: Update `index_repo` Pass 2 to handle TypeScript files**

Replace the existing Pass 2 `for abs_path in &all_files { ... }` loop body with the following structure. The key changes: (a) derive `rel_path` and `file_id` at the top of every iteration using `.get()` (not indexing, which panics on missing keys), and (b) dispatch by extension:

```rust
// Pass 2: parse each file and wire edges
for abs_path in &all_files {
    // Derive rel_path — same pattern as Pass 1
    let Some(rel) = abs_path.strip_prefix(repo_path).ok() else {
        continue;
    };
    let rel_path = rel.to_string_lossy().to_string();

    // Look up the entity id registered in Pass 1 — use .get() to avoid panic
    let file_id = match file_ids.get(&rel_path) {
        Some(id) => id.clone(),
        None => continue, // should not happen; skip gracefully
    };

    let ext = abs_path.extension().map(|e| e.to_string_lossy().to_string());
    match ext.as_deref() {
        Some("rs") => {
            // keep existing Rust handling here, unchanged:
            // parse_rust_source → entity children → mod resolution (EdgeKind::Imports)
            //                                      → use crate:: resolution (EdgeKind::DependsOn)
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
```

**What to preserve from existing Pass 2:** The Rust arm must contain the existing code verbatim (entity children loop, `mod` resolution block, `use crate::` resolution block). Only the outer loop scaffolding changes — `rel_path` derivation and `file_id` lookup move to the top and are shared by both arms.

- [ ] **Step 7: Run full test suite**

```bash
cd /tmp/cartograph-fix && cargo test --all 2>&1 | tail -30
```

Expected: all existing tests pass. New resolution tests pass.

- [ ] **Step 8: Commit**

```bash
cd /tmp/cartograph-fix
git add src/parser/mod.rs
git commit -m "feat: extend index_repo to index TypeScript files and resolve ESM imports"
```

---

## Chunk 5: Integration tests + CLI output

**Files:**
- Modify: `tests/e2e_test.rs`
- Modify: `src/main.rs` (line 85)

### Task 5: Write the TypeScript pipeline integration test

- [ ] **Step 1: Add the integration tests to `tests/e2e_test.rs`**

Add two new test functions after the existing `test_git_mining_on_self`. **Note:** The code blocks below use `\"` in `env!(\"CARGO_MANIFEST_DIR\")` as Markdown escaping only — write plain `"` in the actual Rust source file.

```rust
#[test]
fn test_typescript_pipeline() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    schema::init_db(&conn).unwrap();
    let mut store = GraphStore::new(conn).unwrap();

    let repo_path = Path::new(env!(\"CARGO_MANIFEST_DIR\")).join("fixtures/sample_ts_repo");
    parser::index_repo(&repo_path, &mut store).unwrap();

    // 5 File entities: main.ts, utils.ts, types.ts, index.ts, external.ts
    let entities = store.all_entities();
    let file_entities: Vec<_> = entities.iter()
        .filter(|e| matches!(e.kind, cartograph::store::schema::EntityKind::File))
        .collect();
    assert_eq!(file_entities.len(), 5,
        "expected 5 File entities, got {}. Entities: {:?}",
        file_entities.len(),
        file_entities.iter().map(|e| &e.name).collect::<Vec<_>>());

    // main.ts → utils.ts and main.ts → types.ts
    let main = store.find_entity_by_path("src/main.ts").expect("main.ts not found");
    let main_deps = store.dependencies(&main.id, petgraph::Direction::Outgoing);
    let main_dep_paths: Vec<_> = main_deps.iter()
        .filter_map(|e| e.path.as_deref())
        .collect();
    assert!(main_dep_paths.contains(&"src/utils.ts"),
        "main.ts should import utils.ts, got: {:?}", main_dep_paths);
    assert!(main_dep_paths.contains(&"src/types.ts"),
        "main.ts should import types.ts, got: {:?}", main_dep_paths);

    // index.ts → main.ts (barrel export * from "./main.js")
    let index = store.find_entity_by_path("src/index.ts").expect("index.ts not found");
    let index_deps = store.dependencies(&index.id, petgraph::Direction::Outgoing);
    let index_dep_paths: Vec<_> = index_deps.iter()
        .filter_map(|e| e.path.as_deref())
        .collect();
    assert!(index_dep_paths.contains(&"src/main.ts"),
        "index.ts should import main.ts (barrel), got: {:?}", index_dep_paths);

    // external.ts → no outgoing edges (all non-relative specifiers dropped)
    let external = store.find_entity_by_path("src/external.ts").expect("external.ts not found");
    let external_deps = store.dependencies(&external.id, petgraph::Direction::Outgoing);
    assert!(external_deps.is_empty(),
        "external.ts should have 0 outgoing edges, got: {:?}", external_deps);

    // blast radius from types.ts must include utils.ts and main.ts
    let blast = query::blast_radius::query(&store, "src/types.ts", 3);
    let blast_paths: Vec<_> = blast.iter()
        .filter_map(|r| r.entity_path.as_deref())
        .collect();
    assert!(blast_paths.contains(&"src/utils.ts"),
        "blast radius from types.ts should include utils.ts, got: {:?}", blast_paths);
    assert!(blast_paths.contains(&"src/main.ts"),
        "blast radius from types.ts should include main.ts, got: {:?}", blast_paths);
}

#[test]
fn test_mixed_language_pipeline() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    schema::init_db(&conn).unwrap();
    let mut store = GraphStore::new(conn).unwrap();

    let repo_path = Path::new(env!(\"CARGO_MANIFEST_DIR\")).join("fixtures/sample_mixed_repo");
    parser::index_repo(&repo_path, &mut store).unwrap();

    // Exactly 2 File entities: lib.rs and index.ts
    let entities = store.all_entities();
    let file_entities: Vec<_> = entities.iter()
        .filter(|e| matches!(e.kind, cartograph::store::schema::EntityKind::File))
        .collect();
    assert_eq!(file_entities.len(), 2,
        "expected 2 File entities, got {:?}",
        file_entities.iter().map(|e| &e.name).collect::<Vec<_>>());

    // Zero edges — neither file imports anything
    let lib = store.find_entity_by_path("src/lib.rs").expect("lib.rs not found");
    let ts = store.find_entity_by_path("src/index.ts").expect("index.ts not found");
    assert!(store.dependencies(&lib.id, petgraph::Direction::Outgoing).is_empty());
    assert!(store.dependencies(&ts.id, petgraph::Direction::Outgoing).is_empty());

    // Language metadata
    assert_eq!(lib.language.as_deref(), Some("rust"));
    assert_eq!(ts.language.as_deref(), Some("typescript"));
}
```

**Note:** `store.all_entities()` may not exist yet. Check `src/store/graph.rs` — if it doesn't have this method, use the existing `find_entity_by_path` for each file and count them, or add the method:

```rust
// In src/store/graph.rs, add if missing:
pub fn all_entities(&self) -> Vec<Entity> {
    self.graph
        .node_indices()
        .map(|i| self.graph[i].clone())
        .collect()
}
```

- [ ] **Step 2: Run the new integration tests (expect failure)**

```bash
cd /tmp/cartograph-fix && cargo test -p cartograph test_typescript_pipeline test_mixed_language_pipeline -- --nocapture 2>&1
```

Expected: tests compile but fail — TypeScript files not yet indexed (no File entities for `.ts` paths). This confirms the tests are wired correctly before implementation.

- [ ] **Step 3: (If `all_entities` is missing) Add it to `src/store/graph.rs`**

Check if it exists:
```bash
grep -n "all_entities" /tmp/cartograph-fix/src/store/graph.rs
```

If not found, add to `impl GraphStore`. `Entity` already derives `Clone` (confirmed in `src/store/schema.rs` line 126), so `.clone()` compiles without any additional changes:
```rust
/// Return all entities in the graph.
pub fn all_entities(&self) -> Vec<Entity> {
    self.graph
        .node_indices()
        .map(|i| self.graph[i].clone())
        .collect()
}
```

The `EntityKind::File` filter in the integration tests uses the path `cartograph::store::schema::EntityKind::File`. This resolves correctly: `cartograph` (crate) → `store` (pub mod in `lib.rs`) → `schema` (pub mod in `store/mod.rs`) → `EntityKind` (public enum). No additional re-exports needed.

- [ ] **Step 4: Run integration tests again**

```bash
cd /tmp/cartograph-fix && cargo test -p cartograph test_typescript_pipeline test_mixed_language_pipeline -- --nocapture 2>&1
```

Expected: both tests pass once `index_repo` from Task 4 is in place. If `test_typescript_pipeline` fails on entity count, check that `collect_source_files` is finding all 5 files in `fixtures/sample_ts_repo/src/`.

- [ ] **Step 5: Update `src/main.rs` line 85 — per-language count output**

The `index_repo` function currently returns `()`. To pass file counts to the CLI, either:
- Change `index_repo` to return `(usize, usize)` (rs_count, ts_count) and update the call in `main.rs`
- Or count entities in the store after indexing

The simpler approach: change `index_repo`'s return type to `Result<(usize, usize)>` and return `(rs_count, ts_count)` at the end. Update the signature:

In `src/parser/mod.rs`:
```rust
pub fn index_repo(repo_path: &Path, store: &mut GraphStore) -> Result<(usize, usize)> {
    // ... (same body) ...
    // At the very end, replace `Ok(())` with:
    Ok((rs_count, ts_count))
}
```

In `src/main.rs`, update the `Commands::Index` arm:
```rust
// Replace:
parser::index_repo(&repo_path, &mut store)?;
println!("  Structure: done");

// With:
let (rs_count, ts_count) = parser::index_repo(&repo_path, &mut store)?;
if ts_count > 0 {
    println!("  Structure: {} Rust files, {} TypeScript files", rs_count, ts_count);
} else {
    println!("  Structure: {} Rust files", rs_count);
}
```

- [ ] **Step 6: Run the full test suite**

```bash
cd /tmp/cartograph-fix && cargo test --all 2>&1 | tail -30
```

Expected: all tests pass including `test_typescript_pipeline` and `test_mixed_language_pipeline`. If `test_full_pipeline_on_fixture` fails due to the return type change, update the call in `e2e_test.rs` too — `parser::index_repo` is called with `?` and the return value discarded (use `let _ = parser::index_repo(...)?`).

- [ ] **Step 7: Run clippy and fmt**

```bash
cd /tmp/cartograph-fix && cargo clippy -- -D warnings 2>&1 && cargo fmt --check 2>&1
```

Fix any warnings before committing. Common issues: unused variable if `rs_count`/`ts_count` not used, `#[allow(unused)]` in stub code.

- [ ] **Step 8: Commit**

```bash
cd /tmp/cartograph-fix
git add tests/e2e_test.rs src/main.rs src/parser/mod.rs src/store/graph.rs
git commit -m "feat: integration tests, per-language CLI output, TypeScript pipeline wired end-to-end"
```

---

## Chunk 6: Smoke test on openclaw

**Files:** No code changes — verify success criteria against the live openclaw repo.

### Task 6: Verify against openclaw

- [ ] **Step 1: Run the final test suite**

```bash
cd /tmp/cartograph-fix && cargo test --all 2>&1 | tail -10
```

Expected: all tests pass. Total should be 29 (existing) + ~18 new = ~47 tests.

- [ ] **Step 2: Build release binary**

```bash
cd /tmp/cartograph-fix && cargo build --release 2>&1 | tail -5
```

Expected: compiles without errors.

- [ ] **Step 3: Index openclaw (success criterion 2)**

```bash
/tmp/cartograph-fix/target/release/cartograph --repo /tmp/oc-demo index 2>&1
```

Expected output format:
```
Indexing /tmp/oc-demo...
  Structure: N Rust files, M TypeScript files
  Git history: K commits
  ...
Index complete.
```

Where M ≥ 5,000 (success criterion 2). If M is 0, `collect_source_files` is not finding TypeScript files — check that it recurses into `src/` and that skip directories don't accidentally exclude `src/`.

- [ ] **Step 4: Verify blast-radius (success criterion 3)**

```bash
/tmp/cartograph-fix/target/release/cartograph --repo /tmp/oc-demo blast-radius src/gateway/index.ts 2>&1 | head -20
```

Expected: non-empty table of affected files. If "No results", the entity path may differ — try:
```bash
/tmp/cartograph-fix/target/release/cartograph --repo /tmp/oc-demo hotspots 2>&1 | head -10
```
and use one of the returned paths.

- [ ] **Step 5: Verify hotspots (success criterion 4)**

```bash
/tmp/cartograph-fix/target/release/cartograph --repo /tmp/oc-demo hotspots 2>&1 | head -20
```

Expected: table of files sorted by connection count, with TypeScript files at the top.

- [ ] **Step 6: Generate demo visualization (success criterion 5)**

```bash
python3 /tmp/cartograph-fix/scripts/viz.py \
  --db /tmp/oc-demo/.cartograph/db.sqlite \
  --repo-name "openclaw" \
  --out /tmp/openclaw-demo.html 2>&1
```

Expected: `/tmp/openclaw-demo.html` is created with no errors. Open in a browser to verify the graph renders.

- [ ] **Step 7: Final commit**

```bash
cd /tmp/cartograph-fix
git push origin main
```

---

## Reference: File Map

| File | Change | Purpose |
|------|--------|---------|
| `src/parser/typescript.rs` | **Create** | `parse_typescript_source` — walks AST, emits raw specifiers |
| `src/parser/mod.rs` | **Modify** | Replace `collect_rs_files` → `collect_source_files`; add `resolve_ts_import`; extend Pass 1+2 for TypeScript |
| `src/main.rs:85` | **Modify** | `println!("  Structure: done")` → per-language count |
| `src/store/graph.rs` | **Modify (if needed)** | Add `all_entities()` method |
| `tests/e2e_test.rs` | **Modify** | Add `test_typescript_pipeline`, `test_mixed_language_pipeline` |
| `fixtures/sample_ts_repo/src/*.ts` | **Create** | 5-file TypeScript fixture |
| `fixtures/sample_mixed_repo/src/lib.rs` | **Create** | Mixed fixture — Rust side |
| `fixtures/sample_mixed_repo/src/index.ts` | **Create** | Mixed fixture — TypeScript side |
