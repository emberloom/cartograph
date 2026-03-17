# Predictive Regression Scoring

## Overview
Predicts which files/tests are most likely to regress based on a set of changed files, using structural dependencies, co-change history, and hotspot data as signals.

## Architecture
```
Changed Files → Risk Signals → Scoring Engine → Ranked Risk Report
```

### Components
1. **`src/prediction/mod.rs`** — Module root, public API
2. **`src/prediction/scoring.rs`** — Risk scoring engine
3. **`src/prediction/signals.rs`** — Individual signal extractors

### Risk Signals
1. **Structural coupling** — Files in blast radius of changed files (weighted by depth)
2. **Co-change frequency** — Files that historically change together but weren't changed
3. **Hotspot centrality** — Changed files with high edge degree (many dependents)
4. **Change recency** — Recently-indexed files with many recent changes (churn)
5. **Ownership fragmentation** — Files with many owners (higher coordination risk)

### Scoring Algorithm
```
risk_score(file) = w1 * structural_signal(file)
                 + w2 * cochange_signal(file)
                 + w3 * hotspot_signal(file)
                 + w4 * churn_signal(file)
                 + w5 * ownership_signal(file)
```

Default weights: structural=0.35, cochange=0.30, hotspot=0.20, churn=0.10, ownership=0.05

### Key Types
```rust
pub struct RiskScore {
    pub entity_path: String,
    pub score: f64,
    pub signals: Vec<SignalContribution>,
    pub risk_level: RiskLevel,
}

pub struct SignalContribution {
    pub signal_name: String,
    pub raw_value: f64,
    pub weighted_value: f64,
}

pub struct PredictionConfig {
    pub weights: SignalWeights,
    pub min_score_threshold: f64,
    pub max_results: usize,
}
```

### New CLI Subcommand
```
cartograph predict --changed src/auth.rs,src/billing.rs [--limit 20]
```

### New MCP Tool
`cartograph_predict_risk` — takes changed file list, returns ranked risk scores.
