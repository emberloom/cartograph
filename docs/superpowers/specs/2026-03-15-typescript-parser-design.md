# TypeScript Parser — Design Spec

**Date:** 2026-03-15
**Status:** Approved
**Scope:** Add TypeScript/TSX import parsing to Cartograph (Layer 1 structure) so that TypeScript monorepos like openclaw can be fully indexed.

---

## Problem

Cartograph only parses Rust today. Its git-mining layer (Layer 2) is language-agnostic, but without File entities in the store, co-change and ownership edges cannot be written — making the tool effectively useless on non-Rust codebases. openclaw (314k stars, ~8,500 TypeScript files) is the target case study.

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

Returns a `ParseResult` (reused type) with:
- `imports`: raw specifier strings extracted from `import_statement` and `export_statement` AST nodes
- `modules`: always empty (no Rust-style `mod` declarations in TypeScript)
- `entities`: empty for v1 (no class/function extraction yet)

**AST nodes targeted:**
- `import_statement` — covers all import forms: named, default, namespace, type-only (`import type { ... }`)
- `export_statement` with a `source` child — covers re-exports (`export { foo } from "./bar.js"`, `export * from "./utils.js"`)

Both yield the raw string value of the `string` node (the module specifier). No path resolution at this stage.

**Grammar dependency:** `tree-sitter-typescript = "0.23"` — supports both `.ts` and `.tsx` via feature flag or separate language object.

---

### 2. Path resolution (in `mod.rs`)

Relative imports only. Non-relative specifiers (no `./` or `../` prefix) are silently skipped — this covers `node:fs`, `vitest`, `@scope/pkg`, etc.

**ESM `.js` → `.ts` rewrite:** TypeScript ESM convention uses `.js` in import paths even though the source file is `.ts`. Resolution strips the extension and tries candidates in priority order:

```
"./bar.js"  →  bar.ts, bar/index.ts, bar.tsx, bar/index.tsx
"./bar"     →  bar.ts, bar/index.ts, bar.tsx, bar/index.tsx
"./bar.ts"  →  bar.ts  (direct, uncommon)
```

First candidate found in `file_ids` wins. No match → no edge (not an error).

**Safety:** Any resolved path containing `..` after joining with the declaring file's parent directory is rejected (path traversal guard, same as Rust parser).

---

### 3. `src/parser/mod.rs` changes

`index_repo` gains a unified two-pass flow:

**Pass 0 — file collection:**
- Walk repo recursively
- Collect `.rs`, `.ts`, `.tsx` files
- Skip directories: `target/`, `node_modules/`, `dist/`, `.next/`, `build/`

**Pass 1 — entity registration:**
- For every collected file, create a `File` entity in the store
- Populate `file_ids: HashMap<String, String>` (relative path → entity id)
- Language stored in entity metadata: `"rust"`, `"typescript"`

**Pass 2 — parse and wire:**
- For each file, dispatch to `parse_rust_source` or `parse_typescript_source` based on extension
- Resolve imports to target entity IDs
- Write `Imports` edges (confidence 1.0)

**Public interface unchanged.** `index_repo(repo_path, store)` signature stays the same. CLI `index` subcommand and MCP server require no changes.

**`index` CLI output** gains a language summary line:
```
  Structure: 342 Rust files, 8547 TypeScript files
```

---

### 4. Fixtures and Tests

**`fixtures/sample_ts_repo/`** — minimal TypeScript project:
```
src/
  main.ts       — imports from ./utils.js and ./types.js
  utils.ts      — imports from ./types.js
  types.ts      — no imports
  index.ts      — barrel: export * from "./main.js"
  external.ts   — imports from "vitest" and "node:fs" (both skipped)
```
Covers: named import, `.js`-extension rewrite, type import, barrel re-export, skipped package import.

**`fixtures/sample_mixed_repo/`** — one `.rs` file and one `.ts` file. Confirms both indexed in same store, no cross-language interference.

**Unit tests in `typescript.rs`:**
- `test_parse_named_import` — `import { foo } from "./bar.js"` → specifier `"./bar.js"`
- `test_parse_type_import` — `import type { T } from "./types.js"` → specifier included
- `test_parse_export_from` — `export { foo } from "./utils.js"` → specifier included
- `test_parse_export_star` — `export * from "./core.js"` → specifier included
- `test_skips_package_import` — `import { x } from "vitest"` → empty imports
- `test_parse_tsx` — JSX file with imports parses correctly

**Integration test in `e2e_test.rs`:**
- `test_typescript_pipeline` — indexes `fixtures/sample_ts_repo`, verifies entities, edges, blast radius

---

## Out of Scope (v1)

- `tsconfig.json` path alias resolution (`@/components/foo` → `src/components/foo`)
- Function, class, interface entity extraction
- JavaScript (`.js`) file parsing
- Cross-package workspace resolution

These are natural follow-on features once the parser is proven on openclaw.

---

## Cargo.toml Changes

```toml
tree-sitter-typescript = "0.23"
```

Added alongside `tree-sitter-rust = "0.24"`. No other dependency changes.

---

## Success Criteria

1. `cargo test --all` passes with new unit and integration tests
2. `cartograph --repo /tmp/oc-demo index` indexes openclaw's ~8,500 TypeScript files and writes File entities + Imports edges to the store
3. `cartograph --repo /tmp/oc-demo blast-radius src/gateway/index.ts` returns a non-empty result
4. `cartograph --repo /tmp/oc-demo hotspots` returns openclaw's most-connected files
5. `scripts/viz.py` generates a valid demo HTML from the openclaw database
