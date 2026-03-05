use std::sync::atomic::{AtomicU64, Ordering};

/// Unique identifier for an account's memory generation.
/// Incremented on every `realloc`.
#[derive(Debug)]
pub struct AccountGeneration {
    counter: AtomicU64,
}

impl AccountGeneration {
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(1),
        }
    }

    #[inline]
    pub fn current(&self) -> u64 {
        self.counter.load(Ordering::Acquire)
    }

    #[inline]
    pub fn next(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::Release) + 1
    }
}

impl Default for AccountGeneration {
    fn default() -> Self {
        Self::new()
    }
}
