use crate::tvm::TvmConfig;

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("program too large: {0} bytes (max {1})")] ProgramTooLarge(usize, usize),
    #[error("stack too large: {0} (max {1})")] StackTooLarge(usize, usize),
    #[error("heap too large: {0} (max {1})")] HeapTooLarge(usize, usize),
    #[error("abi invalid: {0}")] AbiInvalid(&'static str),
}

#[derive(Debug, Clone)]
pub struct VerifiedProgram {
    pub code: Vec<u8>,
}

pub fn verify_ebpf(code: &[u8], cfg: &TvmConfig) -> Result<VerifiedProgram, VerifyError> {
    // Minimal ELF sanity checks (no heavy parsing for performance)
    if code.len() < 24 {
        return Err(VerifyError::AbiInvalid("ELF too small"));
    }
    // 0..=3: 0x7F 'E' 'L' 'F'
    if &code[0..4] != b"\x7FELF" {
        return Err(VerifyError::AbiInvalid("bad ELF magic"));
    }
    // EI_CLASS (offset 4): 2 = 64-bit expected for eBPF
    if code[4] != 2u8 {
        return Err(VerifyError::AbiInvalid("ELF not 64-bit"));
    }
    // EI_DATA (offset 5): 1 = little-endian
    if code[5] != 1u8 {
        return Err(VerifyError::AbiInvalid("ELF not little-endian"));
    }
    // EI_VERSION (offset 6): 1 = current
    if code[6] != 1u8 {
        return Err(VerifyError::AbiInvalid("ELF bad ident version"));
    }
    // e_machine (bytes 18..20 little-endian) must be EM_BPF (247)
    let em = u16::from_le_bytes([code[18], code[19]]);
    if em != 247u16 {
        return Err(VerifyError::AbiInvalid("ELF not EM_BPF (247)"));
    }
    // Базовые лимиты размера кода (например, 1MB)
    let max_code = 1_048_576usize;
    if code.len() > max_code {
        return Err(VerifyError::ProgramTooLarge(code.len(), max_code));
    }
    if cfg.max_stack_bytes > 4 * 1024 * 1024 {
        return Err(VerifyError::StackTooLarge(cfg.max_stack_bytes, 4 * 1024 * 1024));
    }
    if cfg.max_heap_bytes > 64 * 1024 * 1024 {
        return Err(VerifyError::HeapTooLarge(cfg.max_heap_bytes, 64 * 1024 * 1024));
    }
    // Intentionally keep heavy instruction/section validation in rbpf loader for performance;
    // here we only enforce lightweight ELF sanity and global size limits.
    Ok(VerifiedProgram { code: code.to_vec() })
}
