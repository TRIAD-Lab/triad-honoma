// Phase 2: Enable advanced features
#![cfg_attr(feature = "portable_simd", feature(portable_simd))]
#![feature(generic_const_exprs)] // Const-eval shard routing
#![allow(incomplete_features)] // generic_const_exprs is experimental

// Legacy modules (temporary during migration)
pub mod core;
pub mod sharding;
pub mod network;
pub mod error_analysis;
pub mod semantic;
pub mod transaction;
pub mod token;
pub mod dag;
pub mod github;
pub mod tvm;
pub mod pipeline;
pub mod crypto;
pub mod storage;
pub mod memory; // Phase 3: QCell Storage
pub mod cpu_pinning; // Phase 4: Adaptive core isolation
pub mod trvm; // Integrated execution engine
// pub mod tvm; // Legacy, replaced by TRVM


// Benchmark helpers (REAL logic for criterion)
pub mod benchmark_helpers;
pub use core::field::CoreField;
pub use transaction::{Transaction, TransactionProcessor, TransactionStorage};
pub use dag::Dag;
pub use pipeline::{PipelineConfig, StageMetrics, Histogram, Pipeline, IngressItem};


