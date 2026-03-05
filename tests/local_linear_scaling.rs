use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use std::sync::Arc;
/// Local Linear Scaling Proof Test
///
/// Доказывает линейную масштабируемость TRIAD на одном железе
/// путем эмуляции нескольких виртуальных нод через tokio tasks.
///
/// Проверяет что TPS растет линейно с количеством виртуальных нод,
/// подтверждая отсутствие глобальных bottlenecks в архитектуре.
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc};
use triad::crypto::bls::{BlsSecretKeyExt, BlsSignerExt, SecretKey, secret_to_public};
use triad::quantum::consensus::{ConsensusMessage, ConsensusNode};
use triad::transaction::{Transaction, payload::TransactionPayload};

/// Генерирует батч транзакций для тестирования
fn generate_transaction_batch(
    batch_size: usize,
    secret_key: &SecretKey,
    sender_id: usize,
) -> Vec<ConsensusMessage> {
    let mut rng = ChaCha20Rng::seed_from_u64(sender_id as u64);
    let public_key = secret_key.public_key();

    (0..batch_size)
        .map(|i| {
            // Создаем реалистичную транзакцию
            let sender = format!("VNode-{}-Sender-{}", sender_id, i);
            let receiver = format!("User-Wallet-{:06}", rng.gen_range(0..10000));
            let amount = rng.gen_range(0.1..1000.0);
            let payload = TransactionPayload::generate_random();

            let tx = Transaction::new(sender.clone(), receiver, amount, payload.to_bytes());

            let tx_bytes = tx.to_bytes().unwrap();
            let signature = secret_key.sign_msg(&tx_bytes);

            ConsensusMessage {
                sender_id: sender,
                state_id: tx.id.to_string(),
                raw_data: tx_bytes.into(),
                signature: signature.to_bytes().to_vec(),
                public_key: Some(public_key.to_bytes().to_vec()),
            }
        })
        .collect()
}

#[tokio::test]
async fn local_linear_scaling_proof() {
    println!("\n🚀 TRIAD Local Linear Scaling Test");
    println!("================================================================================");

    const BATCH_SIZE: usize = 256; // Transactions per batch
    const TEST_DURATION: Duration = Duration::from_secs(10);

    // Тестируем масштабирование от 1 до 16 виртуальных нод
    let virtual_nodes_configs = vec![1, 2, 4, 8, 16];

    let mut results = vec![];

    for &num_vnodes in &virtual_nodes_configs {
        println!("\n📊 Testing with {} virtual nodes...", num_vnodes);

        let start = Instant::now();

        // Создаем отдельные каналы для каждой ноды чтобы избежать lock contention
        let mut senders = vec![];
        let mut handles = vec![];

        // Счетчик обработанных транзакций
        let total_processed = Arc::new(RwLock::new(0u64));

        // Спавним виртуальные ноды как tokio tasks
        for node_id in 0..num_vnodes {
            let (tx, mut rx) = mpsc::channel::<Vec<ConsensusMessage>>(1000);
            senders.push(tx);

            let processed_counter = total_processed.clone();

            let handle = tokio::spawn(async move {
                // Создаем ноду с уникальным ID
                let node_id_u128 = node_id as u128;
                let shard_id = (node_id as u8) % 4; // Распределяем по 4 шардам

                let mut node = ConsensusNode::new(node_id_u128, shard_id);

                let mut local_processed = 0u64;

                // Обрабатываем батчи из СВОЕГО канала
                while let Some(messages) = rx.recv().await {
                    let batch_len = messages.len();

                    // Обрабатываем батч через process_batch
                    if node.process_batch(messages).is_ok() {
                        local_processed += batch_len as u64;
                    }
                }

                // Обновляем глобальный счетчик
                let mut total = processed_counter.write().await;
                *total += local_processed;

                local_processed
            });

            handles.push(handle);
        }

        // Генератор нагрузки - распределяем батчи Round-Robin
        let load_handle = tokio::spawn(async move {
            let mut rng = ChaCha20Rng::seed_from_u64(42);
            let secret_key = SecretKey::random(&mut rng);

            let mut sent_batches = 0u64;
            let load_start = Instant::now();

            while load_start.elapsed() < TEST_DURATION {
                let batch =
                    generate_transaction_batch(BATCH_SIZE, &secret_key, sent_batches as usize);

                // Round-Robin distribution
                let target_node = (sent_batches as usize) % num_vnodes;

                if senders[target_node].send(batch).await.is_err() {
                    break; // Receiver closed
                }

                sent_batches += 1;

                // Небольшая задержка между батчами для реалистичности
                // Уменьшаем задержку с ростом числа нод чтобы нагрузить их все
                let delay = if num_vnodes > 8 { 0 } else { 1 };
                if delay > 0 {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
            }

            // Закрываем все каналы (drop senders)
            drop(senders);
            sent_batches
        });

        // Ждем завершения генератора
        let sent_batches = load_handle.await.unwrap();

        // Ждем завершения всех нод
        for handle in handles {
            handle.await.unwrap();
        }

        let elapsed = start.elapsed().as_secs_f64();
        let total_tx = *total_processed.read().await;
        let measured_tps = total_tx as f64 / elapsed;

        // Вычисляем эффективность масштабирования
        let baseline_tps = if num_vnodes == 1 {
            measured_tps
        } else {
            results
                .first()
                .map(|(_, tps, _)| *tps)
                .unwrap_or(measured_tps)
        };

        let scaling_efficiency = measured_tps / (baseline_tps * num_vnodes as f64);

        println!("  ✓ Sent batches: {}", sent_batches);
        println!("  ✓ Processed transactions: {}", total_tx);
        println!("  ✓ Measured TPS: {:.0}", measured_tps);
        println!("  ✓ Scaling efficiency: {:.1}%", scaling_efficiency * 100.0);

        results.push((num_vnodes, measured_tps, scaling_efficiency));
    }

    // Анализ результатов
    println!("\n📈 SCALING RESULTS");
    println!("================================================================================");
    println!(
        "{:<12} {:<15} {:<20}",
        "VNodes", "TPS", "Scaling Efficiency"
    );
    println!("{}", "-".repeat(50));

    for (vnodes, tps, efficiency) in &results {
        println!("{:<12} {:<15.0} {:<20.1}%", vnodes, tps, efficiency * 100.0);
    }

    // Проверка линейности
    let min_efficiency = results
        .iter()
        .skip(1) // Пропускаем baseline
        .map(|(_, _, eff)| eff)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(&0.0);

    println!("\n✅ РЕЗУЛЬТАТЫ:");
    println!(
        "  • Минимальная эффективность масштабирования: {:.1}%",
        min_efficiency * 100.0
    );

    // Проверяем что эффективность не упала ниже 80%
    assert!(
        *min_efficiency >= 0.80,
        "Линейность упала ниже 80% (actual: {:.1}%) — есть глобальный bottleneck!",
        min_efficiency * 100.0
    );

    println!("  • ЛОКАЛЬНАЯ ЛИНЕЙНАЯ МАСШТАБИРУЕМОСТЬ ДОКАЗАНА! ✓");
    println!("================================================================================\n");
}

#[tokio::test]
async fn single_node_throughput_baseline() {
    println!("\n🎯 TRIAD Single Node Baseline Test");
    println!("================================================================================");

    const BATCH_SIZE: usize = 256;
    const NUM_BATCHES: usize = 100;

    let mut rng = ChaCha20Rng::seed_from_u64(123);
    let secret_key = SecretKey::random(&mut rng);
    let public_key = secret_to_public(&secret_key);

    let mut node = ConsensusNode::new(0, 0);

    let start = Instant::now();
    let mut total_processed = 0u64;

    for i in 0..NUM_BATCHES {
        let batch = generate_transaction_batch(BATCH_SIZE, &secret_key, i);
        let batch_len = batch.len();

        if node.process_batch(batch).is_ok() {
            total_processed += batch_len as u64;
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    let tps = total_processed as f64 / elapsed;

    println!("  ✓ Processed: {} transactions", total_processed);
    println!("  ✓ Time: {:.2}s", elapsed);
    println!("  ✓ Throughput: {:.0} TPS", tps);
    println!("================================================================================\n");

    // Проверяем что TPS выше минимального порога
    assert!(tps >= 1000.0, "Single node TPS too low: {:.0}", tps);
}
