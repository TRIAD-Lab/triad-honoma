use solana_rbpf::vm::ContextObject;

#[derive(Debug, Default, Clone)]
pub struct TvmContext {
    remaining: u64,
}

impl TvmContext {
    pub fn new(remaining: u64) -> Self {
        Self { remaining }
    }
}

impl ContextObject for TvmContext {
    fn trace(&mut self, _state: [u64; 12]) {
        // опционально: можно собирать трассу, сейчас пропускаем для производительности
    }

    fn consume(&mut self, amount: u64) {
        self.remaining = self.remaining.saturating_sub(amount);
    }

    fn get_remaining(&self) -> u64 {
        self.remaining
    }
}
