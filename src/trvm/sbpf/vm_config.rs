use solana_rbpf::vm::Config;

/// Provides the standard sBPF configuration for TRIAD TRVM.
pub struct SbpfConfig;

impl SbpfConfig {
    /// Returns the optimized VM configuration for TRVM.
    pub fn get_config() -> Config {
        Config {
            max_call_depth: 64,
            stack_size: 4096,
            enable_instruction_meter: true,
            enable_instruction_tracing: true,
            ..Config::default()
        }
    }
}
