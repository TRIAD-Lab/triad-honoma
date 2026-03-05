/// Criterion Benchmarks для Attack Scenarios
///
/// Usage: cargo bench --bench attack_scenarios
///
/// IMPORTANT: Используется ТОЛЬКО РЕАЛЬНАЯ логика из benchmark_helpers
/// NO SIMPLIFIED LOGIC!
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use tokio::runtime::Runtime;

// REAL logic from src/benchmark_helpers
use triad::benchmark_helpers::*;

/// Benchmark: Phase calculation (brute force) - REAL LOGIC
fn bench_phase_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("phase_calculation");

    for size in [1_000, 10_000, 100_000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                // REAL implementation from benchmark_helpers
                bench_phase_calculation_real(size, 0.015).len()
            });
        });
    }

    group.finish();
}

/// Benchmark: BLS vs Ed25519 signature verification - REAL LOGIC
fn bench_signature_verification(c: &mut Criterion) {
    let mut group = c.benchmark_group("signature_verification");

    group.bench_function("bls_verify_100", |b| {
        b.iter(|| {
            // REAL BLS verification
            bench_bls_verify_real(100)
        });
    });

    group.bench_function("ed25519_verify_100", |b| {
        b.iter(|| {
            // REAL Ed25519 verification
            bench_ed25519_verify_real(100)
        });
    });

    group.finish();
}

/// Benchmark: BLS batch verification - REAL LOGIC
fn bench_bls_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("bls_batch_verification");

    for size in [100, 1_000, 10_000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                // REAL BLS batch verification
                bench_bls_batch_real(size)
            });
        });
    }

    group.finish();
}

/// Benchmark: Parallel signature verification - REAL LOGIC
fn bench_parallel_verification(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_verification");

    for size in [1_000, 10_000, 50_000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                // REAL parallel verification with Rayon
                bench_parallel_verify_real(size)
            });
        });
    }

    group.finish();
}

/// Benchmark: Amplitude flood economics - REAL LOGIC
fn bench_amplitude_economics(c: &mut Criterion) {
    c.bench_function("amplitude_flood_economics_100k", |b| {
        b.iter(|| {
            // REAL quadratic fee calculation
            bench_amplitude_economics_real(100_000)
        });
    });
}

/// Benchmark: TTL enforcement - REAL LOGIC
fn bench_ttl_enforcement(c: &mut Criterion) {
    c.bench_function("ttl_enforcement_500_txs", |b| {
        b.iter(|| {
            // REAL TTL enforcement logic
            bench_ttl_enforcement_real(500, 10_000)
        });
    });
}

/// Benchmark: Shard distribution - REAL LOGIC
fn bench_shard_distribution(c: &mut Criterion) {
    c.bench_function("shard_distribution_200k_txs", |b| {
        b.iter(|| {
            // REAL shard load distribution
            bench_shard_distribution_real(200_000, 4)
        });
    });
}

criterion_group!(
    benches,
    bench_phase_calculation,
    bench_signature_verification,
    bench_bls_batch,
    bench_parallel_verification,
    bench_amplitude_economics,
    bench_ttl_enforcement,
    bench_shard_distribution
);

criterion_main!(benches);
