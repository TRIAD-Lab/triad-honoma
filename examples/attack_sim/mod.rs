/// Attack Simulation Infrastructure
/// 
/// Модули:
/// - network: Network mock с byzantine nodes
/// - libp2p_integration: Real libp2p gossipsub + QUIC
/// - bls_sigs: BLS signature aggregation
/// - scenarios: 6 basic attack scenarios (wrappers)
/// - killer_scenarios: 6 KILLER scenarios (broke TOP-10 blockchains!)
/// - stress_integration: Real logic from quantum/tests
/// - metrics: Performance tracking + whitepaper export

pub mod network;
pub mod libp2p_integration;
pub mod bls_sigs;
pub mod scenarios;
pub mod killer_scenarios;
pub mod stress_integration;
pub mod metrics;
pub mod types;
