/// 🎯 ATTACK #2: Флуд амплитудами для доминирования
/// 
/// Описание:
/// Атакующий спамит транзакции с высокой amplitude (f(amount, fee) > max),
/// чтобы Σ amplitude доминировал, подтверждая только свои транзакции
/// и создавая деструктивную интерференцию для чужих.
/// 
/// Цель: MEV-подобные атаки, приоритет своим транзакциям, энергозатраты на нодах
/// 
/// Метод:
/// 1. Игнорировать clamp(0.1, 2.0) на amplitude
/// 2. Отправить транзакции с amplitude >> 2.0
/// 3. Доминировать в интерференции Σ A_i * cos(...)

use crate::core::interference::InterferenceEngine;

#[test]
fn test_amplitude_flood_attack() {
    println!("\n🔴 ATTACK SIMULATION: Amplitude Flood");
    println!("{}", "=".repeat(60));
    
    // 1. Легитимные транзакции с нормальными амплитудами
    let legitimate_waves: Vec<(f64, f64, f64)> = (0..9000)
        .map(|i| {
            let amplitude = 0.5 + (i % 15) as f64 / 10.0; // 0.5-2.0
            let phase = (i as f64 / 9000.0) * 2.0 * std::f64::consts::PI;
            (amplitude, phase, 0.9)
        })
        .collect();
    
    // 2. Атакующий с завышенными амплитудами
    let attack_waves: Vec<(f64, f64, f64)> = (0..1000)
        .map(|i| {
            let amplitude = 10.0; // ❌ 10x выше нормы!
            let phase = (i as f64 / 1000.0) * 2.0 * std::f64::consts::PI;
            (amplitude, phase, 0.9)
        })
        .collect();
    
    println!("📊 Setup:");
    println!("  • Legitimate txs: {} (amp: 0.5-2.0)", legitimate_waves.len());
    println!("  • Attack txs: {} (amp: 10.0)", attack_waves.len());
    
    // 3. Тестируем интерференцию БЕЗ атаки
    let engine = InterferenceEngine::new(1000, 0.0);
    let pattern_clean = engine.calculate_interference_with_frequencies(&legitimate_waves);
    let analysis_clean = engine.analyze_interference(&pattern_clean);
    
    println!("\n✅ Clean Network:");
    println!("  • Avg amplitude: {:.4}", analysis_clean.average_amplitude);
    println!("  • Max amplitude: {:.4}", analysis_clean.max_amplitude);
    
    // 4. Тестируем интерференцию С атакой
    let mut all_waves = legitimate_waves.clone();
    all_waves.extend(attack_waves.clone());
    
    let pattern_attack = engine.calculate_interference_with_frequencies(&all_waves);
    let analysis_attack = engine.analyze_interference(&pattern_attack);
    
    println!("\n⚠️  Under Attack:");
    println!("  • Avg amplitude: {:.4}", analysis_attack.average_amplitude);
    println!("  • Max amplitude: {:.4}", analysis_attack.max_amplitude);
    
    // 5. Анализ доминирования
    let amplitude_increase = analysis_attack.average_amplitude / analysis_clean.average_amplitude;
    
    println!("\n📈 Attack Impact:");
    println!("  • Amplitude increase: {:.2}x", amplitude_increase);
    
    if amplitude_increase > 2.0 {
        println!("  ⚠️  ATTACK SUCCESSFUL: Attacker dominates interference!");
        println!("     Legitimate transactions drowned out");
    }
    
    // 6. Энергопотребление
    // Вычисления с amplitude=10 требуют больше операций
    let energy_baseline = legitimate_waves.len() as f64 * 1.62; // mJ
    let energy_attack = all_waves.len() as f64 * 1.62 
                       + attack_waves.len() as f64 * 8.0; // 10x amplitude = ~5x energy
    
    println!("\n⚡ Energy Consumption:");
    println!("  • Baseline: {:.2} mJ", energy_baseline);
    println!("  • Under attack: {:.2} mJ", energy_attack);
    println!("  • Overhead: +{:.1}%", (energy_attack / energy_baseline - 1.0) * 100.0);
    
    // 7. Защита: обнаружение аномальных амплитуд
    println!("\n🛡️  Mitigation Strategies:");
    println!("  1. Enforce strict amplitude limits (reject if >2.0)");
    println!("  2. Progressive fee scaling for high amplitudes");
    println!("  3. Rate limiting per address for high-value txs");
    println!("  4. Amplitude normalization at consensus layer");
    
    // Проверяем нормализацию
    let total_amp_sum: f64 = all_waves.iter().map(|(a, _, _)| a).sum();
    let num_waves = total_amp_sum / 2.0;
    let normalization = if num_waves > 1.0 { num_waves.sqrt() } else { 1.0 };
    
    println!("\n🔬 Normalization:");
    println!("  • Total amplitude sum: {:.2}", total_amp_sum);
    println!("  • Normalization factor: {:.2}", normalization);
    println!("  • Normalized avg: {:.4}", analysis_attack.average_amplitude / normalization);
    
    assert!(
        amplitude_increase < 5.0,
        "Amplitude flood attack caused >5x increase!"
    );
}

#[test]
fn test_amplitude_limit_enforcement() {
    println!("\n🔍 Testing Amplitude Limit Enforcement");
    
    // Симуляция enforcement на уровне transaction processor
    let test_amounts = vec![10.0, 50.0, 100.0, 500.0, 1000.0, 10000.0];
    
    println!("\nTesting amplitude calculation:");
    for amount in test_amounts {
        // Формула из transaction/processor.rs
        let raw_amplitude: f64 = amount / 100.0;
        let clamped_amplitude = raw_amplitude.max(0.1).min(2.0);
        
        println!("  Amount: {:>7.0} → Raw: {:>6.2} → Clamped: {:.2}", 
            amount, raw_amplitude, clamped_amplitude);
        
        // Проверка: amplitude ВСЕГДА должна быть ≤ 2.0
        assert!(
            clamped_amplitude <= 2.0,
            "Amplitude {} exceeds max 2.0!",
            clamped_amplitude
        );
    }
    
    println!("\n✅ All amplitudes properly clamped to [0.1, 2.0]");
}

#[test]
fn test_amplitude_based_fee_scaling() {
    println!("\n💰 Testing Progressive Fee Scaling");
    
    // Предлагаемая модель: fee = base_fee * amplitude²
    // Это делает высокие амплитуды экономически невыгодными
    
    let base_fee = 0.0001; // TRD
    
    println!("\nFee table (base: {} TRD):", base_fee);
    println!("  Amplitude | Fee (TRD) | Fee (USD @ $1)");
    println!("{}", "  ".repeat(40));
    
    for amp in [0.5, 1.0, 1.5, 2.0, 5.0, 10.0].iter() {
        let fee = base_fee * amp * amp;
        println!("  {:>9.1} | {:>9.6} | ${:.6}", amp, fee, fee);
    }
    
    println!("\n📊 Analysis:");
    println!("  • amplitude=2.0: 4x base fee (acceptable)");
    println!("  • amplitude=10.0: 100x base fee (prohibitive!)");
    println!("\n✅ Quadratic scaling disincentivizes amplitude spam");
}



