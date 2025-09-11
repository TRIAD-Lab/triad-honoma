use std::env;

#[derive(Debug, Clone)]
pub struct TvmConfig {
    pub max_instructions: u64,
    pub max_stack_bytes: usize,
    pub max_heap_bytes: usize,
    pub max_input_bytes: usize,
    pub syscalls_enabled: SyscallsEnabled,
}

#[derive(Debug, Clone)]
pub struct SyscallsEnabled {
    pub sha256: bool,
    pub ed25519_verify: bool,
    pub math: bool,
    pub gpu_offload: bool,
}

impl Default for SyscallsEnabled {
    fn default() -> Self {
        Self { sha256: true, ed25519_verify: true, math: true, gpu_offload: false }
    }
}

impl Default for TvmConfig {
    fn default() -> Self {
        Self {
            max_instructions: 10_000_000,
            max_stack_bytes: 256 * 1024,
            max_heap_bytes: 4 * 1024 * 1024,
            max_input_bytes: 64 * 1024,
            syscalls_enabled: SyscallsEnabled::default(),
        }
    }
}

impl TvmConfig {
    pub fn from_env() -> Self {
        let def = Self::default();
        let parse_u64 = |k: &str, d: u64| env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let parse_usize = |k: &str, d: usize| env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let parse_bool = |k: &str, d: bool| env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        Self {
            max_instructions: parse_u64("TRIAD_TVM_MAX_INS", def.max_instructions),
            max_stack_bytes: parse_usize("TRIAD_TVM_MAX_STACK", def.max_stack_bytes),
            max_heap_bytes: parse_usize("TRIAD_TVM_MAX_HEAP", def.max_heap_bytes),
            max_input_bytes: parse_usize("TRIAD_TVM_MAX_INPUT", def.max_input_bytes),
            syscalls_enabled: SyscallsEnabled {
                sha256: parse_bool("TRIAD_TVM_SC_SHA256", def.syscalls_enabled.sha256),
                ed25519_verify: parse_bool("TRIAD_TVM_SC_ED25519", def.syscalls_enabled.ed25519_verify),
                math: parse_bool("TRIAD_TVM_SC_MATH", def.syscalls_enabled.math),
                gpu_offload: parse_bool("TRIAD_TVM_SC_GPU", def.syscalls_enabled.gpu_offload),
            },
        }
    }
}
