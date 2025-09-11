use crate::tvm::TvmConfig;
use crate::tvm::verifier::{verify_ebpf, VerifiedProgram};
use crate::tvm::syscalls::SyscallResolver;
use crate::tvm::context::TvmContext;
use crate::tvm::abi::AbiConst;
use solana_rbpf::{
    elf::Executable,
    program::{BuiltinFunction, BuiltinProgram, FunctionRegistry},
    vm::Config as RbpfConfig,
};
use std::sync::Arc;

#[derive(Debug)]
pub struct TvmProgram {
    pub exec: Executable<TvmContext>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("verify failed: {0}")] Verify(String),
    #[error("rbpf: {0}")] Rbpf(String),
}

pub fn load_program(code_elf: &[u8], cfg: &TvmConfig, resolver: SyscallResolver) -> Result<TvmProgram, LoadError> {
    let _vp: VerifiedProgram = verify_ebpf(code_elf, cfg).map_err(|e| LoadError::Verify(e.to_string()))?;

    // Конфигурация RBPF VM
    let rbpf_cfg = RbpfConfig::default();

    // Регистрируем системные вызовы в FunctionRegistry
    let mut freg: FunctionRegistry<BuiltinFunction<TvmContext>> = FunctionRegistry::default();
    for name in [
        AbiConst::SC_SHA256,
        AbiConst::SC_ED25519_VERIFY,
        AbiConst::SC_MATH_ADD_F64,
        AbiConst::SC_KV_GET,
        AbiConst::SC_KV_PUT,
        AbiConst::SC_KV_DEL,
    ] {
        if let Some(func) = resolver(name) {
            freg
                .register_function_hashed(name.as_bytes(), func)
                .map_err(|e| LoadError::Rbpf(e.to_string()))?;
        }
    }

    // Создаём BuiltinProgram (loader) с конфигурацией
    let loader = Arc::new(BuiltinProgram::new_loader(rbpf_cfg, freg));

    // Загружаем Executable из ELF
    let exec = Executable::<TvmContext>::from_elf(code_elf, loader)
        .map_err(|e| LoadError::Rbpf(e.to_string()))?;

    Ok(TvmProgram { exec })
}
