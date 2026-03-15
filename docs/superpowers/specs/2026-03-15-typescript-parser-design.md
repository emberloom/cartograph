# TypeScript Parser — Design Spec

**Date:** 2026-03-15
**Status:** Approved
**Scope:** Add TypeScript/TSX import parsing to Cartograph (Layer 1 structure) so that TypeScript monorepos like openclaw can be fully indexed.

---

## Problem

Cartograph only parses Rust today. Its git-mining layer (Layer 2) is language-agnostic, but without File entities in the store, co-change and ownership edges cannot be written — making the tool effectively useless on non-Rust codebases. openclaw (314k stars, ~6,064 indexable TypeScript files) is the target case study.

---

## Approach

Mirror the existing Rust parser pattern exactly (Approach A). Add `src/parser/typescript.rs` using `tree-sitter-typescript`. Extend `index_repo` in `src/parser/mod.rs` to collect `.ts`/`.tsx` files alongside `.rs` and dispatch to the correct parser. No schema changes, no new CLI flags, no new entity kinds for v1.

---

## Components

### 1. `src/parser/typescript.rs`

New file. Same public interface as `rust.rs`:

```rust
pub fn parse_typescript_source(source: &str, path: &Path) -> ParseResult
```

**Precondition:** `path` has extension `.ts` or `.tsx` (never `.d.ts`). The caller (`mod.rs` Pass 0) guarantees this — `.d.ts` files are excluded before dispatch. `parse_typescript_source` does not need to re-check.

**`ParseResult` type** (defined in `src/parser/rust.rs`, re-exported from `src/parser/mod.rs`):

```rust
pub struct ParsedEntity {
    pub kind: String,  // "Function", "Struct", "Trait", "Impl"
    pub name: String,
    pub line: usize,
}

pub struct ParseResult {
    pub entities: Vec<ParsedEntity>,  // empty for v1 TypeScript
    pub imports: Vec<String>,         // raw specifier strings (all found, unfiltered)
    pub modules: Vec<String>,         // empty for TypeScript (no mod declarations)
}
```

`path` is used to select the correct grammar: `.tsx` files use `LANGUAGE_TSX`, all other `.ts` files use `LANGUAGE_TYPESCRIPT`. Both constants are exported from `tree-sitter-typescript = "0.23.2"`, which compiles correctly against `tree-sitter = "0.25"` (verified).

Returns a `ParseResult` with:
- `imports`: raw specifier strings extracted from `import_statement` and `export_statement` AST nodes — **all found specifiers, including non-relative ones**; filtering of npm packages and `node:` protocols happens in `mod.rs` at resolution time
- `modules`: always empty (no Rust-style `mod` declarations in TypeScript)
- `entities`: empty for v1 (no class/function extraction yet)

**AST nodes targeted:**

Both `import_statement` and `export_statement` expose the module specifier via a named field `"source"` which is a `string` node (confirmed in `typescript/src/node-types.json` for grammar 0.23.2). Extraction:

```rust
// works for both import_statement and export_statement
if let Some(source_node) = node.child_by_field_name("source") {
    let raw = &source[source_node.start_byte()..source_node.end_byte()];
    let specifier = raw.trim_matches('"').trim_matches('\'');
    // push specifier to result.imports
}
```

The `string` node type in tree-sitter-typescript grammar 0.23.2 uses `"` or `'` delimiters only. Backtick template literal import paths (e.g., `` import x from `./foo` ``) are parsed by tree-sitter as a `template_string` node, not a `string` node — they will not match `child_by_field_name("source")` returning a `string` node, so they are silently ignored. This is acceptable; backtick imports are not valid static ESM syntax and do not appear in openclaw's source.

Nodes covered:
- `import_statement` — named, default, namespace, and type-only (`import type { ... }`) imports; all share the same `source` field
- `export_statement` where `child_by_field_name("source")` is `Some` — covers `export { foo } from "./bar.js"` and `export * from "./utils.js"`; `export_statement` nodes without a `source` field (e.g., `export const x = 1`) are ignored automatically

**Not handled in v1:** Dynamic imports (`import("./foo.js")` call expressions) and `require()` calls. openclaw uses ESM static imports throughout its core `src/` tree; dynamic imports appear at lazy-loading boundaries (plugin loaders, route splitting) and their omission means those lazy-boundary edges will be missing from the graph. This is acceptable for the demo use case — the structural skeleton of the codebase is captured by static imports, and co-change/ownership data from Layer 2 fills in the behavioral picture.

---

### 2. Path resolution (in `mod.rs`)

Relative imports only. Non-relative specifiers (no `./` or `../` prefix) are silently skipped at resolution time — this covers `node:fs`, `node:child_process`, `vitest`, `@scope/pkg`, and all npm packages. The parser still emits these specifiers into `result.imports`; resolution discards them before any lookup.

**`file_ids` key format:** Keys are full relative paths including extension (e.g., `"src/utils.ts"`). The resolution step constructs candidate keys by combining the declaring file's directory with the rewritten specifier and performs a direct map lookup. This is identical to the Rust parser.

**ESM `.js` → `.ts` rewrite:** TypeScript ESM convention uses `.js` in import paths even though the source file is `.ts`. Resolution strips any extension from the specifier stem and tries candidates in priority order. This ordering matches the TypeScript compiler's own resolution order (`.ts` before `.tsx`). The same four-candidate list applies whether the declaring file is `.ts` or `.tsx`:

```
"./bar.js"  →  bar.ts, bar/index.ts, bar.tsx, bar/index.tsx
"./bar"     →  bar.ts, bar/index.ts, bar.tsx, bar/index.tsx
"./bar.ts"  →  bar.ts  (direct, uncommon)
"./bar.tsx" →  bar.tsx (direct, uncommon)
```

First candidate found in `file_ids` wins. No match → no edge (not an error).

**Safety:** Any resolved path containing `..` after joining with the declaring file's parent directory is rejected (path traversal guard, same as Rust parser).

---

### 3. `src/parser/mod.rs` changes

`index_repo` gains a unified two-pass flow:

**Pass 0 — file collection:**
- Replace the existing `collect_rs_files` private helper with a new `collect_source_files` helper that collects all supported extensions in a single walk. Signature: `fn collect_source_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()>` — same shape as `collect_rs_files`, propagating I/O errors to the caller via `?`. It returns only files with extensions `.rs`, `.ts`, or `.tsx` (excluding `.d.ts`). This avoids two parallel walk functions and keeps the skip-directory logic in one place. The existing `test_index_sample_repo` unit test in `mod.rs` is unaffected — it calls `index_repo` which internally calls `collect_source_files`, and the fixture only contains `.rs` files.
- Skip `.d.ts` files (TypeScript declaration files — no runtime code, no import edges worth recording): checked by testing that the path does not end in `.d.ts` before pushing to the output vec
- Skip directories: `target/`, `node_modules/`, `dist/`, `.next/`, `build/`

**Pass 1 — entity registration:**
- For every collected file, create a `File` entity in the store
- Populate `file_ids: HashMap<String, String>` (full relative path including extension → entity id)
- Language stored in entity metadata: `"rust"` or `"typescript"`

**Pass 2 — parse and wire:**
- For each file, dispatch based on extension: `.rs` → `parse_rust_source`, `.ts`/`.tsx` → `parse_typescript_source`
- For TypeScript files: iterate `parse_result.imports`, skip any specifier without `./` or `../` prefix, then apply the four-candidate resolution sequence against `file_ids`
- Write `EdgeKind::Imports` edges (confidence 1.0) for each resolved TypeScript import pair. This mirrors how Rust `mod` declarations are recorded (`EdgeKind::Imports`) rather than how `use crate::` paths are recorded (`EdgeKind::DependsOn`). TypeScript `import` statements are structurally equivalent to Rust `mod` declarations — they declare a module boundary — so `Imports` is the correct kind. All downstream queries (`blast_radius`, `hotspots`, `deps`) traverse both edge kinds, so either choice works for graph traversal; `Imports` is chosen for semantic accuracy.

**Public interface unchanged.** `index_repo(repo_path, store)` signature stays the same. CLI `index` subcommand and MCP server require no changes.

**`index` CLI output** (`src/main.rs:85`) currently prints `println!("  Structure: done")`. Replace this single line with a per-language count line using file counts returned from `index_repo` (or derived from `file_ids` after Pass 1):
```
  Structure: 15 Rust files, 6064 TypeScript files
```

---

### 4. Fixtures and Tests

**`fixtures/sample_ts_repo/`** — minimal TypeScript project:
```
src/
  main.ts       — imports from ./utils.js and ./types.js (named import)
  utils.ts      — imports from ./types.js (type import)
  types.ts      — no imports
  index.ts      — barrel: export * from "./main.js"
  external.ts   — imports from "vitest", "node:fs", "node:child_process"
```
Covers: named import, `.js`-extension rewrite, type-only import, barrel re-export, non-relative specifiers that the parser emits but mod.rs drops at resolution time.

**`fixtures/sample_mixed_repo/`** — one `.rs` file (`src/lib.rs`, no imports) and one `.ts` file (`src/index.ts`, no imports). Integration test asserts exactly 2 File entities, 0 edges, and no panic. Confirms both are indexed in the same store with correct language metadata (`"rust"` and `"typescript"` respectively).

**Unit tests in `typescript.rs`:**
- `test_parse_named_import` — `import { foo } from "./bar.js"` → `result.imports` contains `"./bar.js"`
- `test_parse_default_import` — `import foo from "./bar.js"` → `result.imports` contains `"./bar.js"`
- `test_parse_type_import` — `import type { T } from "./types.js"` → specifier included
- `test_parse_export_from` — `export { foo } from "./utils.js"` → specifier included
- `test_parse_export_star` — `export * from "./core.js"` → specifier included
- `test_parse_export_no_source` — `export const x = 1` → `result.imports` is empty (no source field)
- `test_emits_package_import_specifier` — `import { x } from "vitest"` → `result.imports` contains `"vitest"` (parser emits it; mod.rs drops it at resolution time)
- `test_emits_node_protocol_specifier` — `import fs from "node:fs"` → `result.imports` contains `"node:fs"` (same: emitted by parser, dropped by resolver)
- `test_parse_tsx` — JSX file with imports parses correctly using `LANGUAGE_TSX`

**Resolution unit test in `mod.rs` internal `#[cfg(test)]` module:**
- `test_ts_resolution_js_extension_rewrite` — given `file_ids` containing `"src/utils.ts"` → some id, and declaring file `"src/main.ts"`, calling the (private) TypeScript resolution helper with specifier `"./utils.js"` returns `Some(id_for_utils_ts)`. Verifies the four-candidate sequence and `.js`-to-`.ts` stem-stripping logic in isolation. This test lives in the `#[cfg(test)]` block inside `mod.rs` (same module), which gives it access to private resolution functions without needing to expose them as `pub(crate)`.

**Integration test in `e2e_test.rs`:**
- `test_typescript_pipeline` — indexes `fixtures/sample_ts_repo`, verifies:
  - 5 File entities created
  - `main.ts` has outgoing `Imports` edges to `utils.ts` and `types.ts`
  - `index.ts` has an `Imports` edge to `main.ts` (barrel export resolved via `.js`-extension rewrite)
  - `external.ts` has zero outgoing edges (mod.rs resolution drops non-relative specifiers `"vitest"`, `"node:fs"`, `"node:child_process"` before map lookup)
  - blast radius from `types.ts` includes `utils.ts` and `main.ts`

---

## Out of Scope (v1)

- `tsconfig.json` path alias resolution (`@/components/foo` → `src/components/foo`)
- Function, class, interface entity extraction
- JavaScript (`.js`) file parsing (not `.ts` — plain JS files)
- Dynamic imports (`import("./foo")` call expressions)
- `require()` CommonJS calls
- Cross-package workspace resolution
- Backtick template literal import paths (not valid static ESM; not present in openclaw)

---

## Cargo.toml Changes

```toml
tree-sitter-typescript = "0.23.2"
```

Added alongside `tree-sitter-rust = "0.24"`. Version `0.23.2` is ABI-compatible with `tree-sitter = "0.25"` (verified: `cargo check` passes with both present).

---

## Success Criteria

1. `cargo test --all` passes including all new unit and integration tests
2. `cartograph --repo /tmp/oc-demo index` writes at least 5,000 File entities with `language = 'typescript'` to the store. openclaw has 6,064 indexable `.ts`/`.tsx` files after excluding `node_modules/`, `dist/`, `.d.ts`. The 5,000 floor provides ~17% buffer for any additional exclusions or file count drift in the live repo.
3. `cartograph --repo /tmp/oc-demo blast-radius src/gateway/index.ts` returns a non-empty result
4. `cartograph --repo /tmp/oc-demo hotspots` returns openclaw's most-connected files
5. `scripts/viz.py` generates a valid demo HTML from the openclaw database
