use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone, Copy)]
pub struct StageTimers {
    pub prepare_us: u64,
    pub execute_us: u64,
    pub commit_us: u64,
}

impl StageTimers {
    pub fn new() -> Self { Self { prepare_us: 0, execute_us: 0, commit_us: 0 } }
    pub fn record_prepare(&mut self, d: Duration) { self.prepare_us = d.as_micros() as u64; }
    pub fn record_execute(&mut self, d: Duration) { self.execute_us = d.as_micros() as u64; }
    pub fn record_commit(&mut self, d: Duration) { self.commit_us = d.as_micros() as u64; }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HistogramStats {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}
