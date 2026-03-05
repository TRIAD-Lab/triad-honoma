/// BLS Signature Aggregation Module
///
/// Использует blsful для агрегации подписей вместо Ed25519
/// Позволяет верифицировать 50k+ подписей за один вызов

use blsful::inner_types::*;
use blsful::{Bls12381G2Impl, PublicKey, SecretKey, Signature, MultiSignature};

use std::collections::HashMap;
use rayon::prelude::*;

/// BLS keypair
#[derive(Clone)]
pub struct BlsKeypair {
    pub secret: SecretKey<Bls12381G2Impl>,
    pub public: PublicKey<Bls12381G2Impl>,
}

impl BlsKeypair {
    /// Generate новую BLS keypair
    pub fn generate() -> Self {
        let secret = SecretKey::<Bls12381G2Impl>::random(rand::thread_rng());
        let public = secret.public_key();
        Self { secret, public }
    }
    
    /// Sign message
    pub fn sign(&self, message: &[u8]) -> Signature<Bls12381G2Impl> {
        self.secret.sign(blsful::SignatureSchemes::Basic, message).expect("BLS signing failed")
    }
    
    /// Verify signature
    pub fn verify(&self, message: &[u8], signature: &Signature<Bls12381G2Impl>) -> bool {
        signature.verify(&self.public, message).is_ok()
    }
}

/// BLS signature aggregator
pub struct BlsAggregator {
    signatures: Vec<Signature<Bls12381G2Impl>>,
    public_keys: Vec<PublicKey<Bls12381G2Impl>>,
    messages: Vec<Vec<u8>>,
}

impl BlsAggregator {
    pub fn new() -> Self {
        Self {
            signatures: Vec::new(),
            public_keys: Vec::new(),
            messages: Vec::new(),
        }
    }
    
    /// Add signature to aggregate
    pub fn add_signature(
        &mut self,
        signature: Signature<Bls12381G2Impl>,
        public_key: PublicKey<Bls12381G2Impl>,
        message: Vec<u8>,
    ) {
        self.signatures.push(signature);
        self.public_keys.push(public_key);
        self.messages.push(message);
    }
    
    /// Aggregate all signatures (для одинакового сообщения)
    pub fn aggregate_same_message(&self, message: &[u8]) -> Result<MultiSignature<Bls12381G2Impl>, String> {
        if self.signatures.is_empty() {
            return Err("No signatures to aggregate".to_string());
        }
        
        // Aggregate signatures
        let sigs: Vec<Signature<Bls12381G2Impl>> = self.signatures.iter().cloned().collect();
        let multi_sig = MultiSignature::from_signatures(&sigs).map_err(|e| e.to_string())?;
        
        Ok(multi_sig)
    }
    
    /// Verify aggregated signature
    pub fn verify_aggregated(
        &self,
        agg_sig: &MultiSignature<Bls12381G2Impl>,
        message: &[u8],
    ) -> bool {
        let pubkeys: Vec<PublicKey<Bls12381G2Impl>> = self.public_keys.iter().cloned().collect();
        let multi_pubkey = blsful::MultiPublicKey::from_public_keys(&pubkeys);
        agg_sig.verify(multi_pubkey, message).is_ok()
    }
    
    /// Batch verify (parallel) - для разных сообщений
    pub fn batch_verify_parallel(&self) -> Vec<bool> {
        (0..self.signatures.len())
            .into_par_iter()
            .map(|i| {
                self.signatures[i].verify(&self.public_keys[i], &self.messages[i]).is_ok()
            })
            .collect()
    }
    
    /// Count valid signatures
    pub fn count_valid(&self) -> usize {
        self.batch_verify_parallel().iter().filter(|&&v| v).count()
    }
}

/// Generate N BLS keypairs for simulation
pub fn generate_bls_keys(count: usize) -> Vec<BlsKeypair> {
    (0..count)
        .into_par_iter()
        .map(|_| BlsKeypair::generate())
        .collect()
}

/// Sign transactions with BLS (parallel)
pub fn sign_transactions_bls(
    transactions: &[(String, Vec<u8>)],  // (tx_id, tx_data)
    keypair: &BlsKeypair,
) -> Vec<(String, Signature<Bls12381G2Impl>)> {
    transactions
        .par_iter()
        .map(|(tx_id, tx_data)| {
            let signature = keypair.sign(tx_data);
            (tx_id.clone(), signature)
        })
        .collect()
}

/// Aggregate signatures for multiple transactions (same message)
pub fn aggregate_batch_signatures(
    signatures: &[Signature<Bls12381G2Impl>],
) -> MultiSignature<Bls12381G2Impl> {
    let sigs: Vec<Signature<Bls12381G2Impl>> = signatures.to_vec();
    MultiSignature::from_signatures(&sigs).expect("Failed to aggregate signatures")
}

/// Benchmark: BLS verification speed
pub fn benchmark_bls(num_signatures: usize) -> f64 {
    use std::time::Instant;
    
    let message = b"test transaction data";
    
    // BLS benchmark
    let bls_keypairs = generate_bls_keys(num_signatures);
    let bls_start = Instant::now();
    
    let mut aggregator = BlsAggregator::new();
    for keypair in &bls_keypairs {
        let sig = keypair.sign(message);
        aggregator.add_signature(sig, keypair.public.clone(), message.to_vec());
    }
    
    let agg_sig = aggregator.aggregate_same_message(message).unwrap();
    let _valid = aggregator.verify_aggregated(&agg_sig, message);
    
    bls_start.elapsed().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bls_sign_verify() {
        let keypair = BlsKeypair::generate();
        let message = b"test message";
        let signature = keypair.sign(message);
        assert!(keypair.verify(message, &signature));
    }
    
    #[test]
    fn test_bls_aggregation() {
        let message = b"same message for all";
        let keypairs = generate_bls_keys(10);
        
        let mut aggregator = BlsAggregator::new();
        for keypair in &keypairs {
            let sig = keypair.sign(message);
            aggregator.add_signature(sig, keypair.public.clone(), message.to_vec());
        }
        
        let agg_sig = aggregator.aggregate_same_message(message).unwrap();
        assert!(aggregator.verify_aggregated(&agg_sig, message));
    }
}
