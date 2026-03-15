# Changelog

All notable changes to Cartograph are documented here.

## [0.1.0] — 2026-03-15

First public release.

### Added

- **Rust source parser** — tree-sitter-rust extracts module and `use` relationships into a directed dependency graph
- **Dependency graph store** — petgraph + SQLite; persisted at `.cartograph/db.sqlite` inside the indexed repo
- **Git history mining** — git2 walks commit history to compute co-change frequency and blame-based file ownership
- **Blast radius traversal** — BFS/DFS over the dependency graph from any starting file
- **Hotspots query** — ranks files by total graph connectivity (in-degree + out-degree)
- **MCP stdio server** — 5 tools for Claude Code and any MCP-compatible client: `cartograph_blast_radius`, `cartograph_dependencies`, `cartograph_co_changes`, `cartograph_who_owns`, `cartograph_hotspots`
- **CLI** — 7 subcommands: `index`, `blast-radius`, `hotspots`, `co-changes`, `who-owns`, `deps`, `serve`
- **29 passing tests** — unit and end-to-end coverage with a fixture repo

### Planned

- Additional language parsers (TypeScript, Python, Go)
- Layer 3: institutional knowledge (PR descriptions, commit messages, design docs)
- Layer 4: change simulator — predict impact of a proposed change before it is made