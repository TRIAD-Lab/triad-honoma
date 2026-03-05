//! Interference batch primitives and data structures used by the DAG.
//! Structured per architecture guidelines: `interference_batch` owns the
//! batch lifecycle API, while `merkle` and `mmr` implement the hash-based
//! accumulators used during finalization.

pub mod interference_batch;
pub mod merkle;
pub mod mmr;
pub mod proof;

pub use interference_batch::{
    BatchError,
    BatchFeatures,
    BatchFinalizeEvidence,
    BatchFinalizeMetrics,
    BatchSignature,
    InterferenceBatch,
    TreeStrategy,
};
pub use merkle::{
    merkle_root_parallel,
    verify_merkle_proof,
    ParallelMerkleTree,
};
pub use mmr::{
    verify_mmr_proof,
    Blake3Mmr,
    MmrProof,
};
pub use proof::{ProofNode, ProofPosition};


