use crate::core::consensus::{ConsensusNode, ConsensusMessage};
use super::{Shard, assign_shard};
use rayon::prelude::*;
use bytes::Bytes;

#[test]
fn test_sharding_parallel_processing() {
    let num_shards = 4;
    let nodes_per_shard = 5;
    let messages_per_node = 10;

    // Create shards
    let mut shards: Vec<Shard> = (0..num_shards)
        .map(|shard_id| {
            let nodes = (0..nodes_per_shard)
                .map(|i| {
                    let node_id_str = format!("shard{}_node{}", shard_id, i);
                    let shard_id_u8 = assign_shard(&node_id_str, num_shards);
                    // Generate a deterministic u128 ID from the string
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    use std::hash::Hash;
                    use std::hash::Hasher;
                    node_id_str.hash(&mut hasher);
                    let node_id_u128 = hasher.finish() as u128;
                    
                    ConsensusNode::new(node_id_u128, shard_id_u8)
                })
                .collect();
            Shard::new(shard_id, nodes)
        })
        .collect();

    // For each shard — its own list of messages for each node
    let mut all_shard_messages: Vec<Vec<Vec<ConsensusMessage>>> = vec![
        vec![vec![]; nodes_per_shard]; num_shards
    ];
    for shard_id in 0..num_shards {
        for node_idx in 0..nodes_per_shard {
            let messages = (0..messages_per_node)
                .map(|op| ConsensusMessage {
                    sender_id: format!("shard{}_node{}", shard_id, node_idx),
                    state_id: format!("state{}_{}_{}", shard_id, node_idx, op),
                    raw_data: Bytes::from(vec![op as u8, (op + 1) as u8, (op + 2) as u8]),
                    signature: vec![],
                })
                .collect();
            all_shard_messages[shard_id][node_idx] = messages;
        }
    }

    // Parallel processing of shards
    shards.par_iter_mut().enumerate().for_each(|(shard_id, shard)| {
        shard.process_messages(&all_shard_messages[shard_id]);
    });
} 


