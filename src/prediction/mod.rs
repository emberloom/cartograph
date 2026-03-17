pub mod scoring;
pub mod signals;

use serde::{Deserialize, Serialize};
use std::fmt;

/// Risk level classification for a predicted regression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "low"),
            RiskLevel::Medium => write!(f, "medium"),
            RiskLevel::High => write!(f, "high"),
            RiskLevel::Critical => write!(f, "critical"),
        }
    }
}

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
///
/// Default weights: structural=0.35, cochange=0.30, hotspot=0.25, ownership=0.10.
/// Weights must each be in [0.0, 1.0] and should sum to approximately 1.0.
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

/// Errors from signal weight validation.
#[derive(Debug, Clone, PartialEq)]
pub enum WeightValidationError {
    /// An individual weight is outside [0.0, 1.0].
    OutOfRange { field: &'static str, value: f64 },
    /// Weights do not sum to approximately 1.0 (tolerance: 0.01).
    SumNotOne { sum: f64 },
    /// A weight is NaN or infinite.
    NonFinite { field: &'static str },
}

impl fmt::Display for WeightValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WeightValidationError::OutOfRange { field, value } => {
                write!(f, "weight '{field}' = {value} is outside [0.0, 1.0]")
            }
            WeightValidationError::SumNotOne { sum } => {
                write!(f, "weights sum to {sum:.4}, expected ~1.0 (tolerance 0.01)")
            }
            WeightValidationError::NonFinite { field } => {
                write!(f, "weight '{field}' is NaN or infinite")
            }
        }
    }
}

impl SignalWeights {
    /// Validate that all weights are in [0.0, 1.0] and sum to approximately 1.0.
    pub fn validate(&self) -> Result<(), WeightValidationError> {
        let fields: &[(&'static str, f64)] = &[
            ("structural", self.structural),
            ("cochange", self.cochange),
            ("hotspot", self.hotspot),
            ("ownership", self.ownership),
        ];

        for &(name, value) in fields {
            if !value.is_finite() {
                return Err(WeightValidationError::NonFinite { field: name });
            }
            if !(0.0..=1.0).contains(&value) {
                return Err(WeightValidationError::OutOfRange { field: name, value });
            }
        }

        let sum = self.structural + self.cochange + self.hotspot + self.ownership;
        if (sum - 1.0).abs() > 0.01 {
            return Err(WeightValidationError::SumNotOne { sum });
        }

        Ok(())
    }
}

/// Errors from input validation.
#[derive(Debug, Clone, PartialEq)]
pub enum InputValidationError {
    EmptyChangedFiles,
    TooManyChangedFiles {
        count: usize,
        max: usize,
    },
    PathTraversal {
        path: String,
    },
    PathTooLong {
        path: String,
        len: usize,
        max: usize,
    },
}

impl fmt::Display for InputValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InputValidationError::EmptyChangedFiles => {
                write!(f, "changed_files must not be empty")
            }
            InputValidationError::TooManyChangedFiles { count, max } => {
                write!(f, "too many changed files: {count} (max {max})")
            }
            InputValidationError::PathTraversal { path } => {
                write!(f, "path contains '..': {path}")
            }
            InputValidationError::PathTooLong { path, len, max } => {
                write!(
                    f,
                    "path too long ({len} chars, max {max}): {}",
                    &path[..40.min(path.len())]
                )
            }
        }
    }
}

/// Maximum number of changed files accepted.
pub const MAX_CHANGED_FILES: usize = 200;
/// Maximum length of a single file path.
pub const MAX_PATH_LEN: usize = 1024;

/// Validate the changed_files input list.
pub fn validate_changed_files(changed_files: &[String]) -> Result<(), InputValidationError> {
    if changed_files.is_empty() {
        return Err(InputValidationError::EmptyChangedFiles);
    }
    if changed_files.len() > MAX_CHANGED_FILES {
        return Err(InputValidationError::TooManyChangedFiles {
            count: changed_files.len(),
            max: MAX_CHANGED_FILES,
        });
    }
    for path in changed_files {
        if path.contains("..") {
            return Err(InputValidationError::PathTraversal { path: path.clone() });
        }
        if path.len() > MAX_PATH_LEN {
            return Err(InputValidationError::PathTooLong {
                path: path.clone(),
                len: path.len(),
                max: MAX_PATH_LEN,
            });
        }
    }
    Ok(())
}

/// Clamp a score to the [0.0, 1.0] range, mapping NaN and Inf to 0.0.
pub fn normalize_score(score: f64) -> f64 {
    if score.is_nan() || score.is_infinite() {
        return 0.0;
    }
    score.clamp(0.0, 1.0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level_display() {
        assert_eq!(RiskLevel::Low.to_string(), "low");
        assert_eq!(RiskLevel::Medium.to_string(), "medium");
        assert_eq!(RiskLevel::High.to_string(), "high");
        assert_eq!(RiskLevel::Critical.to_string(), "critical");
    }

    #[test]
    fn test_default_weights_valid() {
        let w = SignalWeights::default();
        assert!(w.validate().is_ok());
    }

    #[test]
    fn test_weight_out_of_range() {
        let w = SignalWeights {
            structural: 1.5,
            ..SignalWeights::default()
        };
        assert_eq!(
            w.validate(),
            Err(WeightValidationError::OutOfRange {
                field: "structural",
                value: 1.5
            })
        );
    }

    #[test]
    fn test_weight_negative() {
        let w = SignalWeights {
            cochange: -0.1,
            structural: 0.35,
            hotspot: 0.25,
            ownership: 0.10,
        };
        assert_eq!(
            w.validate(),
            Err(WeightValidationError::OutOfRange {
                field: "cochange",
                value: -0.1
            })
        );
    }

    #[test]
    fn test_weight_sum_not_one() {
        let w = SignalWeights {
            structural: 0.5,
            cochange: 0.5,
            hotspot: 0.5,
            ownership: 0.5,
        };
        assert!(matches!(
            w.validate(),
            Err(WeightValidationError::SumNotOne { .. })
        ));
    }

    #[test]
    fn test_weight_nan() {
        let w = SignalWeights {
            structural: f64::NAN,
            cochange: 0.30,
            hotspot: 0.25,
            ownership: 0.10,
        };
        assert_eq!(
            w.validate(),
            Err(WeightValidationError::NonFinite {
                field: "structural"
            })
        );
    }

    #[test]
    fn test_weight_inf() {
        let w = SignalWeights {
            structural: f64::INFINITY,
            cochange: 0.30,
            hotspot: 0.25,
            ownership: 0.10,
        };
        assert_eq!(
            w.validate(),
            Err(WeightValidationError::NonFinite {
                field: "structural"
            })
        );
    }

    #[test]
    fn test_validate_empty_changed_files() {
        assert_eq!(
            validate_changed_files(&[]),
            Err(InputValidationError::EmptyChangedFiles)
        );
    }

    #[test]
    fn test_validate_too_many_changed_files() {
        let files: Vec<String> = (0..201).map(|i| format!("file{i}.rs")).collect();
        assert!(matches!(
            validate_changed_files(&files),
            Err(InputValidationError::TooManyChangedFiles { .. })
        ));
    }

    #[test]
    fn test_validate_path_traversal() {
        let files = vec!["../etc/passwd".to_string()];
        assert!(matches!(
            validate_changed_files(&files),
            Err(InputValidationError::PathTraversal { .. })
        ));
    }

    #[test]
    fn test_validate_path_too_long() {
        let long_path = "a".repeat(1025);
        let files = vec![long_path];
        assert!(matches!(
            validate_changed_files(&files),
            Err(InputValidationError::PathTooLong { .. })
        ));
    }

    #[test]
    fn test_validate_valid_changed_files() {
        let files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
        assert!(validate_changed_files(&files).is_ok());
    }

    #[test]
    fn test_normalize_score_clamps_high() {
        assert_eq!(normalize_score(1.5), 1.0);
    }

    #[test]
    fn test_normalize_score_clamps_low() {
        assert_eq!(normalize_score(-0.5), 0.0);
    }

    #[test]
    fn test_normalize_score_nan() {
        assert_eq!(normalize_score(f64::NAN), 0.0);
    }

    #[test]
    fn test_normalize_score_inf() {
        assert_eq!(normalize_score(f64::INFINITY), 0.0);
    }

    #[test]
    fn test_normalize_score_neg_inf() {
        assert_eq!(normalize_score(f64::NEG_INFINITY), 0.0);
    }

    #[test]
    fn test_normalize_score_normal() {
        assert_eq!(normalize_score(0.5), 0.5);
    }
}
