# TRIAD Minkowski Interference Consensus — Technical Specification

## 1. Формализм: Транзакция → Волна (4D Minkowski)

### 1.1 Wave Mapping
Для каждой транзакции `t`:

**Амплитуда:**
```
A_t = α · log₁₀(1 + amount_t)
```
Логарифм обеспечивает устойчивость к аномально большим суммам.

**Начальная фаза:**
```
φ_t = 2π · frac(Reduce(Blake3(block_salt || sender || receiver || txid || nonce)))
```
- `block_salt` — random beacon/VRF из предыдущего блока
- Делает фазу непредсказуемой до формирования блока

**Пространственно-временная точка:**
```
x_t = (t_t, x_t, y_t, z_t)
```
- `t_t` = slot/timestamp транзакции
- `(x_t, y_t, z_t)` = хэш → 3 координаты на 3-торе: `map frac(hash_bytes[0..24]) → [0, L)³`

**Волновой вклад в точке наблюдения (t, r):**
```
ψ_t(t, r) = A_t · e^(i(φ_t + k·Δr - ω·Δt))
```
где:
- `Δt = t - t_t`
- `Δr = ||r - r_t||`
- `ω = ck` (обычно c=1, k=1)

**Суммарное поле:**
```
Ψ(t, r) = Σ_{t ∈ batch, causal} ψ_t(t, r)
```

**Энергия:**
```
E(t, r) = |Ψ(t, r)|²
```

### 1.2 Causal Filter (Световой конус)
Используем Минковский-интервал:
```
Δs² = -c²Δt² + Δx² + Δy² + Δz²
```

Только источники с `Δs² ≤ 0` (каузально достижимые) включаются в суммирование.

### 1.3 Интегральные метрики
- **E_tot** = `(1/|Ω|) ∫_Ω E(t, r) dr` (среднее по точкам выборки)
- **Constructive Ratio** = доля точек где `E(t, r) > τ_construct`
- **Average Amplitude** = среднее `|Ψ|` по выборке

---

## 2. Pipeline: Детерминированный порядок операций

### 2.1 Hard Checks (обязательная первая стадия)
1. **Schnorr batch-verify** (Ristretto255 + Blake3)
   - Если не проходит → tx отвергается
2. **Balance/UTXO/nonce checks**
3. **DAG structural checks** (циклов нет, корректные ссылки)
4. **PoP (Proof of Possession)** — предотвращает rogue-key
5. **Rate limits & Fee checks**

### 2.2 Wave Mapping
Каждой валидной tx присваивается волна по формулам выше.

### 2.3 Causal Bucketing
- **Временной биннинг** по слотам
- **Пространственный quad/octree**
- Берём только waves в световом конусе валидатора
- Детерминированно (одинаковый block_salt → одинаковые buckets)

### 2.4 Interference Compute
Для каждого bucket:
- **Exact mode** (если size ≤ S_exact): O(n²) аналитика
- **Approximate mode** (если size > S_exact):
  - Barnes-Hut
  - Multipole expansion
  - FFT convolution
  - Sampling

Метрики:
- `constructive_ratio`
- `avg_energy`

### 2.5 Pruning Loop
Итеративное определение деструктивных волн:

```
1. Compute M(W) для батча W
2. Если M(W) ≥ R_ok (60%) → accept
3. Если M(W) ≤ R_reject (15%) → reject
4. Иначе (серый промежуток):
   a) Для каждой wave i оцениваем marginal contribution:
      ΔE_i = E(W) - E(W \ {i})
   b) Если ΔE_i < -ε → candidate_for_prune
   c) Удаляем не больше max_prune_fraction (5%)
   d) Повторяем compute (max_iter = 3)
5. Если после pruning M ≥ R_ok → accept pruned batch
6. Иначе → fallback to full deterministic validation
```

### 2.6 Finalize & Evidence
Нода сохраняет:
- Список принятых/отклонённых txs
- Merkle root батча
- Агрегированную подпись (FROST/MuSig2)
- Журнал pruning (какие txs удалены + причина)

---

## 3. Оптимизации для 27–54M волн/CPU

### 3.1 Hierarchical Methods
- **Barnes-Hut** (θ = 0.5)
  - Дальние кластеры → суммарный комплексный момент `C = Σ A·e^(iφ)`
- **Multipole Expansion**
  - `M_0 = Σ A`, `M_1 = Σ A·r·e^(iφ)`, ...
- **QuadTree/Octree** для 3D пространства

### 3.2 Sampling
- **Stratified Monte-Carlo** для constructive_ratio
- `sampling_points = 8192` (≈3% ошибка)

### 3.3 SIMD/AVX
- Pack phases/amplitudes
- Complex adds в SIMD
- Rust: `std::simd` или `packed_simd`

### 3.4 Parallelism
- `rayon` для параллелизма по buckets
- Параллельная обработка точек выборки

### 3.5 Memory
- Streaming (не держать все 54M в памяти)
- Компактное представление: `f32` для amp/phase/pos

---

## 4. Практические параметры (testnet)

```rust
batch_size = 256
S_exact = 4096              // Порог точного расчёта
sampling_points = 8192      // Для глобальной оценки
R_ok = 0.60                 // ≥60% конструктивных → accept
R_reject = 0.15             // ≤15% конструктивных → reject
max_prune_fraction = 0.05   // Не больше 5% tx убирается
max_prune_iter = 3
ε = 1e-3 * E_tot            // Marginal contribution threshold
θ = 0.5                     // Barnes-Hut accuracy
c = 1, k = 1                // Wave parameters
```

---

## 5. Алгоритм Prune (детально)

```rust
fn prune_destructive_waves(batch: &mut Batch, block_salt: &[u8]) -> PruneResult {
    let mut W = batch.waves.clone();
    
    for iter in 0..max_prune_iter {
        let M = compute_metrics(&W, block_salt);
        
        if M.constructive_ratio >= R_ok {
            return PruneResult::Accept(W);
        }
        if M.constructive_ratio <= R_reject {
            return PruneResult::Reject;
        }
        
        // Серый промежуток — пытаемся prune
        let mut candidates = vec![];
        
        for i in 0..W.len() {
            // Быстрая оценка влияния (только локальные соседи)
            let delta_E = estimate_marginal_contribution(&W, i);
            
            if delta_E < -ε {
                candidates.push((i, delta_E));
            }
        }
        
        // Сортируем по эффективности (наибольший выигрыш)
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        
        // Удаляем топ-K
        let max_remove = (W.len() as f64 * max_prune_fraction) as usize;
        let to_remove = candidates.iter().take(max_remove).map(|(i, _)| *i).collect();
        
        W.retain(|idx, _| !to_remove.contains(idx));
    }
    
    // После max_iter всё ещё uncertain → fallback
    PruneResult::Fallback
}
```

---

## 6. Защита от атак

### 6.1 Детерминизм
- Все хеши формируются от `block_salt` (VRF из prev block)
- Фазы/позиции непрогнозируемы заранее

### 6.2 PoP (Proof of Possession)
- Перед регистрацией PK требуется PoP
- Исключает rogue-key атаки

### 6.3 Economic Defenses
- **Fee**: спам дорог
- **Rate limits**: per-PK caps

### 6.4 Bounded Pruning
- Нельзя удалить >5% батча
- Атаки становятся малоэффективными

### 6.5 Audit & Dispute
- Challenge window для споров
- On-chain dispute resolution
- Slashing для злонамеренных proposers

### 6.6 Fallback = Ground Truth
- Если uncertain → строгая O(n log n) проверка UTXO
- Гарантированно находит double-spend
- Интерференция — ранний экономный фильтр

---

## 7. Тесты (обязательные)

### 7.1 Determinism Test
- Same (block_salt, tx set) → identical accept/reject на разных нодах

### 7.2 Adversarial Phase-Crafting
- Attacker генерирует txs с подобранными phases
- Измеряем cost to craft (с учётом block_salt)
- Проверяем, что нельзя манипулировать >max_prune_fraction

### 7.3 False Positive/Negative
- Inject double-spend с constructive geometry
- Ensure fallback его ловит

### 7.4 Performance Benchmarks
- Exact vs Barnes-Hut vs Sampling
- Memory & latency профилирование

### 7.5 Safety Tests
- Проверка max_prune_fraction
- Slashing tests

---

## 8. Метрики мониторинга (prod)

- % батчей: accepted immediately / pruned / fallback
- Avg number pruned per batch
- Constructive_ratio histogram
- Time spent: sig-verify / wave-mapping / interference / pruning
- Memory usage per core
- False positives/negatives (offline audits)

---

## 9. Roadmap реализации

### Phase 1: Core Wave Engine
- [ ] Implement 4D Minkowski wave representation
- [ ] Blake3-based phase/position generation
- [ ] Causal filter (light cone checking)
- [ ] Basic interference calculation (exact mode)

### Phase 2: Schnorr Crypto
- [ ] Ristretto255 + Blake3 Schnorr signatures
- [ ] Batch verification
- [ ] PoP mechanism

### Phase 3: Hard Checks Pipeline
- [ ] Balance/UTXO validation
- [ ] DAG structural checks
- [ ] Rate limiting & fee checks

### Phase 4: Optimizations
- [ ] Barnes-Hut implementation
- [ ] QuadTree/Octree bucketing
- [ ] SIMD/AVX optimizations
- [ ] Rayon parallelization
- [ ] Memory pooling & streaming

### Phase 5: Pruning Algorithm
- [ ] Marginal contribution estimation
- [ ] Iterative pruning loop
- [ ] Fallback to deterministic validation

### Phase 6: Audit & Evidence
- [ ] Merkle proof generation
- [ ] Pruning journal
- [ ] Dispute resolution mechanism
- [ ] FROST/MuSig2 aggregation

### Phase 7: Testing & Validation
- [ ] Determinism tests
- [ ] Adversarial tests
- [ ] Performance benchmarks
- [ ] Safety tests

---

## 10. Интероперабельность и адаптеры форматов

### 10.1 Blockchain Format Adapters
Создать слой конвертации для поддержки транзакций из разных сетей:

**Supported Formats:**
- Ethereum (RLP-encoded transactions)
- Solana (versioned transactions)
- BSC (Ethereum-compatible)
- Bitcoin (UTXO-based)

**Conversion Pipeline:**
```
External Format → Canonical TRIAD Tx → QuantumWave → DAG → Canonical Tx → External Format
```

**Round-Trip Testing:**
```rust
fn test_format_roundtrip<F: BlockchainFormat>(tx: F::Transaction) {
    let canonical = tx.to_canonical();
    let wave = QuantumWave::from(canonical);
    let dag_entry = DagEntry::from(wave);
    let canonical_back = dag_entry.to_canonical();
    let tx_back = F::Transaction::from_canonical(canonical_back);
    
    assert_eq!(tx, tx_back); // Integrity check
}
```

### 10.2 Cross-Chain Bridge
- **Merkle proof** генерация для cross-chain transfers
- **Light client verification** для external chains
- **Atomic swaps** через QuantumWave locking

---

## 11. Telemetry / Observability

### 11.1 Real-Time Metrics
**Core Metrics:**
- `triad_tps_current` — текущий TPS
- `triad_constructive_ratio` — доля конструктивных точек
- `triad_avg_energy` — средняя энергия поля
- `triad_shard_load` — нагрузка по шардам
- `triad_pruning_rate` — процент pruned txs
- `triad_fallback_rate` — частота fallback validation

**Performance Metrics:**
- `triad_interference_compute_ms` — время расчёта интерференции
- `triad_barnes_hut_speedup` — ускорение от Barnes-Hut
- `triad_memory_usage_bytes` — использование памяти

### 11.2 Interference Logs
```rust
struct InterferenceLog {
    batch_id: u64,
    timestamp: u64,
    constructive_ratio: f64,
    avg_energy: f64,
    pruned_txs: Vec<TxId>,
    pruning_reasons: Vec<(TxId, f64)>, // (tx, ΔE)
}
```

Логи сохраняются для:
- Аудита решений
- Визуализации паттернов
- ML-анализа атак

### 11.3 Dashboards
**Public Dashboard (для инвесторов/тестеров):**
- Live TPS graph
- Network health (constructive ratio over time)
- Shard distribution
- Top validators

**Internal Dashboard (для операторов):**
- Barnes-Hut efficiency
- Memory/CPU profiling
- Alert system для аномалий

**Tech Stack:**
- Prometheus для метрик
- Grafana для визуализации
- OpenTelemetry для tracing

---

## 12. Security Hardening

### 12.1 Threat Modeling
**Формализованная модель угроз:**

| Threat | Attack Vector | Mitigation |
|--------|---------------|------------|
| Phase Manipulation | Attacker crafts txs with specific phases | `block_salt` randomization, PoP |
| Amplitude Inflation | Large-value txs to dominate field | Logarithmic amplitude, fee constraints |
| Pruning Abuse | Malicious proposer prunes valid txs | Bounded pruning (5%), audit/dispute |
| Side-Channel (SIMD) | Timing attacks on AVX operations | Constant-time complex arithmetic |
| DOS via Interference | Spam txs to overwhelm compute | Rate limits, fee market, Barnes-Hut |
| Double-Spend with Constructive Geometry | Craft conflicting txs with aligned phases | Fallback deterministic validation |

### 12.2 Side-Channel Protection
**SIMD/AVX Security:**
- Constant-time implementations для critical paths
- Защита от cache-timing attacks
- Randomized execution order для sampling points

### 12.3 DOS Protection
- **Gas limit** для interference compute budget
- **Exponential backoff** для repeated fallback
- **IP-based rate limiting** на уровне p2p

---

## 13. Governance / Parameterization

### 13.1 Configurable Parameters
Параметры, доступные для governance-обновлений:

```rust
struct ConsensusParams {
    // Barnes-Hut
    theta: f64,              // Default: 0.5
    
    // Pruning
    R_ok: f64,               // Default: 0.60
    R_reject: f64,           // Default: 0.15
    max_prune_fraction: f64, // Default: 0.05
    max_prune_iter: usize,   // Default: 3
    epsilon: f64,            // Default: 1e-3 * E_tot
    
    // Sampling
    sampling_points: usize,  // Default: 8192
    
    // Causal filter
    c: f64,                  // Speed of light, default: 1.0
    R_local: f64,            // Local radius for ΔE estimation
    
    // Exact mode threshold
    S_exact: usize,          // Default: 4096
}
```

### 13.2 Parameter Update Mechanism
**Via TVM Smart Contract:**
```rust
fn update_consensus_params(new_params: ConsensusParams, governance_proof: Signature) {
    verify_governance_signature(governance_proof);
    validate_params_safety(new_params); // Ensure within safe ranges
    schedule_parameter_update(new_params, delay: 24_hours);
}
```

### 13.3 A/B Testing Framework
- Parallel execution с разными параметрами (readonly mode)
- Сравнение метрик (TPS, false positive rate)
- Автоматический rollback если метрики ухудшаются

---

## 14. Quantum Engine Validation / Simulation

### 14.1 Massive Load Tests
**Target:** 27–54M волн в sharded field

```rust
#[test]
fn test_54m_waves_stress() {
    let nodes = 1024;
    let waves_per_node = 54_000;
    let total_waves = nodes * waves_per_node; // 55.3M
    
    let field = ShardedQuantumField::new_with_mode(256, total_waves, true);
    
    // Generate waves
    let waves = generate_random_waves(total_waves);
    
    // Measure performance
    let start = Instant::now();
    field.add_waves_batch(waves);
    let elapsed = start.elapsed();
    
    let pattern = field.calculate_interference_pattern_approximate();
    
    println!("54M waves processed in {:?}", elapsed);
    assert!(elapsed.as_secs() < 10); // Target: <10s
}
```

### 14.2 Exact vs Approximate Comparison
**Измерение trade-off:**

```rust
struct AccuracyBenchmark {
    wave_count: usize,
    exact_time_ms: u64,
    barnes_hut_time_ms: u64,
    sampling_time_ms: u64,
    
    exact_constructive_ratio: f64,
    barnes_hut_error: f64,      // % difference from exact
    sampling_error: f64,
}
```

**Target Accuracy:**
- Barnes-Hut (θ=0.5): <1% error, 100x speedup
- Sampling (8192 points): <3% error, 1000x speedup

---

## 15. Developer Experience / SDK

### 15.1 Rust API
```rust
// High-level API
pub struct TriadClient {
    pub fn new(endpoint: &str) -> Self;
    pub async fn submit_transaction(&self, tx: Transaction) -> Result<TxReceipt>;
    pub async fn query_wave(&self, tx_id: &str) -> Result<QuantumWave>;
    pub async fn get_interference_analysis(&self, batch_id: u64) -> Result<InterferenceAnalysis>;
}

// Low-level API for developers
pub mod quantum {
    pub fn create_wave(tx: &Transaction, block_salt: &[u8]) -> QuantumWave;
    pub fn calculate_interference(waves: &[QuantumWave]) -> InterferencePattern;
    pub fn estimate_consensus_probability(pattern: &InterferencePattern) -> f64;
}
```

### 15.2 TypeScript/JavaScript Bindings
```typescript
import { TriadClient, QuantumWave } from '@triad/sdk';

const client = new TriadClient('https://rpc.triad.network');

// Submit transaction
const tx = await client.submitTransaction({
  from: '0x...',
  to: '0x...',
  value: '1000000000000000000', // 1 ETH
});

// Query quantum state
const wave = await client.getWave(tx.id);
console.log(`Amplitude: ${wave.amplitude}, Phase: ${wave.phase}`);
```

### 15.3 Python Bindings (для ML/Research)
```python
from triad import QuantumField, BarnesHut

field = QuantumField(shards=256)
field.add_waves(waves)

# Analyze with custom algorithms
analyzer = BarnesHut(theta=0.5)
pattern = analyzer.compute(field)
```

---

## 16. Audit Readiness / Compliance

### 16.1 Proof Generation
Для каждого batch генерируем полный набор доказательств:

```rust
struct BatchProof {
    // Merkle proofs
    merkle_root: Hash,
    inclusion_proofs: Vec<(TxId, MerkleProof)>,
    exclusion_proofs: Vec<(TxId, MerkleProof)>,
    
    // Pruning evidence
    pruning_journal: PruningJournal,
    pruning_signature: Signature, // Signed by proposer
    
    // PoP evidence
    validator_pops: Vec<(PublicKey, ProofOfPossession)>,
    
    // Consensus evidence
    interference_log: InterferenceLog,
    constructive_ratio: f64,
    
    // Cryptographic proofs
    schnorr_batch_signature: Signature,
    frost_aggregate: Option<FrostSignature>,
}
```

### 16.2 Repository Structure для Аудита
```
TRIAD/
├── src/
│   ├── crypto/          # Cryptographic primitives
│   ├── quantum/         # Interference engine
│   ├── consensus/       # Consensus logic
│   └── ...
├── tests/
│   ├── unit/            # Unit tests
│   ├── integration/     # Integration tests
│   ├── adversarial/     # Adversarial tests
│   └── benchmarks/      # Performance benchmarks
├── docs/
│   ├── spec.md          # Technical specification
│   ├── security.md      # Security analysis
│   └── audit_guide.md   # Guide for auditors
├── audit/
│   ├── threat_model.md
│   ├── test_coverage.md
│   └── known_issues.md
└── .github/
    └── workflows/
        ├── ci.yml       # Continuous integration
        ├── fuzzing.yml  # Continuous fuzzing
        └── audit.yml    # Automated audit checks
```

### 16.3 CI/CD для Аудита
- **Automated fuzzing** (cargo-fuzz)
- **Coverage reports** (target: >90%)
- **Static analysis** (clippy, miri)
- **Dependency audits** (cargo-audit)
- **Formal verification** (для critical paths)

---

## 17. Stress & Chaos Testing

### 17.1 Multi-Shard Reorg / Fork Simulations
```rust
#[test]
fn test_shard_reorg_cascade() {
    // Simulate reorganization affecting 10% of shards
    let affected_shards = (0..256).filter(|i| i % 10 == 0).collect();
    
    // Fork chain at height H
    let fork_a = create_fork(&chain, affected_shards);
    let fork_b = create_fork(&chain, affected_shards);
    
    // Ensure consensus converges
    assert_converges(fork_a, fork_b, max_time: 30_seconds);
}
```

### 17.2 Byzantine Node Simulations
**Моделирование 10–20% злонамеренных нод:**

```rust
enum ByzantineStrategy {
    AlwaysReject,              // Reject all valid txs
    AlwaysPruneMax,            // Prune max allowed txs
    RandomSignatures,          // Send invalid signatures
    ConflictingVotes,          // Vote differently to different peers
    WithholdBlocks,            // Don't propagate blocks
}

#[test]
fn test_byzantine_resistance() {
    let total_nodes = 100;
    let byzantine_count = 20;
    
    let network = setup_network(total_nodes, byzantine_count, ByzantineStrategy::AlwaysPruneMax);
    
    // Submit valid transactions
    let txs = generate_valid_txs(1000);
    
    // Ensure >99% of valid txs are accepted despite Byzantine nodes
    let acceptance_rate = measure_acceptance_rate(network, txs);
    assert!(acceptance_rate > 0.99);
}
```

### 17.3 MEV-Style Attacks
```rust
#[test]
fn test_front_running_resistance() {
    // Victim submits high-value swap
    let victim_tx = create_swap_tx(amount: 1000_ETH);
    
    // Attacker observes mempool and submits front-running tx
    let attacker_tx = create_front_run_tx(victim_tx, gas_price: 2x);
    
    // Verify: Minkowski consensus should treat both fairly
    // (phase randomization via block_salt prevents MEV ordering)
    let batch = vec![victim_tx, attacker_tx];
    let result = process_batch(batch);
    
    // Both should be included or both rejected based on interference,
    // NOT based on gas price manipulation
    assert_no_mev_advantage(result);
}
```

### 17.4 Fee Spam Attacks
```rust
#[test]
fn test_fee_spam_dos() {
    // Attacker floods network with low-fee txs
    let spam_txs = generate_spam_txs(count: 100_000, fee: MIN_FEE);
    
    // Legitimate user submits normal-fee tx
    let legit_tx = create_normal_tx(fee: MEDIAN_FEE);
    
    // Ensure legitimate tx is processed despite spam
    let result = submit_to_network(spam_txs, legit_tx);
    
    assert!(result.contains(legit_tx));
    assert!(result.spam_rejection_rate > 0.95);
}
```

### 17.5 Gas Manipulation Attacks
```rust
#[test]
fn test_gas_limit_manipulation() {
    // Attacker submits tx with extremely high gas limit
    // to exhaust interference compute budget
    let malicious_tx = create_tx_with_gas(gas: u64::MAX);
    
    // System should reject or throttle before interference compute
    let result = process_tx(malicious_tx);
    
    assert!(result.is_rejected_early());
    assert_no_dos_impact();
}
```

---

## 18. Ключевые принципы (обновленные)

> **Криптография = Защита + Подписи**
> **Интерференция = Консенсус**
> **Minkowski Space = Причинность**
> **Шардирование = Масштабируемость**
> **Волны = Агрегируемая, линейно масштабируемая модель**
> **Наблюдаемость = Доверие**
> **Интероперабельность = Adoption**
> **Аудит = Безопасность**

Это новый тип распределённой сети, где физика и криптография работают вместе для достижения беспрецедентной масштабируемости с сохранением безопасности, при этом обеспечивая полную прозрачность, интероперабельность и готовность к независимому аудиту.
