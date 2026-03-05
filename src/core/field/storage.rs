use serde::{Serialize, Deserialize};
use num_complex::Complex;
use std::time::Duration;

/// Speed of light in the simulation (normalized). 
/// Using 1.0 implies units are light-seconds (distance) and seconds (time).
pub const C: f64 = 1.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SpaceTimePoint {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub t: f64,
}

impl SpaceTimePoint {
    pub fn new(x: f64, y: f64, z: f64, t: f64) -> Self {
        Self { x, y, z, t }
    }

    pub fn zero() -> Self {
        Self { x: 0.0, y: 0.0, z: 0.0, t: 0.0 }
    }

    /// Calculates the spacetime interval squared (s^2) using Minkowski metric (-+++)
    /// s^2 = -c^2*dt^2 + dx^2 + dy^2 + dz^2
    pub fn interval_sq(&self, other: &SpaceTimePoint) -> f64 {
        let dt = self.t - other.t;
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        
        -(C * C * dt * dt) + dx * dx + dy * dy + dz * dz
    }
}

impl Default for SpaceTimePoint {
    fn default() -> Self {
        Self::zero()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateVector {
    pub value: Complex<f64>,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterferencePoint {
    pub position: f64,
    pub amplitude: f64,
    pub phase: f64,
    pub energy: f64,
}

#[derive(Debug, Clone)]
pub struct WaveStatistics {
    pub total_waves: usize,
    pub total_amplitude: f64,
    pub average_amplitude: f64,
    pub strong_waves: usize,
    pub weak_waves: usize,
}

#[derive(Debug, Clone)]
pub struct WaveHealth {
    pub wave_id: String,
    pub amplitude: f64,
    pub age_ratio: f64,
    pub status: WaveHealthStatus,
}

#[derive(Debug, Clone)]
pub enum WaveHealthStatus {
    Young,    // Молодая волна
    Mature,   // Зрелая волна
    Aging,    // Стареющая волна
    Expired,  // Истекшая волна
}

// Добавляем enum для результатов валидации волны
#[derive(Debug, Clone)]
pub enum WaveValidationResult {
    Validated(f64),           // Волна валидирована с амплитудой
    PartiallyValidated(f64),  // Частично валидирована
    Rejected(String),         // Отторгнута
}

#[derive(Debug, Clone)]
pub enum WaveFate {
    Amplify,    // Усилить волну
    Attenuate,  // Ослабить волну
    Cancel,     // Погасить волну
}


