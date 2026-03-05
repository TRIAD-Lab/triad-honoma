# Consensus Parameters Reference

> **Runtime-Configurable Parameters** with safety invariants and governance limits

## 1. Wave Mapping Parameters

```rust
pub struct WaveMappingParams {
    /// Amplitude scaling factor
    /// Invariant: 0.1 ≤ α ≤ 10.0
    /// Default: 1.0
    pub alpha: f64,
    
    /// Spatial scale for 3-torus
    /// Invariant: 100.0 ≤ L ≤ 10000.0
    /// Default: 1000.0
    pub spatial_scale: f32,
    
    /// Minimum viable transaction amount (spam threshold)
    /// Invariant: 0 ≤ min_amount ≤ 10^6
    /// Default: 1000 (wei equivalent)
    pub min_viable_amount: u64,
}
```

**Governance Constraints:**
- `alpha` adjustments require 7-day governance vote
- Changes > 2x require supermajority (66%)

---

## 2. Causal Filter Parameters

```rust
pub struct CausalParams {
    /// Speed of light (causality constant)
    /// Invariant: 0.5 ≤ c ≤ 2.0
    /// Default: 1.0
    pub c: f32,
    
    /// Wave number
    /// Invariant: 0.5 ≤ k ≤ 2.0
    /// Default: 1.0
    pub k: f32,
    
    /// Local radius for marginal contribution estimation
    /// Invariant: 0.01L ≤ R_local ≤ 0.5L
    /// Default: 0.1L
    pub r_local: f32,
}
```

---

## 3. Interference Computation

```rust
pub struct InterferenceParams {
    /// Exact computation threshold (wave count)
    /// Invariant: 1000 ≤ S_exact ≤ 10000
    /// Default: 4096
    pub s_exact: usize,
    
    /// Sampling points for approximate mode
    /// Invariant: 1024 ≤ sampling_points ≤ 32768
    /// Default: 8192
    pub sampling_points: usize,
    
    /// Barnes-Hut accuracy parameter
    /// Invariant: 0.3 ≤ θ ≤ 0.8
    /// Default: 0.5 (lower = more accurate, slower)
    pub theta: f64,
}
```

**Performance Trade-offs:**
- `s_exact`: Higher → more accurate but slower for large batches
- `sampling_points`: Higher → better statistical significance, higher compute
- `theta`: Lower → more accurate approximation, slower Barnes-Hut

---

## 4. Pruning Algorithm

```rust
pub struct PruningParams {
    /// Constructive ratio threshold for immediate accept
    /// Invariant: 0.50 ≤ R_ok ≤ 0.80
    /// Default: 0.60
    pub r_ok: f64,
    
    /// Constructive ratio threshold for immediate reject
    /// Invariant: 0.05 ≤ R_reject ≤ 0.30
    /// Default: 0.15
    pub r_reject: f64,
    
    /// Maximum fraction of batch that can be pruned
    /// Invariant: 0.01 ≤ max_prune_fraction ≤ 0.10
    /// Default: 0.05 (5%)
    /// CRITICAL: Never allow > 10% (safety limit)
    pub max_prune_fraction: f64,
    
    /// Maximum pruning iterations before fallback
    /// Invariant: 1 ≤ max_prune_iter ≤ 5
    /// Default: 3
    pub max_prune_iter: usize,
    
    /// Marginal contribution threshold
    /// Computed as: ε = epsilon_factor * E_tot
    /// Invariant: 1e-4 ≤ epsilon_factor ≤ 1e-2
    /// Default: 1e-3
    pub epsilon_factor: f64,
}
```

**Safety Invariants (Enforced in Code):**
```rust
impl PruningParams {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.max_prune_fraction > 0.10 {
            return Err(ValidationError::UnsafePruningLimit);
        }
        
        if self.r_ok <= self.r_reject {
            return Err(ValidationError::InvalidThresholds);
        }
        
        Ok(())
    }
}
```

---

## 5. Compute Budgets

```rust
pub struct ComputeBudgetParams {
    /// Maximum interference compute time per batch (ms)
    /// Invariant: 100 ≤ max_compute_ms ≤ 5000
    /// Default: 1000ms
    pub max_interference_compute_ms: u64,
    
    /// Maximum waves in Barnes-Hut computation
    /// Invariant: 10K ≤ max_bh_points ≤ 1M
    /// Default: 100K
    pub max_barnes_hut_points: usize,
    
    /// Gas limit for transaction processing
    /// Invariant: 1M ≤ gas_limit ≤ 100M
    /// Default: 10M
    pub gas_limit: u64,
}
```

**Adaptive Adjustments:**
- ML model tunes `max_compute_ms` based on network load
- `gas_limit` adjusted via governance based on TVM complexity

---

## 6. Rate Limiting

```rust
pub struct RateLimitParams {
    /// Transactions per second per address
    /// Invariant: 1 ≤ per_address_limit ≤ 1000
    /// Default: 10
    pub per_address_tx_per_sec: u64,
    
    /// Global network throughput limit
    /// Invariant: 1K ≤ global_limit ≤ 1M
    /// Default: 100K
    pub global_tx_per_sec: u64,
    
    /// Minimum fee (base unit, e.g., wei)
    /// Invariant: 0 ≤ min_fee ≤ 10^9
    /// Default: 1000
    pub min_fee: u64,
}
```

**Fee Market Dynamics:**
- `min_fee` adjusts based on network congestion (EIP-1559-style)
- Rate limits prevent spam while allowing legitimate use

---

## 7. Parameter Update Workflow

```rust
pub struct ParamUpdateProposal {
    pub proposed_params: ConsensusParams,
    pub proposer: PublicKey,
    pub justification: String,
    pub activation_delay: Duration,  // Default: 24 hours
}

impl ParamUpdateProposal {
    pub fn validate(&self) -> Result<()> {
        // 1. Check all invariants
        self.proposed_params.validate()?;
        
        // 2. Check governance signature
        verify_governance_signature(&self)?;
        
        // 3. Ensure safe activation delay
        if self.activation_delay < Duration::from_hours(6) {
            return Err(Error::UnsafeActivationDelay);
        }
        
        Ok(())
    }
}
```

**Activation Process:**
1. Proposal submitted to governance contract
2. 7-day voting period
3. If approved (>50% for minor, >66% for major changes)
4. 24-hour safety delay before activation
5. Automatic rollback if metrics degrade >20%

---

## 8. Emergency Overrides

```rust
pub struct EmergencyOverride {
    /// Can reduce max_prune_fraction in case of attack
    /// Requires 5/7 multisig from core team
    pub emergency_prune_limit: Option<f64>,
    
    /// Can increase gas_limit temporarily for critical operations
    pub emergency_gas_limit: Option<u64>,
    
    /// Time-limited: auto-expires after 7 days
    pub expires_at: Timestamp,
}
```

**Use Cases:**
- Detected pruning abuse → reduce `max_prune_fraction` to 1%
- Critical TVM upgrade → temporarily increase `gas_limit`
- DOS attack → reduce `per_address_tx_per_sec`

---

## 9. Historical Parameter Log

All parameter changes logged on-chain:

```rust
pub struct ParamChangeLog {
    pub timestamp: u64,
    pub block_height: u64,
    pub param_name: String,
    pub old_value: Value,
    pub new_value: Value,
    pub proposer: PublicKey,
    pub vote_results: VoteResults,
}
```

**Audit Trail:**
- Full history of all parameter changes
- Signed by governance multisig
- Immutable on-chain record

---

**Last Updated:** 2025-11-30  
**Governance Contract:** `triad://governance/consensus-params`
