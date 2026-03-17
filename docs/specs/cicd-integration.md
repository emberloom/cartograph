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

### Using `--changed-since` with git diff

To analyze only files changed since a specific commit or branch, pipe `git diff` output:

```bash
# Files changed compared to main branch
cartograph ci-report --changed "$(git diff --name-only origin/main)"

# Files changed in the last commit
cartograph ci-report --changed "$(git diff --name-only HEAD~1)"

# Files changed since a specific commit
cartograph ci-report --changed "$(git diff --name-only abc1234)"
```

In a GitHub Actions workflow:
```yaml
- name: Get changed files
  id: changes
  run: |
    FILES=$(git diff --name-only ${{ github.event.pull_request.base.sha }} ${{ github.sha }} | tr '\n' ',')
    echo "files=$FILES" >> $GITHUB_OUTPUT

- name: Run Cartograph CI
  run: cartograph ci-report --changed "${{ steps.changes.outputs.files }}" --format sarif --fail-on high
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
- 0: All checks pass (or no findings exceed threshold)
- 1: Findings exceed threshold
- 2: Analysis error (invalid input, database failure, etc.)

### Input Validation
- Changed files list must not be empty
- Path traversal (`..`) is rejected
- Maximum 500 changed files per invocation

### SARIF Output
Standard SARIF 2.1.0 format for integration with GitHub Code Scanning, VS Code SARIF Viewer, etc.
