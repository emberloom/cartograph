pub mod scoring;
pub mod signals;

use serde::{Deserialize, Serialize};

use crate::integrations::github::RiskLevel;

/// A scored risk prediction for a single entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskScore {
    pub entity_path: String,
    pub score: f64,
    pub signals: Vec<SignalContribution>,
    pub risk_level: RiskLevel,
}

/// Contribution of a single signal to the overall risk score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalContribution {
    pub signal_name: String,
    pub raw_value: f64,
    pub weighted_value: f64,
}

/// Weights for each risk signal.
#[derive(Debug, Clone)]
pub struct SignalWeights {
    pub structural: f64,
    pub cochange: f64,
    pub hotspot: f64,
    pub ownership: f64,
}

impl Default for SignalWeights {
    fn default() -> Self {
        Self {
            structural: 0.35,
            cochange: 0.30,
            hotspot: 0.25,
            ownership: 0.10,
        }
    }
}

/// Configuration for prediction engine.
#[derive(Debug, Clone)]
pub struct PredictionConfig {
    pub weights: SignalWeights,
    pub min_score_threshold: f64,
    pub max_results: usize,
}

impl Default for PredictionConfig {
    fn default() -> Self {
        Self {
            weights: SignalWeights::default(),
            min_score_threshold: 0.1,
            max_results: 20,
        }
    }
}
