# CI/CD Pipeline Integration

## Overview
Provider-agnostic CI/CD integration that runs Cartograph analysis in pipelines and outputs results in standard formats (SARIF, JUnit-style, GitHub Actions annotations).

## Architecture
```
CI Pipeline → cartograph ci-report → Analysis → Output (SARIF / annotations / exit code)
```

### Components
1. **`src/integrations/cicd/mod.rs`** — Module root, output format enum
2. **`src/integrations/cicd/reporter.rs`** — Generates reports from analysis results
3. **`src/integrations/cicd/sarif.rs`** — SARIF format output
4. **`src/integrations/cicd/github_actions.rs`** — GitHub Actions workflow commands (::warning::, ::error::)

### New CLI Subcommand
```
cartograph ci-report [--format sarif|github-actions|json] [--fail-on high] [--changed-files file1,file2]
```

### Key Types
```rust
pub enum OutputFormat {
    Sarif,
    GithubActions,
    Json,
}

pub enum FailThreshold {
    None,
    Low,
    Medium,
    High,
    Critical,
}

pub struct CiReport {
    pub findings: Vec<CiFinding>,
    pub summary: CiSummary,
    pub exit_code: i32,
}

pub struct CiFinding {
    pub file: String,
    pub severity: RiskLevel,
    pub message: String,
    pub rule_id: String,
}
```

### Exit Codes
- 0: All checks pass
- 1: Findings exceed threshold
- 2: Analysis error

### SARIF Output
Standard SARIF 2.1.0 format for integration with GitHub Code Scanning, VS Code SARIF Viewer, etc.
