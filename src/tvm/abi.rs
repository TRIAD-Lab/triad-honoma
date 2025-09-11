/// Константы ABI: имена системных вызовов и символов памяти
pub struct AbiConst;

impl AbiConst {
    // Символы/секции памяти, экспортируемые для программ
    pub const INPUT_SYMBOL: &'static str = "tvm_input"; // ro input region base symbol

    // Системные вызовы (имена должны совпадать при регистрации/вызове)
    pub const SC_SHA256: &'static str = "tvm_sha256";
    pub const SC_ED25519_VERIFY: &'static str = "tvm_ed25519_verify";
    pub const SC_MATH_ADD_F64: &'static str = "tvm_math_add_f64";

    // KV syscalls
    pub const SC_KV_GET: &'static str = "tvm_kv_get";
    pub const SC_KV_PUT: &'static str = "tvm_kv_put";
    pub const SC_KV_DEL: &'static str = "tvm_kv_del";

    // TRD-20 contract syscall (JSON ABI over in-memory buffer)
    pub const SC_TRD20_CALL: &'static str = "tvm_trd20_call";
}

/// Простой объект ABI для потенциальной эволюции версий
#[derive(Debug, Clone, Copy)]
pub struct TvmAbi;
