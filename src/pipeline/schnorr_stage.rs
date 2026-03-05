/// Schnorr Signature Batch Verification Stage
/// 
/// Stage 1 of Hard Checks Pipeline - verifies all transaction signatures
/// using Schnorr batch verification for O(n) performance.

use crate::transaction::Transaction;
use crate::crypto::schnorr::{verify, verify_batch, SchnorrSignature};
use crate::pipeline::hard_checks::RejectReason;
use curve25519_dalek::ristretto::RistrettoPoint;
use rayon::prelude::*;

/// Schnorr batch verifier
pub struct SchnorrBatchVerifier {
    /// Threshold for using batch vs individual verification
    /// Batch verification has overhead, only worth it for larger batches
    batch_threshold: usize,
}

impl SchnorrBatchVerifier {
    pub fn new() -> Self {
        Self {
            batch_threshold: 10,
        }
    }
    
    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            batch_threshold: threshold,
        }
    }
    
    /// Verify batch of transactions with their signatures
    /// 
    /// # Arguments
    /// * `txs` - Transactions to verify
    /// * `public_keys` - Corresponding public keys (must match txs length)
    /// 
    /// # Returns
    /// Tuple of (accepted_txs, rejected_txs_with_reasons)
    pub fn verify_batch(
        &self,
        txs: Vec<Transaction>,
        public_keys: Vec<RistrettoPoint>,
    ) -> (Vec<Transaction>, Vec<(Transaction, RejectReason)>) {
        assert_eq!(txs.len(), public_keys.len(), "Transaction and key count mismatch");
        
        if txs.is_empty() {
            return (Vec::new(), Vec::new());
        }
        
        // For small batches, verify individually (faster)
        if txs.len() < self.batch_threshold {
            return self.verify_individually(txs, public_keys);
        }
        
        // For large batches, use batch verification
        self.verify_in_batch(txs, public_keys)
    }
    
    /// Individual verification for small batches
    fn verify_individually(
        &self,
        txs: Vec<Transaction>,
        public_keys: Vec<RistrettoPoint>,
    ) -> (Vec<Transaction>, Vec<(Transaction, RejectReason)>) {
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        
        for (tx, pk) in txs.into_iter().zip(public_keys.into_iter()) {
            // Parse signature from transaction
            if tx.signature.len() != 64 {
                rejected.push((tx, RejectReason::InvalidSignature));
                continue;
            }
            
            let mut sig_bytes = [0u8; 64];
            sig_bytes.copy_from_slice(&tx.signature);
            
            let signature = match SchnorrSignature::from_bytes(&sig_bytes) {
                Some(sig) => sig,
                None => {
                    rejected.push((tx, RejectReason::InvalidSignature));
                    continue;
                }
            };
            
            // Verify signature over transaction data
            if verify(&pk, &tx.data, &signature) {
                accepted.push(tx);
            } else {
                rejected.push((tx, RejectReason::InvalidSignature));
            }
        }
        
        (accepted, rejected)
    }
    
    /// Batch verification for large batches
    /// 
    /// Uses crypto::schnorr::verify_batch for O(n) verification
    fn verify_in_batch(
        &self,
        txs: Vec<Transaction>,
        public_keys: Vec<RistrettoPoint>,
    ) -> (Vec<Transaction>, Vec<(Transaction, RejectReason)>) {
        // Parse all signatures
        let mut signatures = Vec::with_capacity(txs.len());
        let mut messages: Vec<&[u8]> = Vec::with_capacity(txs.len());
        let mut valid_indices = Vec::new();
        
        for (idx, tx) in txs.iter().enumerate() {
            if tx.signature.len() != 64 {
                continue; // Skip invalid signatures
            }
            
            let mut sig_bytes = [0u8; 64];
            sig_bytes.copy_from_slice(&tx.signature);
            
            if let Some(sig) = SchnorrSignature::from_bytes(&sig_bytes) {
                signatures.push(sig);
                messages.push(&tx.data[..]);
                valid_indices.push(idx);
            }
        }
        
        // Batch verify
        let batch_valid = if !signatures.is_empty() {
            let pks: Vec<RistrettoPoint> = valid_indices.iter()
                .map(|&i| public_keys[i])
                .collect();
            verify_batch(&pks, &messages, &signatures)
        } else {
            false
        };
        
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        
        if batch_valid {
            // All valid signatures - accept those with valid format
            for (idx, tx) in txs.into_iter().enumerate() {
                if valid_indices.contains(&idx) {
                    accepted.push(tx);
                } else {
                    rejected.push((tx, RejectReason::InvalidSignature));
                }
            }
        } else {
            // Batch failed - fall back to individual verification
            // This handles the case where some signatures are invalid
            return self.verify_individually(txs, public_keys);
        }
        
        (accepted, rejected)
    }
}

impl Default for SchnorrBatchVerifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::schnorr::{generate_keypair, sign};
    
    #[test]
    fn test_empty_batch() {
        let verifier = SchnorrBatchVerifier::new();
        let (accepted, rejected) = verifier.verify_batch(Vec::new(), Vec::new());
        assert_eq!(accepted.len(), 0);
        assert_eq!(rejected.len(), 0);
    }
    
    #[test]
    fn test_single_valid_signature() {
        let verifier = SchnorrBatchVerifier::new();
        let keypair = generate_keypair();
        
        let mut tx = Transaction::default();
        tx.data = b"test transaction".to_vec();
        
        let sig = sign(&keypair, &tx.data);
        tx.signature = sig.to_bytes().to_vec();
        
        let (accepted, rejected) = verifier.verify_batch(vec![tx], vec![keypair.public]);
        assert_eq!(accepted.len(), 1);
        assert_eq!(rejected.len(), 0);
    }
    
    #[test]
    fn test_invalid_signature() {
        let verifier = SchnorrBatchVerifier::new();
        let keypair = generate_keypair();
        
        let mut tx = Transaction::default();
        tx.data = b"test transaction".to_vec();
        tx.signature = vec![0u8; 64]; // Invalid signature
        
        let (accepted, rejected) = verifier.verify_batch(vec![tx], vec![keypair.public]);
        assert_eq!(accepted.len(), 0);
        assert_eq!(rejected.len(), 1);
        
        match &rejected[0].1 {
            RejectReason::InvalidSignature => {},
            _ => panic!("Expected InvalidSignature reject reason"),
        }
    }
}


