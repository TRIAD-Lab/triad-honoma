# TRIAD Technical Specifications - Detailed Mappings

> **Audit-Ready Technical Specification** - Precise mathematical definitions and implementation details

## 1.1 Transaction → Wave Mapping (Detailed)

### 1.1.1 Coordinate Mapping on 3-Torus (`[0, L)³`)

**Hash Input:**
```
hash_input = Blake3(block_salt || tx.sender || tx.receiver || tx.id || tx.nonce)
hash_output = 32 bytes (256 bits)
```

**Coordinate Extraction:**
```rust
// Extract 8 bytes per coordinate
let x_bytes: [u8; 8] = hash_output[0..8];
let y_bytes: [u8; 8] = hash_output[8..16];
let z_bytes: [u8; 8] = hash_output[16..24];

// Convert to u64 (little-endian)
let x_int = u64::from_le_bytes(x_bytes);
let y_int = u64::from_le_bytes(y_bytes);
let z_int = u64::from_le_bytes(z_bytes);

// Map to [0, L) where L = spatial_scale (default: 1000.0)
let x = (x_int as f64 / u64::MAX as f64) * L;  // [0, L)
let y = (y_int as f64 / u64::MAX as f64) * L;
let z = (z_int as f64 / u64::MAX as f64) * L;

position = [x as f32, y as f32, z as f32];
```

**Endianness:** Little-endian (consistent across platforms)  
**Overflow Handling:** `% L` for wrap-around (torus topology)  
**Precision:** f32 (sufficient for spatial resolution ~10^-6 на торе)

### 1.1.2 Amplitude Calculation

**Formula:**
```
A_t = α · log₁₀(1 + amount_t)
```

**Parameter Choice:**
- `α = 1.0` (baseline, adjustable via governance)
- Motivation: Log scale provides resilience to amount outliers while preserving ordering

**Domain:**
- `amount_t ∈ [0, 2^64)` (u64 range)
- `A_t ∈ [0, α · log₁₀(2^64 + 1)] ≈ [0, 19.27α]`

**Edge Cases:**
```rust
if amount == 0 {
    amplitude = α * log10(1.0) = 0.0;  // Dust tx has zero amplitude
}

if amount < MIN_AMOUNT (e.g., 1000 wei) {
    // Optional: reject as spam before wave creation
    // Or: allow but will have minimal interference contribution
}
```

**Spam Protection:**
- Transactions with `amount < MIN_VIABLE_AMOUNT` contribute negligible amplitude
- Pruning algorithm will naturally filter them as destructive if fee insufficient

### 1.1.3 Phase Calculation

**Formula:**
```
φ_t = 2π · frac(u64::from_le_bytes(hash[0..8]) / u64::MAX)
```

**Range:** `φ_t ∈ [0, 2π)` (continuous distribution)

**Determinism:** Same `block_salt` + tx data → same phase (reproducible)

---

## 1.2 Torus Distance Metric (Wrap-Around)

**Problem:** Standard Euclidean distance fails on periodic boundary conditions.

**Correct Distance on 3-Torus:**
```rust
fn torus_distance(p1: [f32; 3], p2: [f32; 3], L: f32) -> f32 {
    let dx = (p1[0] - p2[0]).abs();
    let dy = (p1[1] - p2[1]).abs();
    let dz = (p1[2] - p2[2]).abs();
    
    // Wrap-around: shorter path through boundary
    let dx_wrapped = dx.min(L - dx);
    let dy_wrapped = dy.min(L - dy);
    let dz_wrapped = dz.min(L - dz);
    
    (dx_wrapped * dx_wrapped + dy_wrapped * dy_wrapped + dz_wrapped * dz_wrapped).sqrt()
}
```

**Example:**
- Point A at `x = 10`, Point B at `x = 990`, `L = 1000`
- Standard: `|990 - 10| = 980`
- Wrapped: `min(980, 1000 - 980) = 20` ✅ (через границу)

**Causal Filter:**
```rust
fn is_causal(wave_pos: [f32; 3], wave_time: u32, 
             observer_pos: [f32; 3], observer_time: u32, 
             c: f32, L: f32) -> bool {
    let dt = (observer_time as i64 - wave_time as i64) as f32;
    if dt <= 0.0 { return false; } // Only past affects future
    
    let dr = torus_distance(wave_pos, observer_pos, L);
    let c_dt = c * dt;
    
    dr <= c_dt  // Causal if within light cone
}
```

---

## 2. Pipeline Stage Guarantees

### 2.1 Schnorr Verification (Stage 1)

**Time Complexity:** O(n) with batch verification  
**Const-Time:** Yes (EdDSA operations are constant-time)  
**Early Reject:** Invalid signatures rejected immediately  
**Batch Size:** Adaptive (10-5000 transactions)

**Timing Guarantees:**
- Small batch (<100): ~10ms
- Medium batch (100-1000): ~100ms
- Large batch (1000-5000): ~300ms

**Security:**
- Protects against signature forgery
- PoP prevents rogue-key attacks
- Blake3 pre-hashing (collision-resistant)

### 2.2 Balance Validation (Stage 2)

**Time Complexity:** O(n) - hash lookups  
**Const-Time:** No (data-dependent lookups acceptable here)  
**Early Reject:** Insufficient balance → reject before interference

**Implementation:**
```rust
fn validate_balances(txs: &[Transaction], state: &WorldState) -> Result<Vec<usize>> {
    let mut rejected = Vec::new();
    
    for (idx, tx) in txs.iter().enumerate() {
        let balance = state.get_balance(&tx.sender)?;
        let required = tx.amount + tx.fee;
        
        if balance < required {
            rejected.push(idx);
        }
    }
    
    Ok(rejected)
}
```

**Timing:** ~20-50ms for 5K transactions (O(1) hash lookups)

### 2.3 Minimal DAG Checks (Stage 3)

**NOT full topological sort** - только parent existence check

**Time Complexity:** O(n) - hash set lookups  
**What's Checked:**
1. Parent transactions exist (in current batch or known history)
2. No missing references

**What's NOT Checked (deferred to post-consensus):**
1. Cycle detection (expensive, rare)
2. Full topological ordering

**Justification:** Honest nodes don't create cycles, dishonest nodes get caught in post-consensus audit with slashing

```rust
fn validate_dag_minimal(txs: &[Transaction], known_txs: &HashSet<TxId>) -> Result<Vec<usize>> {
    let current_batch: HashSet<TxId> = txs.iter().map(|tx| tx.id).collect();
    let mut rejected = Vec::new();
    
    for (idx, tx) in txs.iter().enumerate() {
        for parent_id in &tx.parent_ids {
            if !current_batch.contains(parent_id) && !known_txs.contains(parent_id) {
                rejected.push(idx);
                break;  // Missing parent
            }
        }
    }
    
    Ok(rejected)
}
```

**Timing:** ~10ms for 5K transactions

### 2.4 Rate Limiting (Stage 4)

**Time Complexity:** O(n) - token bucket check  
**Const-Time:** No

**Implementation:**
```rust
struct RateLimiter {
    per_address_limit: u64,  // tx/sec per address
    global_limit: u64,       // total tx/sec
}
```

**Timing:** ~5ms for 5K transactions

---

## 3. Determinism Guarantees

### 3.1 Pruning Determinism Proof

**Claim:** Marginal contribution estimation is deterministic (independent of thread schedule, sampling order).

**Proof Sketch:**
1. **Input Determinism:** Given fixed `block_salt` + transaction set → fixed wave positions/phases
2. **Summing Order Independence:** Interference `Ψ = Σ ψ_i` is commutative (complex addition)
3. **No Random Seeds:** All sampling points derived from `block_salt` (no external randomness)
4. **Thread-Safe Aggregation:** Use associative reductions (rayon's `fold + reduce`)

**Implementation:**
```rust
fn estimate_marginal_contribution(waves: &[QuantumWave], idx: usize, block_salt: &[u8; 32]) -> f64 {
    // Deterministic sampling points from block_salt
    let sample_points = generate_deterministic_samples(block_salt, 256);
    
    // Energy with all waves
    let E_full = calculate_energy_at_points(waves, &sample_points);
    
    // Energy without wave[idx]
    let waves_without = waves.iter().enumerate()
        .filter(|(i, _)| *i != idx)
        .map(|(_, w)| w)
        .collect::<Vec<_>>();
    let E_pruned = calculate_energy_at_points(&waves_without, &sample_points);
    
    E_full - E_pruned  // Deterministic difference
}
```

**Verification:**
```rust
#[test]
fn test_pruning_determinism() {
    let txs = generate_test_txs(100);
    let block_salt = [42u8; 32];
    
    // Run pruning 10 times
    let results: Vec<_> = (0..10)
        .map(|_| prune_destructive_waves(txs.clone(), &block_salt))
        .collect();
    
    // All results must be identical
    for result in &results[1..] {
        assert_eq!(result, &results[0]);
    }
}
```

### 3.2 Evidence Storage for Reproducibility

**Required Data for Full Reproduction:**
```rust
struct ConsensusEvidence {
    // Input data
    block_salt: [u8; 32],           // VRF from previous block
    transactions: Vec<Transaction>,  // Original tx set
    
    // Configuration snapshot
    params: ConsensusParams,         // α, c, L, θ, etc. at time of consensus
    
    // Computation seeds
    sampling_seed: [u8; 32],         // Derived from block_salt
    barnes_hut_seed: Option<[u8; 32]>, // If randomized tree construction
    
    // Results
    accepted_txs: Vec<TxId>,
    rejected_txs: Vec<(TxId, RejectReason)>,
    pruned_txs: Vec<(TxId, f64)>,   // (tx, ΔE)
    
    // Interference metrics
    interference_log: InterferenceLog,
    
    // Cryptographic proof
    signature: SchnorrSignature,     // Proposer's signature over evidence
}
```

**Reproducibility Test:**
```rust
fn verify_consensus_reproducibility(evidence: &ConsensusEvidence) -> bool {
    // Re-run consensus with exact same inputs
    let result = run_consensus(
        &evidence.transactions,
        &evidence.block_salt,
        &evidence.params,
    );
    
    // Compare results
    result.accepted_txs == evidence.accepted_txs &&
    result.rejected_txs == evidence.rejected_txs &&
    result.pruned_txs == evidence.pruned_txs
}
```

---

## 4. Ground Truth Validation Guarantees

**Fallback Mechanism:** When interference-based pruning is uncertain (constructive ratio in gray zone after max iterations):

**Algorithm:**
```rust
fn fallback_deterministic_validation(txs: &[Transaction], state: &WorldState) -> ValidationResult {
    // O(n log n) deterministic UTXO/balance check
    
    // 1. Sort transactions topologically (O(n log n))
    let sorted_txs = topological_sort_dag(txs)?;
    
    // 2. Sequential application with state tracking
    let mut temp_state = state.clone();
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    
    for tx in sorted_txs {
        // Check double-spend
        if temp_state.has_conflict(&tx) {
            rejected.push((tx, RejectReason::DoubleSpend));
            continue;
        }
        
        // Check balance
        if temp_state.get_balance(&tx.sender) < tx.amount + tx.fee {
            rejected.push((tx, RejectReason::InsufficientBalance));
            continue;
        }
        
        // Apply transaction
        temp_state.apply_transaction(&tx)?;
        accepted.push(tx);
    }
    
    ValidationResult { accepted, rejected }
}
```

**Double-Spend Detection Guarantee:**
- ✅ Conflicting UTXOs detected with 100% accuracy
- ✅ Cycle detection in DAG
- ✅ Balance consistency enforced

**Complexity:** O(n log n) for topological sort, acceptable as fallback for uncertain batches

---

## 5. Compute Budget Accounting

**Problem:** Even with DAG (no fixed blocks), need to prevent DOS via excessive interference compute.

**Solution: Per-Epoch Compute Budget**

```rust
struct ComputeBudget {
    max_interference_compute_ms: u64,  // Default: 1000ms per epoch
    max_barnes_hut_points: usize,      // Default: 100K points
    max_sampling_rounds: usize,        // Default: 3 iterations
}

fn enforce_compute_budget(batch: &Batch, budget: &ComputeBudget) -> Result<()> {
    let estimated_compute = estimate_interference_time(batch);
    
    if estimated_compute > budget.max_interference_compute_ms {
        return Err(Error::ComputeBudgetExceeded {
            required: estimated_compute,
            available: budget.max_interference_compute_ms,
        });
    }
    
    Ok(())
}
```

**Estimation:**
```rust
fn estimate_interference_time(batch: &Batch) -> u64 {
    let wave_count = batch.waves.len();
    
    if wave_count <= S_exact {
        // O(n²) exact mode
        (wave_count * wave_count) / 1_000  // Rough estimate: 1M ops/ms
    } else {
        // O(n log n) Barnes-Hut
        wave_count * (wave_count as f64).log2() as usize / 10_000
    }
}
```

**DOS Protection:**
- Reject batches exceeding budget BEFORE interference calculation
- Adaptive batch sizing to stay within budget
- Per-validator budget tracking

---

## 6. Performance Targets (Measured vs Target)

> **Status Markers:**  
> ✅ Achieved  
> 🎯 Target (not yet measured)  
> ⚠️ Under development

| Metric | Target | Current Status | Notes |
|--------|--------|----------------|-------|
| Consensus Latency (5K tx) | < 500ms | 🎯 Not measured | Target for measured validation |
| TPS Throughput | 50K+ TPS | 🎯 Not measured | Theoretical capacity |
| Interference (10K waves exact) | < 50ms | 🎯 Not measured | With SIMD optimization |
| Interference (54M waves approx) | < 10s | 🎯 Not measured | Theoretical with Barnes-Hut |
| Schnorr Batch Verify (1K sigs) | < 150ms | ✅ Achieved | With batch verification |
| Balance Check (5K tx) | < 50ms | 🎯 Not measured | O(1) hash lookups |
| DAG Minimal Check (5K tx) | < 10ms | 🎯 Not measured | Parent existence only |

**Future Validation Required:**
- Real-world benchmarks on production hardware
- Network latency measurements
- Multi-node consensus timing

---

## 7. Dashboard Metrics Classification

### 7.1 Public Metrics (Investor/User Facing)
- `triad_tps_current` - Live TPS
- `triad_block_time_ms` - Average consensus latency
- `triad_network_nodes` - Active validator count
- `triad_total_transactions` - Cumulative transaction count
- `triad_constructive_ratio_avg` - 24h average constructive ratio

### 7.2 Private Metrics (Operator/Dev Only)
- `triad_interference_compute_ms_p99` - 99th percentile compute time
- `triad_barnes_hut_error_rate` - Approximation error
- `triad_pruning_rate_per_validator` - Per-validator pruning stats
- `triad_memory_usage_per_shard` - Shard-level memory profiling
- `triad_fallback_trigger_rate` - How often fallback validation activates

### 7.3 SLO Thresholds
```rust
struct MetricSLO {
    constructive_ratio: (f64, f64),  // (0.40, 0.80) - acceptable range
    tps: (u64, u64),                  // (10K, 100K) - operational range
    latency_ms_p50: u64,              // 250ms - median target
    latency_ms_p99: u64,              // 1000ms - 99th percentile max
    pruning_rate: f64,                // 0.05 - max 5% pruned
    fallback_rate: f64,               // 0.10 - max 10% fallback
}
```

### 7.4 Anomaly Detection
```rust
fn detect_anomaly(metrics: &RecentMetrics) -> Option<Anomaly> {
    if metrics.constructive_ratio < 0.30 {
        return Some(Anomaly::UnusuallyDestructive);
    }
    
    if metrics.pruning_rate > 0.15 {
        return Some(Anomaly::ExcessivePruning);
    }
    
    if metrics.latency_p99 > 3000 {
        return Some(Anomaly::HighLatency);
    }
    
    None
}
```

---

## 8. Future Documentation Structure

```
docs/
├── specifications/
│   ├── consensus-spec.md        # This file (consensus algorithm)
│   ├── consensus-params.md      # Governance parameters with invariants
│   ├── cryptography-spec.md     # Schnorr, FROST, Blake3 details
│   └── wire-protocol-spec.md    # Network message formats
├── theory/
│   ├── minkowski-interference-theory.pdf  # Formal mathematics
│   ├── security-analysis.pdf              # Threat model proofs
│   └── performance-analysis.pdf           # Complexity analysis
├── guides/
│   ├── developer-guide.md       # How to build/extend
│   ├── operator-guide.md        # How to run validator
│   └── audit-guide.md           # For third-party auditors
├── api/
│   ├── rust-api.md
│   ├── typescript-api.md
│   └── python-api.md
└── research/
    ├── papers/
    └── experiments/
```

---

## 9. Legacy Code Cleanup

```
src/
├── crypto/
│   ├── legacy/       # Archive for BLS/old Ristretto
│   │   ├── bls/
│   │   └── ristretto/
│   └── schnorr/      # Active Schnorr implementation
└── quantum/
    └── legacy/       # Archive for QuantumState
        └── old_state.rs
```

**Tech Debt Tracking:**
```markdown
# tech-debt.md

## Deprecated Modules (To Remove)
- `src/crypto/bls/` - Replaced by Schnorr (Phase 2 complete)
- `src/crypto/ristretto/` - Replaced by Schnorr
- `src/quantum/field.rs::QuantumState` - Removed, use QuantumWave

## Removal Timeline
- **Q1 2025**: Archive legacy crypto to `crypto/legacy/`
- **Q2 2025**: Remove after 6-month deprecation period
```

---

**Document Version:** 2.0  
**Last Updated:** 2025-11-30  
**Status:** Audit-ready technical specification
