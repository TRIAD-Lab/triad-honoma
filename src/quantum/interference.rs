
use crate::quantum::field::{QuantumState, InterferencePoint, QuantumField, QuantumWave};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use rayon::prelude::*;
use sha2::{Sha256, Digest};
use wide::f64x4;

#[cfg(feature = "cuda")]
use cust::prelude::*;
#[cfg(feature = "opencl")]
use ocl::{ProQue, Buffer};

pub trait QuantumInterferenceAccelerator {
    fn calculate(&self, states: &[QuantumState], resolution: usize) -> Vec<InterferencePoint>;
}

// --- SIMD helpers (module-level) ---
#[inline]
fn simd_cos4(x: f64x4) -> f64x4 {
    let two_pi = f64x4::splat(2.0 * std::f64::consts::PI);
    let inv_two_pi = f64x4::splat(1.0 / (2.0 * std::f64::consts::PI));
    let k = (x * inv_two_pi).round();
    let xr = x - k * two_pi;
    let x2 = xr * xr;
    let x4 = x2 * x2;
    let half = f64x4::splat(0.5);
    let inv24 = f64x4::splat(1.0 / 24.0);
    f64x4::splat(1.0) - x2 * half + x4 * inv24
}

#[inline]
fn sum_cos_pairs(pairs: &[(f64,f64)], x: f64, decay: f64, total_amp_sum: f64) -> (f64, f64) {
    let mut amp_sum = 0.0f64;
    let mut phase_wsum = 0.0f64;
    let pi2x = 2.0 * std::f64::consts::PI * x;

    let mut i = 0usize;
    let n = pairs.len();
    while i + 4 <= n {
        let a = f64x4::from([pairs[i].0, pairs[i+1].0, pairs[i+2].0, pairs[i+3].0]);
        let ph = f64x4::from([pairs[i].1, pairs[i+1].1, pairs[i+2].1, pairs[i+3].1]);
        let ang = ph + f64x4::splat(pi2x);
        let c = simd_cos4(ang);
        amp_sum += (a * c).to_array().iter().sum::<f64>();
        phase_wsum += (ph * a).to_array().iter().sum::<f64>();
        i += 4;
    }
    for (a, ph) in &pairs[i..] {
        amp_sum += *a * (pi2x + *ph).cos();
        phase_wsum += *ph * *a;
    }
    amp_sum *= (-x * decay).exp();
    let phase = if total_amp_sum > 0.0 { phase_wsum / total_amp_sum } else { 0.0 };
    (amp_sum, phase)
}

pub struct CpuAccelerator;
impl QuantumInterferenceAccelerator for CpuAccelerator {
    fn calculate(&self, states: &[QuantumState], resolution: usize) -> Vec<InterferencePoint> {
        let step = 1.0 / resolution as f64;
        (0..resolution).into_par_iter().map(|i| {
            let x = i as f64 * step;
            let mut total_amp = 0.0;
            for s in states {
                total_amp += s.amplitude * (2.0 * std::f64::consts::PI * x + s.phase).cos();
            }
            InterferencePoint { position: x, amplitude: total_amp, phase: 0.0 }
        }).collect()
    }
}

#[cfg(feature = "cuda")]
pub struct CudaAccelerator {
    _ctx: cust::context::Context,
}
#[cfg(feature = "cuda")]
impl QuantumInterferenceAccelerator for CudaAccelerator {
    fn calculate(&self, states: &[QuantumState], resolution: usize) -> Vec<InterferencePoint> {
        // CUDA kernel launch (stub, kernel implementation required)
        vec![InterferencePoint { position: 0.0, amplitude: 0.0, phase: 0.0 }; resolution]
    }
}

#[cfg(feature = "opencl")]
pub struct OpenClAccelerator {
    engine: crate::quantum::opencl::OpenClInterferenceEngine,
}

#[cfg(feature = "opencl")]
impl OpenClAccelerator {
    pub fn new() -> Result<Self, String> {
        match crate::quantum::opencl::OpenClInterferenceEngine::new() {
            Ok(engine) => Ok(Self { engine }),
            Err(e) => Err(format!("Failed to initialize OpenCL engine: {}", e)),
        }
    }
}

#[cfg(feature = "opencl")]
impl QuantumInterferenceAccelerator for OpenClAccelerator {
    fn calculate(&self, states: &[QuantumState], resolution: usize) -> Vec<InterferencePoint> {
        match self.engine.calculate_interference(states, resolution) {
            Ok(points) => points,
            Err(e) => {
                eprintln!("OpenCL calculation failed: {}, falling back to CPU", e);
                // Fallback to CPU
                let cpu_acc = CpuAccelerator;
                cpu_acc.calculate(states, resolution)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct InterferenceEngine {
    pub resolution: usize,
    pub threshold: f64,
    pub decay_factor: f64,
    cache: Arc<RwLock<HashMap<String, Vec<InterferencePoint>>>>,
    // Reusable buffer to avoid per-call allocations
    state_buffer: Vec<QuantumState>,
    // Reusable output buffer to reduce reallocations
    pattern_buffer: Vec<InterferencePoint>,
}

impl InterferenceEngine {
    pub fn new(resolution: usize, decay_factor: f64) -> Self {
        Self {
            // Разрешение до 10_000 точек (соответствует pitch "High-resolution patterns")
            resolution: resolution.min(10_000),
            threshold: 0.95,
            decay_factor,
            cache: Arc::new(RwLock::new(HashMap::new())),
            state_buffer: Vec::new(),
            pattern_buffer: Vec::new(),
        }
    }

    pub fn calculate_interference_pattern(&self, states: &[QuantumState]) -> Vec<InterferencePoint> {
        // Check cache
        let cache_key = self.generate_cache_key(states);
        let cache = self.cache.read();
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }

        let step = 1.0 / self.resolution as f64;
        
        // Parallel interference point calculation
        let pattern: Vec<InterferencePoint> = (0..self.resolution)
            .into_par_iter()
            .map(|i| {
                let x = i as f64 * step;
                let mut total_amplitude = 0.0;
                let mut total_phase = 0.0;
                
                // Прямой проход по состояниям без промежуточных векторов
                for state in states {
                    let contribution = InterferenceEngine::calculate_state_contribution_fast(state.amplitude, state.phase, x);
                    total_amplitude += contribution.0;
                    total_phase += contribution.1;
                }
                
                // Экспоненциальное затухание (стабильно и быстро)
                total_amplitude *= (-x * self.decay_factor).exp();
                
                InterferencePoint {
                    position: x,
                    amplitude: total_amplitude,
                    phase: total_phase,
                }
            })
            .collect();
        
        // Save to cache
        let mut cache = self.cache.write();
        cache.insert(cache_key, pattern.clone());
        
        pattern
    }

    /// Fills an internal buffer from the given QuantumField and runs one interference calculation
    /// Reuses the buffer to avoid O(n) allocations on every call
    pub fn calculate_interference_from_field(&mut self, field: &QuantumField) -> Vec<InterferencePoint> {
        // Prepare buffer
        self.state_buffer.clear();
        self.state_buffer.reserve(field.active_waves.len());
        // Map waves to states, избегая избыточного clone() на самой волне
        for wave in field.active_waves.values() {
            self.state_buffer.push(QuantumState::from(wave));
        }
        // Run calculation with GPU acceleration if available
        self.calculate_interference_auto(&self.state_buffer)
    }

    /// Вариант для шардированного поля/снимка волн: строим буфер состояний и считаем inplace.
    pub fn calculate_interference_from_waves_inplace(&mut self, waves: &[QuantumWave]) -> Vec<InterferencePoint> {
        // Перейдём на пары (amp, phase), чтобы не копировать лишние поля
        let pairs: Vec<(f64,f64)> = waves.iter().map(|w| (w.amplitude, w.phase)).collect();
        self.calculate_interference_from_pairs_inplace(&pairs)
    }

    /// In-place версия расчёта интерференции, пишет во внутренний буфер и возвращает копию результата.
    /// Это снижает число аллокаций (буфер переиспользуется между вызовами).
    pub fn calculate_interference_pattern_inplace(&mut self, states: &[QuantumState]) -> Vec<InterferencePoint> {
        // Кэш
        let cache_key = self.generate_cache_key(states);
        {
            let cache = self.cache.read();
            if let Some(cached) = cache.get(&cache_key) {
                return cached.clone();
            }
        }
        // Подготовка буфера результата
        let res = self.resolution;
        if self.pattern_buffer.len() != res {
            self.pattern_buffer.resize(res, InterferencePoint { position: 0.0, amplitude: 0.0, phase: 0.0 });
        }
        let step = 1.0 / self.resolution as f64;
        let decay = self.decay_factor;
        // Параллельное заполнение буфера по индексам без доп. аллокаций
        self.pattern_buffer
            .as_mut_slice()
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, point)| {
                let x = i as f64 * step;
                let mut total_amplitude = 0.0;
                let mut total_phase = 0.0;
                for s in states {
                    let contrib = InterferenceEngine::calculate_state_contribution_fast(s.amplitude, s.phase, x);
                    total_amplitude += contrib.0;
                    total_phase += contrib.1;
                }
                total_amplitude *= (-x * decay).exp();
                *point = InterferencePoint { position: x, amplitude: total_amplitude, phase: total_phase };
            });
        // Кэшируем и возвращаем копию буфера
        {
            let mut cache = self.cache.write();
            cache.insert(cache_key, self.pattern_buffer.clone());
        }
        self.pattern_buffer.clone()
    }

    /// Быстрый расчёт интерференции из пар (amplitude, phase) с кэшированием
    pub fn calculate_interference_from_pairs_inplace(&mut self, pairs: &[(f64,f64)]) -> Vec<InterferencePoint> {
        // Кэш-ключ по числовым параметрам
        let mut hasher = Sha256::new();
        hasher.update((pairs.len() as u64).to_le_bytes());
        for (a,p) in pairs {
            hasher.update(a.to_le_bytes());
            hasher.update(p.to_le_bytes());
        }
        hasher.update((self.resolution as u64).to_le_bytes());
        hasher.update(self.decay_factor.to_le_bytes());
        hasher.update(self.threshold.to_le_bytes());
        let cache_key = format!("{:x}", hasher.finalize());

        {
            let cache = self.cache.read();
            if let Some(cached) = cache.get(&cache_key) { return cached.clone(); }
        }

        let res = self.resolution;
        if self.pattern_buffer.len() != res {
            self.pattern_buffer.resize(res, InterferencePoint{ position:0.0, amplitude:0.0, phase:0.0 });
        }
        let step = 1.0 / self.resolution as f64;
        let decay = self.decay_factor;
        let total_amp_sum: f64 = pairs.iter().map(|(a,_)| *a).sum();
        self.pattern_buffer
            .as_mut_slice()
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, pt)| {
                let x = i as f64 * step;
                let (amp, ph_sum) = sum_cos_pairs(pairs, x, decay, total_amp_sum);
                *pt = InterferencePoint { position: x, amplitude: amp, phase: ph_sum };
            });
        {
            let mut cache = self.cache.write();
            cache.insert(cache_key, self.pattern_buffer.clone());
        }
        self.pattern_buffer.clone()
    }
    
    fn calculate_state_contribution_fast(amplitude: f64, phase: f64, x: f64) -> (f64, f64) {
        // Optimized version with pre-calculated values
        let pi_2 = 2.0 * std::f64::consts::PI;
        let phase_x = pi_2 * x + phase;
        let wave = amplitude * phase_x.cos();
        (wave, phase)
    }

    fn generate_cache_key(&self, states: &[QuantumState]) -> String {
        // Быстрый кэш-ключ без сериализации JSON: хешируем количество и пары (amplitude, phase)
        let mut hasher = Sha256::new();
        hasher.update((states.len() as u64).to_le_bytes());
        for s in states {
            hasher.update(s.amplitude.to_le_bytes());
            hasher.update(s.phase.to_le_bytes());
        }
        // Также включаем resolution/decay_factor/threshold, чтобы кэш учитывать параметры движка
        hasher.update((self.resolution as u64).to_le_bytes());
        hasher.update(self.decay_factor.to_le_bytes());
        hasher.update(self.threshold.to_le_bytes());
        format!("{:x}", hasher.finalize())
    }
    
    pub fn analyze_interference(&self, pattern: &[InterferencePoint]) -> InterferenceAnalysis {
        let mut analysis = InterferenceAnalysis {
            max_amplitude: 0.0,
            min_amplitude: f64::MAX,
            average_amplitude: 0.0,
            constructive_points: Vec::with_capacity(pattern.len() / 2),
            destructive_points: Vec::with_capacity(pattern.len() / 2),
        };
        
        // Parallel analysis
        let (max_amp, min_amp, sum_amp, constructive, destructive) = pattern.par_iter()
            .fold(
                || (0.0f64, f64::MAX, 0.0f64, Vec::new(), Vec::new()),
                |(max, min, sum, mut constr, mut destr), point| {
                    let amplitude = point.amplitude;
                    let new_max = max.max(amplitude);
                    let new_min = min.min(amplitude);
                    let new_sum = sum + amplitude;
                    
                    if amplitude > 0.01 {
                        constr.push(point.clone());
                    } else if amplitude < -0.01 {
                        destr.push(point.clone());
                    }
                    
                    (new_max, new_min, new_sum, constr, destr)
                }
            )
            .reduce(
                || (0.0f64, f64::MAX, 0.0f64, Vec::new(), Vec::new()),
                |(max1, min1, sum1, mut constr1, mut destr1),
                 (max2, min2, sum2, mut constr2, mut destr2)| {
                    constr1.extend(constr2);
                    destr1.extend(destr2);
                    (max1.max(max2), min1.min(min2), sum1 + sum2, constr1, destr1)
                }
            );
        
        analysis.max_amplitude = max_amp;
        analysis.min_amplitude = min_amp;
        analysis.average_amplitude = sum_amp / pattern.len() as f64;
        analysis.constructive_points = constructive;
        analysis.destructive_points = destructive;
        
        analysis
    }

    pub fn export_pattern_to_csv(&self, pattern: &[InterferencePoint], path: &str) -> std::io::Result<()> {
        use std::fs::File;
        use std::io::Write;
        let mut file = File::create(path)?;
        writeln!(file, "position,amplitude,phase")?;
        for point in pattern {
            writeln!(file, "{},{},{}", point.position, point.amplitude, point.phase)?;
        }
        Ok(())
    }

    pub fn calculate_interference_auto(&self, states: &[QuantumState]) -> Vec<InterferencePoint> {
        #[cfg(feature = "cuda")]
        {
            let ctx = cust::quick_init().unwrap();
            let acc = CudaAccelerator { _ctx: ctx }; // Kernel implementation required
            return acc.calculate(states, self.resolution);
        }
        #[cfg(all(not(feature = "cuda"), feature = "opencl"))]
        {
            match OpenClAccelerator::new() {
                Ok(acc) => {
                    println!("Using OpenCL acceleration");
                    return acc.calculate(states, self.resolution);
                }
                Err(e) => {
                    println!("OpenCL initialization failed: {}, falling back to CPU", e);
                }
            }
        }
        // Fallback on CPU
        println!("💻 Using CPU fallback");
        let acc = CpuAccelerator;
        acc.calculate(states, self.resolution)
    }
}

#[derive(Debug)]
pub struct InterferenceAnalysis {
    pub max_amplitude: f64,
    pub min_amplitude: f64,
    pub average_amplitude: f64,
    pub constructive_points: Vec<InterferencePoint>,
    pub destructive_points: Vec<InterferencePoint>,
}

impl InterferenceAnalysis {
    /// Возвращает общее количество точек в паттерне
    pub fn total_points(&self) -> usize {
        self.constructive_points.len() + self.destructive_points.len()
    }
}

#[derive(Debug, Clone)]
pub struct InterferencePattern {
    pub cache: Arc<RwLock<HashMap<String, Vec<InterferencePoint>>>>,
} 