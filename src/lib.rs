// Legacy modules (temporary during migration)
pub mod quantum;
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
pub use quantum::field::QuantumField;
pub use transaction::{Transaction, TransactionProcessor, TransactionStorage};
pub use dag::Dag;
pub use pipeline::{PipelineConfig, StageMetrics, Histogram, Pipeline, IngressItem};