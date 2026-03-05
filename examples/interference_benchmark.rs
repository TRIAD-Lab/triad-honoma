use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::sync::Arc;
use std::time::{Instant, Duration};
use triad::core::{InterferenceEngine, CoreWave};
use triad::sharding::{SHARD_COUNT, get_shard_id};

fn main() {
    println!("🔬 TRIAD Advanced Interference Benchmark - New Architecture");
    println!("============================================================\n");

    // Test configurations
    let wave_counts = vec![10_000, 100_000, 1_000_000, 10_000_000];

    for &num_waves in &wave_counts {
        println!("📊 Testing with {} waves:", format_number(num_waves));
        println!("{}", "=".repeat(70));

        // Generate test data
        let waves = generate_test_data(num_waves);

        // Create engine
        let mut engine = InterferenceEngine::new(1000, 0.95);

        // Test 1: Inplace (optimized path with AVX-512 + Wavelet)
        println!("  1️⃣  Inplace Optimized (AVX-512 + Wavelet + Buffer Reuse):");
        {
            let start = Instant::now();
            let _pattern = engine.calculate_interference_from_waves_inplace(&waves);
            let elapsed = start.elapsed();

            print_results("Inplace", num_waves, elapsed);
        }

        // Test 2: Pairs Inplace (fastest production path)
        println!("  2️⃣  Pairs Inplace (Production Path - Zero-Copy):");
        {
            let pairs: Vec<(f64, f64)> = waves
                .iter()
                .map(|w| (w.amplitude as f64, w.phase as f64))
                .collect();

            let start = Instant::now();
            let _pattern = engine.calculate_interference_from_pairs_inplace(&pairs);
            let elapsed = start.elapsed();

            print_results("Pairs", num_waves, elapsed);
        }

        // Test 3: Sharded processing (64-way parallelism)
        println!(
            "  3️⃣  Sharded Processing ({} shards, O(1) routing):",
            SHARD_COUNT
        );
        {
            let start = Instant::now();

            // Distribute waves across shards using const routing
            let mut shard_waves: Vec<Vec<CoreWave>> = vec![vec![]; SHARD_COUNT];
            for wave in &waves {
                let tx_hash = wave_to_hash(&wave);
                let shard_id = get_shard_id(&tx_hash) as usize;
                shard_waves[shard_id].push(wave.clone());
            }

            // Pre-allocate engines for each shard to avoid allocation during processing
            // This simulates persistent engines in the actual node
            let mut engines: Vec<InterferenceEngine> = (0..SHARD_COUNT)
                .map(|_| InterferenceEngine::new(1000, 0.95))
                .collect();

            // Process each shard in parallel using Rayon, reusing the pre-allocated engines
            use rayon::prelude::*;
            let _results: Vec<_> = shard_waves
                .par_iter_mut()
                .zip(engines.par_iter_mut())
                .map(|(shard_wave_list, engine): (&mut Vec<CoreWave>, &mut InterferenceEngine)| {
                    if !shard_wave_list.is_empty() {
                        engine.calculate_interference_from_waves_inplace(shard_wave_list)
                    } else {
                        vec![]
                    }
                })
                .collect();

            let elapsed = start.elapsed();
            print_results("Sharded", num_waves, elapsed);

            // Show shard distribution
            let non_empty_shards = shard_waves.iter().filter(|s: &&Vec<CoreWave>| !s.is_empty()).count();
            println!("      Active Shards: {}/{}", non_empty_shards, SHARD_COUNT);
            let avg_per_shard = num_waves / non_empty_shards.max(1);
            println!("      Avg per Shard: {}", format_number(avg_per_shard));
        }

        // Test 4: Fast cosine approximation (Quake-3 style)
        if num_waves <= 1_000_000 {
            println!("  4️⃣  Fast Cosine Approximation (Quake-3/Demoscene):");
            {
                let start = Instant::now();
                fast_cosine_interference_test(&waves);
                let elapsed = start.elapsed();

                print_results("FastCos", num_waves, elapsed);
            }
        }

        println!();
    }

    println!("✅ Benchmark complete!");
    println!("\n🎯 Target: 10,000,000 waves/sec");
    println!("Current best should be Pairs Inplace + Sharded combined approach");
}

/// Fast cosine approximation benchmark (×6-8 faster than libm)
fn fast_cosine_interference_test(waves: &[CoreWave]) {
    const RESOLUTION: usize = 1000;
    let step = 1.0 / RESOLUTION as f32;

    let mut pattern = vec![0.0f32; RESOLUTION];

    // Extract amplitude/phase pairs (already f32)
    let pairs: Vec<(f32, f32)> = waves.iter().map(|w| (w.amplitude, w.phase)).collect();

    // Fast parallel calculation with approximated cosine
    use rayon::prelude::*;
    pattern.par_iter_mut().enumerate().for_each(|(i, pt)| {
        let x = i as f32 * step;
        let pi2x = 2.0 * std::f32::consts::PI * x;

        let mut amp_sum = 0.0f32;
        for (amp, phase) in &pairs {
            // Fast cosine approximation (Quake-3 style)
            let angle = pi2x + phase;
            let cos_val = fast_cos(angle);
            amp_sum += amp * cos_val;
        }

        *pt = amp_sum / (pairs.len() as f32).sqrt();
    });
}

/// Fast cosine approximation from demoscene/Quake-3
/// ~6-8x faster than f32::cos() with acceptable error
#[inline]
fn fast_cos(mut x: f32) -> f32 {
    const TWO_PI: f32 = std::f32::consts::TAU;
    const INV_TWO_PI: f32 = 1.0 / std::f32::consts::TAU;

    // Wrap to [-π, π]
    x = x - (x * INV_TWO_PI).round() * TWO_PI;

    // Taylor series approximation: cos(x) ≈ 1 - x²/2 + x⁴/24
    let x2 = x * x;
    let x4 = x2 * x2;
    1.0 - x2 * 0.5 + x4 * (1.0 / 24.0)
}

fn wave_to_hash(wave: &CoreWave) -> [u8; 32] {
    use blake3;
    let data = format!("{}{}{}", wave.amplitude, wave.phase, wave.timestamp);
    let hash = blake3::hash(data.as_bytes());
    *hash.as_bytes()
}

fn generate_test_data(num_waves: usize) -> Vec<CoreWave> {
    let mut rng = StdRng::seed_from_u64(42);
    let mut waves = Vec::with_capacity(num_waves);

    for i in 0..num_waves {
        let amplitude = rng.gen_range(0.1..1.0);
        let phase = rng.gen_range(0.0..(2.0 * std::f32::consts::PI));
        waves.push(CoreWave {
            id: Arc::from(format!("wave_{}", i)),
            amplitude,
            phase,
            position: [
                rng.gen_range(-1.0..1.0),
                rng.gen_range(-1.0..1.0),
                rng.gen_range(-1.0..1.0),
            ],
            timestamp: rng.gen_range(0..1_000_000),
            shard_id: 0,
            shard_index: i % SHARD_COUNT,
            created_at: Instant::now(),
            lifetime: Duration::from_secs(60),
            signature: Vec::new(),
        });
    }

    waves
}

fn print_results(_label: &str, num_waves: usize, elapsed: std::time::Duration) {
    let secs = elapsed.as_secs_f64();
    let waves_per_sec = num_waves as f64 / secs;

    println!("      Time: {:.3}s", secs);
    println!(
        "      Speed: {} waves/sec",
        format_number(waves_per_sec as usize)
    );

    // Show progress towards 10M target
    let target = 10_000_000.0;
    let achievement = (waves_per_sec / target * 100.0).min(999.9);
    if achievement >= 100.0 {
        println!("      🎯 TARGET ACHIEVED! {:.1}% of 10M/s", achievement);
    } else {
        println!("      Progress: {:.1}% of 10M/s target", achievement);
    }
    println!();
}

fn format_number(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.2}k", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}
