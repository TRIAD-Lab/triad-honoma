use rand::Rng;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbabilisticOperation {
    pub description: String,
    pub outcomes: Vec<OperationOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationOutcome {
    pub label: String,
    pub probability: f64, // The sum of all outcomes must be 1.0
    pub value: Option<serde_json::Value>,
}

impl ProbabilisticOperation {
    pub fn new(description: &str, success_probability: f64) -> Self {
        let failure_probability = 1.0 - success_probability;
        Self {
            description: description.to_string(),
            outcomes: vec![
                OperationOutcome {
                    label: "ConsensusReached".to_string(),
                    probability: success_probability,
                    value: Some(serde_json::json!({
                        "confidence": success_probability,
                        "consensus_ratio": success_probability
                    })),
                },
                OperationOutcome {
                    label: "ConflictDetected".to_string(),
                    probability: failure_probability,
                    value: Some(serde_json::json!({
                        "risk": failure_probability,
                        "conflict_level": failure_probability
                    })),
                },
            ],
        }
    }

    /// Создаёт операцию для частичного консенсуса
    pub fn new_partial(description: &str, consensus_ratio: f64) -> Self {
        let partial_probability = consensus_ratio;
        let conflict_probability = 1.0 - consensus_ratio;
        
        Self {
            description: description.to_string(),
            outcomes: vec![
                OperationOutcome {
                    label: "PartialConsensus".to_string(),
                    probability: partial_probability,
                    value: Some(serde_json::json!({
                        "consensus_ratio": consensus_ratio,
                        "partial_success": true
                    })),
                },
                OperationOutcome {
                    label: "ConflictDetected".to_string(),
                    probability: conflict_probability,
                    value: Some(serde_json::json!({
                        "risk": conflict_probability,
                        "conflict_level": conflict_probability
                    })),
                },
            ],
        }
    }

    /// Создаёт операцию для верификации
    pub fn new_verification(description: &str, verification_probability: f64) -> Self {
        let failure_probability = 1.0 - verification_probability;
        Self {
            description: description.to_string(),
            outcomes: vec![
                OperationOutcome {
                    label: "ConsensusReached".to_string(),
                    probability: verification_probability,
                    value: Some(serde_json::json!({
                        "verification_confidence": verification_probability,
                        "trust_level": verification_probability
                    })),
                },
                OperationOutcome {
                    label: "ConflictDetected".to_string(),
                    probability: failure_probability,
                    value: Some(serde_json::json!({
                        "verification_risk": failure_probability,
                        "untrusted": true
                    })),
                },
            ],
        }
    }

    pub fn execute(&self) -> &OperationOutcome {
        let mut rng = rand::thread_rng();
        let random = rng.gen_range(0.0..1.0);
        let mut cumulative = 0.0;
        for outcome in &self.outcomes {
            cumulative += outcome.probability;
            if random <= cumulative {
                return outcome;
            }
        }
        // If it didn't work (for example, due to sum inaccuracy), return the last outcome
        self.outcomes.last().expect("No outcomes for the operation")
    }
} 

