/// Phase 3: Hard Checks Pipeline
/// 
/// Deterministic O(n log n) validation pipeline that runs BEFORE Core interference.
/// Ensures consensus on transaction ordering and validity.

use crate::transaction::Transaction;
use crate::crypto::schnorr::SchnorrSignature;
use curve25519_dalek::ristretto::RistrettoPoint;
use std::time::{Duration, Instant};
use ahash::AHashMap as HashMap;

/// Reasons for transaction rejection
#[derive(Debug, Clone)]
pub enum RejectReason {
    InvalidSignature,
    InsufficientBalance { required: u64, available: u64 },
    InvalidNonce { expected: u64, got: u64 },
    DAGCycle,
    RateLimitExceeded,
    FeeTooLow { required: u64, provided: u64 },
    MissingParent(String),
}

/// Pipeline execution metrics
#[derive(Debug, Clone, Default)]
pub struct PipelineMetrics {
    pub total_txs: usize,
    pub accepted: usize,
    pub rejected_by_stage: HashMap<String, usize>,
    pub stage_timings: HashMap<String, Duration>,
    pub total_time: Duration,
}

/// Result of pipeline execution
#[derive(Debug)]
pub struct PipelineResult {
    pub accepted: Vec<Transaction>,
    pub rejected: Vec<(Transaction, RejectReason)>,
    pub metrics: PipelineMetrics,
}

/// Main Hard Checks Pipeline orchestrator
pub struct HardChecksPipeline {
    // Stage configurations
    enable_schnorr_verify: bool,
    enable_balance_check: bool,
    enable_dag_check: bool,
    enable_rate_limit: bool,
}

impl HardChecksPipeline {
    /// Create new pipeline with all stages enabled
    pub fn new() -> Self {
        Self {
            enable_schnorr_verify: true,
            enable_balance_check: true,
            enable_dag_check: true,
            enable_rate_limit: true,
        }
    }
    
    /// Create pipeline with custom stage configuration
    pub fn with_config(
        schnorr: bool,
        balance: bool,
        dag: bool,
        rate_limit: bool,
    ) -> Self {
        Self {
            enable_schnorr_verify: schnorr,
            enable_balance_check: balance,
            enable_dag_check: dag,
            enable_rate_limit: rate_limit,
        }
    }
    
    /// Execute the full pipeline on a batch of transactions
    /// 
    /// Returns accepted transactions and rejected ones with reasons.
    /// Pipeline is deterministic: same input always produces same output.
    pub fn validate_batch(
        &self,
        txs: Vec<Transaction>,
        public_keys: Vec<RistrettoPoint>,
    ) -> PipelineResult {
        let start = Instant::now();
        let total_txs = txs.len();
        
        let mut metrics = PipelineMetrics {
            total_txs,
            ..Default::default()
        };
        
        let mut remaining = txs;
        let mut rejected = Vec::new();
        
        // Stage 1: Schnorr Signature Verification
        if self.enable_schnorr_verify {
            let stage_start = Instant::now();
            let (accepted, stage_rejected) = self.stage_schnorr_verify(remaining, public_keys);
            remaining = accepted;
            
            let reject_count = stage_rejected.len();
            rejected.extend(stage_rejected);
            
            metrics.stage_timings.insert("schnorr_verify".to_string(), stage_start.elapsed());
            metrics.rejected_by_stage.insert("schnorr_verify".to_string(), reject_count);
        }
        
        // Stage 2: Balance & UTXO Validation
        if self.enable_balance_check {
            let stage_start = Instant::now();
            let (accepted, stage_rejected) = self.stage_balance_check(remaining);
            remaining = accepted;
            
            let reject_count = stage_rejected.len();
            rejected.extend(stage_rejected);
            
            metrics.stage_timings.insert("balance_check".to_string(), stage_start.elapsed());
            metrics.rejected_by_stage.insert("balance_check".to_string(), reject_count);
        }
        
        // Stage 3: DAG Structural Validation
        if self.enable_dag_check {
            let stage_start = Instant::now();
            let (accepted, stage_rejected) = self.stage_dag_structure(remaining);
            remaining = accepted;
            
            let reject_count = stage_rejected.len();
            rejected.extend(stage_rejected);
            
            metrics.stage_timings.insert("dag_structure".to_string(), stage_start.elapsed());
            metrics.rejected_by_stage.insert("dag_structure".to_string(), reject_count);
        }
        
        // Stage 4: Rate Limiting & Fee Checks
        if self.enable_rate_limit {
            let stage_start = Instant::now();
            let (accepted, stage_rejected) = self.stage_rate_limit(remaining);
            remaining = accepted;
            
            let reject_count = stage_rejected.len();
            rejected.extend(stage_rejected);
            
            metrics.stage_timings.insert("rate_limit".to_string(), stage_start.elapsed());
            metrics.rejected_by_stage.insert("rate_limit".to_string(), reject_count);
        }
        
        metrics.accepted = remaining.len();
        metrics.total_time = start.elapsed();
        
        PipelineResult {
            accepted: remaining,
            rejected,
            metrics,
        }
    }
    
    // Stage implementations (placeholders for now)
    
    fn stage_schnorr_verify(
        &self,
        txs: Vec<Transaction>,
        public_keys: Vec<RistrettoPoint>,
    ) -> (Vec<Transaction>, Vec<(Transaction, RejectReason)>) {
        use crate::pipeline::schnorr_stage::SchnorrBatchVerifier;
        
        let verifier = SchnorrBatchVerifier::new();
        verifier.verify_batch(txs, public_keys)
    }
    
    fn stage_balance_check(
        &self,
        txs: Vec<Transaction>,
    ) -> (Vec<Transaction>, Vec<(Transaction, RejectReason)>) {
        // TODO: Implement balance checking
        (txs, Vec::new())
    }
    
    fn stage_dag_structure(
        &self,
        txs: Vec<Transaction>,
    ) -> (Vec<Transaction>, Vec<(Transaction, RejectReason)>) {
        // TODO: Implement DAG validation
        (txs, Vec::new())
    }
    
    fn stage_rate_limit(
        &self,
        txs: Vec<Transaction>,
    ) -> (Vec<Transaction>, Vec<(Transaction, RejectReason)>) {
        // TODO: Implement rate limiting
        (txs, Vec::new())
    }
}

impl Default for HardChecksPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pipeline_creation() {
        let pipeline = HardChecksPipeline::new();
        assert!(pipeline.enable_schnorr_verify);
        assert!(pipeline.enable_balance_check);
        assert!(pipeline.enable_dag_check);
        assert!(pipeline.enable_rate_limit);
    }
    
    #[test]
    fn test_empty_batch() {
        let pipeline = HardChecksPipeline::new();
        let result = pipeline.validate_batch(Vec::new(), Vec::new());
        
        assert_eq!(result.accepted.len(), 0);
        assert_eq!(result.rejected.len(), 0);
        assert_eq!(result.metrics.total_txs, 0);
    }
}



