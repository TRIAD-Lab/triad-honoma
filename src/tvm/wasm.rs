use wasmtime::{Engine, Config as WasmConfig, Module, Store, Linker, Caller, Memory, TypedFunc};
use crate::tvm::{TvmConfig, TvmExecutionResult};
use crate::tvm::state::StateView;
use crate::tvm::host;
use crate::tvm::with_program_context;

pub struct WasmExecutor {
    cfg: TvmConfig,
    engine: Engine,
}

impl WasmExecutor {
    pub fn new(cfg: TvmConfig) -> Self {
        let mut wcfg = WasmConfig::new();
        // Enable fuel consumption for deterministic metering
        wcfg.consume_fuel(true);
        // Multiple memories are fine; do not enable async features
        wcfg.wasm_multi_memory(true);
        let engine = Engine::new(&wcfg).expect("failed to create wasmtime engine");
        Self { cfg, engine }
    }

    /// Executes a WASM contract with simple ABI:
    /// - Exports memory named "memory"
    /// - Exports function `tvm_entry(ptr: i32, len: i32) -> i64`
    /// Host provides syscalls via `env` module: tvm_kv_get/put/del, tvm_sha256, tvm_ed25519_verify,
    /// as well as scratch helpers tvm_scratch_len/tvm_scratch_read/tvm_scratch_clear
    pub fn execute(&self, code_wasm: &[u8], input: &[u8], state: &StateView, network_amp: f64) -> (TvmExecutionResult, crate::tvm::state::WriteBatch) {
        let mut result = TvmExecutionResult { ok: false, return_code: 0, logs: Vec::new() };
        let snapshot = state.snapshot();
        // Выполняем весь жизненный цикл модуля внутри program context, чтобы собрать WriteBatch из syscalls
        let (rc, wb) = with_program_context(snapshot, || {
            let module = match Module::new(&self.engine, code_wasm) {
                Ok(m) => m,
                Err(e) => {
                    result.ok = false; result.return_code = -1; result.logs.push(format!("WASM module error: {}", e));
                    return i64::MIN;
                }
            };
            let mut store = Store::new(&self.engine, ());
            // Add fuel budget based on TVM config instructions
            let _ = store.add_fuel(self.cfg.max_instructions);

            let mut linker = Linker::new(&self.engine);

        // Helper to read guest memory safely
        fn read_mem(caller: &mut Caller<'_, ()>, ptr: i32, len: i32) -> Option<Vec<u8>> {
            let memory = caller.get_export("memory").and_then(|e| e.into_memory())?;
            let data = memory.data(&*caller);
            let start = ptr as usize;
            let end = start.checked_add(len as usize)?;
            if end > data.len() { return None; }
            Some(data[start..end].to_vec())
        }
        // Helper to write guest memory safely
        fn write_mem(caller: &mut Caller<'_, ()>, ptr: i32, src: &[u8]) -> bool {
            if let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) {
                let start = ptr as usize;
                let end = start + src.len();
                if end > memory.data_size(&*caller) { return false; }
                // wasmtime 14 Memory::write API
                if memory.write(caller, start, src).is_ok() { return true; }
            }
            false
        }

            // Link syscalls (KV)
            linker.func_wrap("env", "tvm_kv_get", move |mut caller: Caller<'_, ()>, key_ptr: i32, key_len: i32| -> i32 {
            if let Some(key) = read_mem(&mut caller, key_ptr, key_len) {
                if let Some(val) = host::kv_get(&key) { host::scratch_set(val); return 0; } else { return -1; }
            }
            -1
            }).expect("link tvm_kv_get");
            linker.func_wrap("env", "tvm_kv_put", move |mut caller: Caller<'_, ()>, key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32| -> i32 {
            if let (Some(k), Some(v)) = (read_mem(&mut caller, key_ptr, key_len), read_mem(&mut caller, val_ptr, val_len)) {
                host::kv_put(k, v);
                return 0;
            }
            -1
            }).expect("link tvm_kv_put");
            linker.func_wrap("env", "tvm_kv_del", move |mut caller: Caller<'_, ()>, key_ptr: i32, key_len: i32| -> i32 {
            if let Some(k) = read_mem(&mut caller, key_ptr, key_len) { host::kv_del(k); return 0; }
            -1
            }).expect("link tvm_kv_del");

        // Scratch helpers
            linker.func_wrap("env", "tvm_scratch_len", move || -> i32 { host::scratch_len() as i32 }).expect("link tvm_scratch_len");
            linker.func_wrap("env", "tvm_scratch_read", move |mut caller: Caller<'_, ()>, dst_ptr: i32, max_len: i32| -> i32 {
            let n = max_len.max(0) as usize;
            let mut buf = vec![0u8; n];
            let read = host::scratch_read(&mut buf, 0);
            if read == 0 { return 0; }
            if write_mem(&mut caller, dst_ptr, &buf[..read]) { read as i32 } else { -1 }
            }).expect("link tvm_scratch_read");
            linker.func_wrap("env", "tvm_scratch_clear", move || { host::scratch_set(Vec::new()); }).expect("link tvm_scratch_clear");

        // Crypto helpers
            linker.func_wrap("env", "tvm_sha256", move |mut caller: Caller<'_, ()>, ptr: i32, len: i32| -> i64 {
            if let Some(bytes) = read_mem(&mut caller, ptr, len) {
                use sha2::{Sha256, Digest};
                let mut hasher = Sha256::new();
                hasher.update(bytes);
                let digest = hasher.finalize();
                let mut eight = [0u8; 8];
                eight.copy_from_slice(&digest[..8]);
                return i64::from_le_bytes(eight);
            }
            -1
            }).expect("link tvm_sha256");

            linker.func_wrap("env", "tvm_ed25519_verify", move |mut caller: Caller<'_, ()>, msg_ptr: i32, msg_len: i32, sig_ptr: i32, pk_ptr: i32| -> i32 {
            if let (Some(msg), Some(sig), Some(pk)) = (read_mem(&mut caller, msg_ptr, msg_len), read_mem(&mut caller, sig_ptr, 64), read_mem(&mut caller, pk_ptr, 32)) {
                if sig.len() != 64 || pk.len() != 32 { return 0; }
                use ed25519_dalek::{Signature, VerifyingKey, Verifier};
                let sig = match Signature::from_slice(&sig) { Ok(s) => s, Err(_) => return 0 };
                let mut pk_arr = [0u8; 32]; pk_arr.copy_from_slice(&pk);
                let vk = match VerifyingKey::from_bytes(&pk_arr) { Ok(v) => v, Err(_) => return 0 };
                return if vk.verify(&msg, &sig).is_ok() { 1 } else { 0 };
            }
            0
            }).expect("link tvm_ed25519_verify");

            // Instantiate
            let instance = match linker.instantiate(&mut store, &module) {
                Ok(i) => i,
                Err(e) => { result.ok = false; result.return_code = -1; result.logs.push(format!("WASM instantiate error: {}", e)); return i64::MIN; }
            };
            // Input write into memory at offset 0
            let memory = match instance.get_memory(&mut store, "memory") {
                Some(m) => m,
                None => { result.ok = false; result.return_code = -1; result.logs.push("No exported memory".to_string()); return i64::MIN; }
            };
            if memory.data_size(&store) < input.len() {
                // Try to grow memory to fit input
                let pages_needed = ((input.len() - memory.data_size(&store) + 65535) / 65536) as u64;
                let _ = memory.grow(&mut store, pages_needed);
            }
            if memory.write(&mut store, 0, input).is_err() {
                result.ok = false; result.return_code = -1; result.logs.push("Failed to write input".into());
                return i64::MIN;
            }

            // Call entry
            let func: TypedFunc<(i32, i32), i64> = match instance.get_typed_func(&mut store, "tvm_entry") {
                Ok(f) => f,
                Err(e) => { result.ok = false; result.return_code = -1; result.logs.push(format!("Missing tvm_entry: {}", e)); return i64::MIN; }
            };
            let rc = match func.call(&mut store, (0i32, input.len() as i32)) {
                Ok(v) => v,
                Err(e) => { result.ok = false; result.return_code = -1; result.logs.push(format!("tvm_entry error: {}", e)); i64::MIN }
            };
            rc
        });
        let mut write_batch = wb;

        if rc != i64::MIN {
            result.ok = true; result.return_code = rc as i64;
            // Resonance check
            let amp = f64::from_bits(result.return_code as u64);
            if !crate::tvm::security::resonate_contract(amp, network_amp) {
                result.ok = false;
                result.logs.push(format!("Resonance check failed: amp={} vs network_amp={}", amp, network_amp));
                write_batch = crate::tvm::state::WriteBatch::new();
            }
        }

        (result, write_batch)
    }
}
