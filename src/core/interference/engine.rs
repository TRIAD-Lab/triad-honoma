use crate::core::CoreWave;
use crate::core::field::{CoreField, InterferencePoint};
use rayon::prelude::*;
use std::sync::Arc;
use dashmap::DashMap;
use super::simd::sum_cos_pairs;
use super::barnes_hut::{Octree, BoundingBox};

#[derive(Debug, Clone)]
pub struct InterferenceEngine {
    pub resolution: usize,
    pub decay_factor: f64,
    pub threshold: f64,
    pub cache: Arc<DashMap<u64, Arc<[InterferencePoint]>>>,
}

thread_local! {
    static PATTERN_BUFFER: std::cell::RefCell<Vec<InterferencePoint>> = std::cell::RefCell::new(Vec::new());
}

impl InterferenceEngine {
    pub fn new(resolution: usize, threshold: f64) -> Self {
        Self {
            resolution,
            decay_factor: 0.1,
            threshold,
            cache: Arc::new(DashMap::with_capacity(1024)),
        }
    }

    pub fn calculate_interference_pattern(&self, waves: &[CoreWave], time: f64) -> Vec<InterferencePoint> {
        // Check cache
        let cache_key = self.generate_cache_key(waves);
        if let Some(cached) = self.cache.get(&cache_key) {
            return cached.value().to_vec();
        }

        // Use Barnes-Hut if number of waves is large enough to justify overhead
        let pattern = if waves.len() > 100 {
             self.calculate_interference_barnes_hut(waves, time)
        } else {
             self.calculate_minkowski_interference(waves, time)
        };

        // Save to cache
        self.cache.insert(
            cache_key,
            Arc::<[InterferencePoint]>::from(pattern.clone().into_boxed_slice()),
        );

        pattern
    }
    
    /// Barnes-Hut optimized interference calculation using Octree
    pub fn calculate_interference_barnes_hut(&self, waves: &[CoreWave], time: f64) -> Vec<InterferencePoint> {
        let resolution = self.resolution;
        let step = 1.0 / resolution as f64;
        let decay = self.decay_factor;
        
        // 1. Build Octree
        // Determine bounds of the simulation space (assuming 0.0 to 1.0 for now, or derive from waves)
        let center = [0.5, 0.5, 0.5];
        let half_size = [0.5, 0.5, 0.5];
        let bounds = BoundingBox::new(center, half_size);
        let mut octree = Octree::new(bounds, 16, 8); // Capacity 16, Max Depth 8
        
        for (i, wave) in waves.iter().enumerate() {
            octree.insert(i, wave.position);
        }
        
        // 2. Calculate Interaction Radius
        // R = ln(A_max / threshold) / decay
        // Estimate A_max conservatively
        let max_amp = waves.iter().map(|w| w.amplitude).fold(0.0f32, f32::max) as f64;
        let interaction_radius = if max_amp > self.threshold {
            (max_amp / self.threshold).ln() / decay
        } else {
            0.0 // Should not happen if we filter weak waves
        };
        
        // Clamp radius to avoid checking everything
        let effective_radius = interaction_radius.min(0.5) as f32;
        let radius_sq = effective_radius * effective_radius;

        // 3. Parallel calculation over grid
        // We calculate along X axis (y=0, z=0) as per original implementation
        (0..resolution).into_par_iter().map(|i| {
            let x = i as f64 * step;
            let point_pos = [x as f32, 0.0, 0.0];
            
            // Query Octree
            let query_box = BoundingBox::new(
                point_pos, 
                [effective_radius, effective_radius, effective_radius]
            );
            let nearby_indices = octree.query_range(&query_box);
            
            // Sum contributions
            let mut total_amp = 0.0;
            let mut total_phase_x: f64 = 0.0;
            let mut total_phase_y: f64 = 0.0;
            
            for &idx in &nearby_indices {
                let wave = &waves[idx];
                let dx = x - wave.position[0] as f64;
                let dy = 0.0 - wave.position[1] as f64;
                let dz = 0.0 - wave.position[2] as f64;
                let dist_sq = dx*dx + dy*dy + dz*dz;
                
                // Fine-grained check (Octree returns box, we need sphere)
                if dist_sq as f32 > radius_sq {
                    continue;
                }
                
                let dist = dist_sq.sqrt();
                let amp = (wave.amplitude as f64) * (-decay * dist).exp();
                
                // Phase calculation (simplified)
                let phase = wave.phase as f64 + 2.0 * std::f64::consts::PI * dist; // Traveling wave
                
                total_amp += amp;
                total_phase_x += amp * phase.cos();
                total_phase_y += amp * phase.sin();
            }
            
            let final_phase: f64 = total_phase_y.atan2(total_phase_x);
            let energy = total_amp * total_amp; // Simplified energy
            
            InterferencePoint {
                position: x,
                amplitude: total_amp,
                phase: final_phase,
                energy,
            }
        }).collect()
    }





    /// Вариант для шардированного поля/снимка волн: строим буфер состояний и считаем inplace.
    pub fn calculate_interference_from_waves_inplace(&self, waves: &[CoreWave]) -> Vec<InterferencePoint> {
        // Перейдём на пары (amp, phase), чтобы не копировать лишние поля
        let pairs: Vec<(f64,f64)> = waves.iter().map(|w| (w.amplitude as f64, w.phase as f64)).collect();
        self.calculate_interference_from_pairs_inplace(&pairs)
    }

    /// In-place версия расчёта интерференции, пишет во внутренний буфер и возвращает копию результата.
    /// Это снижает число аллокаций (буфер переиспользуется между вызовами).
    pub fn calculate_interference_pattern_inplace(&self, waves: &[CoreWave], time: f64) -> Vec<InterferencePoint> {
        // Кэш
        let cache_key = self.generate_cache_key(waves);
        if let Some(cached) = self.cache.get(&cache_key) {
            return cached.value().to_vec();
        }
        
        // Use Minkowski
        let pattern = self.calculate_minkowski_interference(waves, time);
        
        // Кэшируем и возвращаем
        self.cache.insert(
            cache_key,
            Arc::<[InterferencePoint]>::from(pattern.clone().into_boxed_slice()),
        );
        pattern
    }

    /// Быстрый расчёт интерференции из пар (amplitude, phase) с кэшированием
    pub fn calculate_interference_from_pairs_inplace(&self, pairs: &[(f64,f64)]) -> Vec<InterferencePoint> {
        // OPTIMIZATION: Calculate both checksum AND total_amp_sum in single pass!
        let mut checksum: u64 = pairs.len() as u64;
        let mut amp_sum_bits: u64 = 0;
        let mut phase_sum_bits: u64 = 0;
        let mut total_amp_sum: f64 = 0.0; // Calculate during same iteration
        for (a, p) in pairs {
            amp_sum_bits = amp_sum_bits.wrapping_add(a.to_bits());
            phase_sum_bits = phase_sum_bits.wrapping_add(p.to_bits());
            total_amp_sum += a; // No extra iteration needed!
        }
        checksum ^= amp_sum_bits;
        checksum ^= phase_sum_bits;
        checksum ^= self.resolution as u64;
        checksum ^= self.decay_factor.to_bits();
        checksum ^= self.threshold.to_bits();
        let cache_key = checksum;

        if let Some(cached) = self.cache.get(&cache_key) {
            return cached.value().to_vec();
        }

        let res = self.resolution;
        let step = 1.0 / self.resolution as f64;
        let decay = self.decay_factor;

        // Use thread-local buffer to avoid allocation
        // Handle re-entrancy (Rayon work stealing) by falling back to new allocation
        PATTERN_BUFFER.with(|cell: &std::cell::RefCell<Vec<InterferencePoint>>| {
            // Try to borrow the thread-local buffer
            // If already borrowed (e.g. nested Rayon task on same thread), fallback to new allocation
            let mut guard = cell.try_borrow_mut();
            let mut local_buffer = if let Ok(ref mut g) = guard {
                // We have the lock, use the thread-local buffer
                if g.len() != res {
                    g.resize(res, InterferencePoint { position: 0.0, amplitude: 0.0, phase: 0.0, energy: 0.0 });
                }
                // We need to return a mutable reference that lives long enough
                // But we can't easily return a reference from inside the Option/Result match if types differ
                // So we'll use a different approach: either use the guard or a temp vec
                None // Signal to use guard
            } else {
                // Fallback: allocate new buffer
                Some(vec![InterferencePoint { position: 0.0, amplitude: 0.0, phase: 0.0, energy: 0.0 }; res])
            };

            // Get reference to the buffer (either from guard or local_buffer)
            let buffer_slice = if let Some(ref mut vec) = local_buffer {
                vec.as_mut_slice()
            } else {
                // Must be Ok(guard)
                guard.as_mut().unwrap().as_mut_slice()
            };

            buffer_slice
                .par_iter_mut()
                .enumerate()
                .for_each(|(i, pt)| {
                    let x = i as f64 * step;
                    let (amp, ph_sum) = sum_cos_pairs(pairs, x, decay, total_amp_sum);
                    *pt = InterferencePoint { position: x, amplitude: amp, phase: ph_sum, energy: amp * amp };
                });
            
            let arc: Arc<[InterferencePoint]> = Arc::<[InterferencePoint]>::from(buffer_slice.to_vec().into_boxed_slice());
            self.cache.insert(cache_key, arc.clone());
            arc.to_vec()
        })
    }
    
    /// CORRECT: Wave contribution with frequency (as used in TRIAD)
    /// Formula: A * cos(2π * f * x + φ)
    #[inline]
    fn calculate_wave_contribution_fast(amplitude: f64, phase: f64, frequency: f64, x: f64) -> (f64, f64) {
        let pi_2 = 2.0 * std::f64::consts::PI;
        let phase_x = pi_2 * frequency * x + phase;
        let wave = amplitude * phase_x.cos();
        (wave, phase)
    }

    /// OPTIMIZATION: Правильный расчёт интерференции с частотами (из CoreWave)
    /// Formula: Σ A_i * cos(2π * f_i * x + φ_i) / √N
    /// 
    /// SIMD-optimized: вычисляет вклад каждой волны для ВСЕХ точек сразу
    /// вместо вычисления всех волн для каждой точки по отдельности.
    /// Это позволяет использовать векторные инструкции CPU (AVX2/AVX512).
    /// Minkowski interference calculation with causality checks
    /// Formula: Σ A_i * cos(ω_i * Δt - k_i * Δr + φ_i) * Θ(-s^2)
    pub fn calculate_minkowski_interference(
        &self,
        waves: &[CoreWave],
        observation_time: f64,
    ) -> Vec<InterferencePoint> {
        let resolution = self.resolution;
        let step = 1.0 / resolution as f64;
        let decay = self.decay_factor;
        
        // Pre-calculate normalization
        let total_amp_sum: f64 = waves.iter().map(|s| s.amplitude as f64).sum();
        let num_waves = total_amp_sum / 2.0;
        let normalization = if num_waves > 1.0 { num_waves.sqrt() } else { 1.0 };
        
        // Initialize result buffers
        let mut amplitudes = vec![0.0f64; resolution];
        let mut phase_weights = vec![0.0f64; resolution];
        
        // We calculate interference along the X axis at y=0, z=0, t=observation_time
        // This is a 1D cross-section of the 4D field
        
        for wave in waves {
            // Use position from CoreWave (x, y, z)
            let origin_x = wave.position[0] as f64;
            let origin_y = wave.position[1] as f64;
            let origin_z = wave.position[2] as f64;
            let origin_t = wave.timestamp as f64; // Assuming timestamp maps to time somehow, or use 0.0 if relative
            
            // Frequency is not in CoreWave anymore, assume 1.0 or derive
            let frequency = 1.0; 
            let omega = frequency * 2.0 * std::f64::consts::PI;
            let k = [1.0, 0.0, 0.0]; // Default wave vector
            
            // Optimization: Pre-calculate time difference
            let dt = observation_time - origin_t;
            
            // Causality check: only process if dt > 0 (future)
            if dt <= 0.0 { continue; }
            
            let dt_sq = dt * dt;
            let omega_dt = omega * dt;
            
            // Iterate over spatial points
            for i in 0..resolution {
                let x = i as f64 * step;
                
                // Calculate spacetime interval
                let dx = x - origin_x;
                let dy = 0.0 - origin_y;
                let dz = 0.0 - origin_z;
                
                // s^2 = -c^2*dt^2 + dx^2 + dy^2 + dz^2 (c=1)
                let s2 = -dt_sq + dx*dx + dy*dy + dz*dz;
                
                // Check if inside light cone (s^2 <= 0)
                if s2 <= 0.0 {
                    // Calculate phase: k*r - w*t + phi
                    // Here r is vector from origin to point
                    let k_dot_r = k[0]*dx + k[1]*dy + k[2]*dz;
                    let phase = k_dot_r - omega_dt + wave.phase as f64;
                    
                    // Calculate amplitude with decay
                    let r = (dx*dx + dy*dy + dz*dz).sqrt();
                    let dist_factor = (-r * decay).exp();
                    let amp = wave.amplitude as f64 * dist_factor;
                    
                    // Add to superposition
                    amplitudes[i] += amp * phase.cos();
                    phase_weights[i] += amp; // Simplified phase weighting
                }
            }
        }
        
        // Construct result
        let mut points = Vec::with_capacity(resolution);
        for i in 0..resolution {
            points.push(InterferencePoint {
                position: i as f64 * step,
                amplitude: amplitudes[i] / normalization,
                phase: 0.0, // Simplified
                energy: 0.0,
            });
        }
        
        points
    }

    fn generate_cache_key(&self, waves: &[CoreWave]) -> u64 {
        // OPTIMIZATION: Fast checksum instead of SHA256
        let mut checksum: u64 = waves.len() as u64;
        let mut amp_sum: u64 = 0;
        let mut phase_sum: u64 = 0;
        for s in waves {
            amp_sum = amp_sum.wrapping_add((s.amplitude as f64).to_bits());
            phase_sum = phase_sum.wrapping_add((s.phase as f64).to_bits());
        }
        checksum ^= amp_sum;
        checksum ^= phase_sum;
        checksum ^= self.resolution as u64;
        checksum ^= self.decay_factor.to_bits();
        checksum ^= self.threshold.to_bits();
        checksum
    }
    
    pub fn analyze_interference(&self, pattern: &[InterferencePoint]) -> InterferenceAnalysis {
        if pattern.is_empty() {
            return InterferenceAnalysis {
                max_amplitude: 0.0,
                min_amplitude: 0.0,
                average_amplitude: 0.0,
                constructive_count: 0,
                destructive_count: 0,
            };
        }
        
        // SINGLE PASS: Find max/min/avg AND count constructive/destructive points
        // This eliminates the O(n) cloning of all interference points
        let (max_amp, min_amp, sum_amp, constructive_count, destructive_count) = pattern.par_iter()
            .fold(
                || (0.0f64, f64::MAX, 0.0f64, 0usize, 0usize),
                |(max, min, sum, c_count, d_count): (f64, f64, f64, usize, usize), point| {
                    let amplitude = point.amplitude.abs();
                    let new_max = max.max(amplitude);
                    let new_min = min.min(amplitude);
                    let new_sum = sum + amplitude;
                    
                    // Calculate average on-the-fly for threshold
                    // We do a second check below, this is a heuristic
                    let is_constructive = amplitude > sum / (c_count + d_count + 1) as f64 * 1.5;
                    
                    (
                        new_max,
                        new_min,
                        new_sum,
                        if is_constructive { c_count + 1 } else { c_count },
                        if !is_constructive { d_count + 1 } else { d_count },
                    )
                }
            )
            .reduce(
                || (0.0f64, f64::MAX, 0.0f64, 0usize, 0usize),
                |(max1, min1, sum1, c1, d1): (f64, f64, f64, usize, usize), (max2, min2, sum2, c2, d2)| {
                    (max1.max(max2), min1.min(min2), sum1 + sum2, c1 + c2, d1 + d2)
                }
            );
            
        let avg_amp = sum_amp / pattern.len() as f64;
        
        // SECOND PASS: Re-count with actual average for accuracy
        // But we don't collect the points themselves - just count them
        let threshold = avg_amp * 1.5;
        let (final_constructive, final_destructive) = pattern.par_iter()
            .fold(
                || (0usize, 0usize),
                |(c, d), point| {
                    if point.amplitude > threshold {
                        (c + 1, d)
                    } else {
                        (c, d + 1)
                    }
                }
            )
            .reduce(
                || (0usize, 0usize),
                |(c1, d1), (c2, d2)| (c1 + c2, d1 + d2)
            );
            
        InterferenceAnalysis {
            max_amplitude: max_amp,
            min_amplitude: min_amp,
            average_amplitude: avg_amp,
            constructive_count: final_constructive,
            destructive_count: final_destructive,
        }
    }

    pub fn calculate_interference_auto(&self, waves: &[CoreWave], _time: f64) -> Vec<InterferencePoint> {
        // Single high-performance path: SIMD-enabled cosine sum with thread-local buffer and cache.
        let pairs: Vec<(f64, f64)> = waves.iter().map(|w| (w.amplitude as f64, w.phase as f64)).collect();
        self.calculate_interference_from_pairs_inplace(&pairs)
    }

    pub fn calculate_interference(&self, waves: &[CoreWave], time: f64) -> Vec<InterferencePoint> {
        #[cfg(feature = "cuda")]
        {
            let ctx = cust::quick_init().unwrap();
            let acc = CudaAccelerator { _ctx: ctx }; // Kernel implementation required
            return acc.calculate(waves, self.resolution, time);
        }
        #[cfg(all(not(feature = "cuda"), feature = "opencl"))]
        {
            match OpenClAccelerator::new() {
                Ok(acc) => {
                    println!("Using OpenCL acceleration");
                    return acc.calculate(waves, self.resolution, time);
                }
                Err(e) => {
                    println!("OpenCL initialization failed: {}, falling back to CPU", e);
                }
            }
        }
        // Fallback on CPU
        // println!("💻 Using CPU fallback");
        let acc = CpuAccelerator;
        acc.calculate(waves, self.resolution, time)
    }
}

#[derive(Debug)]
pub struct InterferenceAnalysis {
    pub max_amplitude: f64,
    pub min_amplitude: f64,
    pub average_amplitude: f64,
    pub constructive_count: usize,
    pub destructive_count: usize,
}

impl InterferenceAnalysis {
    pub fn total_points(&self) -> usize {
        self.constructive_count + self.destructive_count
    }
}

// Accelerators

pub trait CoreInterferenceAccelerator {
    fn calculate(&self, waves: &[CoreWave], resolution: usize, time: f64) -> Vec<InterferencePoint>;
}

/// DEPRECATED: Use InterferenceEngine::calculate_interference_with_frequencies instead!
/// This accelerator doesn't support per-wave frequencies.
pub struct CpuAccelerator;
impl CoreInterferenceAccelerator for CpuAccelerator {
    fn calculate(&self, waves: &[CoreWave], resolution: usize, time: f64) -> Vec<InterferencePoint> {
        // Delegate to the engine's static method or reimplement
        // For now, let's create a temporary engine to reuse logic
        let engine = InterferenceEngine::new(resolution, 0.0);
        engine.calculate_minkowski_interference(waves, time)
    }
}

#[cfg(feature = "cuda")]
pub struct CudaAccelerator {
    _ctx: cust::context::Context,
}
#[cfg(feature = "cuda")]
impl CoreInterferenceAccelerator for CudaAccelerator {
    fn calculate(&self, _waves: &[CoreWave], resolution: usize, _time: f64) -> Vec<InterferencePoint> {
        // CUDA kernel launch (stub, kernel implementation required)
        vec![InterferencePoint { position: 0.0, amplitude: 0.0, phase: 0.0, energy: 0.0 }; resolution]
    }
}

#[cfg(feature = "opencl")]
pub struct OpenClAccelerator {
    engine: crate::core::opencl::OpenClInterferenceEngine,
}

#[cfg(feature = "opencl")]
impl OpenClAccelerator {
    pub fn new() -> Result<Self, String> {
        match crate::core::opencl::OpenClInterferenceEngine::new() {
            Ok(engine) => Ok(Self { engine }),
            Err(e) => Err(format!("Failed to initialize OpenCL engine: {}", e)),
        }
    }
    
    pub fn calculate(&self, waves: &[CoreWave], resolution: usize, time: f64) -> Vec<InterferencePoint> {
         // FIXME: OpenClInterferenceEngine needs update too
         // self.engine.calculate(waves, resolution, time)
         vec![]
    }
}



