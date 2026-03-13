# Emberloom Cartograph

**Codebase world model — maps, understands, and predicts complex software systems.**

[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

---

## What is Cartograph?

Cartograph builds a structural and historical model of any codebase. It parses source code into a dependency graph (Layer 1) and mines git history for co-change patterns and code ownership (Layer 2). The result is queryable via CLI or an MCP server.

**Used by AI agents and humans to understand code before changing it.** Instead of loading entire codebases into context, agents call Cartograph to answer targeted questions: what does this file affect, who owns it, what tends to break together.

---

## Quick Start

```bash
# Clone
git clone https://github.com/emberloom/cartograph.git
cd cartograph

# Build
cargo build --release

# Index a repository (builds the graph + mines git history)
cargo run --release -- --repo /path/to/your/repo index

# Query blast radius (what does changing this file affect?)
cargo run --release -- --repo /path/to/your/repo blast-radius src/main.rs

# Show hotspots (most-connected files)
cargo run --release -- --repo /path/to/your/repo hotspots

# Show co-changes (files that tend to change together)
cargo run --release -- --repo /path/to/your/repo co-changes src/main.rs

# Show ownership (who last touched what)
cargo run --release -- --repo /path/to/your/repo who-owns src/main.rs

# Show dependencies (upstream and downstream)
cargo run --release -- --repo /path/to/your/repo deps src/main.rs
```

The index is stored in `.cartograph/db.sqlite` inside your repo. Re-run `index` to update it.

---

## MCP Setup for Claude Code

Add this to your Claude Code MCP settings (`~/.claude/mcp_servers.json` or project-level `.claude/mcp_servers.json`):

```json
{
  "mcpServers": {
    "cartograph": {
      "command": "cargo",
      "args": [
        "run", "--release",
        "--manifest-path", "/path/to/cartograph/Cargo.toml",
        "--",
        "--repo", "/path/to/your/repo",
        "--db", "/path/to/your/repo/.cartograph/db.sqlite",
        "serve"
      ]
    }
  }
}
```

Cartograph will then be available as tools that Claude can call to understand your codebase. Run `index` once beforehand to build the database.

---

## Available Tools (MCP)

| Tool | Description |
|------|-------------|
| `cartograph_blast_radius` | Impact analysis — which files are reachable from a given file in the dependency graph |
| `cartograph_dependencies` | Upstream and downstream dependencies for a file |
| `cartograph_co_changes` | Files that statistically tend to change together (from git history) |
| `cartograph_who_owns` | Code ownership derived from git blame |
| `cartograph_hotspots` | Most-connected files in the codebase — highest blast radius surface area |

---

## Architecture

```
┌─────────────────────────────────────────┐
│                CLI / MCP                 │
├─────────────────────────────────────────┤
│              Query Engine                │
│  blast_radius · deps · co_changes       │
│  who_owns · hotspots                    │
├──────────────────┬──────────────────────┤
│   Layer 1:       │   Layer 2:           │
│   Structure      │   Dynamics           │
│   (tree-sitter)  │   (git mining)       │
├──────────────────┴──────────────────────┤
│         Store (SQLite + petgraph)        │
└─────────────────────────────────────────┘
```

**Layer 1 — Structure:** tree-sitter parses source files and extracts module/import relationships into a directed dependency graph backed by petgraph and persisted in SQLite.

**Layer 2 — Dynamics:** git2 walks the commit history to compute co-change frequency (files that appear together in commits) and ownership (blame-based author attribution per file).

**Query Engine:** graph traversals and SQL queries over the store, exposed through a unified interface to both the CLI and the MCP stdio server.

---

## v0.1.0 Scope

**Included:**
- Rust source parser (tree-sitter)
- Git history mining — co-changes and ownership
- Dependency graph with blast radius traversal
- CLI with 7 subcommands: `index`, `blast-radius`, `hotspots`, `co-changes`, `who-owns`, `deps`, `serve`
- MCP stdio server with 5 tools
- SQLite + petgraph store
- 29 passing tests

**Coming next:**
- Additional language parsers (TypeScript, Python, Go)
- Layer 3: institutional knowledge (PR descriptions, commit messages, design docs)
- Layer 4: change simulator — predict the probable impact of a proposed change before it's made

---

## License

Apache-2.0. See [LICENSE](LICENSE).

---

Part of [Emberloom](https://github.com/emberloom).
