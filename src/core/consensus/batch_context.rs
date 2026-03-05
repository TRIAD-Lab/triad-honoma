use std::sync::Arc;

use arc_swap::ArcSwap;
use blake3::Hasher;

/// Immutable batch context shared across pipeline/consensus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchContext {
    pub merkle_root: [u8; 32],
    pub prev_batch_hash: [u8; 32],
    pub slot: u64,
    pub shard_id: u8,
    pub timestamp: u64,
}

impl Default for BatchContext {
    fn default() -> Self {
        Self {
            merkle_root: [0u8; 32],
            prev_batch_hash: [0u8; 32],
            slot: 0,
            shard_id: 0,
            timestamp: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BatchContextHandle {
    inner: Arc<ArcSwap<BatchContext>>,
}

impl BatchContextHandle {
    pub fn new() -> Self {
        Self { inner: Arc::new(ArcSwap::from_pointee(BatchContext::default())) }
    }

    #[inline]
    pub fn load(&self) -> Arc<BatchContext> {
        self.inner.load_full()
    }

    #[inline]
    pub fn store(&self, ctx: BatchContext) {
        self.inner.store(Arc::new(ctx));
    }
}

impl Default for BatchContextHandle {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
pub fn derive_block_salt(ctx: &BatchContext, state_id: &str) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(&ctx.merkle_root);
    hasher.update(&ctx.prev_batch_hash);
    hasher.update(&ctx.slot.to_le_bytes());
    hasher.update(&ctx.shard_id.to_le_bytes());
    hasher.update(&ctx.timestamp.to_le_bytes());
    hasher.update(state_id.as_bytes());
    let digest = hasher.finalize();
    *digest.as_bytes()
}


