# Test Coverage Overlay

## Overview
Overlays test coverage data onto the Cartograph dependency graph, enabling coverage-aware queries: which hotspots lack coverage, what's the coverage of a file's blast radius, where are the riskiest uncovered paths.

## Architecture
```
Coverage Data (lcov/json) → Parser → Coverage Store → Overlay Queries
```

### Components
1. **`src/coverage/mod.rs`** — Module root, public API
2. **`src/coverage/parser.rs`** — Parses lcov and JSON coverage formats
3. **`src/coverage/store.rs`** — Stores coverage data alongside graph
4. **`src/coverage/overlay.rs`** — Coverage-aware graph queries

### Supported Formats
- **lcov** — Standard lcov.info format (gcov, istanbul, llvm-cov)
- **JSON** — Simple `{ "file": { "lines_covered": N, "lines_total": M } }` format

### Key Types
```rust
pub struct FileCoverage {
    pub path: String,
    pub lines_covered: u32,
    pub lines_total: u32,
    pub line_coverage_pct: f64,
    pub covered_lines: Vec<u32>,
    pub uncovered_lines: Vec<u32>,
}

pub struct CoverageReport {
    pub files: Vec<FileCoverage>,
    pub total_lines_covered: u32,
    pub total_lines: u32,
    pub overall_pct: f64,
}

pub struct CoverageGap {
    pub entity_path: String,
    pub coverage_pct: f64,
    pub hotspot_score: usize,
    pub risk_description: String,
}
```

### New CLI Subcommands
```
cartograph coverage import --format lcov --file coverage.lcov
cartograph coverage report [--uncovered-hotspots] [--limit 20]
cartograph coverage gaps [--min-connections 3]
```

### New MCP Tools
- `cartograph_coverage_report` — overall coverage stats with graph-aware insights
- `cartograph_coverage_gaps` — hotspots with low/no coverage

### Database Schema Addition
```sql
CREATE TABLE IF NOT EXISTS coverage (
    file_path TEXT PRIMARY KEY,
    lines_covered INTEGER NOT NULL DEFAULT 0,
    lines_total INTEGER NOT NULL DEFAULT 0,
    covered_lines TEXT NOT NULL DEFAULT '[]',
    uncovered_lines TEXT NOT NULL DEFAULT '[]',
    imported_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```
