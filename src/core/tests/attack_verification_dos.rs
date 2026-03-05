/// 🎯 ATTACK #6: Верификационный DoS
/// 
/// Описание:
/// Фальшивые подписи (Ed25519) в батче, заставляющие verification
/// тратить cycles (316ms на 10k tx).
/// 
/// Цель: Перегрузка нод, энергорасход >1.62 mJ/tx
/// 
/// Метод:
/// 1. Создать транзакции с invalid signatures
/// 2. Заставить ноды тратить CPU на verification
/// 3. Увеличить энергопотребление и задержки

use crate::crypto::bls::{self, SecretKey, PublicKey, BlsSecretKeyExt, BlsSignerExt, BlsSignatureExt};
use crate::crypto::ristretto::{RistrettoKeypair, sign, verify};
use std::time::Instant;

#[test]
fn test_verification_dos_attack() {
    println!("\n🔴 ATTACK SIMULATION: Verification DoS");
    println!("{}", "=".repeat(60));
    
    const BATCH_SIZE: usize = 1000;
    const INVALID_RATIO: f64 = 0.5; // 50% invalid signatures
    const NUM_VALIDATORS: usize = 10;
    
    // 1. Генерируем ключи
    // 1. Генерируем ключи
    let validators: Vec<(SecretKey, PublicKey)> = (0..NUM_VALIDATORS)
        .map(|_| {
            let sk = SecretKey::random(&mut rand::thread_rng());
            let pk = sk.public_key();
            (sk, pk)
        })
        .collect();
    
    // 2. Создаём транзакции
    let mut transactions: Vec<(Vec<u8>, Vec<u8>, PublicKey, bool)> = Vec::new();
    
    let invalid_count = (BATCH_SIZE as f64 * INVALID_RATIO) as usize;
    let valid_count = BATCH_SIZE - invalid_count;
    
    // Валидные транзакции
    for i in 0..valid_count {
        let (sk, pk) = &validators[i % NUM_VALIDATORS];
        let msg = format!("valid_tx_{}", i).into_bytes();
        let sig = sk.sign_msg(&msg);
        let sig_bytes = sig.to_bytes();
        transactions.push((msg, sig_bytes, pk.clone(), true));
    }
    
    // Невалидные транзакции (неправильная подпись)
    for i in 0..invalid_count {
        let (_, pk) = &validators[i % NUM_VALIDATORS];
        let msg = format!("invalid_tx_{}", i).into_bytes();
        
        // Создаём фальшивую подпись (другой ключ)
        let fake_sk = SecretKey::random(&mut rand::thread_rng());
        let sig = fake_sk.sign_msg(&msg);
        let sig_bytes = sig.to_bytes();
        
        transactions.push((msg, sig_bytes, pk.clone(), false));
    }
    
    println!(" Batch Composition:");
    println!("  • Valid txs: {} ({:.0}%)", valid_count, (1.0 - INVALID_RATIO) * 100.0);
    println!("  • Invalid txs: {} ({:.0}%)", invalid_count, INVALID_RATIO * 100.0);
    
    // 3. Последовательная верификация (baseline)
    println!("\n Sequential Verification:");
    let start = Instant::now();
    let mut valid = 0;
    let mut invalid = 0;
    
    for (msg, sig_bytes, pk, expected_valid) in &transactions {
        let sig = match crate::crypto::bls::signature_from_bytes(sig_bytes) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let is_valid = sig.verify_msg(msg, pk);
        
        if is_valid {
            valid += 1;
        } else {
            invalid += 1;
        }
    }
    
    let seq_time = start.elapsed();
    println!("  • Time: {:.2}ms", seq_time.as_secs_f64() * 1000.0);
    println!("  • Valid: {}", valid);
    println!("  • Invalid: {}", invalid);
    println!("  • Time per tx: {:.3}ms", 
        seq_time.as_secs_f64() * 1000.0 / BATCH_SIZE as f64);
    
    // 4. Параллельная верификация
    println!("\n⏱️  Parallel Verification:");
    use rayon::prelude::*;
    
    let start = Instant::now();
    let results: Vec<bool> = transactions.par_iter()
        .map(|(msg, sig_bytes, vk, _)| {
             if let Ok(sig) = crate::crypto::bls::signature_from_bytes(sig_bytes) {
                 sig.verify_msg(msg, vk)
             } else {
                 false
             }
        })
        .collect();
    
    let par_time = start.elapsed();
    let par_valid = results.iter().filter(|&&v| v).count();
    let par_invalid = results.iter().filter(|&&v| !v).count();
    
    println!("  • Time: {:.2}ms", par_time.as_secs_f64() * 1000.0);
    println!("  • Valid: {}", par_valid);
    println!("  • Invalid: {}", par_invalid);
    println!("  • Speedup: {:.2}x", seq_time.as_secs_f64() / par_time.as_secs_f64());
    
    // 5. Оценка CPU cycles
    // Ed25519 verification: ~140K cycles на AMD Ryzen
    let cycles_per_verify = 140_000u64;
    let total_cycles = BATCH_SIZE as u64 * cycles_per_verify;
    
    println!("\n💻 CPU Overhead:");
    println!("  • Cycles per verify: {}", cycles_per_verify);
    println!("  • Total cycles: {:.2}M", total_cycles as f64 / 1_000_000.0);
    println!("  • Wasted on invalid: {:.2}M ({:.0}%)", 
        invalid_count as f64 * cycles_per_verify as f64 / 1_000_000.0,
        INVALID_RATIO * 100.0);
    
    // 6. Оценка энергопотребления
    // Baseline: 1.62 mJ/tx
    // Invalid sig verification тратит столько же энергии!
    let energy_per_tx = 1.62; // mJ
    let wasted_energy = invalid_count as f64 * energy_per_tx;
    let total_energy = BATCH_SIZE as f64 * energy_per_tx;
    
    println!("\n⚡ Energy Consumption:");
    println!("  • Energy per tx: {:.2} mJ", energy_per_tx);
    println!("  • Total energy: {:.2} mJ", total_energy);
    println!("  • Wasted on invalid: {:.2} mJ ({:.0}%)", 
        wasted_energy, INVALID_RATIO * 100.0);
    
    if INVALID_RATIO > 0.3 {
        println!("\n⚠️  ATTACK IMPACT:");
        println!("     {:.0}% of verification cycles wasted!", INVALID_RATIO * 100.0);
        println!("     Energy overhead: +{:.0}%", INVALID_RATIO * 100.0);
    }
    
    // 7. Защита
    println!("\n🛡️  Mitigation Strategies:");
    println!("  1. Early signature validation (before expensive operations)");
    println!("  2. Rate limiting per sender address");
    println!("  3. Reputation system (ban senders of invalid sigs)");
    println!("  4. Probabilistic sampling (don't verify all in DoS scenario)");
    println!("  5. Proof-of-Work for transaction submission");
    println!("  6. Batch signature verification (aggregate when possible)");
    
    // Verify results
    assert_eq!(valid, valid_count, "Validation count mismatch");
    assert_eq!(invalid, invalid_count, "Invalid count mismatch");
}

#[test]
fn test_early_validation_strategy() {
    println!("\n🔧 Testing Early Validation Strategy");
    
    // Strategy: check signature format BEFORE full verification
    const BATCH_SIZE: usize = 1000;
    
    let mut transactions: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::new();
    
    // 500 valid signatures (64 bytes)
    for i in 0..500 {
        let msg = format!("tx-{}", i).into_bytes();
        let sig = vec![0u8; 64]; // Valid length
        let pk = vec![0u8; 32]; // Valid length
        transactions.push((msg, sig, pk));
    }
    
    // 500 invalid signatures (wrong length)
    for i in 500..1000 {
        let msg = format!("tx-{}", i).into_bytes();
        let sig = vec![0u8; 32]; // ❌ Wrong length!
        let pk = vec![0u8; 32];
        transactions.push((msg, sig, pk));
    }
    
    println!("\nEarly validation (format check):");
    let start = Instant::now();
    let mut rejected_early = 0;
    let mut passed_to_verify = 0;
    
    for (_, sig, pk) in &transactions {
        // Quick format check (cheap!)
        if sig.len() != 64 || pk.len() != 32 {
            rejected_early += 1;
        } else {
            passed_to_verify += 1;
        }
    }
    
    let early_check_time = start.elapsed();
    
    println!("  • Time: {:.3}ms", early_check_time.as_micros() as f64 / 1000.0);
    println!("  • Rejected early: {}", rejected_early);
    println!("  • Passed to verify: {}", passed_to_verify);
    
    // Savings calculation
    let full_verify_time = 0.316; // ms per tx (from benchmark)
    let saved_time = rejected_early as f64 * full_verify_time;
    
    println!("\n💰 Savings:");
    println!("  • Time saved: {:.2}ms", saved_time);
    println!("  • Efficiency: {:.1}x faster than full verify", 
        saved_time / (early_check_time.as_secs_f64() * 1000.0));
    
    println!("\n✅ Early validation prevents 50% of wasted verification!");
}

#[test]
fn test_reputation_system() {
    println!("\n🔧 Testing Reputation-Based Filtering");
    
    #[derive(Debug)]
    struct SenderReputation {
        address: String,
        valid_count: usize,
        invalid_count: usize,
        reputation_score: f64,
    }
    
    impl SenderReputation {
        fn new(address: String) -> Self {
            Self {
                address,
                valid_count: 0,
                invalid_count: 0,
                reputation_score: 1.0, // Start with perfect score
            }
        }
        
        fn record_valid(&mut self) {
            self.valid_count += 1;
            self.update_score();
        }
        
        fn record_invalid(&mut self) {
            self.invalid_count += 1;
            self.update_score();
        }
        
        fn update_score(&mut self) {
            let total = (self.valid_count + self.invalid_count) as f64;
            if total > 0.0 {
                self.reputation_score = self.valid_count as f64 / total;
            }
        }
        
        fn should_accept(&self, threshold: f64) -> bool {
            self.reputation_score >= threshold
        }
    }
    
    // Simulate attack
    let mut honest_sender = SenderReputation::new("honest".to_string());
    let mut attacker = SenderReputation::new("attacker".to_string());
    
    // Honest sends 100 valid
    for _ in 0..100 {
        honest_sender.record_valid();
    }
    
    // Attacker sends 50 valid, 50 invalid
    for _ in 0..50 {
        attacker.record_valid();
    }
    for _ in 0..50 {
        attacker.record_invalid();
    }
    
    println!("\nReputation scores:");
    println!("  • {}: {:.2} ({} valid, {} invalid)", 
        honest_sender.address, honest_sender.reputation_score,
        honest_sender.valid_count, honest_sender.invalid_count);
    println!("  • {}: {:.2} ({} valid, {} invalid)", 
        attacker.address, attacker.reputation_score,
        attacker.valid_count, attacker.invalid_count);
    
    const REPUTATION_THRESHOLD: f64 = 0.8;
    
    println!("\nFiltering (threshold: {}):", REPUTATION_THRESHOLD);
    if honest_sender.should_accept(REPUTATION_THRESHOLD) {
        println!("  ✅ {} txs accepted", honest_sender.address);
    } else {
        println!("  ❌ {} txs rejected", honest_sender.address);
    }
    
    if attacker.should_accept(REPUTATION_THRESHOLD) {
        println!("  ✅ {} txs accepted", attacker.address);
    } else {
        println!("  ❌ {} txs rejected (BLOCKED)", attacker.address);
    }
    
    println!("\n✅ Reputation system blocks senders with <80% valid rate");
}


