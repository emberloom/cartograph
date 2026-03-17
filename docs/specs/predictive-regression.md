# Predictive Regression Scoring

## Overview
Predicts which files/tests are most likely to regress based on a set of changed files, using structural dependencies, co-change history, and hotspot data as signals.

## Architecture
```
Changed Files -> Risk Signals -> Scoring Engine -> Ranked Risk Report
```

### Components
1. **`src/prediction/mod.rs`** -- Module root, public API, types, input validation
2. **`src/prediction/scoring.rs`** -- Risk scoring engine
3. **`src/prediction/signals.rs`** -- Individual signal extractors

### Risk Signals
1. **Structural coupling** -- Files in blast radius of changed files (weighted by depth)
2. **Co-change frequency** -- Files that historically change together but were not changed
3. **Hotspot centrality** -- Files with high edge degree (many dependents)
4. **Ownership fragmentation** -- Files with many owners (higher coordination risk)

### Scoring Algorithm
```
risk_score(file) = w1 * structural_signal(file)
                 + w2 * cochange_signal(file)
                 + w3 * hotspot_signal(file)
                 + w4 * ownership_signal(file)
```

Default weights: structural=0.35, cochange=0.30, hotspot=0.25, ownership=0.10

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
cartograph predict --changed src/auth.rs,src/billing.rs [--limit 20] [--weights 0.35,0.30,0.25,0.10]
```

### New MCP Tool
`cartograph_predict_risk` -- takes changed file list, returns ranked risk scores.

### Input Validation
- changed_files must not be empty
- Maximum 200 changed files (DoS protection)
- Paths must not contain `..` (path traversal protection)
- Paths must be <= 1024 characters
- Signal weights must each be in [0.0, 1.0] and sum to ~1.0

### Score Normalization
All scores are clamped to [0.0, 1.0]. NaN and Infinity values are mapped to 0.0.
