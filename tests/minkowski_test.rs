use num_complex::Complex;
use triad::quantum::InterferenceEngine;
use triad::quantum::field::{C, QuantumState, QuantumWave, SpaceTimePoint};

#[test]
fn test_causality_spacelike_separation() {
    // Create an engine
    let engine = InterferenceEngine::new(100, 0.1);

    // Observation at t=10, x=0
    let obs_time = 10.0;

    // Wave 1: Origin at t=0, x=20.
    // dt = 10, dx = 20.
    // s^2 = -c^2*dt^2 + dx^2 = -1*100 + 400 = 300 > 0 (Spacelike)
    // Should NOT contribute.
    let wave1 = QuantumState {
        id: "wave1".to_string(),
        shard_id: "shard1".to_string(),
        amplitude: 1.0,
        phase: 0.0,
        frequency: 1.0,
        wave_vector: [1.0, 0.0, 0.0],
        origin: SpaceTimePoint::new(20.0, 0.0, 0.0, 0.0),
        superposition: vec![],
    };

    let points = engine.calculate_minkowski_interference(&[wave1], obs_time);

    // Check amplitude at x=0 (index 0)
    // Since we scan x from 0 to 1 (normalized in engine usually, but let's check resolution)
    // Engine resolution 100, step 0.01. x goes 0.00 to 0.99.
    // Our wave is at x=20.0.
    // Wait, the engine calculates interference pattern over a range [0, 1].
    // But the wave origin is at x=20.0.
    // The distance dx = x_obs - x_origin.
    // If x_obs is in [0, 1], and x_origin is 20, dx is approx -20.
    // dt = 10.
    // s^2 = -100 + (-20)^2 = 300 > 0. Spacelike.
    // So for ALL points in the range [0, 1], the wave should be invisible.

    for p in points {
        assert_eq!(p.amplitude, 0.0, "Spacelike wave should have 0 amplitude");
    }
}

#[test]
fn test_causality_timelike_separation() {
    let engine = InterferenceEngine::new(100, 0.1);
    let obs_time = 10.0;

    // Wave 2: Origin at t=0, x=5.
    // dt = 10, dx = 5 (approx, since x_obs in [0, 1]).
    // s^2 = -100 + 25 = -75 < 0 (Timelike).
    // Should contribute.
    let wave2 = QuantumState {
        id: "wave2".to_string(),
        shard_id: "shard1".to_string(),
        amplitude: 1.0,
        phase: 0.0,
        frequency: 1.0,
        wave_vector: [1.0, 0.0, 0.0],
        origin: SpaceTimePoint::new(5.0, 0.0, 0.0, 0.0),
        superposition: vec![],
    };

    let points = engine.calculate_minkowski_interference(&[wave2], obs_time);

    let mut max_amp: f64 = 0.0;
    for p in points {
        max_amp = max_amp.max(p.amplitude.abs());
    }

    assert!(
        max_amp > 0.0,
        "Timelike wave should have non-zero amplitude"
    );
}

#[test]
fn test_future_wave_ignored() {
    let engine = InterferenceEngine::new(100, 0.1);
    let obs_time = 10.0;

    // Wave 3: Origin at t=20. Future!
    // Should be ignored.
    let wave3 = QuantumState {
        id: "wave3".to_string(),
        shard_id: "shard1".to_string(),
        amplitude: 1.0,
        phase: 0.0,
        frequency: 1.0,
        wave_vector: [1.0, 0.0, 0.0],
        origin: SpaceTimePoint::new(0.0, 0.0, 0.0, 20.0),
        superposition: vec![],
    };

    let points = engine.calculate_minkowski_interference(&[wave3], obs_time);

    for p in points {
        assert_eq!(p.amplitude, 0.0, "Future wave should be ignored");
    }
}

#[test]
fn test_interference_constructive() {
    let engine = InterferenceEngine::new(100, 0.0); // No decay for simpler check
    let obs_time = 10.0;

    // Two identical waves at same origin
    let wave1 = QuantumState {
        id: "w1".to_string(),
        shard_id: "s1".to_string(),
        amplitude: 1.0,
        phase: 0.0,
        frequency: 1.0,
        wave_vector: [1.0, 0.0, 0.0],
        origin: SpaceTimePoint::new(0.0, 0.0, 0.0, 0.0),
        superposition: vec![],
    };

    let wave2 = QuantumState {
        id: "w2".to_string(),
        shard_id: "s1".to_string(),
        amplitude: 1.0,
        phase: 0.0,
        frequency: 1.0,
        wave_vector: [1.0, 0.0, 0.0],
        origin: SpaceTimePoint::new(0.0, 0.0, 0.0, 0.0),
        superposition: vec![],
    };

    let points = engine.calculate_minkowski_interference(&[wave1, wave2], obs_time);

    // Amplitudes should sum up (constructive)
    // Normalization might affect this.
    // Engine normalizes by sqrt(total_amp/2).
    // Total amp = 2. Num waves = 1. Normalization = 1.
    // So amplitude should be approx 2 * cos(...).

    // Let's check if it's roughly double a single wave
    let points_single = engine.calculate_minkowski_interference(
        &[QuantumState {
            id: "w1".to_string(),
            shard_id: "s1".to_string(),
            amplitude: 1.0,
            phase: 0.0,
            frequency: 1.0,
            wave_vector: [1.0, 0.0, 0.0],
            origin: SpaceTimePoint::new(0.0, 0.0, 0.0, 0.0),
            superposition: vec![],
        }],
        obs_time,
    );

    // Compare peak amplitudes
    let max_single = points_single
        .iter()
        .map(|p| p.amplitude.abs())
        .fold(0.0, f64::max);
    let max_double = points.iter().map(|p| p.amplitude.abs()).fold(0.0, f64::max);

    // With normalization logic in engine:
    // Single: total=1, num=0.5, norm=1. Amp ~ 1.
    // Double: total=2, num=1, norm=1. Amp ~ 2.
    // So double should be 2x single.

    assert!(
        max_double > max_single * 1.5,
        "Constructive interference should increase amplitude"
    );
}
