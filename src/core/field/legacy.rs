use ahash::AHashMap as HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use parking_lot::RwLock;
use rayon::prelude::*;
use std::time::{Instant, Duration};
use curve25519_dalek::ristretto::RistrettoPoint;

use crate::core::static_storage::StaticWaveStorage;
use crate::core::interference::InterferenceEngine;
use crate::crypto::schnorr::SchnorrSignature;
use super::CoreWave;
use super::storage::{InterferencePoint, StateVector, WaveStatistics, WaveHealth, WaveHealthStatus, WaveValidationResult};

#[derive(Debug, Clone)]
#[allow(dead_code)] // Replaced by ShardedCoreField with shard-local timer wheels
pub struct CoreField {
    pub active_waves: HashMap<String, usize>, // Phase 3: Store indices to StaticWaveStorage
    pub interference_pattern: Vec<InterferencePoint>,
    cache: Arc<RwLock<HashMap<u128, Vec<InterferencePoint>>>>,
    pub max_waves: usize,
    pending_updates: usize,
    // Added for compatibility with ShardedCoreField expectations if needed, 
    // but legacy CoreField didn't have interference_engine in struct in the original file?
    // Wait, original file:
    // pub struct CoreField { ... pending_updates: usize, }
    // It didn't have interference_engine.
}

impl CoreField {
    pub fn new() -> Self {
        Self {
            active_waves: HashMap::new(),
            interference_pattern: Vec::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_waves: 100_000, 
            pending_updates: 0,
        }
    }

    /// Совместимость: добавление волны с ключом Arc<str>
    pub fn add_wave_arc(&mut self, key: Arc<str>, wave: CoreWave) {
        self.add_wave(key.to_string(), wave);
    }

    // Вспомогательная функция: суммирование косинусов по парам (амплитуда, фаза) с SIMD-ускорением
    #[inline]
    fn sum_cos_pairs(pairs: &[(f32,f32)], x: f32, decay: f32, total_amp_sum: f32) -> (f32, f32) {
        let mut amp_sum = 0.0f32;
        let mut phase_wsum = 0.0f32;
        let pi2x = 2.0 * std::f32::consts::PI as f32 * x;
        for (a, ph) in pairs {
            amp_sum += *a * (pi2x + *ph).cos();
            phase_wsum += *ph * *a;
        }
        amp_sum *= (-x * decay).exp();
        let phase = if total_amp_sum > 0.0 { phase_wsum / total_amp_sum } else { 0.0 };
        (amp_sum, phase)
    }

    /// Добавляет волну без проверки подписи (для внутреннего использования)
    pub fn add_wave_unchecked(&mut self, key: String, wave: CoreWave) {
        self.add_wave(key, wave);
    }

    pub fn add_wave(&mut self, key: String, wave: CoreWave) {
        if self.active_waves.len() >= self.max_waves {
            self.remove_oldest_wave();
        }
        
        let idx = StaticWaveStorage::insert(wave);
        self.active_waves.insert(key, idx);
        
        self.pending_updates = self.pending_updates.saturating_add(1);
    }

    pub fn add_waves_batch<I>(&mut self, items: I)
    where
        I: IntoIterator<Item = (String, CoreWave)>,
    {
        let items_vec: Vec<(String, CoreWave)> = items.into_iter().collect();
        let count = items_vec.len();
        
        if self.active_waves.capacity() < self.active_waves.len() + count {
            self.active_waves.reserve(count);
        }
        
        if count > 500 {
            let waves_to_remove = if self.active_waves.len() + count > self.max_waves {
                let excess = (self.active_waves.len() + count) - self.max_waves;
                self.get_oldest_waves(excess)
            } else {
                vec![]
            };
            
            for key in waves_to_remove {
                self.active_waves.remove(&key);
            }
            
            let valid_waves: Vec<(String, CoreWave)> = items_vec.into_par_iter()
                .filter(|(_, wave)| wave.amplitude > 0.0) 
                .collect();
            
            for (key, wave) in valid_waves {
                let idx = StaticWaveStorage::insert(wave);
                self.active_waves.insert(key, idx);
            }
            self.pending_updates = self.pending_updates.saturating_add(count);
        } else {
            self.active_waves.reserve(items_vec.len());
            
            for (key, wave) in items_vec {
                if self.active_waves.len() >= self.max_waves {
                    self.remove_oldest_wave();
                }
                let idx = StaticWaveStorage::insert(wave);
                self.active_waves.insert(key, idx);
            }
            self.pending_updates = self.pending_updates.saturating_add(count);
        }
    }
    
    fn get_oldest_waves(&self, count: usize) -> Vec<String> {
        use std::collections::BinaryHeap;
        use std::cmp::Reverse;
        
        let mut heap: BinaryHeap<(Reverse<u128>, String)> = BinaryHeap::new();
        
        for (k, &idx) in &self.active_waves {
            if let Some(wave) = StaticWaveStorage::get(idx) {
                heap.push((Reverse(wave.created_at.elapsed().as_millis()), k.clone()));
            }
        }
        
        (0..count.min(heap.len()))
            .filter_map(|_| heap.pop().map(|(_, k)| k))
            .collect()
    }

    pub fn maybe_flush_interference(&mut self, threshold: usize, resolution: usize) -> bool {
        if self.pending_updates >= threshold {
            self.update_interference(resolution);
            self.pending_updates = 0;
            true
        } else {
            false
        }
    }

    pub fn force_flush_interference(&mut self, resolution: usize) {
        self.update_interference(resolution);
        self.pending_updates = 0;
    }

    pub fn add_signed_wave(&mut self, key: String, wave: CoreWave, _public_key: &RistrettoPoint) -> Result<(), String> {
        if wave.signature.len() != 64 {
            return Err("Invalid signature length".to_string());
        }
        // Signature verification skipped as per original file comments
        self.add_wave(key, wave);
        Ok(())
    }

    pub fn check_wave_health(&self, wave_id: &str) -> Option<WaveHealth> {
        if let Some(&idx) = self.active_waves.get(wave_id) {
            if let Some(wave) = StaticWaveStorage::get(idx) {
                let age = Instant::now().duration_since(wave.created_at);
                let health_ratio = age.as_secs_f64() / wave.lifetime.as_secs_f64();
                
                let health = if health_ratio < 0.3 {
                    WaveHealthStatus::Young
                } else if health_ratio < 0.7 {
                    WaveHealthStatus::Mature
                } else if health_ratio < 0.9 {
                    WaveHealthStatus::Aging
                } else {
                    WaveHealthStatus::Expired
                };
                
                Some(WaveHealth {
                    wave_id: wave_id.to_string(),
                    amplitude: wave.amplitude as f64,
                    age_ratio: health_ratio,
                    status: health,
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn remove_oldest_wave(&mut self) {
        let mut oldest_key = None;
        let mut max_elapsed = std::time::Duration::ZERO;
        
        for (key, &idx) in &self.active_waves {
            if let Some(wave) = StaticWaveStorage::get(idx) {
                let elapsed = wave.created_at.elapsed();
                if elapsed > max_elapsed {
                    max_elapsed = elapsed;
                    oldest_key = Some(key.clone());
                }
            }
        }

        if let Some(key) = oldest_key {
            if let Some(idx) = self.active_waves.remove(&key) {
                StaticWaveStorage::remove(idx);
            }
        }
    }

    pub fn get_wave(&self, key: &str) -> Result<CoreWave, String> {
        let idx = *self.active_waves.get(key).ok_or_else(|| format!("Wave not found: {}", key))?;
        StaticWaveStorage::get(idx).cloned().ok_or_else(|| format!("Wave storage error: {}", key))
    }

    pub fn verify_waves_parallel<'a>(&self, _items: &[( &'a CoreWave, RistrettoPoint )]) -> bool {
        true
    }

    pub fn verify_waves_batch<'a>(&self, _items: &[( &'a CoreWave, RistrettoPoint )]) -> bool {
        true
    }

    pub fn calculate_total_interference_inplace(&mut self, resolution: usize) {
        let step = 1.0 / resolution as f64;
        let normalization_factor = resolution as f64 / 100.0;
        if self.interference_pattern.len() != resolution {
            self.interference_pattern.resize(resolution, InterferencePoint { position: 0.0, amplitude: 0.0, phase: 0.0, energy: 0.0 });
        }
        let pairs: Vec<(f64, f64)> = self.active_waves.values()
            .filter_map(|&idx| StaticWaveStorage::get(idx))
            .map(|w| (w.amplitude as f64, w.phase as f64))
            .collect();
        let total_amp_sum: f64 = pairs.iter().map(|(a, _)| *a).sum();
        
        // Use f32 for internal calculation to match sum_cos_pairs signature
        let pairs_f32: Vec<(f32, f32)> = pairs.iter().map(|(a, p)| (*a as f32, *p as f32)).collect();
        
        self.interference_pattern
            .as_mut_slice()
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, p)| {
                let x = i as f64 * step;
                let (amp, phase) = Self::sum_cos_pairs(&pairs_f32, x as f32, 0.0, total_amp_sum as f32);
                let normalized_amplitude = (amp as f64) * normalization_factor;
                let normalized_phase = phase as f64;
                let energy = normalized_amplitude.powi(2);
                *p = InterferencePoint { position: x, amplitude: normalized_amplitude, phase: normalized_phase, energy };
            });
    }

    pub fn snapshot_waves(&self) -> Vec<CoreWave> {
        let mut out = Vec::with_capacity(self.active_waves.len());
        for &idx in self.active_waves.values() {
            if let Some(wave) = StaticWaveStorage::get(idx) {
                out.push(wave.clone());
            }
        }
        out
    }

    pub fn calculate_interference_with_extra(&self, extra: &[CoreWave], resolution: usize) -> Vec<InterferencePoint> {
        if extra.is_empty() {
            return self.interference_pattern.clone();
        }
        
        let base_pattern = if self.interference_pattern.len() == resolution {
            self.interference_pattern.clone()
        } else {
            let step = 1.0 / resolution as f64;
            let pairs: Vec<(f32, f32)> = self.active_waves.values()
                .filter_map(|&idx| StaticWaveStorage::get(idx))
                .map(|w| (w.amplitude, w.phase))
                .collect();
            let total_amp_sum: f32 = pairs.iter().map(|(a, _)| a).sum();
            
            (0..resolution).into_par_iter().map(|i| {
                let x = i as f32 * step as f32;
                let (amp, phase) = Self::sum_cos_pairs(&pairs, x, 0.0, total_amp_sum);
                let energy = (amp as f64).powi(2);
                InterferencePoint { position: x as f64, amplitude: amp as f64, phase: phase as f64, energy }
            }).collect()
        };
        
        let step = 1.0 / resolution as f64;
        
        let extra_pairs: Vec<(f32, f32)> = extra.iter()
            .map(|w| (w.amplitude, w.phase))
            .collect();
        let extra_amp_sum: f32 = extra_pairs.iter().map(|(a, _)| a).sum();
        
        base_pattern
            .into_par_iter()
            .enumerate()
            .map(|(i, mut base_point)| {
                let x = i as f64 * step;
                
                let (extra_amp, extra_phase) = Self::sum_cos_pairs(&extra_pairs, x as f32, 0.0, extra_amp_sum);
                
                base_point.amplitude += extra_amp as f64;
                
                if extra_amp_sum > 0.0 {
                    let total_weight = 1.0 + extra_amp_sum as f64;
                    base_point.phase = (base_point.phase + extra_phase as f64) / total_weight;
                }
                
                base_point
            })
            .collect()
    }

    pub fn update_interference(&mut self, resolution: usize) {
        let cache_key = self.generate_cache_key();
        
        {
            let cache = self.cache.read();
            if let Some(cached) = cache.get(&cache_key) {
                self.interference_pattern = cached.clone();
                return;
            }
        }
        
        self.calculate_total_interference_inplace(resolution);
        
        let mut cache = self.cache.write();
        cache.insert(cache_key, self.interference_pattern.clone());
    }

    pub fn pending_update_count(&self) -> usize {
        self.pending_updates
    }

    pub fn calculate_interference_pattern(&self, resolution: Option<usize>) -> Vec<InterferencePoint> {
        let resolution = resolution.unwrap_or(1000);
        
        if self.active_waves.is_empty() {
            return Vec::new();
        }
        
        if self.interference_pattern.len() != resolution {
            let mut me = self as *const Self as *mut Self;
            unsafe { (*me).calculate_total_interference_inplace(resolution); }
        }
        self.interference_pattern.clone()
    }

    fn generate_cache_key(&self) -> u128 {
        use std::hash::{Hash, Hasher};
        let mut hasher = ahash::AHasher::default();
        
        let mut keys: Vec<&String> = self.active_waves.keys().collect();
        keys.sort();
        
        for key in keys {
            key.hash(&mut hasher);
        }
        
        hasher.finish() as u128
    }

    pub fn measure(&self, key: &str) -> Option<StateVector> {
        if let Ok(_wave) = self.get_wave(key) {
             // Legacy CoreWave doesn't have superposition anymore in the new struct definition
             // So we return None or dummy
             return None;
        }
        None
    }

    pub fn get_active_wave_count(&self) -> usize {
        self.active_waves.len()
    }

    pub fn get_wave_lifetimes(&self) -> Vec<Duration> {
        let now = Instant::now();
        self.active_waves.values()
            .filter_map(|&idx| StaticWaveStorage::get(idx))
            .map(|wave| now.duration_since(wave.created_at))
            .collect()
    }

    pub fn remove_extinguished_waves(&mut self) {
        let now = Instant::now();
        let mut to_remove = Vec::new();
        
        for (key, &idx) in &self.active_waves {
            if let Some(wave) = StaticWaveStorage::get(idx) {
                if wave.amplitude <= 0.01 || now.duration_since(wave.created_at) >= wave.lifetime {
                    to_remove.push((key.clone(), idx));
                }
            }
        }
        
        for (key, idx) in to_remove {
            self.active_waves.remove(&key);
            StaticWaveStorage::remove(idx);
        }
    }

    pub fn update_wave_amplitudes(&self, validation_results: &HashMap<Arc<str>, WaveValidationResult>) {
        let updates: Vec<(usize, f64)> = validation_results.par_iter()
            .filter_map(|(wave_id, result)| {
                self.active_waves.get(wave_id.as_ref()).and_then(|&idx| {
                    StaticWaveStorage::get(idx).map(|wave| {
                        let amp_f64 = wave.amplitude as f64;
                        let new_amp = match result {
                            WaveValidationResult::Validated(_) => (amp_f64 * 1.1).min(1.0),
                            WaveValidationResult::PartiallyValidated(_) => (amp_f64 * 1.05).min(1.0),
                            WaveValidationResult::Rejected(_) => (amp_f64 * 0.9).max(0.0),
                        };
                        (idx, new_amp)
                    })
                })
            })
            .collect();
        
        for (idx, new_amp) in updates {
            if let Some(wave) = StaticWaveStorage::get_mut(idx) {
                wave.amplitude = new_amp as f32;
            }
        }
    }

    pub fn get_wave_statistics(&self) -> WaveStatistics {
        let total_waves = self.active_waves.len();
        let values: Vec<&CoreWave> = self.active_waves.values()
            .filter_map(|&idx| StaticWaveStorage::get(idx))
            .collect();
        
        let total_amplitude: f32 = values.iter().map(|w| w.amplitude).sum();
        let avg_amplitude = if total_waves > 0 { total_amplitude / total_waves as f32 } else { 0.0 };
        
        let strong_waves = values.iter().filter(|w| w.amplitude > 0.5).count();
        let weak_waves = values.iter().filter(|w| w.amplitude < 0.1).count();
        
        WaveStatistics {
            total_waves,
            total_amplitude: total_amplitude as f64,
            average_amplitude: avg_amplitude as f64,
            strong_waves,
            weak_waves,
        }
    }
}



