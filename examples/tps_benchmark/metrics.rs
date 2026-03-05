use std::sync::Arc;
/// Metrics collection for TPS benchmarking
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Node metrics
#[derive(Debug, Clone)]
pub struct NodeMetrics {
    pub node_id: usize,
    pub processed_tx: u64,
    pub rejected_tx: u64,
    pub total_latency: Duration,
    pub bls_verify_time: Duration,
    pub interference_checks: u64,
}

impl NodeMetrics {
    pub fn new(node_id: usize) -> Self {
        Self {
            node_id,
            processed_tx: 0,
            rejected_tx: 0,
            total_latency: Duration::ZERO,
            bls_verify_time: Duration::ZERO,
            interference_checks: 0,
        }
    }

    pub fn reset(&mut self) {
        self.processed_tx = 0;
        self.rejected_tx = 0;
        self.total_latency = Duration::ZERO;
        self.bls_verify_time = Duration::ZERO;
        self.interference_checks = 0;
    }

    pub fn avg_latency_ms(&self) -> f64 {
        if self.processed_tx == 0 {
            return 0.0;
        }
        self.total_latency.as_millis() as f64 / self.processed_tx as f64
    }

    pub fn success_rate(&self) -> f64 {
        let total = self.processed_tx + self.rejected_tx;
        if total == 0 {
            return 0.0;
        }
        (self.processed_tx as f64 / total as f64) * 100.0
    }
}

/// Global benchmark metrics
#[derive(Debug, Clone)]
pub struct BenchmarkMetrics {
    pub start_time: Instant,
    pub total_tx_sent: u64,
    pub total_tx_processed: u64,
    pub total_tx_rejected: u64,
    pub latencies: Vec<Duration>,
    pub tps_samples: Vec<(Instant, u64)>, // (time, tx_count)
}

impl BenchmarkMetrics {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            total_tx_sent: 0,
            total_tx_processed: 0,
            total_tx_rejected: 0,
            latencies: Vec::new(),
            tps_samples: Vec::new(),
        }
    }

    /// Calculate average TPS
    pub fn average_tps(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed == 0.0 {
            return 0.0;
        }
        self.total_tx_processed as f64 / elapsed
    }

    /// Calculate instantaneous TPS from samples
    pub fn instant_tps(&self) -> f64 {
        if self.tps_samples.len() < 2 {
            return 0.0;
        }

        let recent_samples = &self.tps_samples[self.tps_samples.len().saturating_sub(10)..];
        if recent_samples.len() < 2 {
            return 0.0;
        }

        let first = &recent_samples[0];
        let last = &recent_samples[recent_samples.len() - 1];

        let elapsed = last.0.duration_since(first.0).as_secs_f64();
        let tx_diff = last.1 - first.1;

        if elapsed == 0.0 {
            return 0.0;
        }

        tx_diff as f64 / elapsed
    }

    /// Calculate latency percentiles
    pub fn latency_percentile(&self, percentile: f64) -> Duration {
        if self.latencies.is_empty() {
            return Duration::ZERO;
        }

        let mut sorted = self.latencies.clone();
        sorted.sort();

        let index = ((sorted.len() as f64 * percentile) as usize).min(sorted.len() - 1);
        sorted[index]
    }

    /// Calculate average latency
    pub fn avg_latency(&self) -> Duration {
        if self.latencies.is_empty() {
            return Duration::ZERO;
        }

        let total: Duration = self.latencies.iter().sum();
        total / self.latencies.len() as u32
    }

    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.total_tx_processed + self.total_tx_rejected;
        if total == 0 {
            return 0.0;
        }
        (self.total_tx_processed as f64 / total as f64) * 100.0
    }

    /// Add latency sample
    pub fn add_latency(&mut self, latency: Duration) {
        self.latencies.push(latency);
    }

    /// Add TPS sample
    pub fn add_tps_sample(&mut self, tx_count: u64) {
        self.tps_samples.push((Instant::now(), tx_count));
    }

    /// Generate summary report
    pub fn summary(&self) -> String {
        format!(
            "TPS Benchmark Results\n\
             ====================\n\
             Duration: {:.2}s\n\
             Transactions Sent: {}\n\
             Transactions Processed: {}\n\
             Transactions Rejected: {}\n\
             \n\
             TPS Metrics:\n\
             - Average TPS: {:.2}\n\
             - Instantaneous TPS: {:.2}\n\
             \n\
             Latency Metrics:\n\
             - Average: {:.2}ms\n\
             - P50: {:.2}ms\n\
             - P95: {:.2}ms\n\
             - P99: {:.2}ms\n\
             \n\
             Success Rate: {:.2}%",
            self.start_time.elapsed().as_secs_f64(),
            self.total_tx_sent,
            self.total_tx_processed,
            self.total_tx_rejected,
            self.average_tps(),
            self.instant_tps(),
            self.avg_latency().as_millis(),
            self.latency_percentile(0.50).as_millis(),
            self.latency_percentile(0.95).as_millis(),
            self.latency_percentile(0.99).as_millis(),
            self.success_rate(),
        )
    }
}

impl Default for BenchmarkMetrics {
    fn default() -> Self {
        Self::new()
    }
}
