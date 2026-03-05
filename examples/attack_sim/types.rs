/// Common types for attack simulation

use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};

/// Network node configuration
#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub id: usize,
    pub stake_percent: f64,  // % of total stake
    pub is_byzantine: bool,
    pub latency_ms: u64,
    pub packet_loss_rate: f64,
}

/// Transaction for simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimTransaction {
    pub id: String,
    pub from: String,
    pub to: String,
    pub amount: f64,
    pub nonce: u64,
    pub timestamp: u64,
    pub amplitude: f64,
    pub phase: f64,
    pub shard_id: String,
    pub ttl_ms: u64,
    pub signature: Vec<u8>,
}

/// Attack scenario result
#[derive(Debug, Clone)]
pub struct AttackResult {
    pub scenario_name: String,
    pub duration: Duration,
    pub success_rate: f64,  // % of attacks that succeeded
    pub tps: u64,
    pub uptime: f64,  // %
    pub recovery_time_ms: u64,
    pub energy_ratio: f64,  // vs baseline
    pub metrics: AttackMetrics,
}

/// Detailed attack metrics
#[derive(Debug, Clone)]
pub struct AttackMetrics {
    pub total_tx: usize,
    pub malicious_tx: usize,
    pub rejected_tx: usize,
    pub orphaned_tx: usize,
    pub avg_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub nodes_banned: usize,
    pub cross_shard_leaks: usize,
}

/// Network performance snapshot
#[derive(Debug, Clone)]
pub struct NetworkSnapshot {
    pub timestamp: Instant,
    pub active_nodes: usize,
    pub pending_tx: usize,
    pub confirmed_tx: usize,
    pub avg_amplitude: f64,
    pub interference_score: f64,
}

/// Combined attack configuration
#[derive(Debug, Clone)]
pub struct CombinedAttackConfig {
    pub num_nodes: usize,
    pub byzantine_ratio: f64,
    pub packet_loss: f64,
    pub attack_types: Vec<AttackType>,
    pub duration_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackType {
    PhaseManipulation,
    AmplitudeFlood,
    TTLEvasion,
    ShardFlood,
    BatchRejection,
    VerifyDoS,
}

impl AttackType {
    pub fn name(&self) -> &'static str {
        match self {
            AttackType::PhaseManipulation => "Phase Manipulation",
            AttackType::AmplitudeFlood => "Amplitude Flood",
            AttackType::TTLEvasion => "TTL Evasion",
            AttackType::ShardFlood => "Shard Flood",
            AttackType::BatchRejection => "Batch Rejection",
            AttackType::VerifyDoS => "Verify DoS",
        }
    }
}
