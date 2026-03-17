# Policy as Code Engine

## Overview
Defines and enforces architectural policies against the Cartograph graph. Policies are YAML rules that assert constraints like "module A must not depend on module B" or "files with >10 dependents must have tests."

## Architecture
```
Policy Files (YAML) → Policy Engine → Graph Evaluation → Violations Report
```

### Components
1. **`src/policy/mod.rs`** — Module root, public API
2. **`src/policy/rules.rs`** — Rule types and YAML parsing
3. **`src/policy/engine.rs`** — Evaluation engine that checks rules against the graph
4. **`src/policy/report.rs`** — Violation reporting

### Policy Format (YAML)
```yaml
policies:
  - id: no-circular-deps
    description: "No circular dependencies between top-level modules"
    rule:
      type: no_dependency
      from: "src/server/**"
      to: "src/parser/**"
    severity: error

  - id: hotspot-coverage
    description: "Hotspots with >5 connections must have test coverage"
    rule:
      type: max_connections
      pattern: "src/**"
      threshold: 5
      require: coverage
    severity: warning

  - id: ownership-required
    description: "All source files must have an owner"
    rule:
      type: has_edge
      pattern: "src/**/*.rs"
      edge_kind: owned_by
    severity: warning
```

### Rule Types
1. **`no_dependency`** — Assert no dependency edge from `from` glob to `to` glob
2. **`max_connections`** — Flag files exceeding a connection threshold
3. **`has_edge`** — Assert files matching a pattern have a specific edge kind
4. **`layer_boundary`** — Define layers and enforce unidirectional dependencies

### Key Types
```rust
pub struct Policy {
    pub id: String,
    pub description: String,
    pub rule: Rule,
    pub severity: Severity,
}

pub enum Severity {
    Error,
    Warning,
    Info,
}

pub enum Rule {
    NoDependency { from: String, to: String },
    MaxConnections { pattern: String, threshold: usize },
    HasEdge { pattern: String, edge_kind: String },
    LayerBoundary { layers: Vec<LayerDef> },
}

pub struct Violation {
    pub policy_id: String,
    pub severity: Severity,
    pub entity_path: String,
    pub message: String,
}
```

### New CLI Subcommands
```
cartograph policy check --config policies.yaml
cartograph policy init  # generates a starter policies.yaml
```

### New MCP Tool
`cartograph_policy_check` — evaluates policies and returns violations.
