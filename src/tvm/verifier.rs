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
    // TODO: Разобрать ELF и проверить секции/инструкции eBPF (для скорости оставляем верификацию на rbpf при загрузке)
    Ok(VerifiedProgram { code: code.to_vec() })
}
