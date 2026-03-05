pub mod engine;
pub mod simd;
pub mod barnes_hut;

pub use engine::{InterferenceEngine, InterferenceAnalysis, CoreInterferenceAccelerator, CpuAccelerator};
#[cfg(feature = "cuda")]
pub use engine::CudaAccelerator;
#[cfg(feature = "opencl")]
pub use engine::OpenClAccelerator;

pub use simd::{sum_cos_pairs, sum_cos_pairs_wavelet};



