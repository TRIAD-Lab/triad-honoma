use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecEnvelope {
    pub magic: [u8; 4],      // b"TVME"
    pub version: u16,        // 1
    pub meta: ExecMeta,
    pub code: Vec<u8>,       // eBPF ELF blob
    pub code_hash: [u8; 32], // sha256(code)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecMeta {
    pub from: String,
    pub to: String,
    pub nonce: u64,
    pub fee: f64,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
}

impl ExecEnvelope {
    // Жёсткие лимиты для защиты hot-path
    const MAX_CODE_SIZE: usize = 4 * 1024 * 1024; // 4 MiB
    const MAX_TOTAL_KEYS: usize = 2048;           // reads+writes суммарно
    const MAX_KEY_LEN: usize = 128;               // байт в UTF-8 строке ключа

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.magic != *b"TVME" { return Err("bad magic"); }
        if self.version < 1 { return Err("bad version"); }
        let code_len = self.code.len();
        if code_len == 0 || code_len > Self::MAX_CODE_SIZE { return Err("code size out of range"); }
        // сверка хэша
        use sha2::{Sha256, Digest};
        let mut h = Sha256::new(); h.update(&self.code); let got = h.finalize();
        if self.code_hash != got.as_slice() { return Err("code hash mismatch"); }

        let total_keys = self.meta.reads.len() + self.meta.writes.len();
        if total_keys > Self::MAX_TOTAL_KEYS { return Err("too many keys"); }
        for k in self.meta.reads.iter().chain(self.meta.writes.iter()) {
            if k.len() == 0 || k.len() > Self::MAX_KEY_LEN { return Err("key len out of range"); }
        }
        Ok(())
    }

    pub fn is_valid(&self) -> bool { self.validate().is_ok() }
}
