use std::env;

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub batch_size: usize,
    pub channel_capacity: usize,
    pub interf_threshold: usize,
    pub interf_resolution: usize,
    pub resonance_tolerance: f64,
    pub regen_min_per_shard: usize,
    pub regen_weak_threshold: f64,
    pub regen_max_new_per_shard: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            batch_size: 1024,
            channel_capacity: 65_536,
            interf_threshold: 512,
            interf_resolution: 10_000,
            resonance_tolerance: 0.01,      // 1% default tolerance
            regen_min_per_shard: 8,
            regen_weak_threshold: 0.10,
            regen_max_new_per_shard: 8,
        }
    }
}

impl PipelineConfig {
    pub fn from_env() -> Self {
        let def = Self::default();
        let parse = |k: &str, d: usize| env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let parse_f = |k: &str, d: f64| env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        Self {
            batch_size: parse("TRIAD_BATCH_SIZE", def.batch_size),
            channel_capacity: parse("TRIAD_CHANNEL_CAP", def.channel_capacity),
            interf_threshold: parse("TRIAD_INTERF_THRESHOLD", def.interf_threshold),
            interf_resolution: parse("TRIAD_INTERF_RES", def.interf_resolution),
            resonance_tolerance: parse_f("TRIAD_RESONANCE_TOL", def.resonance_tolerance),
            regen_min_per_shard: parse("TRIAD_REGEN_MIN_PER_SHARD", def.regen_min_per_shard),
            regen_weak_threshold: parse_f("TRIAD_REGEN_WEAK_THR", def.regen_weak_threshold),
            regen_max_new_per_shard: parse("TRIAD_REGEN_MAX_NEW", def.regen_max_new_per_shard),
        }
    }
}
