# GitHub PR Bot

## Overview
Automated PR analysis bot that comments on GitHub pull requests with Cartograph insights — blast radius, affected hotspots, co-change warnings, and ownership info for changed files.

## Architecture
```
GitHub Webhook → PR Bot Handler → Cartograph Analysis → GitHub API (comments/checks)
```

### Components
1. **`src/integrations/github/mod.rs`** — Module root, types, config
2. **`src/integrations/github/webhook.rs`** — Webhook payload parsing + signature verification
3. **`src/integrations/github/analysis.rs`** — Runs Cartograph queries on PR diff
4. **`src/integrations/github/client.rs`** — GitHub API client for posting comments/checks

### Data Flow
1. Receive webhook event (PR opened/synchronized)
2. Parse changed files from PR diff
3. For each changed file, compute:
   - Blast radius (depth=2)
   - Co-change partners not in the PR
   - Ownership (who should review)
   - Hotspot score (is this a high-risk file?)
4. Aggregate into a structured report
5. Post as PR comment or Check Run annotation

### Key Types
```rust
pub struct PrAnalysisConfig {
    pub blast_radius_depth: usize,
    pub min_risk_score: f64,
    pub include_ownership: bool,
    pub include_co_changes: bool,
}

pub struct PrReport {
    pub changed_files: Vec<FileAnalysis>,
    pub overall_risk: RiskLevel,
    pub suggested_reviewers: Vec<String>,
    pub missing_co_changes: Vec<CoChangeWarning>,
}
```

### Output Format
Markdown comment with sections:
- **Risk Summary** — overall risk level (low/medium/high/critical)
- **Blast Radius** — table of affected files per changed file
- **Missing Co-Changes** — files that usually change together but weren't included
- **Suggested Reviewers** — based on ownership data

## Security
- HMAC-SHA256 webhook signature verification
- Token stored via environment variable, never logged
- No secrets in report output
