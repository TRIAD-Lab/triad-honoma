pub mod const_routing; // Phase 2: Compile-time shard routing

use crate::core::consensus::{ConsensusNode, ConsensusMessage};
use rayon::prelude::*;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use smartcore::tree::decision_tree_classifier::DecisionTreeClassifier;
use smartcore::linalg::basic::matrix::DenseMatrix;

use smartcore::tree::decision_tree_classifier::DecisionTreeClassifierParameters;

pub use const_routing::{get_shard_id, assign_shards_batch, SHARD_COUNT};

pub struct Shard {
    pub id: usize,
    pub nodes: Vec<ConsensusNode>,
}

impl Shard {
    pub fn new(id: usize, nodes: Vec<ConsensusNode>) -> Self {
        Self { id, nodes }
    }

    /// Message processing inside a shard (each node gets its own queue)
    pub fn process_messages(&mut self, all_messages: &[Vec<ConsensusMessage>]) {
        self.nodes.par_iter_mut().enumerate().for_each(|(node_idx, node)| {
            for message in &all_messages[node_idx] {
                let public_key = node.keypair.public;
                let _ = node.process_message(message.clone(), &public_key);
            }
        });
    }

    pub fn process_batches(&mut self, all_batches: &[Vec<ConsensusMessage>]) {
        self.nodes.par_iter_mut().enumerate().for_each(|(node_idx, node)| {
            for batch in &all_batches[node_idx] {
                let public_key = node.keypair.public;
                let _ = node.process_message(batch.clone(), &public_key);
            }
        });
    }

    pub fn dynamic_reshard(&mut self, load_metrics: HashMap<usize, f64>, ai: Option<&ShardAI>) {
        let mut high_load_shards = vec![];
        for (shard_id, load) in load_metrics {
            let overload = if let Some(ai) = ai {
                ai.predict_overload(vec![load])
            } else {
                load > 0.8
            };
            if overload {
                high_load_shards.push(shard_id);
            }
        }
        for shard_id in high_load_shards {
            // Split shard logic
            let new_shard = Shard::new(self.id + 1, vec![]);
            // Move nodes, etc.
        }
    }
}

pub struct ShardAI {
    model: DecisionTreeClassifier<f64, u32, DenseMatrix<f64>, Vec<u32>>,
}

impl ShardAI {
    pub fn new() -> Self {
        let x = DenseMatrix::from_2d_vec(&vec![vec![0.9], vec![0.1]]);
        let y = vec![1, 0];
        let model = DecisionTreeClassifier::fit(
            &x,
            &y,
            DecisionTreeClassifierParameters::default()
        ).unwrap();
        Self { model }
    }

    pub fn predict_overload(&self, features: Vec<f64>) -> bool {
        let x = DenseMatrix::from_2d_vec(&vec![features]);
        self.model.predict(&x).unwrap()[0] == 1
    }
}



/// Теперь это — ультрабыстрый шардинг
#[inline(always)]
pub fn assign_shard(tx_hash: &[u8; 32]) -> u8 {
    get_shard_id(tx_hash) // ← твой шедевр из const_routing
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardEvent {
    pub shard_id: String,
    pub event_type: String,
    pub node_id: String,
    pub timestamp: std::time::SystemTime,
    pub details: Option<String>,
}

#[cfg(test)]
pub mod test_sharding; 


