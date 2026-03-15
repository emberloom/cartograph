<div align="center">

```
          ┌───────┐
          │ main  │
          └──┬──┬─┘
      ┌──────┘  └──────┐
      ▼                 ▼
  ┌───────┐        ┌────────┐
  │  api  ├───────►│  auth  │
  └──┬────┘        └────┬───┘
     │    ╲              │
     ▼     ╲             ▼
 ┌──────┐   ╲      ┌─────────┐
 │  db  │    ╲────►│  config  │
 └──────┘          └──────────┘
       who owns it? ─── git blame
       what breaks? ─── blast radius
       what changed? ── co-change

   C A R T O G R A P H
```

<h1>Emberloom Cartograph</h1>

<p><strong>Codebase world model — maps, understands, and predicts complex software systems.</strong></p>

[![CI](https://github.com/emberloom/cartograph/actions/workflows/ci.yml/badge.svg)](https://github.com/emberloom/cartograph/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust: stable](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![Version](https://img.shields.io/github/v/tag/emberloom/cartograph?label=version&color=blue)](https://github.com/emberloom/cartograph/releases)
[![Status](https://img.shields.io/badge/status-active%20development-yellow.svg)](CHANGELOG.md)

**[→ Live demo — ripgrep visualized](https://emberloom.github.io/cartograph/demo.html)**

</div>

---

> [!WARNING]
> **Early development.** Cartograph is actively developed and internals may change between versions. Expect rough edges — bug reports and PRs are welcome.

Cartograph builds a **structural and historical model of any codebase**. It parses source code into a dependency graph (Layer 1) and mines git history for co-change patterns and code ownership (Layer 2). The result is queryable via CLI or an MCP server.

**Used by AI agents and humans to understand code before changing it.** Instead of loading entire codebases into context, agents call Cartograph to answer targeted questions: what does this file affect, who owns it, what tends to break together.

> Cartograph is used by [Emberloom Sparks](https://github.com/emberloom/sparks) as its built-in codebase-understanding layer — agents query it before making changes to understand blast radius and ownership.

---

## Table of Contents

- [Demo](#demo)
- [What's New](#whats-new)
- [Quick Start](#quick-start)
- [MCP Setup](#mcp-setup-for-claude-code)
- [Available Tools](#available-tools-mcp)
- [Architecture](#architecture)
- [v0.1.0 Scope](#v010-scope)
- [Contributing](#contributing)
- [License](#license)

---

## Demo

**[Interactive visualization of ripgrep →](https://emberloom.github.io/cartograph/demo.html)**

100 files, 69 import edges, 42 co-change pairs. Click any node to see its blast radius. Toggle import / co-change layers. Nodes sized by connectivity, colored by crate.

Generate your own:

```bash
python3 scripts/viz.py \
  --db /path/to/repo/.cartograph/db.sqlite \
  --repo-name "myrepo" \
  --out demo.html
```

---

## What's New

First public release — see [CHANGELOG.md](CHANGELOG.md) for the full list:

- **Dependency graph** — tree-sitter parses Rust source into a directed graph backed by petgraph and SQLite
- **Git history mining** — co-change frequency and blame-based ownership from git2
- **Blast radius traversal** — reachability query over the dependency graph
- **MCP stdio server** — 5 tools exposed to Claude Code and any MCP-compatible client
- **7-subcommand CLI** — `index`, `blast-radius`, `hotspots`, `co-changes`, `who-owns`, `deps`, `serve`
- **29 passing tests** — unit + end-to-end coverage

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

[Emberloom Sparks](https://github.com/emberloom/sparks) uses Cartograph this way out of the box — if you're running Sparks, add `cartograph` to your MCP registry in `config.toml` to give your agents codebase-aware context.

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

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). In short: `cargo test`, `cargo clippy`, `cargo fmt --check` must all pass before submitting a PR.

Questions and bug reports → [GitHub Issues](https://github.com/emberloom/cartograph/issues)

---

## License

Apache-2.0. See [LICENSE](LICENSE).

---

Part of [Emberloom](https://github.com/emberloom) · Built to work alongside [Emberloom Sparks](https://github.com/emberloom/sparks)