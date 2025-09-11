use std::env;

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub batch_size: usize,
    pub channel_capacity: usize,
    pub interf_threshold: usize,
    pub interf_resolution: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            batch_size: 1024,
            channel_capacity: 65_536,
            interf_threshold: 512,
            interf_resolution: 10_000,
        }
    }
}

impl PipelineConfig {
    pub fn from_env() -> Self {
        let def = Self::default();
        let parse = |k: &str, d: usize| env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        Self {
            batch_size: parse("TRIAD_BATCH_SIZE", def.batch_size),
            channel_capacity: parse("TRIAD_CHANNEL_CAP", def.channel_capacity),
            interf_threshold: parse("TRIAD_INTERF_THRESHOLD", def.interf_threshold),
            interf_resolution: parse("TRIAD_INTERF_RES", def.interf_resolution),
        }
    }
}
