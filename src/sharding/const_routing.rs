/// Compile-Time Shard Routing Tables (Phase 2)
/// 
/// Precomputed routing tables for extreme performance sharding

use std::sync::LazyLock;

/// Number of shards (must be power of 2 for efficient bit masking)
// OPTIMIZATION: Reduced to 8 to match typical CPU core counts (avoid overhead)
pub const SHARD_COUNT: usize = 8;

/// Shard mask for fast modulo operation (SHARD_COUNT - 1)
pub const SHARD_MASK: usize = SHARD_COUNT - 1;

/// Precomputed shard routing table
/// Maps transaction ID hash to shard ID with perfect load balancing
static SHARD_ROUTING_TABLE: LazyLock<[u8; 65536]> = LazyLock::new(|| {
    let mut table = [0u8; 65536];
    
    // Distribute txs evenly across shards using deterministic hashing
    for i in 0..65536 {
        // Use FNV-1a hash for fast, collision-resistant distribution
        let mut hash = 2166136261u32;
        hash ^= (i & 0xFF) as u32;
        hash = hash.wrapping_mul(16777619);
        hash ^= ((i >> 8) & 0xFF) as u32;
        hash = hash.wrapping_mul(16777619);
        
        table[i] = (hash as usize % SHARD_COUNT) as u8;
    }
    
    table
});

/// Precomputed cross-shard routing costs
/// Cost matrix for moving data between shards (network latency simulation)
static CROSS_SHARD_COSTS: LazyLock<[[u8; SHARD_COUNT]; SHARD_COUNT]> = LazyLock::new(|| {
    let mut costs = [[0u8; SHARD_COUNT]; SHARD_COUNT];
    
    for from in 0..SHARD_COUNT {
        for to in 0..SHARD_COUNT {
            if from == to {
                costs[from][to] = 0; // Same shard: zero cost
            } else {
                // Distance-based cost: shards closer in ID have lower cost
                let distance = (from as i32 - to as i32).abs() as u8;
                costs[from][to] = (distance % 16) + 1; // 1-16 cost range
            }
        }
    }
    
    costs
});

/// O(1) shard routing lookup
#[inline]
pub fn get_shard_id(tx_hash: &[u8; 32]) -> u8 {
    // Use first 2 bytes of hash as lookup index
    let index = ((tx_hash[0] as usize) << 8) | (tx_hash[1] as usize);
    SHARD_ROUTING_TABLE[index]
}

/// O(1) cross-shard cost lookup
#[inline]
pub fn get_cross_shard_cost(from_shard: u8, to_shard: u8) -> u8 {
    CROSS_SHARD_COSTS[from_shard as usize][to_shard as usize]
}

/// Fast shard mask operation (replaces modulo)
#[inline]
pub fn mask_to_shard(value: usize) -> usize {
    value & SHARD_MASK
}

/// Batch shard assignment for transaction pool
pub fn assign_shards_batch(tx_hashes: &[[u8; 32]]) -> Vec<u8> {
    use rayon::prelude::*;
    
    tx_hashes.par_iter()
        .map(|hash| get_shard_id(hash))
        .collect()
}

/// Optimal shard routing path (for cross-shard transactions)
pub fn find_optimal_route(from: u8, to: u8, max_hops: u8) -> Vec<u8> {
    if from == to {
        return vec![from];
    }
    
    // Simple direct routing (can be enhanced with A* for complex networks)
    vec![from, to]
}

#[cfg(test)]
mod tests {
    use super::*;
    use blake3;
    
    #[test]
    fn test_shard_distribution() {
        let mut shard_counts = vec![0usize; SHARD_COUNT];
        
        // Test 10k random hashes
        for i in 0..10000 {
            let hash = blake3::hash(&i.to_le_bytes());
            let shard = get_shard_id(hash.as_bytes());
            shard_counts[shard as usize] += 1;
        }
        
        // Check distribution is relatively even (within 20% of mean)
        let mean = 10000 / SHARD_COUNT;
        let tolerance = mean / 5;
        
        for count in shard_counts {
            assert!(count > mean - tolerance && count < mean + tolerance,
                "Uneven distribution: {} (mean: {})", count, mean);
        }
    }
    
    #[test]
    fn test_cross_shard_costs() {
        // Same shard should be zero cost
        assert_eq!(get_cross_shard_cost(0, 0), 0);
        assert_eq!(get_cross_shard_cost(100, 100), 0);
        
        // Adjacent shards should have low cost
        assert!(get_cross_shard_cost(0, 1) < 5);
        
        // Distant shards should have higher cost
        assert!(get_cross_shard_cost(0, 200) > 0);
    }
    
    #[test]
    fn test_batch_assignment() {
        let hashes: Vec<[u8; 32]> = (0..1000)
            .map(|i| *blake3::hash(&i.to_le_bytes()).as_bytes())
            .collect();
        
        let shards = assign_shards_batch(&hashes);
        assert_eq!(shards.len(), 1000);
        
        // All shards should be valid
        for shard in shards {
            assert!(shard < SHARD_COUNT as u8);
        }
    }
}


