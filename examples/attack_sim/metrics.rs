/// Performance metrics tracking для attack simulation

use std::time::{Duration, Instant};
use std::collections::VecDeque;
use serde::{Serialize, Deserialize};

use crate::attack_sim::types::AttackResult;

/// Metrics collector
#[derive(Debug, Clone)]
pub struct MetricsCollector {
    pub start_time: Instant,
    pub samples: VecDeque<MetricsSample>,
    pub max_samples: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSample {
    pub timestamp_ms: u64,
    pub tps: u64,
    pub latency_ms: f64,
    pub pending_tx: usize,
    pub confirmed_tx: usize,
    pub active_nodes: usize,
}

impl MetricsCollector {
    pub fn new(max_samples: usize) -> Self {
        Self {
            start_time: Instant::now(),
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
        }
    }
    
    pub fn record(&mut self, sample: MetricsSample) {
        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }
    
    pub fn average_tps(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        
        let sum: u64 = self.samples.iter().map(|s| s.tps).sum();
        sum as f64 / self.samples.len() as f64
    }
    
    pub fn p99_latency(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        
        let mut latencies: Vec<f64> = self.samples.iter().map(|s| s.latency_ms).collect();
        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let p99_index = ((latencies.len() as f64 * 0.99) as usize).min(latencies.len() - 1);
        latencies[p99_index]
    }
    
    pub fn average_latency(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        
        let sum: f64 = self.samples.iter().map(|s| s.latency_ms).sum();
        sum / self.samples.len() as f64
    }
    
    /// Export для whitepaper (CSV format)
    pub fn export_csv(&self, filename: &str) -> std::io::Result<()> {
        use std::fs::File;
        use std::io::Write;
        
        let mut file = File::create(filename)?;
        
        writeln!(file, "timestamp_ms,tps,latency_ms,pending_tx,confirmed_tx,active_nodes")?;
        
        for sample in &self.samples {
            writeln!(
                file,
                "{},{},{},{},{},{}",
                sample.timestamp_ms,
                sample.tps,
                sample.latency_ms,
                sample.pending_tx,
                sample.confirmed_tx,
                sample.active_nodes
            )?;
        }
        
        Ok(())
    }
    
    /// Export для whitepaper (JSON format)
    pub fn export_json(&self, filename: &str) -> std::io::Result<()> {
        use std::fs::File;
        use std::io::Write;
        
        let json = serde_json::to_string_pretty(&self.samples.iter().collect::<Vec<_>>())?;
        let mut file = File::create(filename)?;
        file.write_all(json.as_bytes())?;
        
        Ok(())
    }
}

/// Whitepaper metrics generator
pub struct WhitepaperMetrics {
    pub results: Vec<AttackResult>,
}

impl WhitepaperMetrics {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }
    
    pub fn add_result(&mut self, result: AttackResult) {
        self.results.push(result);
    }
    
    /// Generate Table 5.3 for whitepaper
    pub fn generate_table(&self) -> String {
        let mut table = String::from("# Table 5.3: Real-World Simulation Outcomes\n\n");
        table.push_str("| Attack Scenario | Success Rate | TPS | Uptime | Recovery | Notes |\n");
        table.push_str("|----------------|--------------|-----|--------|----------|-------|\n");
        
        for result in &self.results {
            table.push_str(&format!(
                "| {} | {:.2}% | {} | {:.1}% | {}ms | {} |\n",
                result.scenario_name,
                result.success_rate,
                result.tps,
                result.uptime,
                result.recovery_time_ms,
                format!("{} tx, {} rejected", result.metrics.total_tx, result.metrics.rejected_tx)
            ));
        }
        
        table
    }
    
    /// Generate summary statistics
    pub fn summary(&self) -> String {
        let avg_tps: u64 = self.results.iter().map(|r| r.tps).sum::<u64>() / self.results.len().max(1) as u64;
        let avg_uptime: f64 = self.results.iter().map(|r| r.uptime).sum::<f64>() / self.results.len().max(1) as f64;
        let max_recovery: u64 = self.results.iter().map(|r| r.recovery_time_ms).max().unwrap_or(0);
        let avg_success: f64 = self.results.iter().map(|r| r.success_rate).sum::<f64>() / self.results.len().max(1) as f64;
        
        format!(
            "# Summary Statistics\n\n\
            - Average TPS: {}\n\
            - Average Uptime: {:.1}%\n\
            - Max Recovery Time: {}ms\n\
            - Average Attack Success Rate: {:.2}%\n\
            - Total Scenarios: {}\n",
            avg_tps,
            avg_uptime,
            max_recovery,
            avg_success,
            self.results.len()
        )
    }
    
    /// Export all results to file
    pub fn export(&self, filename: &str) -> std::io::Result<()> {
        use std::fs::File;
        use std::io::Write;
        
        let mut file = File::create(filename)?;
        
        file.write_all(self.generate_table().as_bytes())?;
        file.write_all(b"\n\n")?;
        file.write_all(self.summary().as_bytes())?;
        
        Ok(())
    }
}

/// Compute statistics helper
pub fn compute_stats(values: &[f64]) -> (f64, f64, f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let median = sorted[sorted.len() / 2];
    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    
    (mean, median, min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attack_sim::types::{AttackMetrics, AttackType};
    
    #[test]
    fn test_metrics_collector() {
        let mut collector = MetricsCollector::new(100);
        
        for i in 0..10 {
            collector.record(MetricsSample {
                timestamp_ms: i * 1000,
                tps: 1000 + i * 100,
                latency_ms: 50.0 + i as f64 * 5.0,
                pending_tx: 100,
                confirmed_tx: i as usize * 50,
                active_nodes: 200,
            });
        }
        
        assert_eq!(collector.samples.len(), 10);
        assert!(collector.average_tps() > 1000.0);
    }
    
    #[test]
    fn test_whitepaper_metrics() {
        let mut wp_metrics = WhitepaperMetrics::new();
        
        wp_metrics.add_result(AttackResult {
            scenario_name: "Test Attack".to_string(),
            duration: Duration::from_secs(60),
            success_rate: 0.05,
            tps: 10000,
            uptime: 99.5,
            recovery_time_ms: 1500,
            energy_ratio: 1.2,
            metrics: AttackMetrics {
                total_tx: 100000,
                malicious_tx: 10000,
                rejected_tx: 9950,
                orphaned_tx: 0,
                avg_latency_ms: 45.0,
                p99_latency_ms: 120.0,
                nodes_banned: 20,
                cross_shard_leaks: 5,
            },
        });
        
        let table = wp_metrics.generate_table();
        assert!(table.contains("Test Attack"));
        assert!(table.contains("99.5%"));
    }
}
