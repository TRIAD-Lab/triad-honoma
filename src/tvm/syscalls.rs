use sha2::{Digest, Sha256};
use ed25519_dalek::{Verifier, VerifyingKey, Signature};
use std::sync::Arc;
use crate::tvm::abi::AbiConst;
use crate::tvm::context::TvmContext;
use crate::tvm::host;
use crate::tvm::config::TvmConfig;
use solana_rbpf::program::BuiltinFunction;
use solana_rbpf::vm::EbpfVm;
use solana_rbpf::error::ProgramResult;
use solana_rbpf::memory_region::AccessType;
use serde_json::Value;

// Тип для регистрации системных вызовов в RBPF (с контекстом TvmContext)
pub type SyscallResolver = Arc<dyn Fn(&str) -> Option<BuiltinFunction<TvmContext>> + Send + Sync>;

// KV работает через host-контекст (Snapshot/WriteBatch) в tvm::host

pub struct SyscallRegistryBuilder {
    entries: Vec<(String, BuiltinFunction<TvmContext>)>,
}

impl SyscallRegistryBuilder {
    pub fn new() -> Self { 
        Self { 
            entries: Vec::new() 
        } 
    }

    pub fn with_sha256(mut self) -> Self {
        self.entries.push((AbiConst::SC_SHA256.into(), builtin_sha256()));
        self
    }

    pub fn with_ed25519_verify(mut self) -> Self {
        self.entries.push((AbiConst::SC_ED25519_VERIFY.into(), builtin_ed25519_verify()));
        self
    }

    pub fn with_math(mut self) -> Self {
        self.entries.push((AbiConst::SC_MATH_ADD_F64.into(), builtin_math_add_f64()));
        self
    }

    pub fn with_kv(mut self) -> Self {
        self.entries.push((AbiConst::SC_KV_GET.into(), builtin_kv_get()));
        self.entries.push((AbiConst::SC_KV_PUT.into(), builtin_kv_put()));
        self.entries.push((AbiConst::SC_KV_DEL.into(), builtin_kv_del()));
        self
    }

    pub fn with_trd20(mut self) -> Self {
        self.entries.push((AbiConst::SC_TRD20_CALL.into(), builtin_trd20_call()));
        self
    }

    pub fn build(self) -> SyscallResolver {
        let map: std::collections::HashMap<String, BuiltinFunction<TvmContext>> = self.entries.into_iter().collect();
        Arc::new(move |name: &str| map.get(name).cloned())
    }
    
}

/// Builds a syscall resolver according to TvmConfig.syscalls_enabled
pub fn build_resolver_from_config(cfg: &TvmConfig) -> SyscallResolver {
    let mut b = SyscallRegistryBuilder::new();
    if cfg.syscalls_enabled.sha256 { b = b.with_sha256(); }
    if cfg.syscalls_enabled.ed25519_verify { b = b.with_ed25519_verify(); }
    if cfg.syscalls_enabled.math { b = b.with_math(); }
    // Enable KV/TRD20 according to whitelist toggles
    if cfg.syscalls_enabled.kv { b = b.with_kv(); }
    if cfg.syscalls_enabled.trd20 { b = b.with_trd20(); }
    b.build()
}

fn builtin_sha256() -> BuiltinFunction<TvmContext> { sha256_vm }

fn sha256_vm(vm: *mut EbpfVm<TvmContext>, buf_ptr: u64, buf_len: u64, _c: u64, _d: u64, _e: u64) {
    let vm = unsafe { &mut *vm };
    // Читаем байты из памяти гостя
    let host_ptr = match vm.memory_mapping.map(AccessType::Load, buf_ptr, buf_len) {
        ProgramResult::Ok(addr) => addr,
        _ => { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
    };
    let bytes = unsafe { std::slice::from_raw_parts(host_ptr as *const u8, buf_len as usize) };
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut eight = [0u8; 8];
    eight.copy_from_slice(&digest[..8]);
    vm.program_result = ProgramResult::Ok(u64::from_le_bytes(eight));
}

fn builtin_ed25519_verify() -> BuiltinFunction<TvmContext> { ed25519_verify_vm }

fn ed25519_verify_vm(vm: *mut EbpfVm<TvmContext>, msg_ptr: u64, msg_len: u64, sig_ptr: u64, pk_ptr: u64, _e: u64) {
    let vm = unsafe { &mut *vm };
    // Читаем сообщение
    let msg_host = match vm.memory_mapping.map(AccessType::Load, msg_ptr, msg_len) {
        ProgramResult::Ok(addr) => addr,
        _ => { vm.program_result = ProgramResult::Ok(0); return; }
    };
    let msg = unsafe { std::slice::from_raw_parts(msg_host as *const u8, msg_len as usize) };
    // Подпись 64 байта
    let sig_host = match vm.memory_mapping.map(AccessType::Load, sig_ptr, 64) {
        ProgramResult::Ok(addr) => addr,
        _ => { vm.program_result = ProgramResult::Ok(0); return; }
    };
    let sig_bytes = unsafe { std::slice::from_raw_parts(sig_host as *const u8, 64) };
    // Публичный ключ 32 байта
    let pk_host = match vm.memory_mapping.map(AccessType::Load, pk_ptr, 32) {
        ProgramResult::Ok(addr) => addr,
        _ => { vm.program_result = ProgramResult::Ok(0); return; }
    };
    let pk_bytes = unsafe { std::slice::from_raw_parts(pk_host as *const u8, 32) };

    let sig = match Signature::from_slice(sig_bytes) {
        Ok(s) => s,
        Err(_) => { vm.program_result = ProgramResult::Ok(0); return; }
    };
    let vk = match VerifyingKey::from_bytes(&pk_bytes.try_into().unwrap_or([0u8;32])) {
        Ok(v) => v,
        Err(_) => { vm.program_result = ProgramResult::Ok(0); return; }
    };
    let ok = vk.verify(msg, &sig).is_ok();
    vm.program_result = ProgramResult::Ok(if ok { 1 } else { 0 });
}

fn builtin_math_add_f64() -> BuiltinFunction<TvmContext> { math_add_f64_vm }

fn math_add_f64_vm(vm: *mut EbpfVm<TvmContext>, a_bits: u64, b_bits: u64, _c: u64, _d: u64, _e: u64) {
    let vm = unsafe { &mut *vm };
    let a = f64::from_bits(a_bits);
    let b = f64::from_bits(b_bits);
    let c = a + b;
    vm.program_result = ProgramResult::Ok(c.to_bits());
}

fn builtin_kv_get() -> BuiltinFunction<TvmContext> { kv_get_vm }

fn kv_get_vm(vm: *mut EbpfVm<TvmContext>, key_ptr: u64, key_len: u64, out_ptr: u64, out_len: u64, _e: u64) {
    let vm = unsafe { &mut *vm };
    let key_host = match vm.memory_mapping.map(AccessType::Load, key_ptr, key_len) {
        ProgramResult::Ok(addr) => addr,
        _ => { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
    };
    let key = unsafe { std::slice::from_raw_parts(key_host as *const u8, key_len as usize) };
    if let Some(value) = host::kv_get(key) {
        let copy_len = std::cmp::min(value.len(), out_len as usize) as u64;
        if let ProgramResult::Ok(out_host) = vm.memory_mapping.map(AccessType::Store, out_ptr, copy_len) {
            unsafe { std::ptr::copy_nonoverlapping(value.as_ptr(), out_host as *mut u8, copy_len as usize); }
            vm.program_result = ProgramResult::Ok(copy_len);
        } else {
            vm.program_result = ProgramResult::Ok(u64::MAX);
        }
    } else {
        // not found
        vm.program_result = ProgramResult::Ok(u64::MAX);
    }
}

fn builtin_kv_put() -> BuiltinFunction<TvmContext> { kv_put_vm }

fn kv_put_vm(vm: *mut EbpfVm<TvmContext>, key_ptr: u64, key_len: u64, val_ptr: u64, val_len: u64, _e: u64) {
    let vm = unsafe { &mut *vm };
    let key_host = match vm.memory_mapping.map(AccessType::Load, key_ptr, key_len) {
        ProgramResult::Ok(addr) => addr,
        _ => { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
    };
    let val_host = match vm.memory_mapping.map(AccessType::Load, val_ptr, val_len) {
        ProgramResult::Ok(addr) => addr,
        _ => { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
    };
    let key = unsafe { std::slice::from_raw_parts(key_host as *const u8, key_len as usize) };
    let val = unsafe { std::slice::from_raw_parts(val_host as *const u8, val_len as usize) };
    host::kv_put(key.to_vec(), val.to_vec());
    vm.program_result = ProgramResult::Ok(0);
}

fn builtin_kv_del() -> BuiltinFunction<TvmContext> { kv_del_vm }

fn kv_del_vm(vm: *mut EbpfVm<TvmContext>, key_ptr: u64, key_len: u64, _c: u64, _d: u64, _e: u64) {
    let vm = unsafe { &mut *vm };
    let key_host = match vm.memory_mapping.map(AccessType::Load, key_ptr, key_len) {
        ProgramResult::Ok(addr) => addr,
        _ => { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
    };
    let key = unsafe { std::slice::from_raw_parts(key_host as *const u8, key_len as usize) };
    // Сохраняем семантику: возвращаем 0 если ключ существовал, иначе u64::MAX
    let existed = host::kv_get(key).is_some();
    host::kv_del(key.to_vec());
    vm.program_result = ProgramResult::Ok(if existed { 0 } else { u64::MAX });
}

// функции разбора arg_data больше не нужны — аргументы приходят в регистрах R1..R5

fn builtin_trd20_call() -> BuiltinFunction<TvmContext> { trd20_call_vm }

fn trd20_call_vm(vm: *mut EbpfVm<TvmContext>, json_ptr: u64, json_len: u64, _c: u64, _d: u64, _e: u64) {
    let vm = unsafe { &mut *vm };
    // Load JSON payload from guest memory
    let host_ptr = match vm.memory_mapping.map(AccessType::Load, json_ptr, json_len) {
        ProgramResult::Ok(addr) => addr,
        _ => { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
    };
    let bytes = unsafe { std::slice::from_raw_parts(host_ptr as *const u8, json_len as usize) };
    let Ok(s) = std::str::from_utf8(bytes) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
    let Ok(v) = serde_json::from_str::<Value>(s) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };

    // Extract method and arguments
    let Some(method) = v.get("method").and_then(|x| x.as_str()) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };

    // Key helpers
    fn key_total() -> Vec<u8> { b"trd20/total_supply".to_vec() }
    fn key_balance(acct: &str) -> Vec<u8> { format!("trd20/balance/{}", acct).into_bytes() }
    fn key_allow(owner: &str, spender: &str) -> Vec<u8> { format!("trd20/allow/{}/{}", owner, spender).into_bytes() }

    fn read_u128_opt(b: Option<Vec<u8>>) -> Option<u128> {
        let v = b?; if v.len() < 16 { return None; }
        let mut arr = [0u8;16]; arr.copy_from_slice(&v[..16]); Some(u128::from_le_bytes(arr))
    }
    fn to_le_bytes_u128(x: u128) -> Vec<u8> { x.to_le_bytes().to_vec() }

    // Methods
    match method {
        "mint" => {
            let Some(to) = v.get("to").and_then(|x| x.as_str()) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            let Some(amount) = v.get("amount").and_then(|x| x.as_u64()).map(|x| x as u128)
                .or_else(|| v.get("amount").and_then(|x| x.as_str()).and_then(|s| s.parse::<u128>().ok())) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            if amount == 0 { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
            let supply = read_u128_opt(host::kv_get(&key_total())).unwrap_or(0);
            let new_supply = match supply.checked_add(amount) { Some(x) => x, None => { vm.program_result = ProgramResult::Ok(u64::MAX); return; } };
            host::kv_put(key_total(), to_le_bytes_u128(new_supply));
            let bal = read_u128_opt(host::kv_get(&key_balance(to))).unwrap_or(0);
            let new_bal = match bal.checked_add(amount) { Some(x) => x, None => { vm.program_result = ProgramResult::Ok(u64::MAX); return; } };
            host::kv_put(key_balance(to), to_le_bytes_u128(new_bal));
            vm.program_result = ProgramResult::Ok(0);
        }
        "burn" => {
            let Some(from) = v.get("from").and_then(|x| x.as_str()) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            let Some(amount) = v.get("amount").and_then(|x| x.as_u64()).map(|x| x as u128)
                .or_else(|| v.get("amount").and_then(|x| x.as_str()).and_then(|s| s.parse::<u128>().ok())) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            if amount == 0 { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
            let supply = read_u128_opt(host::kv_get(&key_total())).unwrap_or(0);
            if supply < amount { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
            host::kv_put(key_total(), to_le_bytes_u128(supply - amount));
            let bal = read_u128_opt(host::kv_get(&key_balance(from))).unwrap_or(0);
            if bal < amount { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
            host::kv_put(key_balance(from), to_le_bytes_u128(bal - amount));
            vm.program_result = ProgramResult::Ok(0);
        }
        "transfer" => {
            let Some(from) = v.get("from").and_then(|x| x.as_str()) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            let Some(to) = v.get("to").and_then(|x| x.as_str()) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            let Some(amount) = v.get("amount").and_then(|x| x.as_u64()).map(|x| x as u128)
                .or_else(|| v.get("amount").and_then(|x| x.as_str()).and_then(|s| s.parse::<u128>().ok())) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            if amount == 0 || from == to { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
            let from_bal = read_u128_opt(host::kv_get(&key_balance(from))).unwrap_or(0);
            if from_bal < amount { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
            let to_bal = read_u128_opt(host::kv_get(&key_balance(to))).unwrap_or(0);
            host::kv_put(key_balance(from), to_le_bytes_u128(from_bal - amount));
            host::kv_put(key_balance(to), to_le_bytes_u128(to_bal + amount));
            vm.program_result = ProgramResult::Ok(0);
        }
        "approve" => {
            let Some(owner) = v.get("owner").and_then(|x| x.as_str()) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            let Some(spender) = v.get("spender").and_then(|x| x.as_str()) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            let Some(amount) = v.get("amount").and_then(|x| x.as_u64()).map(|x| x as u128)
                .or_else(|| v.get("amount").and_then(|x| x.as_str()).and_then(|s| s.parse::<u128>().ok())) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            if owner == spender { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
            host::kv_put(key_allow(owner, spender), to_le_bytes_u128(amount));
            vm.program_result = ProgramResult::Ok(0);
        }
        "transfer_from" => {
            let Some(spender) = v.get("spender").and_then(|x| x.as_str()) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            let Some(owner) = v.get("owner").and_then(|x| x.as_str()) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            let Some(to) = v.get("to").and_then(|x| x.as_str()) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            let Some(amount) = v.get("amount").and_then(|x| x.as_u64()).map(|x| x as u128)
                .or_else(|| v.get("amount").and_then(|x| x.as_str()).and_then(|s| s.parse::<u128>().ok())) else { vm.program_result = ProgramResult::Ok(u64::MAX); return; };
            if amount == 0 || owner == to { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
            let allow = read_u128_opt(host::kv_get(&key_allow(owner, spender))).unwrap_or(0);
            if allow < amount { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
            host::kv_put(key_allow(owner, spender), to_le_bytes_u128(allow - amount));
            let owner_bal = read_u128_opt(host::kv_get(&key_balance(owner))).unwrap_or(0);
            if owner_bal < amount { vm.program_result = ProgramResult::Ok(u64::MAX); return; }
            let to_bal = read_u128_opt(host::kv_get(&key_balance(to))).unwrap_or(0);
            host::kv_put(key_balance(owner), to_le_bytes_u128(owner_bal - amount));
            host::kv_put(key_balance(to), to_le_bytes_u128(to_bal + amount));
            vm.program_result = ProgramResult::Ok(0);
        }
        _ => { vm.program_result = ProgramResult::Ok(u64::MAX); }
    }
}
