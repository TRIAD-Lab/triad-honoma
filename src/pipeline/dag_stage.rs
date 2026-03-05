/// Simplified DAG Structure Validation
/// 
/// Fast O(n) validation that only checks:
/// 1. Parent transactions exist
/// 2. No missing references
/// 
/// Trade-off: Does NOT detect cycles (deferred to post-consensus)
/// Target: < 50ms for 10K transactions

use crate::transaction::Transaction;
use crate::pipeline::hard_checks::RejectReason;
use ahash::AHashSet as HashSet;
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;

/// Lightweight DAG structure validator
pub struct SimplifiedDAGChecker {
    /// Known transaction IDs from current and recent batches
    known_txs: Arc<RwLock<HashSet<String>>>,
    /// FIFO order of cached ids to support bounded eviction
    order: Arc<RwLock<VecDeque<String>>>,
    /// Maximum cache size
    max_cache_size: usize,
}

impl SimplifiedDAGChecker {
    pub fn new() -> Self {
        Self {
            known_txs: Arc::new(RwLock::new(HashSet::new())),
            order: Arc::new(RwLock::new(VecDeque::new())),
            max_cache_size: 100_000,
        }
    }

    pub fn with_cache_size(size: usize) -> Self {
        Self {
            known_txs: Arc::new(RwLock::new(HashSet::with_capacity(size))),
            order: Arc::new(RwLock::new(VecDeque::with_capacity(size))),
            max_cache_size: size,
        }
    }
    
    /// Fast validation: only check parent existence
    /// 
    /// # Performance
    /// O(n) with hash lookups - target < 50ms for 10K txs
    pub fn validate_lite(
        &self,
        txs: Vec<Transaction>,
    ) -> (Vec<Transaction>, Vec<(Transaction, RejectReason)>) {
        let mut accepted = Vec::with_capacity(txs.len());
        let mut rejected = Vec::new();
        
        // Build set of current batch tx IDs
        let current_batch: HashSet<String> = txs.iter()
            .map(|tx| tx.id.to_string())
            .collect();
        
        // Read known txs once
        let known = self.known_txs.read();
        
        for tx in txs {
            // Check if parent exists (in current batch OR in known txs)
            if !tx.parent_ids.is_empty() {
                let mut all_parents_exist = true;
                
                for parent_id in &tx.parent_ids {
                    if !current_batch.contains(parent_id) && !known.contains::<str>(parent_id) {
                        rejected.push((
                            tx.clone(),
                            RejectReason::MissingParent(parent_id.clone())
                        ));
                        all_parents_exist = false;
                        break;
                    }
                }
                
                if all_parents_exist {
                    accepted.push(tx);
                }
            } else {
                // No parents - genesis transaction or independent tx
                accepted.push(tx);
            }
        }
        
        drop(known); // Release read lock
        
        // Update known txs cache
        self.update_cache(current_batch.into_iter());

        (accepted, rejected)
    }
    
    /// Update the known transactions cache
    fn update_cache<I>(&self, new_txs: I)
    where
        I: IntoIterator<Item = String>,
    {
        let mut known = self.known_txs.write();
        let mut order = self.order.write();

        for tx_id in new_txs {
            if known.insert(tx_id.clone()) {
                order.push_back(tx_id);
            }
        }

        while known.len() > self.max_cache_size {
            if let Some(oldest) = order.pop_front() {
                known.remove(&oldest);
            } else {
                break;
            }
        }
    }
    
    /// Clear the cache (for testing or reset)
    pub fn clear_cache(&self) {
        self.known_txs.write().clear();
        self.order.write().clear();
    }
}

impl Default for SimplifiedDAGChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_empty_batch() {
        let checker = SimplifiedDAGChecker::new();
        let (accepted, rejected) = checker.validate_lite(Vec::new());
        assert_eq!(accepted.len(), 0);
        assert_eq!(rejected.len(), 0);
    }
    
    #[test]
    fn test_genesis_transaction() {
        let checker = SimplifiedDAGChecker::new();
        
        let mut tx = Transaction::default();
        tx.id = "tx1".to_string();
        tx.parent_ids = Vec::new(); // No parents
        
        let (accepted, rejected) = checker.validate_lite(vec![tx]);
        assert_eq!(accepted.len(), 1);
        assert_eq!(rejected.len(), 0);
    }
    
    #[test]
    fn test_missing_parent() {
        let checker = SimplifiedDAGChecker::new();
        
        let mut tx = Transaction::default();
        tx.id = "tx2".to_string();
        tx.parent_ids = vec!["missing_tx".to_string()];
        
        let (accepted, rejected) = checker.validate_lite(vec![tx]);
        assert_eq!(accepted.len(), 0);
        assert_eq!(rejected.len(), 1);
        
        match &rejected[0].1 {
            RejectReason::MissingParent(id) => assert_eq!(id, "missing_tx"),
            _ => panic!("Expected MissingParent reject reason"),
        }
    }
    
    #[test]
    fn test_parent_in_same_batch() {
        let checker = SimplifiedDAGChecker::new();
        
        let mut tx1 = Transaction::default();
        tx1.id = "tx1".to_string();
        tx1.parent_ids = Vec::new();
        
        let mut tx2 = Transaction::default();
        tx2.id = "tx2".to_string();
        tx2.parent_ids = vec!["tx1".to_string()];
        
        let (accepted, rejected) = checker.validate_lite(vec![tx1, tx2]);
        assert_eq!(accepted.len(), 2); // Both should pass
        assert_eq!(rejected.len(), 0);
    }
    
    #[test]
    fn test_cache_persistence() {
        let checker = SimplifiedDAGChecker::new();
        
        // First batch
        let mut tx1 = Transaction::default();
        tx1.id = "tx1".to_string();
        tx1.parent_ids = Vec::new();
        
        checker.validate_lite(vec![tx1]);
        
        // Second batch referencing first
        let mut tx2 = Transaction::default();
        tx2.id = "tx2".to_string();
        tx2.parent_ids = vec!["tx1".to_string()];
        
        let (accepted, rejected) = checker.validate_lite(vec![tx2]);
        assert_eq!(accepted.len(), 1); // Should find tx1 in cache
        assert_eq!(rejected.len(), 0);
    }
}


