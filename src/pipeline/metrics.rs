use std::time::{Duration, Instant};
use std::collections::VecDeque;

#[derive(Debug, Clone, Default)]
pub struct Histogram {
    window: VecDeque<f64>,
    cap: usize,
}

impl Histogram {
    pub fn with_capacity(cap: usize) -> Self { Self { window: VecDeque::with_capacity(cap), cap } }
    pub fn record(&mut self, v: f64) {
        if self.window.len() == self.cap { let _ = self.window.pop_front(); }
        self.window.push_back(v);
    }
    pub fn percentile(&self, p: f64) -> f64 {
        if self.window.is_empty() { return 0.0; }
        let mut v: Vec<f64> = self.window.iter().copied().collect();
        v.sort_by(|a,b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((p.clamp(0.0, 1.0)) * ((v.len()-1) as f64)).round() as usize;
        v[idx]
    }
    pub fn p50(&self) -> f64 { self.percentile(0.50) }
    pub fn p95(&self) -> f64 { self.percentile(0.95) }
    pub fn p99(&self) -> f64 { self.percentile(0.99) }
}

#[derive(Debug)]
pub struct StageMetrics {
    pub processed: usize,
    pub accepted: usize,
    pub hist: Histogram,
    pub queue_hist: Histogram,
    last_report: Instant,
}

impl StageMetrics {
    pub fn new(hist_cap: usize) -> Self { Self { processed: 0, accepted: 0, hist: Histogram::with_capacity(hist_cap), queue_hist: Histogram::with_capacity(hist_cap), last_report: Instant::now() } }
    pub fn observe(&mut self, dur: Duration, accepted: usize, processed: usize) {
        self.hist.record(dur.as_secs_f64() * 1000.0);
        self.accepted = self.accepted.saturating_add(accepted);
        self.processed = self.processed.saturating_add(processed);
    }
    pub fn observe_wait(&mut self, wait: Duration) {
        self.queue_hist.record(wait.as_secs_f64() * 1000.0);
    }
    pub fn maybe_report(&mut self, stage: &str) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_report);
        if elapsed.as_secs_f64() >= 1.0 {
            let sec = elapsed.as_secs_f64();
            let acc_tps = (self.accepted as f64) / sec;
            let proc_tps = (self.processed as f64) / sec;
            tracing::info!(target: "pipeline", "{}: TPS(acc)={:.0} TPS(proc)={:.0} p50={:.2}ms p95={:.2}ms p99={:.2}ms | queue p50={:.2}ms p95={:.2}ms p99={:.2}ms", stage, acc_tps, proc_tps, self.hist.p50(), self.hist.p95(), self.hist.p99(), self.queue_hist.p50(), self.queue_hist.p95(), self.queue_hist.p99());
            self.accepted = 0; self.processed = 0; self.last_report = now;
        }
    }
}
